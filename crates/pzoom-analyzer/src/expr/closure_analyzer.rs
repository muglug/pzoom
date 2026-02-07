//! Closure and arrow function analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::function_like::arrow_function::ArrowFunction;
use mago_syntax::ast::ast::function_like::closure::Closure;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter as MagoParameter;
use mago_syntax::ast::ast::statement::Statement;

use pzoom_code_info::algebra::ClauseKey;
use pzoom_code_info::{
    FunctionLikeParameter, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use pzoom_syntax::resolve_hint;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a closure expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    closure: &Closure<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let closure_offset = closure.span().start.offset;
    let inline_callable_annotation = analyzer.get_inline_callable_annotation(closure_offset);

    if inline_callable_annotation.is_some_and(|annotation| annotation.has_template_annotation) {
        add_issue(
            analyzer,
            analysis_data,
            closure.span().start.offset,
            closure.span().end.offset,
            IssueKind::InvalidDocblock,
            "Template annotations are not allowed on closures",
        );
    }

    // Create a new scope for the closure body
    let mut closure_context = context.clone();
    if closure.r#static.is_some() {
        strip_this_from_context(analyzer, &mut closure_context);
    }
    strip_property_path_assumptions(analyzer, &mut closure_context);

    let param_ids: FxHashSet<StrId> = closure
        .parameter_list
        .parameters
        .iter()
        .map(|param| analyzer.interner.intern(param.variable.name))
        .collect();

    // Handle use() clause for captured variables
    if let Some(ref use_clause) = closure.use_clause {
        for use_var in use_clause.variables.iter() {
            let var_name = use_var.variable.name;
            let var_id = analyzer.interner.intern(var_name);
            let normalized_name = var_name.trim_start_matches('$');

            if param_ids.contains(&var_id) {
                add_issue(
                    analyzer,
                    analysis_data,
                    use_var.variable.span().start.offset,
                    use_var.variable.span().end.offset,
                    IssueKind::DuplicateParam,
                    format!("Closure use duplicates param name ${}", normalized_name),
                );
            }

            // Copy the variable's type from the outer context
            if let Some(var_type) = context.locals.get(&var_id) {
                // For &$var (by reference), the inner changes affect outer
                // For $var (by value), it's a copy
                closure_context.locals.insert(var_id, var_type.clone());
                if use_var.ampersand.is_some() {
                    context.mark_external_reference(var_id);
                    closure_context.mark_external_reference(var_id);
                }
            } else if use_var.ampersand.is_some() {
                // Allow recursive self-capture patterns like `$f = function () use (&$f) { ... }`.
                closure_context.locals.insert(var_id, TUnion::mixed());
                context.mark_external_reference(var_id);
                closure_context.mark_external_reference(var_id);
            } else if context.check_variables
                && normalized_name != "argv"
                && normalized_name != "argc"
            {
                add_issue(
                    analyzer,
                    analysis_data,
                    use_var.variable.span().start.offset,
                    use_var.variable.span().end.offset,
                    IssueKind::UndefinedVariable,
                    format!("Undefined variable ${}", normalized_name),
                );
                closure_context.locals.insert(var_id, TUnion::mixed());
            }
        }
    }

    // Extract parameter types
    let mut params = extract_param_types(
        analyzer,
        &closure.parameter_list.parameters,
        context.namespace,
        context.self_class,
        context.parent_class,
    );
    if let Some(inline_annotation) = inline_callable_annotation {
        apply_inline_callable_param_types(
            analyzer,
            &closure.parameter_list.parameters,
            &mut params,
            inline_annotation,
        );
    }
    if let Some(expected_callable_type) = context.expected_callable_arg_types.get(&closure_offset) {
        apply_expected_callable_param_types(
            &closure.parameter_list.parameters,
            &mut params,
            expected_callable_type,
        );
    }

    // Add parameters to the closure context
    for (param, param_info) in closure.parameter_list.parameters.iter().zip(params.iter()) {
        let param_name = param.variable.name;
        let param_id = analyzer.interner.intern(param_name);
        closure_context
            .locals
            .insert(param_id, param_info.param_type.clone());
    }

    let hinted_return_type = closure.return_type_hint.as_ref().map(|rth| {
        resolve_hint(
            &rth.hint,
            analyzer.interner,
            context.namespace,
            context.self_class,
            context.parent_class,
            None,
            Some(analyzer.resolved_names),
        )
    });
    let inline_return_type =
        inline_callable_annotation.and_then(|annotation| annotation.return_type.clone());

    let return_types_start = analysis_data.inferred_return_types.len();
    let yield_types_start = analysis_data.inferred_yield_types.len();

    let mut closure_function_info = analyzer.function_info.cloned().unwrap_or_default();
    let has_explicit_pure_annotation =
        inline_callable_annotation.is_some_and(|annotation| annotation.is_pure);
    let infer_purity = !has_explicit_pure_annotation;
    closure_function_info.is_pure = has_explicit_pure_annotation || infer_purity;
    closure_function_info.is_mutation_free = false;
    let closure_expected_return_type = hinted_return_type
        .clone()
        .or_else(|| inline_return_type.clone());
    closure_function_info.return_type = closure_expected_return_type.clone();
    closure_function_info.signature_return_type = closure_expected_return_type.clone();
    closure_function_info.returns_by_ref = closure.ampersand.is_some();

    let closure_stmt_analyzer = StatementsAnalyzer {
        codebase: analyzer.codebase,
        interner: analyzer.interner,
        function_info: Some(&closure_function_info),
        file_path: analyzer.file_path,
        source: analyzer.source,
        resolved_names: analyzer.resolved_names,
        config: analyzer.config,
    };

    let issue_count_before = analysis_data.issues.len();
    let _ = stmt_analyzer::analyze_stmts(
        &closure_stmt_analyzer,
        closure.body.statements.as_slice(),
        analysis_data,
        &mut closure_context,
    );
    let saw_impure_issue =
        strip_inferred_impure_issues(analysis_data, issue_count_before, !infer_purity);
    let has_obvious_side_effect_stmt = closure_body_has_obvious_side_effect_statements(closure);
    let closure_is_pure = !saw_impure_issue
        && !has_obvious_side_effect_stmt
        && (closure_function_info.is_pure || closure_function_info.is_mutation_free);

    let yielded_return_type = infer_yielded_return_type(analysis_data, yield_types_start);
    let inferred_return_type = if yielded_return_type.is_some() {
        yielded_return_type.clone()
    } else {
        infer_inferred_return_type(analysis_data, return_types_start)
    };

    let has_expected_callable_context = context
        .expected_callable_arg_types
        .contains_key(&closure_offset);

    emit_closure_return_issues(
        analyzer,
        analysis_data,
        closure.span().start.offset,
        closure.span().end.offset,
        closure_context.has_returned,
        closure_expected_return_type.as_ref(),
        inferred_return_type.as_ref(),
        has_expected_callable_context,
    );

    let return_type = if let Some(hinted_return_type) = hinted_return_type {
        if let Some(inferred_return_type) = inferred_return_type {
            let mut comparison_result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                &inferred_return_type,
                &hinted_return_type,
                false,
                false,
                &mut comparison_result,
            ) {
                Some(inferred_return_type)
            } else {
                Some(hinted_return_type)
            }
        } else {
            Some(hinted_return_type)
        }
    } else {
        inline_return_type.or(inferred_return_type)
    };

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: Some(params),
        return_type: return_type.map(Box::new),
        is_pure: Some(closure_is_pure),
    });

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze an arrow function expression.
pub fn analyze_arrow_function(
    analyzer: &StatementsAnalyzer<'_>,
    arrow: &ArrowFunction<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let arrow_offset = arrow.span().start.offset;
    let inline_callable_annotation = analyzer.get_inline_callable_annotation(arrow_offset);

    if inline_callable_annotation.is_some_and(|annotation| annotation.has_template_annotation) {
        add_issue(
            analyzer,
            analysis_data,
            arrow.span().start.offset,
            arrow.span().end.offset,
            IssueKind::InvalidDocblock,
            "Template annotations are not allowed on closures",
        );
    }

    // Arrow functions have implicit variable capture - all variables from outer scope
    // are captured by value automatically

    // Add parameters to context
    let mut arrow_context = context.clone();
    if arrow.r#static.is_some() {
        strip_this_from_context(analyzer, &mut arrow_context);
    }
    strip_property_path_assumptions(analyzer, &mut arrow_context);

    // Extract parameter types
    let mut params = extract_param_types(
        analyzer,
        &arrow.parameter_list.parameters,
        context.namespace,
        context.self_class,
        context.parent_class,
    );
    if let Some(inline_annotation) = inline_callable_annotation {
        apply_inline_callable_param_types(
            analyzer,
            &arrow.parameter_list.parameters,
            &mut params,
            inline_annotation,
        );
    }
    if let Some(expected_callable_type) = context.expected_callable_arg_types.get(&arrow_offset) {
        apply_expected_callable_param_types(
            &arrow.parameter_list.parameters,
            &mut params,
            expected_callable_type,
        );
    }

    for (param, param_info) in arrow.parameter_list.parameters.iter().zip(params.iter()) {
        let param_name = param.variable.name;
        let param_id = analyzer.interner.intern(param_name);
        arrow_context
            .locals
            .insert(param_id, param_info.param_type.clone());
    }

    let mut arrow_function_info = analyzer.function_info.cloned().unwrap_or_default();
    let has_explicit_pure_annotation =
        inline_callable_annotation.is_some_and(|annotation| annotation.is_pure);
    let infer_purity = !has_explicit_pure_annotation;
    arrow_function_info.is_pure = has_explicit_pure_annotation || infer_purity;
    arrow_function_info.is_mutation_free = false;

    let arrow_expr_analyzer = StatementsAnalyzer {
        codebase: analyzer.codebase,
        interner: analyzer.interner,
        function_info: Some(&arrow_function_info),
        file_path: analyzer.file_path,
        source: analyzer.source,
        resolved_names: analyzer.resolved_names,
        config: analyzer.config,
    };

    // Analyze the body expression to infer return type
    let issue_count_before = analysis_data.issues.len();
    let body_pos = expression_analyzer::analyze(
        &arrow_expr_analyzer,
        arrow.expression,
        analysis_data,
        &mut arrow_context,
    );
    let saw_impure_issue =
        strip_inferred_impure_issues(analysis_data, issue_count_before, !infer_purity);
    let arrow_is_pure =
        !saw_impure_issue && (arrow_function_info.is_pure || arrow_function_info.is_mutation_free);
    let inferred_return_type = analysis_data.get_expr_type(body_pos).map(|t| (*t).clone());

    let hinted_return = arrow.return_type_hint.as_ref().map(|rth| {
        resolve_hint(
            &rth.hint,
            analyzer.interner,
            context.namespace,
            context.self_class,
            context.parent_class,
            None,
            Some(analyzer.resolved_names),
        )
    });
    let inline_return_type =
        inline_callable_annotation.and_then(|annotation| annotation.return_type.clone());

    if arrow.ampersand.is_some() && !is_reference_returnable_expression(arrow.expression) {
        add_issue(
            analyzer,
            analysis_data,
            arrow.expression.span().start.offset,
            arrow.expression.span().end.offset,
            IssueKind::NonVariableReferenceReturn,
            "Only variable references should be returned by reference",
        );
    }

    let closure_expected_return_type = hinted_return.clone().or_else(|| inline_return_type.clone());
    emit_closure_return_issues(
        analyzer,
        analysis_data,
        arrow.span().start.offset,
        arrow.span().end.offset,
        true,
        closure_expected_return_type.as_ref(),
        inferred_return_type.as_ref(),
        context
            .expected_callable_arg_types
            .contains_key(&arrow_offset),
    );

    let return_type = hinted_return
        .or(inline_return_type)
        .or(inferred_return_type);

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: Some(params),
        return_type: return_type.map(Box::new),
        is_pure: Some(arrow_is_pure),
    });

    analysis_data.set_expr_type(pos, expr_type);
}

