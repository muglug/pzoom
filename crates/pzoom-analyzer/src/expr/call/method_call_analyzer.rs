//! Method call analyzer.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::ast::ast::call::{MethodCall, NullSafeMethodCall};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{
    Issue, IssueKind, TAtomic, TUnion,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::{
    argument_analyzer, callable_validation, function_call_analyzer,
};

use super::atomic_method_call_analyzer::*;

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
pub(crate) fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

pub(crate) fn emit_invalid_dynamic_method_name_issues(
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

pub(crate) fn is_closure_like_argument(arg: &mago_syntax::ast::ast::argument::Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

pub(crate) fn get_closure_like_argument_offset(
    arg: &mago_syntax::ast::ast::argument::Argument<'_>,
) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

pub(crate) fn analyze_closure_args_without_context(
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

pub(crate) fn get_no_arg_method_call_var_id(
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

pub(crate) fn get_cached_no_arg_method_call_type(
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

pub(crate) fn get_reconciled_receiver_type_for_expression(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    object_expr: &Expression<'_>,
) -> Option<TUnion> {
    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let object_id = analyzer.interner.find(&object_key)?;
    context.locals.get(&object_id).cloned()
}

/// Look up the return type of a method on a type.
/// True when a `method_exists($obj, 'method')` check earlier in this scope guards the
/// current call, so the method is known to exist at runtime. Mirrors the
/// `@method_exists(...)` assertion recorded by the assertion finder.
pub(crate) fn is_method_guarded_by_method_exists(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    object_expr: &Expression<'_>,
    method_name: &str,
) -> bool {
    let Some(object_key) = expression_identifier::get_expression_var_key(object_expr) else {
        return false;
    };
    let key = crate::assertion_finder::method_exists_assertion_key(&object_key, method_name);
    let key_id = analyzer.interner.intern(&key);
    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

#[derive(Clone)]
pub(crate) struct InheritedParamType {
    pub(crate) param_type: TUnion,
    pub(crate) from_docblock: bool,
    pub(crate) source_is_interface: bool,
}

pub(crate) fn analyze_pending_closure_args_for_method(
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
    template_defaults.extend_overlay(function_call_analyzer::get_template_defaults(method_info));

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

            localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
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

pub(crate) fn invalidate_property_narrowings_after_mutation(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    // Mirror Psalm's default config (`remember_property_assignments_after_call = true`):
    // a non-mutation-free method call does NOT clear property narrowings such as
    // `$a->prop`. Only memoized method-call results (e.g. `$a->foo()`) are dropped,
    // matching `Context::removeMutableObjectVars(methods_only: true)`.
    let keys_to_remove: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            let var_name = analyzer.interner.lookup(*var_id);
            (var_name.contains("->") || var_name.contains("::")) && var_name.contains("()")
        })
        .collect();

    for var_id in keys_to_remove {
        context.locals.remove(&var_id);
    }
}

pub(crate) fn is_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
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
