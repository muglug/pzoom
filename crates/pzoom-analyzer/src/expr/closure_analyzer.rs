//! Closure and arrow function analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::function_like::arrow_function::ArrowFunction;
use mago_syntax::ast::ast::function_like::closure::Closure;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter as MagoParameter;

use pzoom_code_info::VarName;
use pzoom_code_info::{
    FunctionLikeParameter, Issue, IssueKind, TAtomic, TUnion, VarId, VariableSourceKind,
    combine_union_types,
};
use pzoom_str::StrId;
use pzoom_syntax::resolve_hint;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::function_like_analyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

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
    // The body is a fresh statement scope: expression-position flags from
    // the enclosing expression (e.g. a closure returned or passed as an
    // argument) must not leak into it — Psalm builds a new Context.
    closure_context.inside_return = false;
    closure_context.inside_call = false;
    closure_context.inside_conditional = false;
    closure_context.inside_general_use = false;
    closure_context.inside_throw = false;
    closure_context.inside_isset = false;
    if closure.r#static.is_some() {
        closure_context.strip_this_assumptions();
    }
    closure_context.strip_property_path_assumptions();

    let param_ids: FxHashSet<VarName> = closure
        .parameter_list
        .parameters
        .iter()
        .map(|param| VarName::new(param.variable.name))
        .collect();

    // Handle use() clause for captured variables
    if let Some(ref use_clause) = closure.use_clause {
        for use_var in use_clause.variables.iter() {
            let var_name = use_var.variable.name;
            let var_id = VarName::new(var_name);
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
                closure_context
                    .locals
                    .insert(var_id.clone(), var_type.clone());
                if use_var.ampersand.is_some() {
                    context.mark_external_reference(var_id.clone());
                    closure_context.mark_external_reference(var_id.clone());
                }
                // Psalm also carries property/array paths rooted at the
                // captured var (`preg_match('/^\$name[\[\-]/', $var_id)`), so
                // a narrowing like `$arg->name !== null` made before the
                // closure holds inside it.
                let property_prefix = format!("{}->", var_name);
                let offset_prefix = format!("{}[", var_name);
                for (outer_id, outer_type) in context.locals.iter() {
                    if outer_id.starts_with(property_prefix.as_str())
                        || outer_id.starts_with(offset_prefix.as_str())
                    {
                        closure_context
                            .locals
                            .insert(outer_id.clone(), outer_type.clone());
                    }
                }
            } else if use_var.ampersand.is_some() {
                // Allow recursive self-capture patterns like `$f = function () use (&$f) { ... }`.
                // Psalm leaves the variable typeless; mark the placeholder so
                // checks that skip typeless values (e.g. MixedFunctionCall)
                // can do the same.
                let mut placeholder = TUnion::mixed();
                placeholder.from_undefined_by_ref = true;
                closure_context.locals.insert(var_id.clone(), placeholder);
                context.mark_external_reference(var_id.clone());
                closure_context.mark_external_reference(var_id.clone());
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
                closure_context
                    .locals
                    .insert(var_id.clone(), TUnion::mixed());
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
    // Psalm's FunctionLikeAnalyzer: a closure/arrow-fn parameter with no
    // declared type is MissingClosureParamType — the closure counterpart of
    // MissingParamType. The type counts as provided when the expected-callable
    // *signature* declares it (`callable(E): R` fills the param, so
    // `inferClosureParamTypeFromContext` is clean), but NOT when the body type
    // is merely inferred from an array element behind an untyped `?callable`
    // (Psalm reports it for `array_filter` — see its arrayFilterWithAssert
    // test, which ignores MissingClosureParamType). An inline `@var
    // callable(...)` annotation that declares the signature also suppresses it.
    if inline_callable_annotation.is_none() {
        let signature_param_types = context
            .expected_callable_arg_types
            .get(&closure_offset)
            .map(extract_expected_callable_param_types)
            .unwrap_or_default();
        for (index, param) in closure.parameter_list.parameters.iter().enumerate() {
            // The type counts as provided when the expected callable signature
            // has a param at this index — even `mixed` (Psalm sets
            // `$function_param->type` and so does not report). Only a param the
            // signature does not cover at all stays "missing".
            if param.hint.is_none() && signature_param_types.get(index).is_none() {
                let span = param.variable.span();
                add_issue(
                    analyzer,
                    analysis_data,
                    span.start.offset,
                    span.end.offset,
                    IssueKind::MissingClosureParamType,
                    format!("Parameter {} has no provided type", param.variable.name),
                );
            }
        }
    }

    // Psalm's closure storage keeps only the DECLARED param types — context-
    // seeded types power the body analysis, but an untyped param stays mixed
    // for callable-compatibility comparison (input param type null -> mixed).
    let declared_params = params.clone();
    if let Some(expected_callable_type) = context.expected_callable_arg_types.get(&closure_offset) {
        apply_expected_callable_param_types(
            analyzer,
            &closure.parameter_list.parameters,
            &mut params,
            expected_callable_type,
        );
    }

    // Add parameters to the closure context. Hakana's `functionlike_analyzer`
    // seeds each closure parameter with a `ClosureParam` variable-use source
    // node (by-ref params become `InoutParam` sources).
    for (param_index, (param, param_info)) in closure
        .parameter_list
        .parameters
        .iter()
        .zip(params.iter())
        .enumerate()
    {
        let param_name = param.variable.name;
        let param_id = VarName::new(param_name);
        let mut param_type = param_info.param_type.clone();
        // A variadic param collects its arguments (Psalm wraps in
        // array<array-key, T> since variadics accept named arguments).
        if param_info.is_variadic {
            param_type = TUnion::new(TAtomic::array(TUnion::array_key(), param_type));
        }
        let param_span = param.variable.span();
        let parent_node = crate::data_flow::add_param_dataflow_node(
            &mut analysis_data.data_flow_graph,
            if param_info.by_ref {
                VariableSourceKind::InoutParam
            } else {
                VariableSourceKind::ClosureParam
            },
            VarId(analyzer.interner.intern(&param_id)),
            crate::data_flow::make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            Some(
                &pzoom_code_info::data_flow::node::FunctionLikeIdentifier::Closure(
                    analyzer.file_path,
                    closure.span().start.offset,
                ),
            ),
            param_index,
            Some(&param_info.param_type),
        );
        analysis_data
            .param_sources
            .push(crate::function_analysis_data::ParamSourceInfo {
                node_id: parent_node.id.clone(),
                function_key: closure.span().start.offset,
                param_index,
                is_closure: true,
                reportable: true,
                is_promoted: false,
                by_ref: param_info.by_ref,
                function_end: closure.span().end.offset,
                name: param_name.to_string(),
                span: (param_span.start.offset, param_span.end.offset),
                method_param_meta: None,
            });
        param_type.parent_nodes.push(parent_node);
        let param_var = VarName::new(param_name);
        if param_info.by_ref {
            // Writes to a by-ref param are visible to the caller.
            closure_context.mark_external_reference(param_var.clone());
        }
        closure_context.locals.insert(param_var.clone(), param_type);

        // Parameters are definitely assigned: clear any possibly-assigned
        // demotion inherited from the enclosing scope (both key spellings).
        closure_context.possibly_assigned_var_ids.remove(&param_var);
        closure_context
            .assigned_var_ids
            .entry(param_var)
            .or_insert(1);
        let alternate = if let Some(stripped) = param_name.strip_prefix('$') {
            VarName::new(stripped)
        } else {
            VarName::from(format!("${}", param_name))
        };
        closure_context.possibly_assigned_var_ids.remove(&alternate);
        closure_context
            .assigned_var_ids
            .entry(alternate)
            .or_insert(1);
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
    // Psalm's `potentiallyInferTypesOnClosureFromParentReturnType` also infers
    // the closure's return type from the enclosing function's callable return
    // type (`inferInnerClosureTypeFromParent`): a closure that declares no
    // return type — or a less specific one — adopts the parent callable's.
    let parent_callable_return_type = context
        .returned_closure_parent_return_types
        .get(&closure_offset)
        .and_then(extract_expected_callable_return_type);
    let closure_expected_return_type =
        crate::stmt::return_analyzer::infer_inner_closure_type_from_parent(
            analyzer.codebase,
            hinted_return_type
                .clone()
                .or_else(|| inline_return_type.clone()),
            parent_callable_return_type.as_ref(),
        );
    closure_function_info.return_type = closure_expected_return_type.clone();
    closure_function_info.signature_return_type = closure_expected_return_type.clone();
    closure_function_info.returns_by_ref = closure.ampersand.is_some();

    let mut closure_stmt_analyzer = analyzer.for_nested_function(Some(&closure_function_info));
    closure_stmt_analyzer.inside_closure = true;

    // Psalm models by-ref closure uses by analyzing the body inside a virtual
    // `while` loop (FunctionLikeAnalyzer's $byref_uses block): assignments
    // from "previous invocations" widen the use vars before the real pass, so
    // a narrowing check against the widened type isn't reported as redundant.
    // pzoom approximates the fixed point with one discarded widening pre-pass.
    let byref_use_vars: Vec<VarName> = closure
        .use_clause
        .as_ref()
        .map(|use_clause| {
            use_clause
                .variables
                .iter()
                .filter(|use_var| use_var.ampersand.is_some())
                .map(|use_var| VarName::new(use_var.variable.name))
                .collect()
        })
        .unwrap_or_default();
    if !byref_use_vars.is_empty() && !closure.body.statements.is_empty() {
        let mut scratch_context = closure_context.clone();
        let return_types_mark = analysis_data.inferred_return_types.len();
        let yield_types_mark = analysis_data.inferred_yield_types.len();
        analysis_data.start_recording_issues();
        let _ = stmt_analyzer::analyze_stmts(
            &closure_stmt_analyzer,
            closure.body.statements.as_slice(),
            analysis_data,
            &mut scratch_context,
        );
        let _ = analysis_data.clear_currently_recorded_issues();
        analysis_data.stop_recording_issues();
        analysis_data
            .inferred_return_types
            .truncate(return_types_mark);
        analysis_data
            .inferred_yield_types
            .truncate(yield_types_mark);

        for var_id in &byref_use_vars {
            if let (Some(seed_type), Some(widened_type)) = (
                closure_context.locals.get(var_id),
                scratch_context.locals.get(var_id),
            ) {
                let combined = combine_union_types(seed_type, widened_type, false);
                closure_context.locals.insert(var_id.clone(), combined);
            }
        }
    }

    let issue_marks_before = analysis_data.issue_emission_marks();
    let prev_is_generator = analysis_data.current_function_is_generator;
    let body_contains_yield =
        stmt_analyzer::body_contains_yield(closure.body.statements.as_slice());
    analysis_data.current_function_is_generator = body_contains_yield;
    let saved_var_appearances = std::mem::take(&mut analysis_data.first_var_appearances);
    let _ = stmt_analyzer::analyze_stmts(
        &closure_stmt_analyzer,
        closure.body.statements.as_slice(),
        analysis_data,
        &mut closure_context,
    );
    analysis_data.first_var_appearances = saved_var_appearances;
    analysis_data.current_function_is_generator = prev_is_generator;
    let saw_impure_issue = function_like_analyzer::strip_inferred_impure_issues(
        analysis_data,
        issue_marks_before,
        !infer_purity,
    );
    let has_obvious_side_effect_stmt =
        function_like_analyzer::body_has_obvious_side_effect_statements(
            closure.body.statements.as_slice(),
        );
    let closure_is_pure = !saw_impure_issue
        && !has_obvious_side_effect_stmt
        && (closure_function_info.is_pure || closure_function_info.is_mutation_free);

    let inferred_return_type = if body_contains_yield {
        // A yielding closure returns a Generator (Psalm's ReturnTypeCollector):
        // key/value from the recorded yields (mixed when only bare `yield;`),
        // send mixed, return from the body's return statements.
        Some(infer_generator_return_type(
            analysis_data,
            yield_types_start,
            return_types_start,
        ))
    } else {
        let mut combined = analysis_data.combine_inferred_return_types(return_types_start);
        // A body that always exits (exit/throw on every path, no returns)
        // infers `never`, not void — Psalm's never ⊂ any declared callable
        // return (`set_error_handler(function () { exit(1); })`).
        if combined.is_void() {
            let control_actions = crate::stmt::scope_analyzer::get_control_actions(
                closure.body.statements.as_slice(),
                analysis_data,
                &[],
                true,
            );
            if control_actions.len() == 1
                && control_actions.contains(&crate::stmt::scope_analyzer::ControlAction::End)
            {
                combined = TUnion::nothing();
            }
        }
        Some(combined)
    };

    // Psalm skips MissingClosureReturnType for closures inside calls
    // (ReturnTypeAnalyzer's \$closure_inside_call, which MatchAnalyzer also
    // sets for match arms) unless the inferred return is mixed.
    let has_expected_callable_context = context
        .expected_callable_arg_types
        .contains_key(&closure_offset)
        || (context.inside_call
            && !inferred_return_type
                .as_ref()
                .is_some_and(|inferred| inferred.is_mixed()));

    let closure_body_always_leaves = closure_context.has_returned
        || (!closure.body.statements.is_empty()
            && !crate::stmt::scope_analyzer::get_control_actions(
                closure.body.statements.as_slice(),
                analysis_data,
                &[],
                true,
            )
            .contains(&crate::stmt::scope_analyzer::ControlAction::None));

    emit_closure_return_issues(
        analyzer,
        analysis_data,
        closure.span().start.offset,
        closure.span().end.offset,
        closure_body_always_leaves,
        closure_expected_return_type.as_ref(),
        inferred_return_type.as_ref(),
        has_expected_callable_context,
    );

    // Psalm precedence: a docblock `@return` overrides the native hint; a
    // bare native hint defers to the inferred body type when contained.
    let return_type = if let Some(inline_return_type) = inline_return_type {
        Some(inline_return_type)
    } else if let Some(hinted_return_type) = hinted_return_type {
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
        inferred_return_type
    };

    // The closure's recorded return/yield types are consumed above; drop them
    // so the enclosing function-like's slice (everything since its own start
    // mark) only sees its own returns (Psalm's ReturnTypeCollector never
    // descends into nested function expressions).
    analysis_data
        .inferred_return_types
        .truncate(return_types_start);
    analysis_data
        .inferred_yield_types
        .truncate(yield_types_start);

    let mut expr_type = TUnion::new(TAtomic::TClosure {
        params: Some(declared_params),
        return_type: return_type.map(Box::new),
        is_pure: Some(closure_is_pure),
    });

    add_closure_reference_dataflow(analyzer, analysis_data, &mut expr_type, closure_offset, pos);

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

/// Port of Hakana `closure_analyzer`'s whole-program dataflow: the closure
/// expression's type is parented by a `ReferenceTo(closure)` node fed by the
/// closure's `CallTo` return node, so taints returned by the closure reach
/// uses of the closure value. Function-body graphs get no closure nodes in
/// Hakana, so this is gated to whole-program mode.
fn add_closure_reference_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    closure_type: &mut TUnion,
    closure_offset: u32,
    pos: Pos,
) {
    if !matches!(
        analysis_data.data_flow_graph.kind,
        pzoom_code_info::GraphKind::WholeProgram(_)
    ) {
        return;
    }

    let closure_id =
        pzoom_code_info::FunctionLikeIdentifier::Closure(analyzer.file_path, closure_offset);
    let closure_pos = crate::data_flow::make_data_flow_node_position(analyzer, pos);

    let application_node =
        pzoom_code_info::DataFlowNode::get_for_method_reference(&closure_id, Some(closure_pos));

    let closure_return_node =
        pzoom_code_info::DataFlowNode::get_for_method_return(&closure_id, Some(closure_pos), None);

    analysis_data.data_flow_graph.add_path(
        &closure_return_node.id,
        &application_node.id,
        pzoom_code_info::PathKind::Default,
        vec![],
        vec![],
    );

    analysis_data
        .data_flow_graph
        .add_node(application_node.clone());

    closure_type.parent_nodes = vec![application_node];
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
    // Fresh statement scope (see the closure path above) — except that an
    // arrow body IS its return value, so it analyzes inside a return.
    arrow_context.inside_return = true;
    arrow_context.inside_call = false;
    arrow_context.inside_conditional = false;
    arrow_context.inside_general_use = false;
    arrow_context.inside_throw = false;
    arrow_context.inside_isset = false;
    if arrow.r#static.is_some() {
        arrow_context.strip_this_assumptions();
    }
    arrow_context.strip_property_path_assumptions();

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
    // See the closure path: the TClosure atomic keeps declared param types.
    let declared_params = params.clone();
    if let Some(expected_callable_type) = context.expected_callable_arg_types.get(&arrow_offset) {
        apply_expected_callable_param_types(
            analyzer,
            &arrow.parameter_list.parameters,
            &mut params,
            expected_callable_type,
        );
    }

    // Hakana's `functionlike_analyzer` seeds each closure parameter with a
    // `ClosureParam` variable-use source node (by-ref params → `InoutParam`).
    for (param_index, (param, param_info)) in arrow
        .parameter_list
        .parameters
        .iter()
        .zip(params.iter())
        .enumerate()
    {
        let param_name = param.variable.name;
        let param_id = VarName::new(param_name);
        let mut param_type = param_info.param_type.clone();
        // A variadic param collects its arguments (Psalm wraps in
        // array<array-key, T> since variadics accept named arguments).
        if param_info.is_variadic {
            param_type = TUnion::new(TAtomic::array(TUnion::array_key(), param_type));
        }
        let param_span = param.variable.span();
        let parent_node = crate::data_flow::add_param_dataflow_node(
            &mut analysis_data.data_flow_graph,
            if param_info.by_ref {
                VariableSourceKind::InoutParam
            } else {
                VariableSourceKind::ClosureParam
            },
            VarId(analyzer.interner.intern(&param_id)),
            crate::data_flow::make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            Some(
                &pzoom_code_info::data_flow::node::FunctionLikeIdentifier::Closure(
                    analyzer.file_path,
                    arrow.span().start.offset,
                ),
            ),
            param_index,
            Some(&param_info.param_type),
        );
        analysis_data
            .param_sources
            .push(crate::function_analysis_data::ParamSourceInfo {
                node_id: parent_node.id.clone(),
                function_key: arrow.span().start.offset,
                param_index,
                is_closure: true,
                reportable: true,
                is_promoted: false,
                by_ref: param_info.by_ref,
                function_end: arrow.span().end.offset,
                name: param_name.to_string(),
                span: (param_span.start.offset, param_span.end.offset),
                method_param_meta: None,
            });
        param_type.parent_nodes.push(parent_node);
        let param_var = VarName::new(param_name);
        if param_info.by_ref {
            // Writes to a by-ref param are visible to the caller.
            arrow_context.mark_external_reference(param_var.clone());
        }
        arrow_context.locals.insert(param_var.clone(), param_type);

        // Parameters are definitely assigned: clear any possibly-assigned
        // demotion inherited from the enclosing scope (both key spellings).
        arrow_context.possibly_assigned_var_ids.remove(&param_var);
        arrow_context.assigned_var_ids.entry(param_var).or_insert(1);
        let alternate = if let Some(stripped) = param_name.strip_prefix('$') {
            VarName::new(stripped)
        } else {
            VarName::from(format!("${}", param_name))
        };
        arrow_context.possibly_assigned_var_ids.remove(&alternate);
        arrow_context.assigned_var_ids.entry(alternate).or_insert(1);
    }

    let mut arrow_function_info = analyzer.function_info.cloned().unwrap_or_default();
    let has_explicit_pure_annotation =
        inline_callable_annotation.is_some_and(|annotation| annotation.is_pure);
    let infer_purity = !has_explicit_pure_annotation;
    arrow_function_info.is_pure = has_explicit_pure_annotation || infer_purity;
    arrow_function_info.is_mutation_free = false;

    let mut arrow_expr_analyzer = analyzer.for_nested_function(Some(&arrow_function_info));
    arrow_expr_analyzer.inside_closure = true;

    // Analyze the body expression to infer return type
    let issue_marks_before = analysis_data.issue_emission_marks();
    let body_pos = expression_analyzer::analyze(
        &arrow_expr_analyzer,
        arrow.expression,
        analysis_data,
        &mut arrow_context,
    );
    let saw_impure_issue = function_like_analyzer::strip_inferred_impure_issues(
        analysis_data,
        issue_marks_before,
        !infer_purity,
    );
    let arrow_is_pure =
        !saw_impure_issue && (arrow_function_info.is_pure || arrow_function_info.is_mutation_free);
    // An arrow body IS its return statement, so an inline unnamed `@var`
    // docblock on the body expression overrides the inferred value type —
    // Psalm's ReturnAnalyzer `$var_comment_type` (e.g. `fn(): array =>
    // /** @var string[] */ require $path`).
    let inferred_return_type = crate::stmt::return_analyzer::get_inline_return_annotation_type(
        analyzer,
        arrow.expression,
        None,
    )
    .or_else(|| {
        analysis_data
            .expr_types
            .get(&body_pos)
            .cloned()
            .map(|t| (*t).clone())
    });

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

    if arrow.ampersand.is_some()
        && !crate::stmt::return_analyzer::is_reference_returnable_expression(arrow.expression)
    {
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
            .contains_key(&arrow_offset)
            // Psalm's \$closure_inside_call (also set by MatchAnalyzer for
            // match arms) skips MissingClosureReturnType unless the inferred
            // return is mixed.
            || (context.inside_call
                && !inferred_return_type
                    .as_ref()
                    .is_some_and(|inferred| inferred.is_mixed())),
    );

    // Psalm precedence: a docblock `@return` overrides the native hint; a
    // bare native hint defers to the inferred body type when that's more
    // specific (contained) — `fn(...): array => [...]` keeps the list shape.
    let return_type = if let Some(inline_return_type) = inline_return_type {
        Some(inline_return_type)
    } else if let Some(hinted_return) = hinted_return {
        match inferred_return_type {
            Some(inferred_return_type)
                if union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &inferred_return_type,
                    &hinted_return,
                    false,
                    false,
                    &mut TypeComparisonResult::new(),
                ) =>
            {
                Some(inferred_return_type)
            }
            _ => Some(hinted_return),
        }
    } else {
        inferred_return_type
    };

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: Some(declared_params),
        return_type: return_type.map(Box::new),
        is_pure: Some(arrow_is_pure),
    });

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