fn strip_inferred_impure_issues(
    analysis_data: &mut FunctionAnalysisData,
    issue_count_before: usize,
    retain_impure_issues: bool,
) -> bool {
    if analysis_data.issues.len() == issue_count_before {
        return false;
    }

    let new_issues = analysis_data.issues.split_off(issue_count_before);
    let mut filtered = Vec::with_capacity(new_issues.len());
    let mut saw_impure_issue = false;

    for issue in new_issues {
        if is_impure_issue_kind(issue.kind) {
            saw_impure_issue = true;
            if retain_impure_issues {
                filtered.push(issue);
            }
        } else {
            filtered.push(issue);
        }
    }

    analysis_data.issues.extend(filtered);
    saw_impure_issue
}

fn is_impure_issue_kind(kind: IssueKind) -> bool {
    matches!(
        kind,
        IssueKind::ImpureFunctionCall
            | IssueKind::ImpureMethodCall
            | IssueKind::ImpurePropertyAssignment
    )
}

fn closure_body_has_obvious_side_effect_statements(closure: &Closure<'_>) -> bool {
    closure.body.statements.iter().any(|statement| {
        matches!(
            statement,
            Statement::Echo(_)
                | Statement::EchoTag(_)
                | Statement::Unset(_)
                | Statement::Global(_)
                | Statement::Static(_)
        )
    })
}

