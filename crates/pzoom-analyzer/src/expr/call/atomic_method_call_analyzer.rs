//! Atomic method-call analyzer.
//!
//! Resolves and checks an instance method call against a single receiver type:
//! method resolution up the hierarchy, magic `__call`/magic-property calls,
//! visibility, template/`static` localization, inherited return/param types,
//! return-type-provider adjustments, argument verification, and post-call
//! assertions. Mirrors Psalm's `AtomicMethodCallAnalyzer` / `ExistingAtomicMethodCallAnalyzer`.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::functionlike_info::AssertionType;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;

use super::argument_analyzer;
use super::{arguments_analyzer, existing_atomic_method_call_analyzer, function_call_analyzer};

use super::method_call_analyzer::*;

use super::method_call_prohibition_analyzer::*;
use super::method_call_return_type_fetcher::*;
use super::method_visibility_analyzer::*;
use super::missing_method_call_handler::*;
use pzoom_code_info::TemplateResult;

/// Fold one receiver member's return type into the accumulator: union members
/// are combined (Psalm combines per receiver atomic), intersection components
/// are intersected (Psalm `getIntersectionReturnType` / Hakana
/// `intersect_union_types_simple`). A genuinely empty intersection keeps the
/// accumulator rather than collapsing the result to `nothing`.
fn accumulate_member_return(
    accumulated: Option<TUnion>,
    member_return: TUnion,
    is_intersection: bool,
    codebase: &pzoom_code_info::CodebaseInfo,
) -> TUnion {
    match accumulated {
        None => member_return,
        Some(existing) => {
            if is_intersection {
                assertion_reconciler::intersect_union_with_union_with_codebase(
                    &existing,
                    &member_return,
                    Some(codebase),
                )
                .unwrap_or(existing)
            } else {
                pzoom_code_info::combine_union_types_with_codebase(
                    &existing,
                    &member_return,
                    false,
                    codebase,
                )
            }
        }
    }
}

