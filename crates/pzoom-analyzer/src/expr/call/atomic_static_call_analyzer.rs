//! Atomic static-call analyzer.
//!
//! Resolves and checks a static method call against a single (named or
//! dynamically-typed) class type: method resolution up the hierarchy, magic
//! `__callStatic`, visibility, template context, argument verification, and
//! return-type inference. Mirrors Psalm's `AtomicStaticCallAnalyzer` and
//! Hakana's `atomic_static_call_analyzer`.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::ast::ast::call::StaticMethodCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{
    Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;


use super::static_call_analyzer::*;
use super::existing_atomic_static_call_analyzer::*;

/// Static calls follow Hakana's `method_call_return_type_fetcher::add_dataflow`
/// with no receiver expression: the call's return node becomes the returned
/// union's only parent. (Hakana models no class-expression flow for static
/// calls; argument flow goes through the `FunctionLikeArg` nodes attached in
/// `argument_analyzer`.)
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_static_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    class_id: StrId,
    method_name: &str,
    method_info: Option<&pzoom_code_info::FunctionLikeInfo>,
    arg_positions: &[Pos],
    pos: Pos,
    return_type: TUnion,
) -> TUnion {
    super::method_call_return_type_fetcher::add_method_call_dataflow(
        analyzer,
        return_type,
        None,
        class_id,
        analyzer.interner.intern(method_name),
        method_info,
        arg_positions,
        analysis_data,
        pos,
    )
}

pub(crate) fn get_called_class_type_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    resolved_class_id: StrId,
) -> StrId {
    // For `self::`, `static::`, and `parent::` calls, late static binding keeps the
    // `static` type bound to the *current* class context, so a `static` return type
    // resolves to the current class (not the defining/parent class).
    let current_static = analyzer.get_declaring_class().unwrap_or(StrId::STATIC);
    match expr.unparenthesized() {
        Expression::Self_(_) => current_static,
        Expression::Static(_) => current_static,
        Expression::Parent(_) => current_static,
        Expression::Identifier(id) => {
            let value = id.value();
            if value.eq_ignore_ascii_case("self")
                || value.eq_ignore_ascii_case("static")
                || value.eq_ignore_ascii_case("parent")
            {
                current_static
            } else {
                let span = id.span();
                let source_value = analyzer
                    .get_source_substring(span.start.offset as usize, span.end.offset as usize)
                    .trim();
                if source_value.eq_ignore_ascii_case("self")
                    || source_value.eq_ignore_ascii_case("static")
                    || source_value.eq_ignore_ascii_case("parent")
                {
                    current_static
                } else {
                    resolved_class_id
                }
            }
        }
        _ => resolved_class_id,
    }
}