fn infer_inferred_return_type(
    analysis_data: &FunctionAnalysisData,
    start_index: usize,
) -> Option<TUnion> {
    let new_return_types = &analysis_data.inferred_return_types[start_index..];
    if new_return_types.is_empty() {
        return Some(TUnion::void());
    }

    let mut combined = new_return_types[0].clone();
    for return_type in &new_return_types[1..] {
        combined = combine_union_types(&combined, return_type, false);
    }

    Some(combined)
}

fn infer_yielded_return_type(
    analysis_data: &FunctionAnalysisData,
    start_index: usize,
) -> Option<TUnion> {
    let new_yield_types = &analysis_data.inferred_yield_types[start_index..];
    if new_yield_types.is_empty() {
        return None;
    }

    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for (yield_key_type, yield_value_type) in new_yield_types {
        let this_key_type = yield_key_type.clone().unwrap_or_else(TUnion::int);
        key_type = Some(if let Some(existing) = key_type {
            combine_union_types(&existing, &this_key_type, false)
        } else {
            this_key_type
        });

        value_type = Some(if let Some(existing) = value_type {
            combine_union_types(&existing, yield_value_type, false)
        } else {
            yield_value_type.clone()
        });
    }

    Some(TUnion::new(TAtomic::TIterable {
        key_type: Box::new(key_type.unwrap_or_else(TUnion::array_key)),
        value_type: Box::new(value_type.unwrap_or_else(TUnion::mixed)),
    }))
}