pub(crate) fn get_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    obj_type: &TUnion,
    method_name: &str,
    pos: Pos,
    method_name_pos: Pos,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    enforce_mutation_free: bool,
    suppress_possibly_null_reference_issue: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    if method_name.eq_ignore_ascii_case("__construct")
        // Psalm guards this on `$stmt->var instanceof Variable` — a constructor
        // reached through a non-variable receiver (e.g. `factory()->__construct()`)
        // is not reported.
        && matches!(object_expr.unparenthesized(), Expression::Variable(_))
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::DirectConstructorCall,
            "Constructors should not be called directly",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let reconciled_receiver_type =
        get_reconciled_receiver_type_for_expression(context, object_expr)
            .and_then(|tracked_type| {
                assertion_reconciler::intersect_union_with_union(obj_type, &tracked_type)
            })
            .map(|mut intersected| {
                // The intersection rebuilds the union; the receiver's leniency
                // flags (@psalm-ignore-*-return) must survive for the
                // PossiblyFalse/NullReference gates below.
                intersected.ignore_falsable_issues |= obj_type.ignore_falsable_issues;
                intersected.ignore_nullable_issues |= obj_type.ignore_nullable_issues;
                intersected
            })
            .unwrap_or_else(|| obj_type.clone());
    let expanded_obj_type =
        crate::expr::call::method_call_return_type_fetcher::expand_template_object_union_with_type_variables(
            &reconciled_receiver_type,
            Some(&analysis_data.type_variable_bounds),
        );

    // A `nothing`/`never` receiver is unreachable: the call yields `nothing`
    // with no diagnostic (Hakana returns get_nothing(); pzoom previously fell
    // through to a spurious InvalidMethodCall).
    if expanded_obj_type.is_nothing() {
        return Some(TUnion::nothing());
    }

    let mut resolved_method: Option<(
        pzoom_str::StrId,
        pzoom_str::StrId,
        Option<Vec<TUnion>>,
        pzoom_code_info::FunctionLikeInfo,
    )> = None;
    // Other union members' resolutions: Psalm analyzes the call once per
    // receiver atomic and combines the return types, so `Scalar|TArray`
    // calling toPhpString() yields ?string even though TArray's override
    // returns string. The primary drives argument checks and dataflow; the
    // secondaries only fold their return types in at the end.
    let mut secondary_methods: Vec<(
        pzoom_str::StrId,
        pzoom_str::StrId,
        Option<Vec<TUnion>>,
        pzoom_code_info::FunctionLikeInfo,
    )> = Vec::new();
    // The remaining components of a `A&B` intersection receiver: unlike union
    // members (whose returns are combined), intersection-component returns are
    // *intersected* into the primary's return (Psalm `getIntersectionReturnType`
    // / Hakana `intersect_union_types_simple`).
    let mut intersection_secondary_methods: Vec<(
        pzoom_str::StrId,
        pzoom_str::StrId,
        Option<Vec<TUnion>>,
        pzoom_code_info::FunctionLikeInfo,
    )> = Vec::new();
    let mut has_unsealed_magic_call = false;
    let mut magic_call_return_type: Option<TUnion> = None;
    let mut has_valid_receiver = false;
    let mut has_null_receiver = false;
    let mut has_false_receiver = false;
    let mut has_invalid_receiver = false;
    let mut has_receiver_without_method = false;
    // Psalm's AtomicMethodCallAnalysisResult::too_many_arguments aggregation:
    // TooManyArguments only reports when NO union candidate accepts the
    // provided argument count (maybeNotTooManyArgumentsToInstance).
    let mut any_candidate_accepts_arg_count = false;
    let mut first_missing_interface: Option<StrId> = None;
    let is_this_call =
        expression_identifier::get_expression_var_key(object_expr).as_deref() == Some("$this");
    let calling_class = analyzer.get_declaring_class();

    // An enum case (or bare enum) receiver dispatches as an instance of its
    // enum class (Psalm expands TEnumCase to the enum's storage for calls).
    let receiver_atomics: Vec<TAtomic> = expanded_obj_type
        .types
        .iter()
        .map(|atomic| match atomic {
            TAtomic::TEnumCase { enum_name, .. } | TAtomic::TEnum { name: enum_name } => {
                TAtomic::TNamedObject {
                    name: *enum_name,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                }
            }
            other => other.clone(),
        })
        .collect();

    for atomic in &receiver_atomics {
        match atomic {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if let Some((resolved_class, resolved_type_params, method_info)) =
                        resolve_named_object_instance_method(
                            analyzer,
                            class_info,
                            type_params.as_deref(),
                            method_name,
                            Some(&analysis_data.type_variable_bounds),
                        )
                    {
                        has_valid_receiver = true;
                        // Psalm's ExistingAtomicMethodCallAnalyzer clears
                        // result->too_many_arguments when this candidate has a
                        // variadic, enough params, or comes from the callmap.
                        any_candidate_accepts_arg_count = any_candidate_accepts_arg_count
                            || method_info.params.last().is_some_and(|p| p.is_variadic)
                            || method_info.params.len() >= args.len()
                            || analyzer
                                .codebase
                                .files
                                .get(&method_info.file_path)
                                .is_some_and(|file_info| file_info.is_stub);
                        if let Some(existing) = &mut resolved_method {
                            let existing_is_interface = analyzer
                                .codebase
                                .get_class(existing.1)
                                .is_some_and(|info| info.kind == ClassLikeKind::Interface);
                            if class_info.kind == ClassLikeKind::Interface
                                && existing_is_interface
                                && method_has_more_specific_return(
                                    analyzer,
                                    &method_info,
                                    &existing.3,
                                )
                            {
                                *existing =
                                    (*name, resolved_class, resolved_type_params, method_info);
                            } else if existing.1 != resolved_class {
                                secondary_methods.push((
                                    *name,
                                    resolved_class,
                                    resolved_type_params,
                                    method_info,
                                ));
                            }
                        } else {
                            resolved_method =
                                Some((*name, resolved_class, resolved_type_params, method_info));
                        }
                    } else if class_info.kind == ClassLikeKind::Interface
                        && !class_info.override_method_visibility
                        && first_missing_interface.is_none()
                    {
                        first_missing_interface = Some(*name);
                    } else if !(class_has_magic_call(class_info)
                        && !class_has_sealed_methods(analyzer, class_info))
                    {
                        has_receiver_without_method = true;
                    }

                    if class_has_magic_call(class_info)
                        && !class_has_sealed_methods(analyzer, class_info)
                    {
                        has_valid_receiver = true;
                        has_unsealed_magic_call = true;

                        if let Some(magic_call_info) = class_info.methods.get(&StrId::CALL) {
                            analyze_pending_closure_args_for_method(
                                analyzer,
                                args,
                                arg_positions,
                                magic_call_info,
                                class_info,
                                type_params.as_deref(),
                                *name,
                                *name,
                                class_info.parent_class,
                                analysis_data,
                                context,
                            );

                            let template_result =
                                existing_atomic_method_call_analyzer::build_method_template_context(
                                    analyzer,
                                    class_info,
                                    type_params.as_deref(),
                                    magic_call_info,
                                    is_this_call,
                                    args,
                                    arg_positions,
                                    analysis_data,
                                    context,
                                );

                            let resolved_magic_return = crate::methods::get_method_return_type(
                                analyzer,
                                *name,
                                "__call",
                                magic_call_info,
                                &template_result,
                                &rustc_hash::FxHashMap::default(),
                                args.len(),
                            );

                            let localized_magic_return = localize_special_class_type_union(
                                analyzer.codebase,
                                analyzer.interner,
                                &resolved_magic_return,
                                *name,
                                *name,
                                class_info.parent_class,
                            );

                            magic_call_return_type =
                                Some(if let Some(existing) = magic_call_return_type {
                                    combine_union_types(&existing, &localized_magic_return, false)
                                } else {
                                    localized_magic_return
                                });
                        }
                    }
                } else if let Some(anonymous_methods) =
                    analysis_data.anonymous_class_methods.get(name)
                {
                    // Anonymous classes live in a per-file side table rather
                    // than the codebase (keyed by their synthetic StrId, so no
                    // interner lookup is needed to recognise one); resolve the
                    // method from there. Unknown methods stay exempt from
                    // issues (synthetic receiver).
                    let method_name_id = analyzer
                        .interner
                        .find(method_name)
                        .unwrap_or(pzoom_str::StrId::EMPTY);
                    if let Some(method_info) = anonymous_methods.get(&method_name_id) {
                        has_valid_receiver = true;
                        let method_info = method_info.clone();
                        resolved_method = Some((*name, *name, None, method_info));
                    }
                } else if !matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                    // Unknown receiver class: Psalm reports
                    // UndefinedDocblockClass when the type came from a
                    // docblock and UndefinedClass otherwise. Late-bound
                    // sentinels and anonymous-class synthetics are exempt.
                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(pos.0);
                    let (kind, message) = if expanded_obj_type.from_docblock {
                        (
                            IssueKind::UndefinedDocblockClass,
                            format!(
                                "Docblock-defined class or interface {} does not exist",
                                class_name
                            ),
                        )
                    } else {
                        (
                            IssueKind::UndefinedClass,
                            crate::class_casing::undefined_class_message(analyzer, &class_name),
                        )
                    };
                    analysis_data.add_issue(Issue::new(
                        kind,
                        message,
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
            TAtomic::TObjectIntersection { types } => {
                let mut components: Vec<(
                    pzoom_str::StrId,
                    pzoom_str::StrId,
                    Option<Vec<TUnion>>,
                    pzoom_code_info::FunctionLikeInfo,
                )> = Vec::new();

                for nested in types {
                    let TAtomic::TNamedObject {
                        name, type_params, ..
                    } = nested
                    else {
                        continue;
                    };

                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        continue;
                    };

                    let Some((resolved_class, resolved_type_params, method_info)) =
                        resolve_named_object_instance_method(
                            analyzer,
                            class_info,
                            type_params.as_deref(),
                            method_name,
                            Some(&analysis_data.type_variable_bounds),
                        )
                    else {
                        // For intersections (e.g. A&I), missing a method on one component
                        // does not mean the concrete object cannot provide it.
                        continue;
                    };

                    has_valid_receiver = true;
                    components.push((*name, resolved_class, resolved_type_params, method_info));
                }

                if resolved_method.is_none() && !components.is_empty() {
                    // Choose the primary component as before: prefer one that
                    // supplies generic params (e.g. `IParent<C>` over a bare
                    // `IChild`) or has a more specific return.
                    let mut primary_idx = 0;
                    for idx in 1..components.len() {
                        let supplies_params =
                            components[primary_idx].2.is_none() && components[idx].2.is_some();
                        let more_specific = method_has_more_specific_return(
                            analyzer,
                            &components[idx].3,
                            &components[primary_idx].3,
                        );
                        if supplies_params || more_specific {
                            primary_idx = idx;
                        }
                    }

                    let primary = components.remove(primary_idx);
                    // The remaining components' returns are intersected into the
                    // primary's return later (Psalm getIntersectionReturnType).
                    intersection_secondary_methods.extend(components);
                    resolved_method = Some(primary);
                }
            }
            TAtomic::TObject | TAtomic::TMixed => {
                has_valid_receiver = true;
            }
            // `$closure->__invoke(...)` is the same as calling it directly
            // (Psalm routes it through the closure's signature).
            TAtomic::TClosure {
                params,
                return_type,
                is_pure,
            }
            | TAtomic::TCallable {
                params,
                return_type,
                is_pure,
            } if method_name.eq_ignore_ascii_case("__invoke") => {
                has_valid_receiver = true;
                let synthesized = pzoom_code_info::FunctionLikeInfo {
                    name: StrId::INVOKE,
                    params: params
                        .as_ref()
                        .map(|params| {
                            params
                                .iter()
                                .enumerate()
                                .map(|(index, param)| {
                                    pzoom_code_info::functionlike_info::ParamInfo {
                                        name: param.name.unwrap_or_else(|| {
                                            analyzer
                                                .interner
                                                .find(&format!("$arg{}", index))
                                                .unwrap_or(pzoom_str::StrId::EMPTY)
                                        }),
                                        param_type: Some(param.param_type.clone()),
                                        is_optional: param.is_optional,
                                        is_variadic: param.is_variadic,
                                        by_ref: param.by_ref,
                                        ..Default::default()
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    return_type: return_type.as_deref().cloned(),
                    is_pure: is_pure.unwrap_or(false),
                    ..Default::default()
                };
                if resolved_method.is_none() {
                    resolved_method = Some((StrId::CLOSURE, StrId::CLOSURE, None, synthesized));
                }
            }
            TAtomic::TNull | TAtomic::TVoid => {
                has_null_receiver = true;
            }
            TAtomic::TFalse => {
                has_false_receiver = true;
            }
            // `never` in a union (e.g. `Foo|never`) contributes nothing and is
            // not an invalid receiver.
            TAtomic::TNever => {}
            _ => {
                has_invalid_receiver = true;
            }
        }
    }

    if resolved_method.is_some() && has_receiver_without_method {
        // Psalm points at the method name node.
        let (line, col) = analyzer.get_line_column(method_name_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyUndefinedMethod,
            format!(
                "Method {} may not exist on one or more possible object types",
                method_name
            ),
            analyzer.file_path,
            method_name_pos.0,
            method_name_pos.1,
            line,
            col,
        ));
    }

    if is_this_call
        && let Some(calling_class_id) = calling_class
        && resolved_method
            .as_ref()
            .is_some_and(|(_, resolved_class_id, _, _)| *resolved_class_id != calling_class_id)
    {
        let existing_type_params = resolved_method
            .as_ref()
            .and_then(|(_, _, type_params, _)| type_params.clone());

        if let Some(calling_class_info) = analyzer.codebase.get_class(calling_class_id)
            && let Some((self_resolved_class_id, _, self_method_info)) =
                resolve_named_object_instance_method(
                    analyzer,
                    calling_class_info,
                    None,
                    method_name,
                    Some(&analysis_data.type_variable_bounds),
                )
        {
            // When collecting constructor initialisations `$this` is typed as the
            // concrete class being constructed (a descendant of the method's
            // lexical class), so a `$this->m()` call must late-bind to that
            // descendant's override — exactly what PHP does for public/protected
            // methods. Only a *private* method on the lexical class shadows the
            // call (early binding); otherwise keep the receiver's resolution.
            // Outside collection `$this` is the lexical class and this rebinding is
            // a no-op, so the existing behaviour is preserved.
            if !context.collect_initializations
                || matches!(self_method_info.visibility, Visibility::Private)
            {
                resolved_method = Some((
                    calling_class_id,
                    self_resolved_class_id,
                    existing_type_params,
                    self_method_info,
                ));
            }
        }
    }

    // Build the list of receiver members to analyze, mirroring Hakana's
    // per-atomic loop: the primary, then the other union members (their returns
    // are combined), then the other intersection components (their returns are
    // intersected). Each member gets the full per-class analysis
    // (visibility / arguments / purity / return), and the results are folded
    // together — instead of only the primary being fully analyzed.
    let mut members: Vec<(
        bool,
        (
            StrId,
            StrId,
            Option<Vec<TUnion>>,
            pzoom_code_info::FunctionLikeInfo,
        ),
    )> = Vec::new();
    if let Some(primary) = resolved_method {
        members.push((false, primary));
    }
    for secondary in secondary_methods {
        members.push((false, secondary));
    }
    for secondary in intersection_secondary_methods {
        members.push((true, secondary));
    }

    let had_resolved_member = !members.is_empty();
    let mut accumulated_return: Option<TUnion> = None;

    for (
        member_idx,
        (member_is_intersection, (receiver_class_id, class_id, object_type_params, method_info)),
    ) in members.into_iter().enumerate()
    {
        if let Some(member_return) = existing_atomic_method_call_analyzer::analyze(
            analyzer,
            object_expr,
            obj_type,
            &expanded_obj_type,
            method_name,
            pos,
            method_name_pos,
            args,
            arg_positions,
            enforce_mutation_free,
            suppress_possibly_null_reference_issue,
            is_this_call,
            calling_class,
            any_candidate_accepts_arg_count,
            has_null_receiver,
            has_false_receiver,
            has_invalid_receiver,
            first_missing_interface,
            &receiver_atomics,
            receiver_class_id,
            class_id,
            object_type_params,
            method_info,
            member_idx == 0,
            analysis_data,
            context,
        ) {
            accumulated_return = Some(accumulate_member_return(
                accumulated_return,
                member_return,
                member_is_intersection,
                analyzer.codebase,
            ));
        }
    }

    if had_resolved_member {
        return accumulated_return;
    }

    if has_unsealed_magic_call {
        return Some(magic_call_return_type.unwrap_or_else(TUnion::mixed));
    }

    if !has_valid_receiver && !has_unsealed_magic_call {
        // A preceding `method_exists($obj, 'foo')` guard proves the method exists at
        // runtime even though it is absent from the declared class, so treat the call as
        // returning `mixed` rather than reporting UndefinedMethod (matching Psalm).
        if is_method_guarded_by_method_exists(context, object_expr, method_name) {
            return Some(TUnion::mixed());
        }

        let mut saw_named_object = false;
        let mut saw_non_interface = false;
        let mut first_interface: Option<StrId> = None;

        for atomic in &expanded_obj_type.types {
            match atomic {
                TAtomic::TNamedObject { name, .. } => {
                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        saw_non_interface = true;
                        continue;
                    };

                    saw_named_object = true;
                    if class_info.kind != ClassLikeKind::Interface {
                        saw_non_interface = true;
                        continue;
                    }

                    if first_interface.is_none() {
                        first_interface = Some(*name);
                    }
                }
                TAtomic::TObjectIntersection { types } => {
                    for nested in types {
                        let TAtomic::TNamedObject { name, .. } = nested else {
                            continue;
                        };

                        let Some(class_info) = analyzer.codebase.get_class(*name) else {
                            saw_non_interface = true;
                            continue;
                        };

                        saw_named_object = true;
                        if class_info.kind != ClassLikeKind::Interface {
                            saw_non_interface = true;
                            continue;
                        }

                        if first_interface.is_none() {
                            first_interface = Some(*name);
                        }
                    }
                }
                _ => {}
            }
        }

        if saw_named_object && !saw_non_interface && !context.inside_conditional {
            let interface_name = analyzer
                .interner
                .lookup(first_interface.unwrap_or(StrId::EMPTY));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedInterfaceMethod,
                crate::class_casing::undefined_method_message(
                    analyzer,
                    &interface_name,
                    method_name,
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            return None;
        }
    }

    for atomic in &expanded_obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if is_datetime_interface_add(*name, method_name) {
                        return Some(TUnion::new(TAtomic::TNamedObject {
                            name: *name,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        }));
                    }

                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(pos.0);

                    if class_has_magic_call(class_info) {
                        if class_has_sealed_methods(analyzer, class_info) {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedMagicMethod,
                                format!(
                                    "Magic method {}::{} does not exist",
                                    class_name, method_name
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        } else {
                            return Some(TUnion::mixed());
                        }
                    } else {
                        if let Some(visibility_scope) =
                            find_private_method_visibility_scope(analyzer, *name, method_name)
                        {
                            let issue_kind = if should_report_private_method_as_undefined(
                                analyzer,
                                calling_class,
                                visibility_scope,
                            ) {
                                IssueKind::UndefinedMethod
                            } else {
                                IssueKind::InaccessibleMethod
                            };

                            let message = if issue_kind == IssueKind::UndefinedMethod {
                                crate::class_casing::undefined_method_message(
                                    analyzer,
                                    &class_name,
                                    method_name,
                                )
                            } else {
                                format!(
                                    "Cannot access private method {}::{}",
                                    analyzer.interner.lookup(visibility_scope),
                                    method_name
                                )
                            };

                            analysis_data.add_issue(Issue::new(
                                issue_kind,
                                message,
                                analyzer.file_path,
                                method_name_pos.0,
                                method_name_pos.1,
                                line,
                                col,
                            ));
                        } else {
                            // A template-bounded receiver (`T as C`) may be a
                            // subclass that does declare the method: Psalm
                            // reports PossiblyUndefinedMethod for non-final
                            // bounds.
                            let receiver_is_template_bound = !class_info.is_final
                                && obj_type.types.iter().any(|original| {
                                    matches!(original, TAtomic::TTemplateParam { .. })
                                });
                            analysis_data.add_issue(Issue::new(
                                if receiver_is_template_bound {
                                    IssueKind::PossiblyUndefinedMethod
                                } else {
                                    IssueKind::UndefinedMethod
                                },
                                crate::class_casing::undefined_method_message(
                                    analyzer,
                                    &class_name,
                                    method_name,
                                ),
                                analyzer.file_path,
                                method_name_pos.0,
                                method_name_pos.1,
                                line,
                                col,
                            ));
                        }
                    }
                }
            }
            TAtomic::TObjectIntersection { .. } => {}
            TAtomic::TObject => {
                // Generic object - can't look up method, just return mixed
            }
            // Psalm models `non-empty-mixed` as a TMixed subclass, so a method
            // call on it is a MixedMethodCall ("cannot determine the type") just
            // like plain `mixed` — not an InvalidMethodCall. This is the type a
            // truthy-narrowed mixed receiver carries, e.g. `$x && $x->foo()`.
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                if matches!(object_expr.unparenthesized(), Expression::ArrayAccess(_)) {
                    continue;
                }
                if !analyzer.config.is_issue_suppressed("MixedMethodCall") {
                    // Psalm's location is the method NAME node.
                    let (name_start, name_end) = method_name_pos;
                    let (line, col) = analyzer.get_line_column(name_start);
                    // Psalm names the variable when the receiver is one,
                    // with the mixed value's dataflow origin as a secondary
                    // location.
                    let mut origin_secondary = None;
                    let message = if let Expression::Variable(
                        mago_syntax::ast::ast::variable::Variable::Direct(direct),
                    ) = object_expr.unparenthesized()
                    {
                        origin_secondary = analysis_data
                            .expr_types
                            .get(&(
                                object_expr.span().start.offset,
                                object_expr.span().end.offset,
                            ))
                            .cloned()
                            .and_then(|receiver_type| {
                                crate::data_flow::mixed_origin_secondary(
                                    analyzer,
                                    analysis_data,
                                    &receiver_type,
                                    pos.0,
                                )
                            });
                        format!(
                            "Cannot determine the type of {} when calling method {}",
                            direct.name, method_name
                        )
                    } else {
                        format!("Cannot call method {} on mixed type", method_name)
                    };
                    analysis_data.add_issue(
                        Issue::new(
                            IssueKind::MixedMethodCall,
                            message,
                            analyzer.file_path,
                            name_start,
                            name_end,
                            line,
                            col,
                        )
                        .with_secondary_opt(origin_secondary),
                    );
                }
            }
            TAtomic::TNull | TAtomic::TVoid => {
                let (line, col) = analyzer.get_line_column(pos.0);
                // Psalm: a pure-null receiver is NullReference; null alongside
                // other possibilities is only possibly null.
                let (kind, message) = if expanded_obj_type.types.len() > 1 {
                    (
                        IssueKind::PossiblyNullReference,
                        format!("Cannot call method {} on possibly null value", method_name),
                    )
                } else {
                    (
                        IssueKind::NullReference,
                        format!("Cannot call method {} on null", method_name),
                    )
                };
                analysis_data.add_issue(Issue::new(
                    kind,
                    message,
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            // `never` receiver: unreachable, no diagnostic (see early return).
            TAtomic::TNever => {}
            _ => {
                let type_desc = atomic.get_id(Some(analyzer.interner));
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidMethodCall,
                    format!("Cannot call method {} on {}", method_name, type_desc),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    None
}

pub(crate) fn resolve_named_object_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
    type_variable_bounds: Option<
        &rustc_hash::FxHashMap<String, pzoom_code_info::TypeVariableBounds>,
    >,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if let Some(method_info) = get_method_info(analyzer, class_info, method_name) {
        let visibility_scope_class_id =
            get_method_visibility_scope_class_id(class_info, method_info);

        if method_info.visibility != Visibility::Private
            || visibility_scope_class_id == class_info.name
            // A parent's private method is callable on a subclass-typed
            // receiver when the calling context IS the declaring class
            // (Psalm resolves the method and checks visibility against
            // $context->self, not the receiver class).
            || analyzer.get_declaring_class() == Some(visibility_scope_class_id)
        {
            let mut method_info = method_info.clone();
            // Psalm's Methods::getMethodReturnType consults the receiver
            // class's pseudo methods FIRST: an @method annotation overrides
            // an inherited real method's return type.
            if let Some(pseudo_info) = get_pseudo_method_info(analyzer, class_info, method_name)
                && pseudo_info.return_type.is_some()
            {
                method_info.return_type = pseudo_info.return_type.clone();
                method_info.declaring_class = pseudo_info.declaring_class;
            }
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info,
            ));
        }
    }

    if class_info.kind == ClassLikeKind::Interface || class_has_magic_call(class_info) {
        if let Some(method_info) = get_pseudo_method_info(analyzer, class_info, method_name) {
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info.clone(),
            ));
        }
    }

    resolve_named_mixin_instance_method(
        analyzer,
        class_info,
        object_type_params,
        method_name,
        type_variable_bounds,
    )
}

