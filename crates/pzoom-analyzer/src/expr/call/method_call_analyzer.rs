//! Method call analyzer.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::cst::cst::call::{MethodCall, NullSafeMethodCall};
use mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector;
use mago_syntax::cst::cst::expression::Expression;

use pzoom_code_info::VarName;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::{argument_analyzer, callable_validation, function_call_analyzer};

use super::atomic_method_call_analyzer::*;
use std::rc::Rc;

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
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let obj_pos =
        expression_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let obj_type = analysis_data.expr_types.get(&obj_pos).cloned();

    // Psalm AtomicMethodCallAnalyzer type-coverage: per receiver atomic, a call
    // on `mixed` counts as mixed, otherwise non-mixed.
    if let Some(obj_t) = obj_type.as_deref() {
        for atomic in &obj_t.types {
            analysis_data.record_mixedness(
                context,
                matches!(
                    atomic,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset
                ),
            );
        }
    }

    let args: Vec<_> = method_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();

    // Get the method name
    let method_name = get_method_name(&method_call.method);
    // Dynamic method selectors (`$obj->$m()`) consume their inner expression
    // (general use).
    analyze_dynamic_selector(analyzer, &method_call.method, analysis_data, context);

    // Predeclare by-ref out-params (`$obj->m(..., &$out)`) before analyzing
    // the argument expressions, mirroring the function-call path.
    let resolved_method_info = method_name.and_then(|method_name| {
        pre_resolve_instance_method_info(analyzer, obj_type.as_deref(), method_name)
    });
    if let Some(method_info) = resolved_method_info {
        super::arguments_analyzer::predeclare_by_ref_argument_vars(
            analyzer,
            Some("instance-method"),
            Some(method_info),
            &method_call.argument_list.arguments,
            context,
        );
    }

    // Psalm's evaluateArbitraryParam: when the method is unknown, an
    // undefined direct-variable argument might be passed by reference —
    // report PossiblyUndefinedVariable and seed it as mixed instead of
    // treating it as undefined.
    if resolved_method_info.is_none() {
        for arg in &args {
            let Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(direct)) =
                arg.value().unparenthesized()
            else {
                continue;
            };
            let var_id = VarName::new(pzoom_syntax::bytes_to_str(direct.name));
            if context.locals.contains_key(&var_id) {
                continue;
            }
            if context.check_variables {
                let span = arg.value().span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyUndefinedVariable,
                    format!(
                        "Variable {} must be defined prior to use within an unknown function or method",
                        pzoom_syntax::bytes_to_str(direct.name)
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
            // we don't know if it exists, assume it's passed by reference
            let mut placeholder = TUnion::mixed();
            placeholder.from_undefined_by_ref = true;
            context.set_var_type(var_id, placeholder);
        }
    }

    for arg in &args {
        if is_closure_like_argument(arg) {
            continue;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    // Try to look up method return type from each atomic type in the union
    if let (Some(obj_t), Some(method_name)) = (obj_type.as_ref(), method_name) {
        let return_type = get_method_return_type(
            analyzer,
            method_call.object,
            &obj_t,
            method_name,
            pos,
            {
                let span = method_call.method.span();
                (span.start.offset, span.end.offset)
            },
            &args,
            &arg_positions,
            enforce_mutation_free,
            has_nullsafe(method_call.object),
            analysis_data,
            context,
        );
        if let Some(return_type) = return_type {
            // A method declared to return `never`/`nothing` terminates control
            // flow (Psalm sets $context->has_returned; Hakana sets has_returned
            // plus ControlAction::End). Guarded on !inside_loop like both upstreams.
            if return_type.is_nothing() && !context.inside_loop {
                context.has_returned = true;
                context
                    .control_actions
                    .insert(crate::stmt::scope_analyzer::ControlAction::End);
            }
            analysis_data.expr_types.insert(pos, Rc::new(return_type));
            return;
        }
    } else if let Some(obj_t) = obj_type.as_ref() {
        if method_name.is_none() {
            emit_invalid_dynamic_method_name_issues(analyzer, &obj_t, pos, analysis_data);
        }
    }

    analyze_closure_args_without_context(analyzer, &args, analysis_data, context);

    // Fall back to mixed
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::mixed()));
}