pub(crate) fn handle_dynamic_static_call(
    analyzer: &StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    class_expr_type: &TUnion,
    method_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut emitted_invalid_string_class = false;
    let mut emitted_invalid_static_invocation = false;
    let mut emitted_undefined_class = false;
    let mut emitted_mixed_method_call = false;
    let mut emitted_undefined_method = false;
    let mut combined_return_type: Option<TUnion> = None;

    let mut flattened_targets = Vec::new();
    for atomic in &class_expr_type.types {
        collect_dynamic_static_call_target_atomics(
            analyzer,
            atomic,
            &mut flattened_targets,
            false,
            None,
        );
    }

    for (atomic, static_binding) in &flattened_targets {
        match atomic {
            TAtomic::TString | TAtomic::TLiteralString { .. } => {
                if emitted_invalid_string_class {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidStringClass,
                    "String cannot be used as a class",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_invalid_string_class = true;
            }
            TAtomic::TNamedObject { name, .. } => {
                let Some(class_info) = analyzer.codebase.get_class(*name) else {
                    if emitted_undefined_class {
                        continue;
                    }

                    if is_parse_artifact_class_name(analyzer.interner.lookup(*name).as_ref()) {
                        continue;
                    }

                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedClass,
                        crate::class_casing::undefined_class_message(analyzer, analyzer.interner.lookup(*name)),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_class = true;
                    continue;
                };

                if let Some((resolved_class_id, _, method_info, allow_non_static_via_magic)) =
                    resolve_named_object_static_method(analyzer, class_info, method_name)
                {
                    if analyzer.config.find_unused_code {
                        super::atomic_method_call_analyzer::record_method_reference(
                            analyzer,
                            resolved_class_id,
                            method_info.declaring_class,
                            method_name,
                            context,
                            analysis_data,
                        );
                        if context.self_class != Some(class_info.name) {
                            analysis_data.referenced_classes.insert(class_info.name);
                        }
                    }
                    let resolved_return_type = method_info
                        .return_type
                        .as_ref()
                        .or(method_info.signature_return_type.as_ref())
                        .cloned()
                        .unwrap_or_else(TUnion::mixed);
                    let parent_class_id = analyzer
                        .codebase
                        .get_class(resolved_class_id)
                        .and_then(|resolved_info| resolved_info.parent_class);
                    let localized_return_type = if let Some(static_binding) = static_binding {
                        crate::type_expander::localize_special_class_type_union_with_static_object(
                            analyzer.codebase,
                            analyzer.interner,
                            &resolved_return_type,
                            resolved_class_id,
                            static_binding.clone(),
                            parent_class_id,
                        )
                    } else {
                        localize_special_class_type_union(
                            analyzer.codebase,
                            analyzer.interner,
                            &resolved_return_type,
                            resolved_class_id,
                            resolved_class_id,
                            parent_class_id,
                        )
                    };

                    combined_return_type = Some(if let Some(existing) = combined_return_type {
                        combine_union_types(&existing, &localized_return_type, false)
                    } else {
                        localized_return_type
                    });

                    if !method_info.is_static && !allow_non_static_via_magic {
                        if emitted_invalid_static_invocation {
                            continue;
                        }

                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidStaticInvocation,
                            format!(
                                "Cannot call non-static method {}::{} statically",
                                analyzer.interner.lookup(resolved_class_id),
                                method_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                        emitted_invalid_static_invocation = true;
                    }
                } else if !emitted_undefined_method
                    && !is_method_guarded_by_exists(context, method_name)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedMethod,
                        crate::class_casing::undefined_method_message(
                            analyzer,
                            analyzer.interner.lookup(*name),
                            method_name,
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_method = true;
                }
            }
            TAtomic::TObject => {
                if emitted_invalid_static_invocation {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidStaticInvocation,
                    format!("Cannot call non-static method {} statically", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_invalid_static_invocation = true;
            }
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_found_method = false;

                for intersection_type in types {
                    let TAtomic::TNamedObject { name, .. } = intersection_type else {
                        continue;
                    };

                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        continue;
                    };

                    if let Some((resolved_class_id, _, method_info, allow_non_static_via_magic)) =
                        resolve_named_object_static_method(analyzer, class_info, method_name)
                    {
                        intersection_found_method = true;
                        if analyzer.config.find_unused_code {
                            super::atomic_method_call_analyzer::record_method_reference(
                                analyzer,
                                resolved_class_id,
                                method_info.declaring_class,
                                method_name,
                                context,
                                analysis_data,
                            );
                        }

                        let resolved_return_type = method_info
                            .return_type
                            .as_ref()
                            .or(method_info.signature_return_type.as_ref())
                            .cloned()
                            .unwrap_or_else(TUnion::mixed);
                        let parent_class_id = analyzer
                            .codebase
                            .get_class(resolved_class_id)
                            .and_then(|resolved_info| resolved_info.parent_class);
                        let localized_return_type = if let Some(static_binding) = static_binding {
                            crate::type_expander::localize_special_class_type_union_with_static_object(
                                analyzer.codebase,
                                analyzer.interner,
                                &resolved_return_type,
                                resolved_class_id,
                                static_binding.clone(),
                                parent_class_id,
                            )
                        } else {
                            localize_special_class_type_union(
                                analyzer.codebase,
                                analyzer.interner,
                                &resolved_return_type,
                                resolved_class_id,
                                resolved_class_id,
                                parent_class_id,
                            )
                        };

                        combined_return_type = Some(if let Some(existing) = combined_return_type {
                            combine_union_types(&existing, &localized_return_type, false)
                        } else {
                            localized_return_type
                        });

                        if !method_info.is_static && !allow_non_static_via_magic {
                            if emitted_invalid_static_invocation {
                                continue;
                            }

                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidStaticInvocation,
                                format!(
                                    "Cannot call non-static method {}::{} statically",
                                    analyzer.interner.lookup(resolved_class_id),
                                    method_name
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                            emitted_invalid_static_invocation = true;
                        }
                    }
                }

                if !intersection_found_method
                    && !emitted_undefined_method
                    && !is_method_guarded_by_exists(context, method_name)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedMethod,
                        format!("Method {} does not exist", method_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_method = true;
                }
            }
            TAtomic::TClassString { .. } => {
                // `class-string` can represent a valid runtime class, but may not
                // have enough information to resolve a concrete method target here.
                continue;
            }
            TAtomic::TMixed => {
                if emitted_mixed_method_call {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedMethodCall,
                    "Cannot call method on an unknown class",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_mixed_method_call = true;
            }
            _ => {
                if emitted_undefined_class {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    format!(
                        "Type {} cannot be called as a class",
                        atomic.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_undefined_class = true;
            }
        }
    }

    if static_call.argument_list.arguments.is_empty() && emitted_invalid_string_class {
        // Preserve Psalm behavior where string static calls degrade result to mixed,
        // which can trigger downstream MixedAssignment.
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedAssignment,
            "Unable to determine return type of dynamically-invoked static method",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    combined_return_type
}

/// Flattens a dynamic static-call receiver into named-object targets. Each
/// target carries the late-static binding the receiver implies (Psalm's
/// AtomicStaticCallAnalyzer keeps the template param as `$static_type`, so a
/// `static` return on a `T`-typed or `class-string<T>`-typed receiver
/// resolves to `T`).
fn collect_dynamic_static_call_target_atomics(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    output: &mut Vec<(TAtomic, Option<TAtomic>)>,
    from_class_string_context: bool,
    static_binding: Option<&TAtomic>,
) {
    // Psalm's AtomicStaticCallAnalyzer special-cases `TDependentGetClass`: when
    // the source was mixed (`as_type` = object) it falls through to the plain
    // string branch (InvalidStringClass); when the source has no object type it
    // resolves to the single named object if there is one and is otherwise
    // silent (e.g. `get_class(null)` under a suppression).
    if let TAtomic::TDependentGetClass { as_type, .. } = atomic {
        if as_type.types.iter().any(|a| matches!(a, TAtomic::TObject)) {
            push_unique_dynamic_static_target(output, TAtomic::TString, static_binding);
        } else if let Some(named) = as_type
            .get_single()
            .filter(|a| matches!(a, TAtomic::TNamedObject { .. }))
        {
            push_unique_dynamic_static_target(output, named.clone(), static_binding);
        }
        return;
    }

    // A dependent `gettype($x)` value is a string; resolve the static-call
    // target from its string equivalent.
    if let Some(equiv) = atomic.dependent_string_equivalent() {
        collect_dynamic_static_call_target_atomics(
            analyzer,
            &equiv,
            output,
            from_class_string_context,
            static_binding,
        );
        return;
    }

    match atomic {
        TAtomic::TTemplateParam { as_type, .. } => {
            if from_class_string_context
                && (as_type.is_mixed()
                    || as_type.types.iter().all(|nested| {
                        matches!(
                            nested,
                            TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
                        )
                    }))
            {
                push_unique_dynamic_static_target(
                    output,
                    TAtomic::TClassString { as_type: None },
                    static_binding,
                );
                return;
            }

            let binding = static_binding.or(Some(atomic));
            for nested in &as_type.types {
                collect_dynamic_static_call_target_atomics(
                    analyzer,
                    nested,
                    output,
                    from_class_string_context,
                    binding,
                );
            }
        }
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            if from_class_string_context
                && matches!(
                    as_type.as_ref(),
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
                )
            {
                push_unique_dynamic_static_target(
                    output,
                    TAtomic::TClassString { as_type: None },
                    static_binding,
                );
                return;
            }

            // Instantiating/calling through `class-string<T>` late-binds
            // `static` to `T`.
            let template_binding = match as_type.as_ref() {
                template @ TAtomic::TTemplateParam { .. } => template.clone(),
                bound => TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(pzoom_code_info::TUnion::new(bound.clone())),
                },
            };
            let binding = static_binding.cloned().or(Some(template_binding));
            collect_dynamic_static_call_target_atomics(
                analyzer,
                as_type.as_ref(),
                output,
                from_class_string_context,
                binding.as_ref(),
            );
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            let binding = static_binding.or(match as_type.as_ref() {
                template @ TAtomic::TTemplateParam { .. } => Some(template),
                _ => None,
            });
            collect_dynamic_static_call_target_atomics(
                analyzer,
                as_type.as_ref(),
                output,
                true,
                binding,
            );
        }
        TAtomic::TLiteralClassString { name } => {
            push_unique_dynamic_static_target(
                output,
                TAtomic::TNamedObject {
                    name: analyzer.interner.intern(name.trim_start_matches('\\')),
                    type_params: None,
                is_static: false, remapped_params: false },
                static_binding,
            );
        }
        TAtomic::TObjectIntersection { types } => {
            if from_class_string_context {
                push_unique_dynamic_static_target(output, atomic.clone(), static_binding);
            } else {
                let start_len = output.len();
                for nested in types {
                    collect_dynamic_static_call_target_atomics(
                        analyzer,
                        nested,
                        output,
                        from_class_string_context,
                        static_binding,
                    );
                }

                if output.len() == start_len {
                    push_unique_dynamic_static_target(output, atomic.clone(), static_binding);
                }
            }
        }
        _ => push_unique_dynamic_static_target(output, atomic.clone(), static_binding),
    }
}

fn push_unique_dynamic_static_target(
    output: &mut Vec<(TAtomic, Option<TAtomic>)>,
    atomic: TAtomic,
    static_binding: Option<&TAtomic>,
) {
    if !output.iter().any(|(existing, _)| existing == &atomic) {
        output.push((atomic, static_binding.cloned()));
    }
}

pub(crate) fn infer_closure_from_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let callback_pos = *arg_positions.first()?;
    let callback_type = analysis_data.expr_types.get(&callback_pos).cloned()?;
    let mut closure_types = Vec::new();

    for atomic in &callback_type.types {
        collect_typed_closure_from_callable_atomic(analyzer, atomic, &mut closure_types);
    }

    if closure_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(closure_types))
    }
}

fn collect_typed_closure_from_callable_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    closure_types: &mut Vec<TAtomic>,
) {
    match atomic {
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        }
        | TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => {
            closure_types.push(TAtomic::TClosure {
                params: params.clone(),
                return_type: return_type.clone(),
                is_pure: *is_pure,
            });
        }
        TAtomic::TNamedObject { name, .. } => {
            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                return;
            };
            let Some(invoke_method) = class_info.methods.get(&StrId::INVOKE) else {
                return;
            };

            if invoke_method.visibility != Visibility::Public {
                return;
            }

            closure_types.push(functionlike_to_typed_closure(invoke_method));
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested_atomic in &as_type.types {
                collect_typed_closure_from_callable_atomic(analyzer, nested_atomic, closure_types);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                collect_typed_closure_from_callable_atomic(analyzer, nested_atomic, closure_types);
            }
        }
        _ => {}
    }
}

fn functionlike_to_typed_closure(function_info: &pzoom_code_info::FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| pzoom_code_info::FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TClosure {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure),
    }
}