fn resolve_named_mixin_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
    type_variable_bounds: Option<
        &rustc_hash::FxHashMap<String, pzoom_code_info::TypeVariableBounds>,
    >,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if class_info.named_mixins.is_empty() {
        return None;
    }

    let mut class_template_result = function_call_analyzer::get_class_template_defaults(class_info);
    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut class_template_result,
        class_info,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut class_template_result,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    for mixin_atomic in &class_info.named_mixins {
        let mut localized_mixin = function_call_analyzer::replace_templates_in_union(
            &TUnion::new(mixin_atomic.clone()),
            &class_template_result,
        );

        // A `@mixin T` localized through a type-variable receiver param needs
        // the variable's accumulated lower bounds to name a class.
        if let Some(type_variable_bounds) = type_variable_bounds {
            localized_mixin = crate::template::resolve_type_variables_in_union(
                &localized_mixin,
                type_variable_bounds,
            );
        }

        for localized_atomic in localized_mixin.types {
            let TAtomic::TNamedObject {
                name: mixin_class_id,
                type_params: mixin_type_params,
                ..
            } = localized_atomic
            else {
                continue;
            };

            let Some(mixin_class_info) = analyzer.codebase.get_class(mixin_class_id) else {
                continue;
            };

            if let Some(method_info) = get_method_info(analyzer, mixin_class_info, method_name) {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }

            if let Some(method_info) =
                get_pseudo_method_info(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }
        }
    }

    None
}