fn infer_generator_return_type(
    analysis_data: &FunctionAnalysisData,
    yield_types_start: usize,
    return_types_start: usize,
) -> TUnion {
    let new_yield_types = &analysis_data.inferred_yield_types[yield_types_start..];

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

    let generator_return_type = analysis_data.combine_inferred_return_types(return_types_start);

    TUnion::new(TAtomic::TNamedObject {
        name: StrId::GENERATOR,
        type_params: Some(vec![
            key_type.unwrap_or_else(TUnion::mixed),
            value_type.unwrap_or_else(TUnion::mixed),
            TUnion::mixed(),
            generator_return_type,
        ]),
        is_static: false,
        remapped_params: false,
    })
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
        let param_id = VarName::new(param.variable.name);
        let by_name = inline_annotation.params.iter().find(|inline_param| {
            inline_param
                .param_name
                .is_some_and(|name| analyzer.interner.lookup(name).as_ref() == param_id.as_str())
        });
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
    analyzer: &StatementsAnalyzer<'_>,
    parameters: I,
    params: &mut [FunctionLikeParameter],
    expected_callable_type: &TUnion,
) where
    I: IntoIterator<Item = &'a MagoParameter<'a>>,
{
    let expected_param_types = extract_expected_callable_param_types(expected_callable_type);

    for (index, (param, param_info)) in parameters.into_iter().zip(params.iter_mut()).enumerate() {
        let Some(expected_param_type) = expected_param_types.get(index) else {
            continue;
        };

        if expected_param_type.is_mixed() {
            continue;
        }

        if param.hint.is_some() || !param_info.param_type.is_mixed() {
            // Psalm's handleClosureArg also NARROWS a signature-typed closure
            // param when the expected arg element type is contained by the
            // declared hint (`fn(Atomic $a) => $a->value` over a list of
            // literal atomics analyzes with the literal type). A docblock'd
            // param keeps its type — pzoom applies inline callable
            // annotations before this, leaving the type non-mixed and the
            // containment check to decide.
            let mut comparison_result = crate::type_comparator::TypeComparisonResult::default();
            if !param_info.param_type.is_mixed()
                && !expected_param_type.is_nothing()
                && union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    expected_param_type,
                    &param_info.param_type,
                    false,
                    false,
                    &mut comparison_result,
                )
                && expected_param_type != &param_info.param_type
            {
                param_info.param_type = expected_param_type.clone();
            }
            continue;
        }

        param_info.param_type = expected_param_type.clone();
    }
}