fn apply_inline_callable_param_types<'a, I>(
    analyzer: &StatementsAnalyzer<'_>,
    parameters: I,
    params: &mut [FunctionLikeParameter],
    inline_annotation: &pzoom_code_info::InlineCallableTypeAnnotation,
) where
    I: IntoIterator<Item = &'a MagoParameter<'a>>,
{
    for (index, (param, param_info)) in parameters.into_iter().zip(params.iter_mut()).enumerate() {
        let param_id = analyzer.interner.intern(param.variable.name);
        let by_name = inline_annotation
            .params
            .iter()
            .find(|inline_param| inline_param.param_name == Some(param_id));
        let by_position = inline_annotation
            .params
            .get(index)
            .filter(|inline_param| inline_param.param_name.is_none());

        if let Some(inline_param) = by_name.or(by_position) {
            param_info.param_type = inline_param.param_type.clone();
        }
    }
}

fn apply_expected_callable_param_types<'a, I>(
    parameters: I,
    params: &mut [FunctionLikeParameter],
    expected_callable_type: &TUnion,
) where
    I: IntoIterator<Item = &'a MagoParameter<'a>>,
{
    let expected_param_types = extract_expected_callable_param_types(expected_callable_type);

    for (index, (param, param_info)) in parameters.into_iter().zip(params.iter_mut()).enumerate() {
        if param.hint.is_some() || !param_info.param_type.is_mixed() {
            continue;
        }

        let Some(expected_param_type) = expected_param_types.get(index) else {
            continue;
        };

        if expected_param_type.is_mixed() {
            continue;
        }

        param_info.param_type = expected_param_type.clone();
    }
}

fn extract_expected_callable_param_types(expected_callable_type: &TUnion) -> Vec<TUnion> {
    let mut param_types: Vec<Option<TUnion>> = Vec::new();

    for atomic in &expected_callable_type.types {
        collect_expected_callable_param_types_from_atomic(atomic, &mut param_types);
    }

    param_types
        .into_iter()
        .map(|param_type| param_type.unwrap_or_else(TUnion::mixed))
        .collect()
}

fn collect_expected_callable_param_types_from_atomic(
    atomic: &TAtomic,
    param_types: &mut Vec<Option<TUnion>>,
) {
    match atomic {
        TAtomic::TCallable {
            params: Some(params),
            ..
        }
        | TAtomic::TClosure {
            params: Some(params),
            ..
        } => {
            for (index, param) in params.iter().enumerate() {
                if param_types.len() <= index {
                    param_types.resize_with(index + 1, || None);
                }

                param_types[index] = Some(match &param_types[index] {
                    Some(existing) => combine_union_types(existing, &param.param_type, false),
                    None => param.param_type.clone(),
                });
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested_atomic in &as_type.types {
                collect_expected_callable_param_types_from_atomic(nested_atomic, param_types);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                collect_expected_callable_param_types_from_atomic(nested_atomic, param_types);
            }
        }
        _ => {}
    }
}

/// Extract parameter type information from a list of parameters.
fn extract_param_types<'a, I>(
    analyzer: &StatementsAnalyzer<'_>,
    parameters: I,
    namespace: Option<StrId>,
    self_class: Option<StrId>,
    parent_class: Option<StrId>,
) -> Vec<FunctionLikeParameter>
where
    I: IntoIterator<Item = &'a MagoParameter<'a>>,
{
    parameters
        .into_iter()
        .map(|param| FunctionLikeParameter {
            name: Some(analyzer.interner.intern(param.variable.name)),
            param_type: param
                .hint
                .as_ref()
                .map(|hint| {
                    resolve_hint(
                        hint,
                        analyzer.interner,
                        namespace,
                        self_class,
                        parent_class,
                        None,
                        Some(analyzer.resolved_names),
                    )
                })
                .unwrap_or_else(TUnion::mixed),
            is_optional: param.default_value.is_some(),
            is_variadic: param.ellipsis.is_some(),
            by_ref: param.ampersand.is_some(),
        })
        .collect()
}