pub(crate) fn get_literal_string_argument(
    analysis_data: &FunctionAnalysisData,
    arg_pos: Option<&Pos>,
) -> Option<String> {
    let arg_pos = *arg_pos?;
    let arg_type = analysis_data.expr_types.get(&arg_pos).cloned()?;
    let atomic = arg_type.get_single()?;

    if let TAtomic::TLiteralString { value } = atomic {
        return Some(value.clone());
    }

    None
}

pub(crate) fn find_concrete_receiver_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    object_type: &TUnion,
) -> Option<StrId> {
    for atomic in &object_type.types {
        if let Some(class_id) = find_concrete_receiver_class_id_in_atomic(analyzer, atomic) {
            return Some(class_id);
        }
    }

    None
}

fn find_concrete_receiver_class_id_in_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .and_then(|class_info| (class_info.kind == ClassLikeKind::Class).then_some(*name)),
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                if let Some(class_id) = find_concrete_receiver_class_id_in_atomic(analyzer, nested)
                {
                    return Some(class_id);
                }
            }
            None
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            find_concrete_receiver_class_id(analyzer, as_type)
        }
        _ => None,
    }
}

pub(crate) fn union_contains_static_reference(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_static_reference)
}

/// Replace the self-localized class in a return type with the receiver's
/// full intersection (Psalm keeps `A&B` through an `@return self` member).
pub(crate) fn intersect_self_return_with_receiver(
    localized_return_type: &TUnion,
    receiver_type: &TUnion,
    self_class_id: StrId,
) -> TUnion {
    let receiver_named_types: Vec<TAtomic> = collect_receiver_named_types(receiver_type)
        .into_iter()
        .filter(|atomic| {
            !matches!(atomic, TAtomic::TNamedObject { name, .. } if *name == StrId::STATIC || *name == StrId::SELF)
        })
        .collect();
    if receiver_named_types.is_empty() {
        return localized_return_type.clone();
    }

    let mut changed = false;
    let mut merged = Vec::with_capacity(localized_return_type.types.len());
    for atomic in &localized_return_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } if *name == self_class_id => {
                let mut intersection_types = vec![atomic.clone()];
                for receiver_named in &receiver_named_types {
                    if !intersection_types.contains(receiver_named) {
                        intersection_types.push(receiver_named.clone());
                    }
                }
                if intersection_types.len() > 1 {
                    merged.push(TAtomic::TObjectIntersection {
                        types: intersection_types,
                    });
                    changed = true;
                } else {
                    merged.push(atomic.clone());
                }
            }
            _ => merged.push(atomic.clone()),
        }
    }

    if changed {
        TUnion::from_types(merged)
    } else {
        localized_return_type.clone()
    }
}