/// The return type of a callable/closure expected type, combined across atomics
/// (mirrors reading `$parent_callable_return_type->return_type` in Psalm's
/// `potentiallyInferTypesOnClosureFromParentReturnType`).
fn extract_expected_callable_return_type(expected_callable_type: &TUnion) -> Option<TUnion> {
    let mut combined: Option<TUnion> = None;
    for atomic in &expected_callable_type.types {
        let atomic_return = match atomic {
            TAtomic::TCallable {
                return_type: Some(return_type),
                ..
            }
            | TAtomic::TClosure {
                return_type: Some(return_type),
                ..
            } => Some((**return_type).clone()),
            TAtomic::TTemplateParam { as_type, .. } => {
                extract_expected_callable_return_type(as_type)
            }
            _ => None,
        };
        if let Some(atomic_return) = atomic_return {
            combined = Some(match combined {
                Some(existing) => combine_union_types(&existing, &atomic_return, false),
                None => atomic_return,
            });
        }
    }
    combined
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
            && !expected_return_type.is_nullable()
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
        atomic.is_array()
            || matches!(atomic, TAtomic::TIterable { .. })
            || matches!(
                atomic,
                TAtomic::TNamedObject { name, .. }
                    if *name == StrId::GENERATOR
                        || *name == StrId::TRAVERSABLE
                        || *name == StrId::ITERATOR
                        || *name == StrId::ITERATOR_AGGREGATE
            )
    })
}