fn add_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start: u32,
    end: u32,
    kind: IssueKind,
    message: impl Into<String>,
) {
    let (line, col) = analyzer.get_line_column(start);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        start,
        end,
        line,
        col,
    ));
}

fn strip_this_from_context(analyzer: &StatementsAnalyzer<'_>, context: &mut BlockContext) {
    let this_related_vars: Vec<StrId> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            *var_id == StrId::THIS_VAR
                || analyzer
                    .interner
                    .lookup(*var_id)
                    .as_ref()
                    .starts_with("$this->")
        })
        .collect();

    for var_id in this_related_vars {
        context.locals.remove(&var_id);
        context.assigned_var_ids.remove(&var_id);
        context.possibly_assigned_var_ids.remove(&var_id);
    }
}

fn strip_property_path_assumptions(analyzer: &StatementsAnalyzer<'_>, context: &mut BlockContext) {
    let property_path_vars: Vec<StrId> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| analyzer.interner.lookup(*var_id).as_ref().contains("->"))
        .collect();

    for var_id in property_path_vars {
        context.locals.remove(&var_id);
        context.assigned_var_ids.remove(&var_id);
        context.possibly_assigned_var_ids.remove(&var_id);
        context.class_string_origins.remove(&var_id);
    }

    context.clauses.retain(|clause| {
        !clause.possibilities.keys().any(|key| {
            matches!(key, ClauseKey::Name(name) if name.contains("->"))
        })
    });
}

fn emit_closure_return_issues(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start: u32,
    end: u32,
    closure_has_definite_return: bool,
    expected_return_type: Option<&TUnion>,
    inferred_return_type: Option<&TUnion>,
    has_expected_callable_context: bool,
) {
    if let Some(expected_return_type) = expected_return_type {
        if !closure_has_definite_return
            && !expected_return_type.is_void()
            && !expected_return_type.is_mixed()
            && !expected_return_type.is_nothing()
            && !expected_return_type.is_nullable
            && !union_allows_implicit_yield_return(expected_return_type)
        {
            add_issue(
                analyzer,
                analysis_data,
                start,
                end,
                IssueKind::InvalidReturnType,
                format!(
                    "Not all code paths of closure end in a return statement, expected {}",
                    expected_return_type.get_id(Some(analyzer.interner))
                ),
            );
        }

        if inferred_return_type.is_some_and(union_contains_mixed)
            && !expected_return_type.is_mixed()
            && !expected_return_type.is_void()
        {
            add_issue(
                analyzer,
                analysis_data,
                start,
                end,
                IssueKind::MixedReturnStatement,
                "Could not infer a return type due to mixed return values",
            );
        }

        return;
    }

    if has_expected_callable_context {
        return;
    }

    if inferred_return_type
        .is_some_and(|inferred| !inferred.is_void() && !union_contains_mixed(inferred))
    {
        add_issue(
            analyzer,
            analysis_data,
            start,
            end,
            IssueKind::MissingClosureReturnType,
            "Closure does not have a return type",
        );
    }
}

fn union_contains_mixed(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
}

fn union_allows_implicit_yield_return(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TIterable { .. }
                | TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
        ) || matches!(
            atomic,
            TAtomic::TNamedObject { name, .. }
                if *name == StrId::GENERATOR
                    || *name == StrId::TRAVERSABLE
                    || *name == StrId::ITERATOR
                    || *name == StrId::ITERATOR_AGGREGATE
        )
    })
}

fn is_reference_returnable_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Variable(_)
            | Expression::Access(Access::Property(_))
            | Expression::Access(Access::StaticProperty(_))
    )
}