fn atomic_contains_static_reference(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
            if *name == StrId::STATIC {
                return true;
            }

            type_params.as_ref().is_some_and(|type_params| {
                type_params
                    .iter()
                    .any(|type_param| union_contains_static_reference(type_param))
            })
        }
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_contains_static_reference)
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_static_reference(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_contains_static_reference(as_type),
        _ => false,
    }
}

pub(crate) fn collect_receiver_named_types(receiver_type: &TUnion) -> Vec<TAtomic> {
    let mut named_types = Vec::new();
    for atomic in &receiver_type.types {
        collect_receiver_named_types_in_atomic(atomic, &mut named_types);
    }
    named_types
}

fn collect_receiver_named_types_in_atomic(atomic: &TAtomic, target: &mut Vec<TAtomic>) {
    match atomic {
        TAtomic::TNamedObject { .. } => {
            if !target.contains(atomic) {
                target.push(atomic.clone());
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                collect_receiver_named_types_in_atomic(nested, target);
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested in &as_type.types {
                collect_receiver_named_types_in_atomic(nested, target);
            }
        }
        _ => {}
    }
}

pub(crate) fn verify_method_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_pos: Pos,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
    // Psalm's AtomicMethodCallAnalysisResult::too_many_arguments: when some
    // other union candidate accepts the provided argument count, the call is
    // not reported as TooManyArguments.
    union_candidate_accepts_arg_count: bool,
    // A templated receiver (`T as Id`): `static` params resolve to the
    // template itself (Psalm's $static_class_type carries the lhs type).
    receiver_template_atomic: Option<&TAtomic>,
) {
    let callable_name = format!("{}::{}", class_name, method_name);
    let arg_param_indices = arguments_analyzer::check_arguments_match(
        analyzer,
        args,
        arg_positions,
        method_info,
        &callable_name,
        analysis_data,
        context,
        Some(template_result),
        call_pos,
        false,
        false,
    );

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = method_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let issue_pos = arg_positions.first().copied().unwrap_or(call_pos);
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to method {}, {} expected, {} provided",
                callable_name,
                required_params,
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = method_info.params.last().is_some_and(|p| p.is_variadic);
    if !has_spread
        && !accepts_unbounded
        && !union_candidate_accepts_arg_count
        && args.len() > method_info.params.len()
    {
        let issue_pos = arg_positions
            .get(method_info.params.len())
            .copied()
            .or_else(|| arg_positions.first().copied())
            .unwrap_or((0, 0));
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to method {}, {} expected, {} provided",
                callable_name,
                method_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        if arg.is_unpacked() {
            if let Some(arg_type) =
                arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
            {
                argument_analyzer::verify_unpacked_argument(
                    analyzer,
                    arg_pos,
                    &arg_type,
                    &callable_name,
                    method_info.no_named_arguments,
                    analysis_data,
                );
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| method_info.params.get(mapped_index))
            .or_else(|| method_info.params.last().filter(|p| p.is_variadic));

        if let (Some(param), Some(arg_type)) = (
            param,
            arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos),
        ) {
            let mut effective_param = param.clone();
            if let Some(inherited_param_type) =
                get_inherited_method_param_type(analyzer, self_class_id, method_name, idx)
            {
                let can_auto_inherit_docblock = inherited_param_type.from_docblock
                    && !method_has_docblock_param_types(method_info)
                    && !method_has_docblock_return_type(method_info);
                let can_inherit_interface_contract =
                    inherited_param_type.source_is_interface && !param.has_docblock_type;

                // Only fall back to the inherited type when the override declares
                // no type of its own (neither native nor docblock); an explicit
                // native param type on the override takes precedence (e.g. a child
                // widening `string` to `?string`). The docblock-propagation cases
                // below still apply when the override lacks a docblock refinement.
                let should_use_inherited = effective_param.get_type().is_none()
                    || (method_info.inherits_docblock && !param.has_docblock_type)
                    || (can_auto_inherit_docblock && !param.has_docblock_type)
                    || can_inherit_interface_contract;

                if should_use_inherited {
                    effective_param.param_type = Some(inherited_param_type.param_type);
                }
            }

            if let Some(param_type) = effective_param.get_type() {
                let replaced_param_type =
                    if crate::template::template_result_is_empty(template_result) {
                        param_type.clone()
                    } else {
                        function_call_analyzer::replace_templates_in_union(
                            param_type,
                            template_result,
                        )
                    };

                effective_param.param_type = Some(localize_special_class_type_union(
                    analyzer.codebase,
                    analyzer.interner,
                    &replaced_param_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ));
            }

            // A `static` param on a templated receiver is the receiver's own
            // template type.
            if let Some(receiver_atomic) = receiver_template_atomic
                && let Some(param_type) = &effective_param.param_type
                && param_type.types.iter().any(|atomic| {
                    matches!(
                        atomic,
                        TAtomic::TNamedObject {
                            is_static: true,
                            ..
                        }
                    )
                })
            {
                let replaced: Vec<TAtomic> = param_type
                    .types
                    .iter()
                    .map(|atomic| match atomic {
                        TAtomic::TNamedObject {
                            is_static: true, ..
                        } => receiver_atomic.clone(),
                        other => other.clone(),
                    })
                    .collect();
                effective_param.param_type = Some(TUnion::from_types(replaced));
            }

            argument_analyzer::verify_type(
                analyzer,
                arg,
                arg_pos,
                &arg_type,
                &effective_param,
                param_index.unwrap_or(idx),
                &callable_name,
                analysis_data,
                context,
                Some(arguments_analyzer::call_dataflow_for_method_call(
                    static_class_id,
                    method_info,
                    call_pos,
                )),
            );
        }
    }
}

