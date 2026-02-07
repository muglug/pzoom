//! Method call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::call::{MethodCall, NullSafeMethodCall};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::functionlike_info::AssertionType;
use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{
    can_access_internal, format_caller_context, format_internal_scope_phrase,
};
use crate::issue_suppression;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::attribute_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::{object_type_comparator, union_type_comparator};

use super::{
    argument_analyzer, arguments_analyzer, callable_validation,
    existing_atomic_method_call_analyzer, function_call_analyzer, method_call_return_type_fetcher,
};

/// Analyze a method call expression ($obj->method()).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    method_call: &MethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the object expression
    let obj_pos =
        expression_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    let args: Vec<_> = method_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();
    for arg in &args {
        if is_closure_like_argument(arg) {
            continue;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    // Get the method name
    let method_name = get_method_name(&method_call.method);

    // Try to look up method return type from each atomic type in the union
    if let (Some(obj_t), Some(method_name)) = (obj_type.as_ref(), method_name) {
        let return_type = get_method_return_type(
            analyzer,
            method_call.object,
            &obj_t,
            method_name,
            pos,
            &args,
            &arg_positions,
            enforce_mutation_free,
            false,
            analysis_data,
            context,
        );
        if let Some(return_type) = return_type {
            analysis_data.set_expr_type(pos, return_type);
            return;
        }
    } else if let Some(obj_t) = obj_type.as_ref() {
        if method_name.is_none() {
            emit_invalid_dynamic_method_name_issues(analyzer, &obj_t, pos, analysis_data);
        }
    }

    analyze_closure_args_without_context(analyzer, &args, analysis_data, context);

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Analyze a null-safe method call expression ($obj?->method()).
pub fn analyze_nullsafe(
    analyzer: &StatementsAnalyzer<'_>,
    method_call: &NullSafeMethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the object expression
    let obj_pos =
        expression_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    let args: Vec<_> = method_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();
    for arg in &args {
        if is_closure_like_argument(arg) {
            continue;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    // Get the method name
    let method_name = get_method_name(&method_call.method);

    // Try to look up method return type
    if let (Some(obj_t), Some(method_name)) = (obj_type.as_ref(), method_name) {
        // For null-safe calls, get the return type and add null to it
        if let Some(mut return_type) = get_method_return_type(
            analyzer,
            method_call.object,
            &obj_t,
            method_name,
            pos,
            &args,
            &arg_positions,
            enforce_mutation_free,
            true,
            analysis_data,
            context,
        ) {
            // If the object could be null, the result could be null
            let object_type_for_nullsafe =
                get_reconciled_receiver_type_for_expression(analyzer, context, method_call.object)
                    .unwrap_or_else(|| (**obj_t).clone());
            if object_type_for_nullsafe.is_nullable {
                return_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, return_type);
            return;
        }
    } else if let Some(obj_t) = obj_type.as_ref() {
        if method_name.is_none() {
            emit_invalid_dynamic_method_name_issues(analyzer, &obj_t, pos, analysis_data);
        }
    }

    analyze_closure_args_without_context(analyzer, &args, analysis_data, context);

    // Fall back to mixed|null
    let mut result = TUnion::mixed();
    result.add_type(TAtomic::TNull);
    analysis_data.set_expr_type(pos, result);
}

/// Get the method name from a method selector.
fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn emit_invalid_dynamic_method_name_issues(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted_invalid = false;
    let mut emitted_mixed = false;
    let mut emitted_null = false;

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TMixed => {
                if emitted_mixed || analyzer.config.is_issue_suppressed("MixedMethodCall") {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedMethodCall,
                    "Cannot call method with unknown name on mixed type",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_mixed = true;
            }
            TAtomic::TNull | TAtomic::TVoid => {
                if emitted_null {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullReference,
                    "Cannot call method on null",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_null = true;
            }
            TAtomic::TNamedObject { .. }
            | TAtomic::TObject
            | TAtomic::TObjectIntersection { .. }
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. } => {}
            _ => {
                if emitted_invalid {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidMethodCall,
                    format!(
                        "Cannot call method on {}",
                        atomic.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_invalid = true;
            }
        }
    }
}

fn is_closure_like_argument(arg: &mago_syntax::ast::ast::argument::Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

fn get_closure_like_argument_offset(
    arg: &mago_syntax::ast::ast::argument::Argument<'_>,
) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn analyze_closure_args_without_context(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for arg in args {
        if is_closure_like_argument(arg) {
            argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        }
    }
}

fn get_no_arg_method_call_var_id(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    method: &ClassLikeMemberSelector<'_>,
    arg_count: usize,
) -> Option<StrId> {
    if arg_count != 0 {
        return None;
    }

    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let method_name = match method {
        ClassLikeMemberSelector::Identifier(identifier) => identifier.value,
        _ => return None,
    };

    analyzer
        .interner
        .find(&format!("{}->{}()", object_key, method_name))
}

fn get_cached_no_arg_method_call_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    object_expr: &Expression<'_>,
    method_name: &str,
    arg_count: usize,
) -> Option<TUnion> {
    if arg_count != 0 {
        return None;
    }

    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let var_id = analyzer
        .interner
        .find(&format!("{}->{}()", object_key, method_name))?;
    context.locals.get(&var_id).cloned()
}

fn get_reconciled_receiver_type_for_expression(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    object_expr: &Expression<'_>,
) -> Option<TUnion> {
    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let object_id = analyzer.interner.find(&object_key)?;
    context.locals.get(&object_id).cloned()
}

/// Look up the return type of a method on a type.
fn get_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    obj_type: &TUnion,
    method_name: &str,
    pos: Pos,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    enforce_mutation_free: bool,
    suppress_possibly_null_reference_issue: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    if method_name.eq_ignore_ascii_case("__construct") {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::DirectConstructorCall,
            "Constructors should only be called with new",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let reconciled_receiver_type =
        get_reconciled_receiver_type_for_expression(analyzer, context, object_expr)
            .and_then(|tracked_type| {
                assertion_reconciler::intersect_union_with_union(obj_type, &tracked_type)
            })
            .unwrap_or_else(|| obj_type.clone());
    let expanded_obj_type = expand_template_object_union(&reconciled_receiver_type);

    let mut resolved_method: Option<(
        pzoom_str::StrId,
        pzoom_str::StrId,
        Option<Vec<TUnion>>,
        pzoom_code_info::FunctionLikeInfo,
    )> = None;
    let mut has_unsealed_magic_call = false;
    let mut magic_call_return_type: Option<TUnion> = None;
    let mut has_valid_receiver = false;
    let mut has_null_receiver = false;
    let mut has_false_receiver = false;
    let mut has_invalid_receiver = false;
    let mut has_receiver_without_method = false;
    let mut first_missing_interface: Option<StrId> = None;
    let is_this_call =
        expression_identifier::get_expression_var_key(object_expr).as_deref() == Some("$this");
    let calling_class = analyzer.get_declaring_class();

    for atomic in &expanded_obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, type_params } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if let Some((resolved_class, resolved_type_params, method_info)) =
                        resolve_named_object_instance_method(
                            analyzer,
                            class_info,
                            type_params.as_deref(),
                            method_name,
                        )
                    {
                        has_valid_receiver = true;
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
                        && !class_has_sealed_methods(class_info))
                    {
                        has_receiver_without_method = true;
                    }

                    if class_has_magic_call(class_info) && !class_has_sealed_methods(class_info) {
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

                            let (template_defaults, template_replacements) =
                                existing_atomic_method_call_analyzer::build_method_template_context(
                                    analyzer,
                                    class_info,
                                    type_params.as_deref(),
                                    magic_call_info,
                                    args,
                                    arg_positions,
                                    analysis_data,
                                    context,
                                );

                            let resolved_magic_return = resolve_effective_method_return_type(
                                analyzer,
                                *name,
                                "__call",
                                magic_call_info,
                                &template_defaults,
                                &template_replacements,
                                args.len(),
                            );

                            let localized_magic_return = localize_special_class_type_union(
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
                }
            }
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_resolved: Option<(
                    pzoom_str::StrId,
                    pzoom_str::StrId,
                    Option<Vec<TUnion>>,
                    pzoom_code_info::FunctionLikeInfo,
                )> = None;

                for nested in types {
                    let TAtomic::TNamedObject { name, type_params } = nested else {
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
                        )
                    else {
                        // For intersections (e.g. A&I), missing a method on one component
                        // does not mean the concrete object cannot provide it.
                        continue;
                    };

                    has_valid_receiver = true;
                    if let Some(existing) = &mut intersection_resolved {
                        if method_has_more_specific_return(analyzer, &method_info, &existing.3) {
                            *existing = (*name, resolved_class, resolved_type_params, method_info);
                        }
                    } else {
                        intersection_resolved =
                            Some((*name, resolved_class, resolved_type_params, method_info));
                    }
                }

                if resolved_method.is_none() {
                    resolved_method = intersection_resolved;
                }
            }
            TAtomic::TObject | TAtomic::TMixed => {
                has_valid_receiver = true;
            }
            TAtomic::TNull | TAtomic::TVoid => {
                has_null_receiver = true;
            }
            TAtomic::TFalse => {
                has_false_receiver = true;
            }
            _ => {
                has_invalid_receiver = true;
            }
        }
    }

    if resolved_method.is_some() && has_receiver_without_method {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyUndefinedMethod,
            format!(
                "Method {} may not exist on one or more possible object types",
                method_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
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
                )
        {
            resolved_method = Some((
                calling_class_id,
                self_resolved_class_id,
                existing_type_params,
                self_method_info,
            ));
        }
    }

    if let Some((receiver_class_id, class_id, object_type_params, method_info)) = resolved_method {
        let class_name = analyzer.interner.lookup(class_id);
        let parent_class_id = analyzer
            .codebase
            .get_class(class_id)
            .and_then(|class_info| class_info.parent_class);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if class_info.is_deprecated
                && analyzer
                    .get_declaring_class()
                    .is_none_or(|declaring_class| declaring_class != class_id)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedClass,
                    format!("{} is marked deprecated", class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                let scope_phrase = format_internal_scope_phrase(analyzer, &class_info.internal);
                let caller_phrase = format_caller_context(analyzer, Some(context));
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InternalMethod,
                    format!(
                        "The method {}::{} is internal to {} but called from {}",
                        class_name, method_name, scope_phrase, caller_phrase
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }

        let (template_defaults, template_replacements) =
            if let Some(class_info) = analyzer.codebase.get_class(class_id) {
                analyze_pending_closure_args_for_method(
                    analyzer,
                    args,
                    arg_positions,
                    &method_info,
                    class_info,
                    object_type_params.as_deref(),
                    class_id,
                    receiver_class_id,
                    parent_class_id,
                    analysis_data,
                    context,
                );

                existing_atomic_method_call_analyzer::build_method_template_context(
                    analyzer,
                    class_info,
                    object_type_params.as_deref(),
                    &method_info,
                    args,
                    arg_positions,
                    analysis_data,
                    context,
                )
            } else {
                let template_defaults = function_call_analyzer::get_template_defaults(&method_info);
                let template_replacements =
                    function_call_analyzer::infer_template_replacements_from_args(
                        analyzer,
                        args,
                        arg_positions,
                        &method_info.params,
                        &template_defaults,
                        analysis_data,
                        context,
                    );
                (template_defaults, template_replacements)
            };

        verify_method_arguments(
            analyzer,
            args,
            arg_positions,
            &method_info,
            class_name.as_ref(),
            method_name,
            analysis_data,
            context,
            pos,
            &template_defaults,
            &template_replacements,
            class_id,
            receiver_class_id,
            parent_class_id,
        );

        apply_post_call_assertions(
            analyzer,
            object_expr,
            args,
            &method_info,
            context,
            &template_defaults,
            &template_replacements,
            class_id,
            receiver_class_id,
            parent_class_id,
        );

        existing_atomic_method_call_analyzer::maybe_emit_if_this_is_mismatch(
            analyzer,
            &method_info,
            receiver_class_id,
            object_type_params.as_deref(),
            &template_defaults,
            &template_replacements,
            parent_class_id,
            pos,
            analysis_data,
        );

        attribute_analyzer::analyze_reflection_get_attributes_call(
            analyzer,
            class_id,
            method_name,
            args,
            arg_positions,
            analysis_data,
        );

        if let Some(resolved_class_info) = analyzer.codebase.get_class(class_id) {
            let visibility_scope_class_id =
                get_method_visibility_scope_class_id(resolved_class_info, &method_info);

            match method_info.visibility {
                Visibility::Public => {}
                Visibility::Private => {
                    let is_same_class = calling_class
                        .is_some_and(|caller_class| caller_class == visibility_scope_class_id);

                    if !is_same_class
                        && !receiver_allows_method_visibility_override(
                            analyzer,
                            &expanded_obj_type,
                            visibility_scope_class_id,
                        )
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        let issue_kind = if should_report_private_method_as_undefined(
                            analyzer,
                            calling_class,
                            visibility_scope_class_id,
                        ) {
                            IssueKind::UndefinedMethod
                        } else {
                            IssueKind::InaccessibleMethod
                        };
                        let message = if issue_kind == IssueKind::UndefinedMethod {
                            format!("Method {}::{} does not exist", class_name, method_name)
                        } else {
                            format!(
                                "Cannot access private method {}::{}",
                                analyzer.interner.lookup(visibility_scope_class_id),
                                method_name
                            )
                        };
                        analysis_data.add_issue(Issue::new(
                            issue_kind,
                            message,
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
                Visibility::Protected => {
                    let can_access = calling_class.is_some_and(|caller_class| {
                        can_access_protected_member_visibility(
                            analyzer,
                            caller_class,
                            visibility_scope_class_id,
                        )
                    });

                    if !can_access
                        && !receiver_allows_method_visibility_override(
                            analyzer,
                            &expanded_obj_type,
                            visibility_scope_class_id,
                        )
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InaccessibleMethod,
                            format!(
                                "Cannot access protected method {}::{}",
                                analyzer.interner.lookup(visibility_scope_class_id),
                                method_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
            }
        }

        if method_info.is_deprecated {
            let message = method_info
                .deprecation_message
                .as_ref()
                .map(|m| {
                    format!(
                        "Method {}::{} is deprecated: {}",
                        class_name, method_name, m
                    )
                })
                .unwrap_or_else(|| format!("Method {}::{} is deprecated", class_name, method_name));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::DeprecatedMethod,
                message,
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if !can_access_internal(analyzer, &method_info.internal, Some(context)) {
            let scope_phrase = format_internal_scope_phrase(analyzer, &method_info.internal);
            let caller_phrase = format_caller_context(analyzer, Some(context));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InternalMethod,
                format!(
                    "The method {}::{} is internal to {} but called from {}",
                    class_name, method_name, scope_phrase, caller_phrase
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if enforce_mutation_free {
            if let Some(class_info) = analyzer.codebase.get_class(class_id) {
                if !method_is_mutation_free(&method_info, class_info) {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ImpureMethodCall,
                        format!(
                            "Cannot call a possibly-mutating method {}::{} from a mutation-free context",
                            class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }

        if let Some(magic_property_return) = analyze_magic_property_method_call(
            analyzer,
            class_id,
            object_type_params.as_deref(),
            method_name,
            object_expr,
            args,
            arg_positions,
            pos,
            analysis_data,
        ) {
            return Some(magic_property_return);
        }

        let resolved_return_type =
            method_call_return_type_fetcher::fetch(analyzer, class_id, method_name)
                .or_else(|| {
                    maybe_resolve_dom_append_child_return_type(
                        analyzer,
                        class_id,
                        method_name,
                        arg_positions,
                        analysis_data,
                    )
                })
                .unwrap_or_else(|| {
                    resolve_effective_method_return_type(
                        analyzer,
                        class_id,
                        method_name,
                        &method_info,
                        &template_defaults,
                        &template_replacements,
                        args.len(),
                    )
                });

        let static_class_id =
            find_concrete_receiver_class_id(analyzer, obj_type).unwrap_or(receiver_class_id);

        let mut localized_return_type = localize_special_class_type_union(
            &resolved_return_type,
            class_id,
            static_class_id,
            parent_class_id,
        );

        if should_strip_false_from_datetime_modify_return(
            analyzer,
            receiver_class_id,
            method_name,
            arg_positions,
            analysis_data,
        ) || should_strip_false_from_pdo_prepare_return(analyzer, receiver_class_id, method_name)
        {
            localized_return_type
                .types
                .retain(|atomic| !matches!(atomic, TAtomic::TFalse));
        }

        if union_contains_static_reference(&resolved_return_type) {
            localized_return_type =
                merge_receiver_intersection_into_return_type(&localized_return_type, obj_type);
        }

        if let Some(tracked_type) = get_cached_no_arg_method_call_type(
            analyzer,
            context,
            object_expr,
            method_name,
            args.len(),
        ) {
            if let Some(intersection) = assertion_reconciler::intersect_union_with_union(
                &localized_return_type,
                &tracked_type,
            ) {
                localized_return_type = intersection;
            }
        }

        let method_is_mutation_free = analyzer
            .codebase
            .get_class(class_id)
            .map(|class_info| method_is_mutation_free(&method_info, class_info))
            .unwrap_or(method_info.is_mutation_free);

        if !method_is_mutation_free {
            invalidate_property_narrowings_after_mutation(analyzer, context);
        }

        if args.is_empty() {
            let can_memoize = method_is_mutation_free;

            if can_memoize {
                if let Some(object_key) = expression_identifier::get_expression_var_key(object_expr)
                {
                    let call_key = format!("{}->{}()", object_key, method_name);
                    let call_id = analyzer.interner.intern(&call_key);
                    context
                        .locals
                        .insert(call_id, localized_return_type.clone());
                }
            }
        }

        if has_null_receiver
            && !suppress_possibly_null_reference_issue
            && !expanded_obj_type.ignore_nullable_issues
            && !issue_suppression::is_issue_suppressed_at(analyzer, pos.0, "PossiblyNullReference")
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyNullReference,
                format!("Cannot call method {} on possibly null value", method_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if has_false_receiver && !expanded_obj_type.ignore_falsable_issues {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyFalseReference,
                format!("Cannot call method {} on possibly false value", method_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if has_invalid_receiver {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidMethodCall,
                format!(
                    "Cannot call method {} on possibly invalid type",
                    method_name
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if let Some(interface_id) = first_missing_interface
            && !context.inside_conditional
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedInterfaceMethod,
                format!(
                    "Method {}::{} does not exist",
                    analyzer.interner.lookup(interface_id),
                    method_name
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        let call_node = DataFlowNode::get_for_call(
            FunctionLikeIdentifier::Method(class_id, analyzer.interner.intern(method_name)),
            make_data_flow_node_position(analyzer, pos),
        );
        analysis_data.data_flow_graph.add_node(call_node.clone());

        add_default_dataflow_paths(
            &mut analysis_data.data_flow_graph,
            &expanded_obj_type.parent_nodes,
            &call_node,
        );

        for arg_pos in arg_positions {
            if let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) {
                add_default_dataflow_paths(
                    &mut analysis_data.data_flow_graph,
                    &arg_type.parent_nodes,
                    &call_node,
                );
            }
        }

        pzoom_code_info::ttype::extend_dataflow_uniquely(
            &mut localized_return_type.parent_nodes,
            vec![call_node],
        );

        return Some(localized_return_type);
    }

    if has_unsealed_magic_call {
        return Some(magic_call_return_type.unwrap_or_else(TUnion::mixed));
    }

    if !has_valid_receiver && !has_unsealed_magic_call {
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
                format!("Method {}::{} does not exist", interface_name, method_name),
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
                    if is_datetime_interface_add(analyzer, *name, method_name) {
                        return Some(TUnion::new(TAtomic::TNamedObject {
                            name: *name,
                            type_params: None,
                        }));
                    }

                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(pos.0);

                    if class_has_magic_call(class_info) {
                        if class_has_sealed_methods(class_info) {
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
                                format!("Method {}::{} does not exist", class_name, method_name)
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
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        } else {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedMethod,
                                format!("Method {}::{} does not exist", class_name, method_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
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
            TAtomic::TMixed => {
                if matches!(object_expr.unparenthesized(), Expression::ArrayAccess(_)) {
                    continue;
                }
                if !analyzer.config.is_issue_suppressed("MixedMethodCall") {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedMethodCall,
                        format!("Cannot call method {} on mixed type", method_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
            TAtomic::TNull | TAtomic::TVoid => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullReference,
                    format!("Cannot call method {} on null", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
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

fn method_has_more_specific_return(
    analyzer: &StatementsAnalyzer<'_>,
    candidate_method: &pzoom_code_info::FunctionLikeInfo,
    current_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let Some(candidate_return) = candidate_method
        .signature_return_type
        .as_ref()
        .or(candidate_method.return_type.as_ref())
    else {
        return false;
    };

    let Some(current_return) = current_method
        .signature_return_type
        .as_ref()
        .or(current_method.return_type.as_ref())
    else {
        return true;
    };

    let mut candidate_in_current = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        candidate_return,
        current_return,
        false,
        false,
        &mut candidate_in_current,
    ) {
        return false;
    }

    let mut current_in_candidate = TypeComparisonResult::new();
    let current_is_contained_by_candidate = union_type_comparator::is_contained_by(
        analyzer.codebase,
        current_return,
        candidate_return,
        false,
        false,
        &mut current_in_candidate,
    );

    !current_is_contained_by_candidate
}

fn receiver_allows_method_visibility_override(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_type: &TUnion,
    target_class: StrId,
) -> bool {
    let mut has_target_class = false;
    let mut has_override_interface = false;

    let mut track_named = |name: StrId| {
        if name == target_class {
            has_target_class = true;
        }

        if analyzer.codebase.get_class(name).is_some_and(|info| {
            info.kind == ClassLikeKind::Interface && info.override_method_visibility
        }) {
            has_override_interface = true;
        }
    };

    for atomic in &receiver_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => track_named(*name),
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if let TAtomic::TNamedObject { name, .. } = nested {
                        track_named(*name);
                    }
                }
            }
            _ => {}
        }
    }

    has_target_class && has_override_interface
}

fn get_method_visibility_scope_class_id(
    class_info: &ClassLikeInfo,
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> StrId {
    class_info
        .appearing_method_ids
        .get(&method_info.name)
        .copied()
        .or(method_info.declaring_class)
        .unwrap_or(class_info.name)
}

fn can_access_protected_member_visibility(
    analyzer: &StatementsAnalyzer<'_>,
    caller_class: StrId,
    visibility_scope_class: StrId,
) -> bool {
    caller_class == visibility_scope_class
        || object_type_comparator::is_class_subtype_of(
            caller_class,
            visibility_scope_class,
            analyzer.codebase,
        )
        || object_type_comparator::is_class_subtype_of(
            visibility_scope_class,
            caller_class,
            analyzer.codebase,
        )
}

fn resolve_named_object_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if let Some(method_info) = get_method_info_case_insensitive(analyzer, class_info, method_name) {
        let visibility_scope_class_id =
            get_method_visibility_scope_class_id(class_info, method_info);

        if method_info.visibility != Visibility::Private
            || visibility_scope_class_id == class_info.name
        {
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info.clone(),
            ));
        }
    }

    if class_info.kind == ClassLikeKind::Interface || class_has_magic_call(class_info) {
        if let Some(method_info) =
            get_pseudo_method_info_case_insensitive(analyzer, class_info, method_name)
        {
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info.clone(),
            ));
        }
    }

    resolve_named_mixin_instance_method(analyzer, class_info, object_type_params, method_name)
}

fn resolve_named_mixin_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if class_info.named_mixins.is_empty() {
        return None;
    }

    let class_template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut class_template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut class_template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    for mixin_atomic in &class_info.named_mixins {
        let localized_mixin = function_call_analyzer::replace_templates_in_union(
            &TUnion::new(mixin_atomic.clone()),
            &class_template_replacements,
            &class_template_defaults,
        );

        for localized_atomic in localized_mixin.types {
            let TAtomic::TNamedObject {
                name: mixin_class_id,
                type_params: mixin_type_params,
            } = localized_atomic
            else {
                continue;
            };

            let Some(mixin_class_info) = analyzer.codebase.get_class(mixin_class_id) else {
                continue;
            };

            if let Some(method_info) =
                get_method_info_case_insensitive(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }

            if let Some(method_info) =
                get_pseudo_method_info_case_insensitive(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }
        }
    }

    None
}

fn is_datetime_interface_add(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: pzoom_str::StrId,
    method_name: &str,
) -> bool {
    if !method_name.eq_ignore_ascii_case("add") {
        return false;
    }

    class_name == analyzer.interner.intern("DateTimeInterface")
        || class_name == analyzer.interner.intern("\\DateTimeInterface")
}

fn should_strip_false_from_datetime_modify_return(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class_id: StrId,
    method_name: &str,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> bool {
    if !method_name.eq_ignore_ascii_case("modify") {
        return false;
    }

    if !is_datetime_like_class(analyzer, receiver_class_id) {
        return false;
    }

    let Some(first_arg_pos) = arg_positions.first().copied() else {
        return false;
    };

    let Some(first_arg_type) = analysis_data.get_expr_type(first_arg_pos) else {
        return false;
    };

    !first_arg_type.types.is_empty()
        && first_arg_type.types.iter().all(|atomic| match atomic {
            TAtomic::TLiteralString { value } => !value.trim().is_empty(),
            _ => false,
        })
}

fn should_strip_false_from_pdo_prepare_return(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class_id: StrId,
    method_name: &str,
) -> bool {
    if !method_name.eq_ignore_ascii_case("prepare") {
        return false;
    }

    is_pdo_like_class(analyzer, receiver_class_id)
}

fn is_datetime_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let candidates = [
        analyzer.interner.intern("DateTime"),
        analyzer.interner.intern("\\DateTime"),
        analyzer.interner.intern("DateTimeImmutable"),
        analyzer.interner.intern("\\DateTimeImmutable"),
        analyzer.interner.intern("DateTimeInterface"),
        analyzer.interner.intern("\\DateTimeInterface"),
    ];

    for candidate in candidates {
        if class_id == candidate {
            return true;
        }

        if analyzer
            .codebase
            .all_classlike_descendants
            .get(&candidate)
            .is_some_and(|descendants| descendants.contains(&class_id))
        {
            return true;
        }
    }

    false
}

fn is_pdo_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let candidates = [
        analyzer.interner.intern("PDO"),
        analyzer.interner.intern("\\PDO"),
    ];

    for candidate in candidates {
        if class_id == candidate {
            return true;
        }

        if analyzer
            .codebase
            .all_classlike_descendants
            .get(&candidate)
            .is_some_and(|descendants| descendants.contains(&class_id))
        {
            return true;
        }
    }

    false
}

fn maybe_resolve_dom_append_child_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    if !method_name.eq_ignore_ascii_case("appendchild") {
        return None;
    }

    if !is_domnode_or_descendant(analyzer, class_id) {
        return None;
    }

    let first_arg_pos = *arg_positions.first()?;
    let arg_type = analysis_data.get_expr_type(first_arg_pos)?;

    if !arg_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TNamedObject { .. } | TAtomic::TObject | TAtomic::TObjectIntersection { .. }
        )
    }) {
        return None;
    }

    Some((*arg_type).clone())
}

fn is_domnode_or_descendant(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    if is_domnode_name(analyzer, class_id) {
        return true;
    }

    analyzer
        .codebase
        .get_class(class_id)
        .is_some_and(|class_info| {
            class_info
                .all_parent_classes
                .iter()
                .copied()
                .any(|parent_id| is_domnode_name(analyzer, parent_id))
        })
}

fn is_domnode_name(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let class_name = analyzer.interner.lookup(class_id);
    class_name.eq_ignore_ascii_case("domnode") || class_name.eq_ignore_ascii_case("\\domnode")
}

fn class_has_magic_call(class_info: &ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::CALL)
}

fn should_report_private_method_as_undefined(
    analyzer: &StatementsAnalyzer<'_>,
    calling_class: Option<StrId>,
    visibility_scope: StrId,
) -> bool {
    let Some(caller_class) = calling_class else {
        return false;
    };

    if caller_class == visibility_scope {
        return false;
    }

    let caller_is_subclass = analyzer
        .codebase
        .get_class(caller_class)
        .is_some_and(|caller_info| caller_info.all_parent_classes.contains(&visibility_scope));

    if !caller_is_subclass {
        return false;
    }

    analyzer
        .codebase
        .get_class(visibility_scope)
        .is_some_and(|scope_info| {
            scope_info.used_traits.is_empty() && scope_info.trait_method_aliases.is_empty()
        })
}

fn find_private_method_visibility_scope(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> Option<StrId> {
    let method_id = analyzer.interner.intern(method_name);
    let mut current_class = analyzer
        .codebase
        .get_class(class_id)
        .and_then(|class_info| class_info.parent_class);

    while let Some(parent_id) = current_class {
        let parent_info = analyzer.codebase.get_class(parent_id)?;
        if let Some(method_info) = parent_info.methods.get(&method_id)
            && method_info.visibility == Visibility::Private
        {
            return parent_info
                .declaring_method_ids
                .get(&method_id)
                .copied()
                .or(method_info.declaring_class)
                .or(Some(parent_id));
        }

        current_class = parent_info.parent_class;
    }

    None
}

fn class_has_sealed_methods(class_info: &ClassLikeInfo) -> bool {
    class_info.sealed_methods.unwrap_or(false)
}

fn class_has_sealed_properties(class_info: &ClassLikeInfo) -> bool {
    class_info.sealed_properties.unwrap_or(false) && !class_info.no_seal_properties
}

fn analyze_magic_property_method_call(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
    object_expr: &Expression<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let method_lc = method_name.to_ascii_lowercase();
    let is_this_call =
        expression_identifier::get_expression_var_key(object_expr).as_deref() == Some("$this");

    if method_lc == "__get" {
        let Some(prop_name) = get_literal_string_argument(analysis_data, arg_positions.first())
        else {
            return None;
        };
        let prop_id = analyzer.interner.intern(&prop_name);

        if let Some(pseudo_property_type) = class_info.pseudo_property_get_types.get(&prop_id) {
            return Some(localize_class_union_type(
                class_info,
                object_type_params,
                pseudo_property_type,
            ));
        }

        if class_has_sealed_properties(class_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            let issue_kind = if is_this_call {
                IssueKind::UndefinedThisPropertyFetch
            } else {
                IssueKind::UndefinedMagicPropertyFetch
            };
            let class_name = analyzer.interner.lookup(class_id);
            let message = if is_this_call {
                format!("Property {}::${} does not exist", class_name, prop_name)
            } else {
                format!(
                    "Magic property {}::${} does not exist",
                    class_name, prop_name
                )
            };
            analysis_data.add_issue(Issue::new(
                issue_kind,
                message,
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        return None;
    }

    if method_lc == "__set" {
        let Some(prop_name) = get_literal_string_argument(analysis_data, arg_positions.first())
        else {
            return None;
        };
        let prop_id = analyzer.interner.intern(&prop_name);

        if let Some(pseudo_property_type) = class_info.pseudo_property_set_types.get(&prop_id) {
            if let Some(second_arg_pos) = arg_positions.get(1) {
                if let Some(value_type) = analysis_data.get_expr_type(*second_arg_pos) {
                    let pseudo_property_type = localize_class_union_type(
                        class_info,
                        object_type_params,
                        pseudo_property_type,
                    );
                    let mut comparison_result = TypeComparisonResult::new();
                    let is_contained = union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &value_type,
                        &pseudo_property_type,
                        false,
                        false,
                        &mut comparison_result,
                    );

                    if !is_contained {
                        let can_be_contained = union_type_comparator::can_be_contained_by(
                            analyzer.codebase,
                            &value_type,
                            &pseudo_property_type,
                        );
                        let issue_kind = if can_be_contained {
                            IssueKind::PossiblyInvalidPropertyAssignmentValue
                        } else {
                            IssueKind::InvalidPropertyAssignmentValue
                        };
                        let class_name = analyzer.interner.lookup(class_id);
                        let message = if can_be_contained {
                            format!(
                                "Property {}::${} expects {}, possibly different type {} provided",
                                class_name,
                                prop_name,
                                pseudo_property_type.get_id(Some(analyzer.interner)),
                                value_type.get_id(Some(analyzer.interner))
                            )
                        } else {
                            format!(
                                "Property {}::${} expects {}, got {}",
                                class_name,
                                prop_name,
                                pseudo_property_type.get_id(Some(analyzer.interner)),
                                value_type.get_id(Some(analyzer.interner))
                            )
                        };
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            issue_kind,
                            message,
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
            }

            return None;
        }

        if class_has_sealed_properties(class_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            let issue_kind = if is_this_call {
                IssueKind::UndefinedThisPropertyAssignment
            } else {
                IssueKind::UndefinedMagicPropertyAssignment
            };
            let class_name = analyzer.interner.lookup(class_id);
            let message = if is_this_call {
                format!("Property {}::${} does not exist", class_name, prop_name)
            } else {
                format!(
                    "Magic property {}::${} does not exist",
                    class_name, prop_name
                )
            };
            analysis_data.add_issue(Issue::new(
                issue_kind,
                message,
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        let _ = args;
        return None;
    }

    None
}

fn get_literal_string_argument(
    analysis_data: &FunctionAnalysisData,
    arg_pos: Option<&Pos>,
) -> Option<String> {
    let arg_pos = *arg_pos?;
    let arg_type = analysis_data.get_expr_type(arg_pos)?;
    let atomic = arg_type.get_single()?;

    if let TAtomic::TLiteralString { value } = atomic {
        return Some(value.clone());
    }

    None
}

fn localize_class_union_type(
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    union: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return union.clone();
    }

    let template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    if template_defaults.is_empty() && template_replacements.is_empty() {
        return union.clone();
    }

    function_call_analyzer::replace_templates_in_union(
        union,
        &template_replacements,
        &template_defaults,
    )
}

fn get_pseudo_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.pseudo_methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .pseudo_methods
        .iter()
        .find_map(|(stored_id, method_info)| {
            analyzer
                .interner
                .lookup(*stored_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
                .then_some(method_info)
        })
}

fn get_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .methods
        .iter()
        .find_map(|(stored_id, method_info)| {
            analyzer
                .interner
                .lookup(*stored_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
                .then_some(method_info)
        })
}

fn resolve_effective_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> TUnion {
    let own_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        method_info,
        template_defaults,
        template_replacements,
        arg_count,
    );

    let inherited_return_type = get_inherited_method_return_type(
        analyzer,
        class_id,
        method_name,
        template_defaults,
        template_replacements,
        arg_count,
    );

    match (own_return_type, inherited_return_type) {
        (Some(own_return_type), Some(inherited_return_type))
            if should_prefer_inherited_return(
                analyzer,
                method_info,
                &own_return_type,
                &inherited_return_type,
            ) =>
        {
            inherited_return_type
        }
        (Some(own_return_type), _) => own_return_type,
        (None, Some(inherited_return_type)) => inherited_return_type,
        (None, None) => TUnion::mixed(),
    }
}

pub(super) fn localize_special_class_type_union(
    union: &TUnion,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = Vec::with_capacity(union.types.len());
    for atomic in &union.types {
        let localized_atomic = localize_special_class_type_atomic(
            atomic,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        if !localized.contains(&localized_atomic) {
            localized.push(localized_atomic);
        }
    }

    let mut localized_union = union.clone();
    localized_union.types = localized;
    localized_union.is_nullable = localized_union.types.iter().any(|t| t.is_nullable());
    localized_union.is_falsable = localized_union.types.iter().any(|t| t.is_falsable());
    localized_union
}

fn localize_special_class_type_atomic(
    atomic: &TAtomic,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            let localized_name = if *name == StrId::SELF {
                self_class_id
            } else if *name == StrId::STATIC {
                static_class_id
            } else if *name == StrId::PARENT {
                parent_class_id.unwrap_or(StrId::PARENT)
            } else {
                *name
            };

            TAtomic::TNamedObject {
                name: localized_name,
                type_params: type_params.as_ref().map(|params| {
                    params
                        .iter()
                        .map(|param| {
                            localize_special_class_type_union(
                                param,
                                self_class_id,
                                static_class_id,
                                parent_class_id,
                            )
                        })
                        .collect()
                }),
            }
        }
        TAtomic::TObjectIntersection { types } => {
            let mut localized = Vec::with_capacity(types.len());
            for nested in types {
                let localized_nested = localize_special_class_type_atomic(
                    nested,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                );
                if !localized.contains(&localized_nested) {
                    localized.push(localized_nested);
                }
            }
            TAtomic::TObjectIntersection { types: localized }
        }
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: localize_special_class_type_union(
                            &param.param_type,
                            self_class_id,
                            static_class_id,
                            parent_class_id,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(localize_special_class_type_union(
                    return_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: localize_special_class_type_union(
                            &param.param_type,
                            self_class_id,
                            static_class_id,
                            parent_class_id,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(localize_special_class_type_union(
                    return_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_type_union(
                as_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParamClass {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_type_atomic(
                as_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )),
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|as_type| {
                Box::new(localize_special_class_type_atomic(
                    as_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
        },
        _ => atomic.clone(),
    }
}

fn find_concrete_receiver_class_id(
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

fn union_contains_static_reference(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_static_reference)
}

fn atomic_contains_static_reference(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
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

fn merge_receiver_intersection_into_return_type(
    localized_return_type: &TUnion,
    receiver_type: &TUnion,
) -> TUnion {
    let receiver_named_types = collect_receiver_named_types(receiver_type);
    if receiver_named_types.is_empty() {
        return localized_return_type.clone();
    }

    let mut changed = false;
    let mut merged = Vec::with_capacity(localized_return_type.types.len());

    for atomic in &localized_return_type.types {
        match atomic {
            TAtomic::TObjectIntersection { types } => {
                let mut merged_types = types.clone();
                for receiver_named in &receiver_named_types {
                    if !merged_types.contains(receiver_named) {
                        merged_types.push(receiver_named.clone());
                        changed = true;
                    }
                }

                merged.push(TAtomic::TObjectIntersection {
                    types: merged_types,
                });
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

fn collect_receiver_named_types(receiver_type: &TUnion) -> Vec<TAtomic> {
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

fn get_inherited_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }

    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.interfaces.iter().copied());
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();

    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        if candidate_method_info.return_type.is_none() {
            continue;
        }

        let mut candidate_defaults = template_defaults.clone();
        for (template_name, template_default) in
            function_call_analyzer::get_class_template_defaults(candidate_class_info)
        {
            candidate_defaults
                .entry(template_name)
                .or_insert(template_default);
        }
        for (template_name, template_default) in
            function_call_analyzer::get_template_defaults(candidate_method_info)
        {
            candidate_defaults
                .entry(template_name)
                .or_insert(template_default);
        }

        let mut candidate_replacements = template_replacements.clone();
        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            for (template_name, mapped_type) in candidate_template_map {
                let resolved_mapped_type = function_call_analyzer::replace_templates_in_union(
                    mapped_type,
                    &candidate_replacements,
                    &candidate_defaults,
                );
                candidate_replacements.insert(*template_name, resolved_mapped_type);
            }
        }

        let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            candidate_method_info,
            &candidate_defaults,
            &candidate_replacements,
            arg_count,
        )
        .unwrap_or_else(TUnion::mixed);

        return Some(resolved_return_type);
    }

    None
}

#[derive(Clone)]
struct InheritedParamType {
    param_type: TUnion,
    from_docblock: bool,
    source_is_interface: bool,
}

fn get_inherited_method_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    param_index: usize,
) -> Option<InheritedParamType> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }

    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();
    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        let Some(candidate_param) = candidate_method_info.params.get(param_index) else {
            continue;
        };
        let Some(candidate_param_type) = candidate_param.get_type().cloned() else {
            continue;
        };

        let mut resolved_param_type = candidate_param_type;
        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            let mut candidate_defaults =
                function_call_analyzer::get_class_template_defaults(candidate_class_info);
            candidate_defaults.extend(function_call_analyzer::get_template_defaults(
                candidate_method_info,
            ));
            resolved_param_type = function_call_analyzer::replace_templates_in_union(
                &resolved_param_type,
                candidate_template_map,
                &candidate_defaults,
            );
        }

        return Some(InheritedParamType {
            param_type: resolved_param_type,
            from_docblock: candidate_param.has_docblock_type,
            source_is_interface: candidate_class_info.kind == ClassLikeKind::Interface,
        });
    }

    None
}

fn method_has_docblock_return_type(method_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    method_info.return_type.is_some()
        && method_info.return_type != method_info.signature_return_type
}

fn method_has_docblock_param_types(method_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    method_info
        .params
        .iter()
        .any(|param| param.has_docblock_type)
}

fn should_prefer_inherited_return(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    own_return_type: &TUnion,
    inherited_return_type: &TUnion,
) -> bool {
    if method_info.return_type != method_info.signature_return_type {
        return false;
    }

    if own_return_type.is_mixed() && !inherited_return_type.is_mixed() {
        return true;
    }

    if let (
        Some(TAtomic::TNamedObject {
            name: own_name,
            type_params: own_params,
        }),
        Some(TAtomic::TNamedObject {
            name: inherited_name,
            type_params: inherited_params,
        }),
    ) = (
        own_return_type.get_single(),
        inherited_return_type.get_single(),
    ) && own_name == inherited_name
        && own_params.is_none()
        && inherited_params.is_some()
    {
        return true;
    }

    let mut inherited_to_own = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        inherited_return_type,
        own_return_type,
        false,
        false,
        &mut inherited_to_own,
    ) {
        return false;
    }

    let mut own_to_inherited = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        own_return_type,
        inherited_return_type,
        false,
        false,
        &mut own_to_inherited,
    )
}

fn should_prefer_inherited_param(
    analyzer: &StatementsAnalyzer<'_>,
    param: &pzoom_code_info::functionlike_info::ParamInfo,
    inherited_param_type: &TUnion,
) -> bool {
    if param.has_docblock_type {
        return false;
    }

    let Some(own_param_type) = param.get_type() else {
        return true;
    };

    if own_param_type.is_mixed() && !inherited_param_type.is_mixed() {
        return true;
    }

    let mut inherited_to_own = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        inherited_param_type,
        own_param_type,
        false,
        false,
        &mut inherited_to_own,
    ) {
        return false;
    }

    let mut own_to_inherited = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        own_param_type,
        inherited_param_type,
        false,
        false,
        &mut own_to_inherited,
    )
}

fn expand_template_object_union(obj_type: &TUnion) -> TUnion {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TTemplateParam { as_type, .. } => {
                for as_atomic in &as_type.types {
                    if !expanded_types.contains(as_atomic) {
                        expanded_types.push(as_atomic.clone());
                    }
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !expanded_types.contains(as_type) {
                    expanded_types.push((**as_type).clone());
                }
            }
            TAtomic::TObjectIntersection { types } => {
                let mut expanded_intersection = Vec::new();

                for intersection_atomic in types {
                    match intersection_atomic {
                        TAtomic::TTemplateParam { as_type, .. } => {
                            for as_atomic in &as_type.types {
                                if !expanded_intersection.contains(as_atomic) {
                                    expanded_intersection.push(as_atomic.clone());
                                }
                            }
                        }
                        TAtomic::TTemplateParamClass { as_type, .. } => {
                            if !expanded_intersection.contains(as_type) {
                                expanded_intersection.push((**as_type).clone());
                            }
                        }
                        _ => {
                            if !expanded_intersection.contains(intersection_atomic) {
                                expanded_intersection.push(intersection_atomic.clone());
                            }
                        }
                    }
                }

                if !expanded_intersection.is_empty() {
                    let expanded_atomic = TAtomic::TObjectIntersection {
                        types: expanded_intersection,
                    };

                    if !expanded_types.contains(&expanded_atomic) {
                        expanded_types.push(expanded_atomic);
                    }
                }
            }
            _ => {
                if !expanded_types.contains(atomic) {
                    expanded_types.push(atomic.clone());
                }
            }
        }
    }

    TUnion::from_types(expanded_types)
}

fn analyze_pending_closure_args_for_method(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_info: &pzoom_code_info::ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    template_defaults.extend(function_call_analyzer::get_template_defaults(method_info));

    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    for (idx, arg) in args.iter().enumerate() {
        let Some(closure_offset) = get_closure_like_argument_offset(arg) else {
            continue;
        };

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        if analysis_data.get_expr_type(arg_pos).is_some() {
            continue;
        }

        let param = if idx < method_info.params.len() {
            Some(&method_info.params[idx])
        } else {
            method_info.params.last().filter(|param| param.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
            let replaced_param_type =
                if template_defaults.is_empty() && template_replacements.is_empty() {
                    param_type.clone()
                } else {
                    function_call_analyzer::replace_templates_in_union(
                        param_type,
                        &template_replacements,
                        &template_defaults,
                    )
                };

            localize_special_class_type_union(
                &replaced_param_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )
        });

        if let Some(expected_param_type) = expected_param_type {
            if callable_validation::union_has_callable(&expected_param_type) {
                context
                    .expected_callable_arg_types
                    .insert(closure_offset, expected_param_type);
            }
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        context.expected_callable_arg_types.remove(&closure_offset);
    }
}

fn method_is_mutation_free(
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    method_info.is_pure
        || method_info.is_mutation_free
        || (class_info.is_immutable && !method_info.is_static)
}

fn invalidate_property_narrowings_after_mutation(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let keys_to_remove: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            let var_name = analyzer.interner.lookup(*var_id);
            var_name.contains("->")
        })
        .collect();

    for var_id in keys_to_remove {
        context.locals.remove(&var_id);
    }
}

fn is_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    if function_info.is_pure || function_info.is_mutation_free {
        return true;
    }

    if function_info.is_static {
        return false;
    }

    if let Some(class_id) = function_info.declaring_class {
        return analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| class_info.is_immutable);
    }

    false
}

fn verify_method_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_pos: Pos,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
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
        Some(template_defaults),
        Some(template_replacements),
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
    if !has_spread && !accepts_unbounded && args.len() > method_info.params.len() {
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
                callable_validation::verify_unpacked_argument(
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

                let should_use_inherited = effective_param.param_type.is_none()
                    || (method_info.inherits_docblock && !param.has_docblock_type)
                    || (can_auto_inherit_docblock && !param.has_docblock_type)
                    || can_inherit_interface_contract;

                if should_use_inherited {
                    effective_param.param_type = Some(inherited_param_type.param_type);
                }
            }

            if let Some(param_type) = effective_param.get_type() {
                let replaced_param_type =
                    if template_defaults.is_empty() && template_replacements.is_empty() {
                        param_type.clone()
                    } else {
                        function_call_analyzer::replace_templates_in_union(
                            param_type,
                            template_replacements,
                            template_defaults,
                        )
                    };

                effective_param.param_type = Some(localize_special_class_type_union(
                    &replaced_param_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ));
            }

            callable_validation::verify_argument_type(
                analyzer,
                arg,
                arg_pos,
                &arg_type,
                &effective_param,
                param_index.unwrap_or(idx),
                &callable_name,
                analysis_data,
                context,
            );
        }
    }
}

fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    if method_info.assertions.is_empty() {
        return;
    }

    for assertion in &method_info.assertions {
        let resolved_assertion_type = replace_and_localize_assertion_type(
            &assertion.assertion_type,
            template_replacements,
            template_defaults,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if assertion_name.as_ref() == "$this" {
            apply_assertion_to_expression(analyzer, object_expr, &resolved_assertion_type, context);
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
            argument.value(),
            &resolved_assertion_type,
            context,
        );
    }
}

fn replace_and_localize_assertion_type(
    assertion_type: &AssertionType,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => {
            AssertionType::IsType(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsEqual(asserted_type) => {
            AssertionType::IsEqual(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsLooselyEqual(asserted_type) => {
            AssertionType::IsLooselyEqual(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotType(asserted_type) => {
            AssertionType::IsNotType(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotEqual(asserted_type) => {
            AssertionType::IsNotEqual(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotLooselyEqual(asserted_type) => {
            AssertionType::IsNotLooselyEqual(localize_special_class_type_union(
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

fn apply_assertion_to_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
) {
    let Some(var_key) = expression_identifier::get_expression_var_key(expr) else {
        return;
    };

    let var_id = analyzer.interner.intern(&var_key);
    let existing_type = context
        .locals
        .get(&var_id)
        .cloned()
        .unwrap_or_else(TUnion::mixed);
    let narrowed_type = apply_functionlike_assertion_to_union(&existing_type, assertion_type);
    context.locals.insert(var_id, narrowed_type);
}

fn find_assertion_param_index(
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
        AssertionType::Truthy | AssertionType::NotEmpty => existing_type.clone(),
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