/// Whether the receiver chain contains a null-safe operator (Psalm's
/// `MethodCallAnalyzer::hasNullsafe`). PHP's `?->` short-circuits the whole
/// remaining chain, so the null introduced by an upstream `?->` never reaches
/// this call — Psalm suppresses PossiblyNullReference in that case.
pub(crate) fn has_nullsafe(expr: &Expression<'_>) -> bool {
    use mago_syntax::cst::cst::access::Access;
    use mago_syntax::cst::cst::call::Call;

    match expr.unparenthesized() {
        Expression::Call(Call::Method(method_call)) => has_nullsafe(method_call.object),
        Expression::Access(Access::Property(prop_access)) => has_nullsafe(prop_access.object),
        Expression::Call(Call::NullSafeMethod(_))
        | Expression::Access(Access::NullSafeProperty(_)) => true,
        _ => false,
    }
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
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let obj_pos =
        expression_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let obj_type = analysis_data.expr_types.get(&obj_pos).cloned();

    // Psalm AtomicMethodCallAnalyzer type-coverage: per receiver atomic, a call
    // on `mixed` counts as mixed, otherwise non-mixed.
    if let Some(obj_t) = obj_type.as_deref() {
        for atomic in &obj_t.types {
            analysis_data.record_mixedness(
                context,
                matches!(
                    atomic,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset
                ),
            );
        }
    }

    let args: Vec<_> = method_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();

    // Get the method name
    let method_name = get_method_name(&method_call.method);
    // Dynamic method selectors (`$obj->$m()`) consume their inner expression
    // (general use).
    analyze_dynamic_selector(analyzer, &method_call.method, analysis_data, context);

    // Predeclare by-ref out-params (`$obj->m(..., &$out)`) before analyzing
    // the argument expressions, mirroring the function-call path.
    if let Some(method_name) = method_name
        && let Some(method_info) =
            pre_resolve_instance_method_info(analyzer, obj_type.as_deref(), method_name)
    {
        super::arguments_analyzer::predeclare_by_ref_argument_vars(
            analyzer,
            Some("instance-method"),
            Some(method_info),
            &method_call.argument_list.arguments,
            context,
        );
    }

    for arg in &args {
        if is_closure_like_argument(arg) {
            continue;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    // Try to look up method return type
    if let (Some(obj_t), Some(method_name)) = (obj_type.as_ref(), method_name) {
        // For null-safe calls, get the return type and add null to it
        if let Some(mut return_type) = get_method_return_type(
            analyzer,
            method_call.object,
            &obj_t,
            method_name,
            pos,
            {
                let span = method_call.method.span();
                (span.start.offset, span.end.offset)
            },
            &args,
            &arg_positions,
            enforce_mutation_free,
            true,
            analysis_data,
            context,
        ) {
            // If the object could be null, the result could be null
            let object_type_for_nullsafe =
                get_reconciled_receiver_type_for_expression(context, method_call.object)
                    .unwrap_or_else(|| (**obj_t).clone());
            if object_type_for_nullsafe.is_nullable() {
                return_type.add_type(TAtomic::TNull);
            }
            // A `never`-returning call terminates flow — unless `?->` could
            // short-circuit to null, in which case the result is `nothing|null`
            // (not nothing) and this check correctly does not fire.
            if return_type.is_nothing() && !context.inside_loop {
                context.has_returned = true;
                context
                    .control_actions
                    .insert(crate::stmt::scope_analyzer::ControlAction::End);
            }
            analysis_data.expr_types.insert(pos, Rc::new(return_type));
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
    analysis_data.expr_types.insert(pos, Rc::new(result));
}

/// Analyze a non-identifier member selector's inner expression under
/// general use (`$obj->$m()` uses `$m`).
fn analyze_dynamic_selector(
    analyzer: &StatementsAnalyzer<'_>,
    selector: &ClassLikeMemberSelector<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    match selector {
        ClassLikeMemberSelector::Identifier(_) => {}
        ClassLikeMemberSelector::Missing(_) => {}
        ClassLikeMemberSelector::Variable(var) => {
            let _ = crate::expression_analyzer::analyze(
                analyzer,
                &mago_syntax::cst::cst::expression::Expression::Variable(var.clone()),
                analysis_data,
                context,
            );
        }
        ClassLikeMemberSelector::Expression(expr) => {
            let _ = crate::expression_analyzer::analyze(
                analyzer,
                expr.expression,
                analysis_data,
                context,
            );
        }
    }
    context.inside_general_use = was_inside_general_use;
}

/// Get the method name from a method selector.
pub(crate) fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(pzoom_syntax::bytes_to_str(id.value)),
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
            // `non-empty-mixed` (a truthy-narrowed mixed) is a TMixed subtype in
            // Psalm, so a dynamic-name call on it is a MixedMethodCall too.
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
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

pub(crate) fn is_closure_like_argument(
    arg: &mago_syntax::cst::cst::argument::Argument<'_>,
) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

pub(crate) fn get_closure_like_argument_offset(
    arg: &mago_syntax::cst::cst::argument::Argument<'_>,
) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn analyze_closure_args_without_context(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::cst::cst::argument::Argument<'_>],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for arg in args {
        if is_closure_like_argument(arg) {
            argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        }
    }
}

pub(crate) fn get_cached_no_arg_method_call_type(
    context: &BlockContext,
    object_expr: &Expression<'_>,
    method_name: &str,
    arg_count: usize,
) -> Option<TUnion> {
    if arg_count != 0 {
        return None;
    }

    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    context
        .locals
        .get(format!("{}->{}()", object_key, method_name.to_ascii_lowercase()).as_str())
        .map(|__t| (**__t).clone())
}

pub(crate) fn get_reconciled_receiver_type_for_expression(
    context: &BlockContext,
    object_expr: &Expression<'_>,
) -> Option<TUnion> {
    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    context.locals.get(object_key.as_str()).map(|__t| (**__t).clone())
}

/// Look up the return type of a method on a type.
/// True when a `method_exists($obj, 'method')` check earlier in this scope guards the
/// current call, so the method is known to exist at runtime. Mirrors the
/// `@method_exists(...)` assertion recorded by the assertion finder.
pub(crate) fn is_method_guarded_by_method_exists(
    context: &BlockContext,
    object_expr: &Expression<'_>,
    method_name: &str,
) -> bool {
    let Some(object_key) = expression_identifier::get_expression_var_key(object_expr) else {
        return false;
    };
    let key = crate::assertion_finder::method_exists_assertion_key(&object_key, method_name);
    let key_id = VarName::new(&key);
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
    args: &[&mago_syntax::cst::cst::argument::Argument<'_>],
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
    let mut template_result = function_call_analyzer::get_class_template_defaults(class_info);
    for template_type in &method_info.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut template_result,
        class_info,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_result,
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
        if analysis_data.expr_types.get(&arg_pos).cloned().is_some() {
            continue;
        }

        let param = if idx < method_info.params.len() {
            Some(&method_info.params[idx])
        } else {
            method_info.params.last().filter(|param| param.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
            let replaced_param_type = if crate::template::template_result_is_empty(&template_result)
            {
                param_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(param_type, &template_result)
            };

            localize_special_class_type_union(
                analyzer.codebase,
                analyzer.interner,
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

pub(crate) fn invalidate_property_narrowings_after_mutation(context: &mut BlockContext) {
    // Mirror Psalm's default config (`remember_property_assignments_after_call = true`):
    // a non-mutation-free method call does NOT clear property narrowings such as
    // `$a->prop`. Only memoized method-call results (e.g. `$a->foo()`) are dropped,
    // matching `Context::removeMutableObjectVars(methods_only: true)`.
    let keys_to_remove: Vec<_> = context
        .locals
        .keys()
        .cloned()
        .filter(|var_id| {
            let var_name = var_id.as_str();
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

    // Psalm enforces purity in the body for `@psalm-pure` functions,
    // `@psalm-mutation-free` methods, and methods of `@psalm-immutable` /
    // `@psalm-external-mutation-free` classes. (Bare `@mutation-free` is not
    // a Psalm tag and is dropped at scan.) Constructors are exempt:
    // FunctionLikeAnalyzer skips `__construct` when setting
    // `$context->mutation_free` — initialization may call impure code.
    if function_info.is_pure {
        return true;
    }

    if function_info.name == pzoom_str::StrId::CONSTRUCT {
        return false;
    }

    // Psalm sets `$context->mutation_free` only for a *declared*
    // `@psalm-mutation-free` method (`!$storage->mutation_free_inferred`); a
    // method pzoom inferred as mutation-free is not enforced, so calls it makes
    // are not ImpureMethodCall.
    if function_info.is_mutation_free && !function_info.mutation_free_inferred {
        return true;
    }

    if function_info.is_static {
        return false;
    }

    if let Some(class_id) = function_info.declaring_class {
        return analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| {
                class_info.is_immutable || class_info.is_external_mutation_free
            });
    }

    false
}

/// An `@psalm-immutable` / `@psalm-external-mutation-free` class runs its
/// constructor in Psalm's `external_mutation_free` context: FunctionLikeAnalyzer
/// sets `$context->external_mutation_free` for such methods with no `__construct`
/// exemption (unlike `mutation_free`). `is_mutation_free_context` exempts every
/// constructor, so callers that want the external-mutation-free purity checks —
/// impure method calls (with a same-class relaxation) and impure function calls —
/// detect the case with this helper.
pub(crate) fn is_external_mutation_free_constructor_context(
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    analyzer.function_info.is_some_and(|function_info| {
        function_info.name == pzoom_str::StrId::CONSTRUCT
            && !function_info.is_static
            && !function_info.mutation_free_inferred
            && function_info
                .declaring_class
                .and_then(|class_id| analyzer.codebase.get_class(class_id))
                .is_some_and(|class_info| {
                    class_info.is_immutable || class_info.is_external_mutation_free
                })
    })
}

/// Best-effort early resolution of an instance call's method storage (per
/// receiver atomic, class hierarchy walk) so by-ref out-params can be
/// predeclared before argument analysis.
fn pre_resolve_instance_method_info<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    obj_type: Option<&pzoom_code_info::TUnion>,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer
        .interner
        .find(method_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);

    for atomic in &obj_type?.types {
        let pzoom_code_info::TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        let mut current = Some(*name);
        while let Some(current_id) = current {
            let Some(class_info) = analyzer.codebase.get_class(current_id) else {
                break;
            };
            if let Some(method_info) = class_info.methods.get(&method_id) {
                return Some(method_info);
            }
            current = class_info.parent_class;
        }
    }

    None
}