pub(crate) fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    object_expr: &Expression<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    if method_info.assertions.is_empty() {
        return;
    }

    for assertion in &method_info.assertions {
        let resolved_assertion_type = crate::assertion_finder::get_untemplated_copy(
            analyzer.codebase,
            analyzer.interner,
            &assertion.assertion_type,
            template_result,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if assertion_name.as_ref() == "$this" {
            apply_assertion_to_expression(
                analyzer,
                analysis_data,
                object_expr,
                &resolved_assertion_type,
                context,
            );
            continue;
        }

        // Property-path assertion on the receiver, e.g. `@psalm-assert !null
        // $this->other`. Rebase the `$this` prefix onto the actual receiver
        // expression and narrow that property in scope, seeding the declared
        // property type when it isn't a local yet. Mirrors Psalm applying
        // `$this->prop` assertions via the reconciler.
        if let Some(prop_suffix) = assertion_name.strip_prefix("$this->") {
            if let Some(receiver_key) = expression_identifier::get_expression_var_key(object_expr) {
                let full_key = format!("{}->{}", receiver_key, prop_suffix);
                let var_id = VarName::new(&full_key);
                let existing_type = context
                    .locals
                    .get(&var_id)
                    .map(|__t| (**__t).clone())
                    .or_else(|| crate::reconciler::resolve_key_type(&full_key, context, analyzer))
                    .unwrap_or_else(TUnion::mixed);
                let narrowed_type =
                    apply_functionlike_assertion_to_union(&existing_type, &resolved_assertion_type);
                context.locals.insert(var_id, narrowed_type);
            }
            continue;
        }

        let Some(param_idx) =
            find_assertion_param_index(analyzer, &method_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(argument) = args.get(param_idx) else {
            continue;
        };

        apply_assertion_to_expression(
            analyzer,
            analysis_data,
            argument.value(),
            &resolved_assertion_type,
            context,
        );
    }
}

/// Post-call `@psalm-assert` application for static calls: only
/// param-indexed assertion targets apply (there is no receiver for
/// `\$this`-rooted ones). Psalm's CallAnalyzer::applyAssertionsToContext runs
/// for static calls the same as instance calls.
pub(crate) fn apply_post_static_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    if method_info.assertions.is_empty() {
        return;
    }

    for assertion in &method_info.assertions {
        let resolved_assertion_type = crate::assertion_finder::get_untemplated_copy(
            analyzer.codebase,
            analyzer.interner,
            &assertion.assertion_type,
            template_result,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        // A static-property target (`self::$q`, `A::$q`) narrows the scope
        // entry of the same spelling directly.
        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if assertion_name.contains("::$") {
            apply_assertion_to_scope_key(
                analyzer,
                &assertion_name,
                &resolved_assertion_type,
                context,
            );
            continue;
        }

        let Some(param_idx) =
            find_assertion_param_index(analyzer, &method_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(argument) = args.get(param_idx) else {
            continue;
        };

        apply_assertion_to_expression(
            analyzer,
            analysis_data,
            argument.value(),
            &resolved_assertion_type,
            context,
        );
    }
}

/// Apply an `IsType`-style assertion to a known scope key (a static property
/// spelling like `self::$q`): the asserted type replaces/narrows the entry.
fn apply_assertion_to_scope_key(
    analyzer: &StatementsAnalyzer<'_>,
    scope_key: &str,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
) {
    let var_id = pzoom_code_info::VarName::from(scope_key.to_string());
    match assertion_type {
        AssertionType::IsType(asserted) => {
            let narrowed = match context.locals.get(&var_id) {
                Some(existing) => {
                    crate::reconciler::assertion_reconciler::intersect_union_with_union_with_codebase(
                        existing,
                        asserted,
                        Some(analyzer.codebase),
                    )
                    .unwrap_or_else(|| asserted.clone())
                }
                None => asserted.clone(),
            };
            context.locals.insert(var_id, narrowed);
        }
        _ => {}
    }
}

fn apply_assertion_to_expression(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    expr: &Expression<'_>,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
) {
    let Some(var_key) = expression_identifier::get_expression_var_key(expr) else {
        // Psalm's CallAnalyzer no-var-id fallback: a single Truthy / Falsy /
        // IsType(true) assertion on an argument expression (e.g.
        // `assertTrue(isset($x->y[0]['k']))`) applies the expression's OWN
        // truths through the formula (FormulaGenerator::getFormula on the
        // arg value).
        let truthy = matches!(assertion_type, AssertionType::Truthy)
            || matches!(
                assertion_type,
                AssertionType::IsType(union)
                    if matches!(union.get_single(), Some(TAtomic::TTrue))
            );
        let falsy = matches!(assertion_type, AssertionType::Falsy);
        if truthy || falsy {
            let assertions = crate::assertion_finder::get_assertions(analyzer, expr, analysis_data);
            let truths = if truthy {
                &assertions.if_true
            } else {
                &assertions.if_false
            };
            if !truths.is_empty() {
                let mut changed = rustc_hash::FxHashSet::default();
                let inside_loop = context.inside_loop;
                crate::reconciler::reconcile_keyed_types(
                    truths,
                    context,
                    &mut changed,
                    analyzer,
                    analysis_data,
                    inside_loop,
                    false,
                    crate::reconciler::EmissionMode::Silent,
                    None,
                );
            }
        }
        return;
    };

    let var_id = VarName::new(&var_key);
    let existing_type = context
        .locals
        .get(&var_id)
        .map(|__t| (**__t).clone())
        .unwrap_or_else(TUnion::mixed);
    let narrowed_type = apply_functionlike_assertion_to_union(&existing_type, assertion_type);
    context.locals.insert(var_id, narrowed_type);
}

pub(crate) fn find_assertion_param_index(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertion_var_id: pzoom_str::StrId,
) -> Option<usize> {
    let assertion_name = analyzer.interner.lookup(assertion_var_id);
    let normalized_assertion = assertion_name
        .strip_prefix('$')
        .unwrap_or(assertion_name.as_ref());

    params.iter().position(|param| {
        if param.name == assertion_var_id {
            return true;
        }

        let param_name = analyzer.interner.lookup(param.name);
        let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name.as_ref());
        normalized_param == normalized_assertion
    })
}

fn apply_functionlike_assertion_to_union(
    existing_type: &TUnion,
    assertion_type: &AssertionType,
) -> TUnion {
    match assertion_type {
        AssertionType::IsType(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsEqual(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsLooselyEqual(_) => existing_type.clone(),
        AssertionType::IsNotType(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotEqual(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotLooselyEqual(_) => existing_type.clone(),
        // Psalm's '@psalm-assert truthy/!empty' narrows through the truthy
        // reconciler (removes null/false/empty values).
        AssertionType::Truthy | AssertionType::NotEmpty => {
            crate::expr::call::function_call_assertion_analyzer::narrow_union_to_truthy(
                existing_type,
            )
        }
        AssertionType::Falsy => existing_type.clone(),
        AssertionType::NotNull => subtract_union(existing_type, &TUnion::new(TAtomic::TNull)),
    }
}

fn subtract_union(existing_type: &TUnion, type_to_remove: &TUnion) -> TUnion {
    let filtered_types: Vec<_> = existing_type
        .types
        .iter()
        .filter(|atomic| !type_to_remove.types.contains(atomic))
        .cloned()
        .collect();

    if filtered_types.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered_types)
    }
}

/// Record a resolved method call for find_unused_code (Psalm's
/// addMethodReferenceToClassMember + isMethodReturnReferenced recording).
/// Self-recursion does not mark a method referenced.
pub(crate) fn record_method_reference(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    declaring_class: Option<StrId>,
    method_name: &str,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let method_lc = analyzer
        .interner
        .find(&method_name.to_lowercase())
        .unwrap_or(pzoom_str::StrId::EMPTY);
    let is_recursion = analyzer.function_info.is_some_and(|caller| {
        analyzer
            .interner
            .lookup(caller.name)
            .eq_ignore_ascii_case(method_name)
            && (caller.declaring_class == Some(class_id)
                || caller.declaring_class == declaring_class)
            && caller.declaring_class.is_some()
    });
    if !is_recursion {
        analysis_data
            .referenced_class_members
            .insert((class_id, method_lc));
        analysis_data.add_class_member_reference(
            &context.function_context,
            (class_id, method_lc),
            false,
        );
        if let Some(declaring_class) = declaring_class {
            analysis_data
                .referenced_class_members
                .insert((declaring_class, method_lc));
            analysis_data.add_class_member_reference(
                &context.function_context,
                (declaring_class, method_lc),
                false,
            );
        }
        // Psalm also records the overridden parent/interface methods as
        // referenced — calling an implementation uses its declaration.
        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            for (overridden_name, parents) in &class_info.overridden_method_ids {
                if analyzer
                    .interner
                    .lookup(*overridden_name)
                    .eq_ignore_ascii_case(method_name)
                {
                    for parent_id in parents {
                        analysis_data
                            .referenced_class_members
                            .insert((*parent_id, method_lc));
                        analysis_data.add_overridden_member_reference(
                            &context.function_context,
                            (*parent_id, method_lc),
                        );
                    }
                }
            }
        }
    }
    if context.inside_use() {
        analysis_data
            .method_returns_used
            .insert((class_id, method_lc));
        if let Some(declaring_class) = declaring_class {
            analysis_data
                .method_returns_used
                .insert((declaring_class, method_lc));
        }
        // Using an implementation's return value also uses the return of the
        // parent/interface method it overrides (mirrors the referenced-member
        // propagation above). Without this, calling a concrete override marks
        // only the concrete method's return used, and the interface/abstract
        // declaration is wrongly reported as PossiblyUnusedReturnValue.
        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            for (overridden_name, parents) in &class_info.overridden_method_ids {
                if analyzer
                    .interner
                    .lookup(*overridden_name)
                    .eq_ignore_ascii_case(method_name)
                {
                    for parent_id in parents {
                        analysis_data
                            .method_returns_used
                            .insert((*parent_id, method_lc));
                    }
                }
            }
        }
    }
}
