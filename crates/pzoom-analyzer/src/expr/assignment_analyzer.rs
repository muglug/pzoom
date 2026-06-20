//! Assignment expression analyzer.
//!
//! This module handles various forms of PHP assignments:
//! - Simple variable assignment: $x = value
//! - Property assignment: $obj->prop = value (handled by instance_property_assignment_analyzer)
//! - Static property assignment: Class::$prop = value (handled by static_property_assignment_analyzer)
//! - Array assignment: $arr[key] = value (handled by array_assignment_analyzer)
//! - Destructuring: list($a, $b) = $arr or [$a, $b] = $arr

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::assignment::{Assignment, AssignmentOperator};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_str::StrId;

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, ClauseKey, combine_ored_clauses};
use pzoom_code_info::t_atomic::{ArrayKey, NON_SPECIFIC_LITERAL_STRING_VALUE};
use pzoom_code_info::{
    Assertion, DataFlowNode, GraphKind, Issue, IssueKind, PathKind, TAtomic, TUnion, VarId,
    VariableSourceKind, combine_union_types,
};
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expr::assignment::{
    array_assignment_analyzer, instance_property_assignment_analyzer,
    static_property_assignment_analyzer,
};
use crate::expr::binop::coalesce_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::issue_suppression;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator::is_class_subtype_of;
use crate::type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator};

/// Analyze an assignment expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    assignment: &Assignment<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Check if this is a property assignment - handle specially
    if let Expression::Access(access) = assignment.lhs {
        use mago_syntax::ast::ast::access::Access;

        match access {
            Access::Property(prop_access) => {
                instance_property_assignment_analyzer::analyze(
                    analyzer,
                    prop_access,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                );
                return;
            }
            Access::StaticProperty(static_prop) => {
                static_property_assignment_analyzer::analyze(
                    analyzer,
                    static_prop,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                    // A compound assignment reads the old value.
                    !matches!(assignment.operator, AssignmentOperator::Assign(_)),
                );
                return;
            }
            _ => {}
        }
    }

    // Check if this is an array element assignment
    if let Expression::ArrayAccess(array_access) = assignment.lhs {
        array_assignment_analyzer::analyze(
            analyzer,
            array_access,
            assignment.rhs,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    // Check if this is an array append assignment
    if let Expression::ArrayAppend(array_append) = assignment.lhs {
        array_assignment_analyzer::analyze_append(
            analyzer,
            array_append,
            assignment.rhs,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    if let Some(reference_operand) = get_reference_operand(assignment.rhs) {
        if analyze_reference_assignment(
            analyzer,
            assignment.lhs,
            reference_operand,
            pos,
            analysis_data,
            context,
        ) {
            return;
        }

        if !issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            pos.0,
            "UnsupportedReferenceUsage",
        ) {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnsupportedReferenceUsage,
                "This reference assignment cannot be analyzed",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Psalm (AssignmentAnalyzer): a closure assigned to a variable it by-ref
    // captures (`$f = function () use (&$f) {...}`) pre-declares the variable
    // as Closure so the recursive self-reference isn't mixed inside the body.
    if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
        && let Expression::Closure(closure) = assignment.rhs
        && let Some(use_clause) = &closure.use_clause
        && use_clause
            .variables
            .iter()
            .any(|use_var| use_var.ampersand.is_some() && use_var.variable.name == direct_var.name)
    {
        let var_id = VarName::new(direct_var.name);
        context.locals.insert(
            var_id.clone(),
            TUnion::new(TAtomic::TClosure {
                params: None,
                return_type: None,
                is_pure: None,
            }),
        );
        context.vars_possibly_in_scope.insert(var_id);
    }

    // Analyze the right-hand side first (Psalm sets inside_assignment while
    // analyzing the assigned value — the value is "used" by the assignment).
    let was_inside_assignment = context.inside_assignment;
    context.inside_assignment = true;
    let rhs_pos = expression_analyzer::analyze(analyzer, assignment.rhs, analysis_data, context);
    context.inside_assignment = was_inside_assignment;
    let rhs_type = analysis_data
        .expr_types
        .get(&rhs_pos)
        .cloned()
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Psalm's AssignmentAnalyzer: assigning a literal int to a variable that is
    // a protected loop counter (the for-init/increment var or foreach target of
    // an enclosing loop) invalidates the loop's own conditional —
    // LoopInvalidation. The literal-int guard skips the loop's `$i++` increment
    // (whose value is `int`, not a literal).
    if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
        && matches!(assignment.operator, AssignmentOperator::Assign(_))
        && rhs_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TLiteralInt { .. }))
    {
        let var_name = VarName::new(direct_var.name);
        if analysis_data
            .loop_scopes
            .iter()
            .any(|scope| scope.protected_var_ids.contains(&var_name))
        {
            let span = assignment.lhs.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::LoopInvalidation,
                format!(
                    "Variable {} has already been assigned in a for/foreach loop",
                    direct_var.name
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
    }

    let rhs_type = if matches!(assignment.operator, AssignmentOperator::Concat(_)) {
        let mut concat_type =
            infer_concat_assignment_type(analyzer, assignment.lhs, &rhs_type, context);

        // Hakana rewrites `$a .= $b` to `$a = $a . $b`, where the concat analyzer adds
        // a composition node taking parents from both operands.
        let decision_node =
            DataFlowNode::get_for_composition(make_data_flow_node_position(analyzer, pos));
        analysis_data
            .data_flow_graph
            .add_node(decision_node.clone());

        if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
            && let Some(lhs_type) = context.get_var_type(direct_var.name)
        {
            concat_type.parent_nodes.push(decision_node.clone());

            for old_parent_node in &lhs_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }

        if let Some(rhs_expr_type) = analysis_data.expr_types.get(&rhs_pos).cloned() {
            concat_type.parent_nodes.push(decision_node.clone());

            for old_parent_node in &rhs_expr_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }

        concat_type
    } else if !matches!(assignment.operator, AssignmentOperator::Assign(_)) {
        // Other compound ops (`+=`, `/=`, `??=`, …) read the old value too:
        // Hakana rewrites `$a op= $b` to `$a = $a op $b`, so the old
        // variable's dataflow parents feed the new assignment through a
        // composition node (this is what marks a param used by `$hue /= 360`
        // and earlier `$x = …` assignments used by `$x ??= $y`).
        //
        // `??=` also keeps the *type* of the old value: Psalm desugars it to
        // `$a = $a ?? $b`, whose result is `non_null($a) | $b`. The old value
        // survives when it isn't null, so flags like `reference_free` /
        // `allow_mutations` aren't laundered into a fresh `$b` (e.g. an
        // immutable param assigned via `$a ??= clone $this` stays non-pure-
        // compatible, matching Psalm's readonly diagnostics).
        let mut compound_type = if matches!(assignment.operator, AssignmentOperator::Coalesce(_)) {
            coalesce_assignment_type(assignment.lhs, &rhs_type, context)
        } else {
            rhs_type
        };
        let decision_node =
            DataFlowNode::get_for_composition(make_data_flow_node_position(analyzer, pos));
        analysis_data
            .data_flow_graph
            .add_node(decision_node.clone());

        if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
            && let Some(lhs_type) = context.get_var_type(direct_var.name)
        {
            for old_parent_node in &lhs_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }

        for old_parent_node in &compound_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &old_parent_node.id,
                &decision_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
        compound_type.parent_nodes.push(decision_node);

        compound_type
    } else {
        rhs_type
    };

    // Psalm's AssignmentAnalyzer: assigning a void value to a variable is an
    // AssignmentToVoid ("Cannot assign $a to type void").
    if rhs_type.is_void()
        && let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::AssignmentToVoid,
            format!("Cannot assign {} to type void", direct_var.name),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Psalm's AssignmentAnalyzer: a `never` assigned value means every possible
    // type for the variable was invalidated — likely dead code.
    if !rhs_type.types.is_empty()
        && rhs_type.is_nothing()
        && let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs
    {
        let span = direct_var.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::NoValue,
            "All possible types for this assignment were invalidated - This may be dead code",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    // Psalm's `registerVariable`: remember each variable's first assignment
    // location so an always-exiting guard can later retract a MixedAssignment
    // reported there (IfElseAnalyzer's `IssueBuffer::remove`).
    if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs {
        analysis_data
            .first_var_appearances
            .entry(VarName::new(direct_var.name))
            .or_insert(pos.0);
    }

    emit_mixed_assignment_issue_if_needed(
        analyzer,
        assignment.lhs,
        assignment.rhs,
        &rhs_type,
        pos,
        analysis_data,
    );

    // Handle the left-hand side
    analyze_assignment_lhs(
        analyzer,
        assignment.lhs,
        assignment.rhs,
        &rhs_type,
        pos.0,
        analysis_data,
        context,
    );

    if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs {
        handle_assignment_with_boolean_logic(
            analyzer,
            direct_var.name,
            assignment.lhs,
            assignment.rhs,
            &rhs_type,
            analysis_data,
            context,
        );
    }

    // The assignment expression itself has the type of the RHS
    analysis_data.expr_types.insert(pos, Rc::new(rhs_type));
}

/// Result type of `$a ??= $b`, mirroring Psalm's desugaring to `$a = $a ?? $b`.
/// Psalm computes this by reusing `CoalesceAnalyzer`, so we defer to the very
/// same `non_null($a) | $b` combine the `??` operator uses
/// ([`coalesce_analyzer::combine_coalesce_value_types`]) rather than re-deriving
/// it: the old value survives when it isn't null, so its `reference_free` flag
/// is carried into the result instead of being laundered into a fresh `$b` (an
/// immutable param assigned via `$a ??= clone $this` stays non-pure-compatible,
/// matching Psalm's readonly diagnostics). Only direct variables are handled
/// here — property / array-access targets are routed to their own assignment
/// analyzers before this point — and the dataflow wiring is left to the caller,
/// so the returned type keeps only `$b`'s parent nodes.
fn coalesce_assignment_type(
    lhs: &Expression<'_>,
    rhs_type: &TUnion,
    context: &BlockContext,
) -> TUnion {
    let Expression::Variable(Variable::Direct(direct_var)) = lhs else {
        return rhs_type.clone();
    };
    let Some(old_type) = context.get_var_type(direct_var.name) else {
        return rhs_type.clone();
    };

    let mut combined = coalesce_analyzer::combine_coalesce_value_types(old_type, rhs_type);
    combined.parent_nodes = rhs_type.parent_nodes.clone();
    combined
}

fn infer_concat_assignment_type(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_type: &TUnion,
    context: &BlockContext,
) -> TUnion {
    const MAX_LITERAL_CONCAT_COMBINATIONS: usize = 64;

    let lhs_type = match lhs {
        Expression::Variable(Variable::Direct(direct_var)) => context.get_var_type(direct_var.name),
        _ => None,
    };

    if let (Some(lhs_type), Some(rhs_literals)) =
        (lhs_type, extract_concat_literal_fragments(rhs_type))
        && let Some(lhs_literals) = extract_concat_literal_fragments(lhs_type)
    {
        let combinations = lhs_literals.len() * rhs_literals.len();
        if combinations > 0 && combinations < MAX_LITERAL_CONCAT_COMBINATIONS {
            let mut concatenated_literals = Vec::with_capacity(combinations);

            for lhs_literal in lhs_literals {
                for rhs_literal in &rhs_literals {
                    let combined_literal = format!("{}{}", lhs_literal, rhs_literal);
                    // Too long for a literal type: the whole result degrades,
                    // as in Psalm's ConcatAnalyzer (any string this long is
                    // truthy).
                    if combined_literal.len() >= analyzer.config.max_string_length {
                        return TUnion::new(TAtomic::TTruthyString);
                    }
                    if !concatenated_literals.contains(&combined_literal) {
                        concatenated_literals.push(combined_literal);
                    }
                }
            }

            if !concatenated_literals.is_empty() {
                return TUnion::from_types(
                    concatenated_literals
                        .into_iter()
                        .map(|value| TAtomic::TLiteralString { value })
                        .collect(),
                );
            }
        }
    }

    TUnion::new(TAtomic::TString)
}

fn extract_concat_literal_fragments(union: &TUnion) -> Option<Vec<String>> {
    let mut fragments = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let fragment = match atomic {
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    return None;
                }
                value.clone()
            }
            TAtomic::TLiteralInt { value } => value.to_string(),
            TAtomic::TLiteralFloat { value } => value.to_string(),
            TAtomic::TTrue => "1".to_string(),
            TAtomic::TFalse | TAtomic::TNull => String::new(),
            _ => return None,
        };

        if !fragments.contains(&fragment) {
            fragments.push(fragment);
        }
    }

    if fragments.is_empty() {
        None
    } else {
        Some(fragments)
    }
}

fn emit_mixed_assignment_issue_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !rhs_type.is_mixed() {
        return;
    }

    if matches!(
        rhs_expr.unparenthesized(),
        Expression::Access(Access::ClassConstant(_))
    ) {
        return;
    }

    let Expression::Variable(Variable::Direct(direct_var)) = lhs else {
        return;
    };

    if direct_var.name.starts_with("$_") {
        return;
    }

    let var_id = VarName::new(direct_var.name);
    let has_inline_annotation = get_inline_var_annotation_type(analyzer, pos.0, &var_id)
        .or_else(|| {
            analysis_data.current_stmt_start.and_then(|stmt_start| {
                get_inline_var_annotation_type(analyzer, stmt_start, &var_id)
            })
        })
        .is_some();

    if has_inline_annotation {
        return;
    }

    let issue_offset = analysis_data.current_stmt_start.unwrap_or(pos.0);
    if issue_suppression::is_issue_suppressed_at(
        analyzer,
        analysis_data,
        issue_offset,
        "MixedAssignment",
    ) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);
    let origin_secondary =
        crate::data_flow::mixed_origin_secondary(analyzer, analysis_data, rhs_type, pos.0);
    analysis_data.add_issue(
        Issue::new(
            IssueKind::MixedAssignment,
            format!(
                "Unable to determine the type that {} is being assigned to",
                direct_var.name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        )
        .with_secondary_opt(origin_secondary),
    );
}

/// Analyze the left-hand side of an assignment and set variable types.
fn analyze_assignment_lhs(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match lhs {
        Expression::Variable(var) => {
            if let Variable::Indirect(indirect) = var {
                // `${$name} = $value`: the destination is dynamic, so both
                // the name expression and the assigned value escape tracking
                // (general use; Hakana treats non-Lvar roots as dead-end
                // usage).
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;
                let _ = expression_analyzer::analyze(
                    analyzer,
                    indirect.expression,
                    analysis_data,
                    context,
                );
                context.inside_general_use = was_inside_general_use;

                if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
                    && !rhs_type.parent_nodes.is_empty()
                {
                    let span = lhs.span();
                    let escape_sink =
                        DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(
                            analyzer,
                            (span.start.offset, span.end.offset),
                        ));
                    for parent_node in &rhs_type.parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            &parent_node.id,
                            &escape_sink.id,
                            PathKind::Default,
                            vec![],
                            vec![],
                        );
                    }
                    analysis_data.data_flow_graph.add_node(escape_sink);
                }
            }
            if let Variable::Direct(direct) = var {
                let var_name = direct.name;

                // Intern the variable name
                let var_id = VarName::new(var_name);

                // Mirrors Psalm `AssignmentAnalyzer`: assigning to a by-reference variable
                // mutates the caller's scope, so it is impure in a mutation-free context
                // (Psalm gates on `$context->mutation_free` and detects the `by_ref` flag
                // on the in-scope variable; pzoom approximates that via the by-ref params).
                let assigns_by_ref = analyzer.function_info.is_some_and(|info| {
                    info.params.iter().any(|param| {
                        analyzer.interner.lookup(param.name).as_ref() == var_id.as_str()
                            && param.by_ref
                    })
                });
                if assigns_by_ref
                    && crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer)
                {
                    let (line, col) = analyzer.get_line_column(assignment_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ImpureByReferenceAssignment,
                        format!(
                            "Variable {} cannot be assigned to as it is passed by reference",
                            var_name
                        ),
                        analyzer.file_path,
                        assignment_offset,
                        assignment_offset.saturating_add(1),
                        line,
                        col,
                    ));
                }

                if var_id == "$this" && context.get_var_type("$this").is_none() {
                    if !issue_suppression::is_issue_suppressed_at(
                        analyzer,
                        analysis_data,
                        assignment_offset,
                        "InvalidScope",
                    ) {
                        let (line, col) = analyzer.get_line_column(assignment_offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidScope,
                            "Invalid assignment to $this in a non-class context",
                            analyzer.file_path,
                            assignment_offset,
                            assignment_offset.saturating_add(1),
                            line,
                            col,
                        ));
                    }
                    return;
                }

                if context.has_confusing_reference(&var_id) {
                    let (line, col) = analyzer.get_line_column(assignment_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ReferenceReusedFromConfusingScope,
                        format!(
                            "${} may be a reference from a previous conditional/loop scope",
                            var_name.trim_start_matches('$')
                        ),
                        analyzer.file_path,
                        assignment_offset,
                        assignment_offset.saturating_add(1),
                        line,
                        col,
                    ));
                }

                let inline_annotation_type = get_inline_var_annotation_type(
                    analyzer,
                    assignment_offset,
                    &var_id,
                )
                .or_else(|| {
                    analysis_data.current_stmt_start.and_then(|stmt_start| {
                        // A nameless statement-level @var binds to the
                        // statement's top-level assignment only —
                        // Psalm never applies it to an assignment
                        // nested in the rhs (`$outer = ($inner = ...)`).
                        if assignment_offset == stmt_start {
                            get_inline_var_annotation_type(analyzer, stmt_start, &var_id)
                        } else {
                            get_named_inline_var_annotation_type(analyzer, stmt_start, &var_id)
                        }
                    })
                });
                if let Some(annotation_type) = &inline_annotation_type {
                    crate::expr::variable_fetch_analyzer::emit_undefined_docblock_classes_in_annotation(
                        analyzer,
                        annotation_type,
                        (assignment_offset, assignment_offset.saturating_add(1)),
                        analysis_data,
                    );
                    // A generic class-like in the `@var` type must supply the
                    // right number of params (`@var C<int, int>` on a one-param
                    // `C` is TooManyTemplateParams) — Psalm validates var
                    // comments the same way it validates class-member docblocks.
                    crate::stmt::class_analyzer::check_docblock_generic_param_counts(
                        analyzer,
                        annotation_type,
                        assignment_offset,
                        analysis_data,
                    );

                    // Psalm (find_unused_variables): a @var annotation whose
                    // type matches the inferred assigned type exactly is
                    // unnecessary.
                    if analyzer.config.report_unused
                        && !annotation_type.is_mixed()
                        && annotation_type.get_id(Some(analyzer.interner))
                            == rhs_type.get_id(Some(analyzer.interner))
                    {
                        let (line, col) = analyzer.get_line_column(assignment_offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::UnnecessaryVarAnnotation,
                            format!(
                                "The @var {} annotation for {} is unnecessary",
                                annotation_type.get_id(Some(analyzer.interner)),
                                direct.name
                            ),
                            analyzer.file_path,
                            assignment_offset,
                            assignment_offset.saturating_add(1),
                            line,
                            col,
                        ));
                    }
                }
                let mut assigned_type = match inline_annotation_type {
                    Some(mut annotation_type) => {
                        // A `@var` annotation overrides the inferred type but
                        // not the value's dataflow (Psalm keeps the assigned
                        // expression's parent nodes — `@var array $unsafe`
                        // over `$_GET['unsafe']` still carries taint).
                        annotation_type.parent_nodes = rhs_type.parent_nodes.clone();
                        annotation_type
                    }
                    None => rhs_type.clone(),
                };

                emit_reference_constraint_issue_if_needed(
                    analyzer,
                    context,
                    var_id.clone(),
                    &assigned_type,
                    assignment_offset,
                    analysis_data,
                );

                // Hakana `analyze_assignment_to_variable`: in function-body mode the
                // assignment is a variable-use source node (feeding unused-variable
                // analysis); in whole-program (taint) mode it is a plain lvar vertex.
                // Hakana's `pure`/`has_awaitable`/`has_await_call`/`from_loop_init`
                // inputs have no pzoom equivalents yet, so they are `false` here.
                let direct_span = direct.span();
                let var_expr_pos = make_data_flow_node_position(
                    analyzer,
                    (direct_span.start.offset, direct_span.end.offset),
                );
                let has_parent_nodes = !assigned_type.parent_nodes.is_empty();
                let assignment_node =
                    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
                        DataFlowNode::get_for_variable_source(
                            // Assigning through a by-ref closure use writes to
                            // the enclosing scope (Psalm skips `byref_uses` in
                            // checkUnreferencedVars); InoutArg sources are not
                            // reported as unused variables.
                            if context.references_to_external_scope.contains(&var_id)
                                || context.static_var_ids.contains(&var_id)
                            {
                                VariableSourceKind::InoutArg
                            } else {
                                VariableSourceKind::Default
                            },
                            VarId(analyzer.interner.intern(var_name)),
                            var_expr_pos,
                            false,
                            has_parent_nodes,
                            false,
                            false,
                            false,
                        )
                    } else {
                        DataFlowNode::get_for_lvar(
                            VarId(analyzer.interner.intern(var_name)),
                            var_expr_pos,
                        )
                    };
                analysis_data
                    .data_flow_graph
                    .add_node(assignment_node.clone());
                if has_parent_nodes {
                    // Psalm `AssignmentAnalyzer::taintAssignment`: a
                    // `@psalm-taint-escape <kind>` docblock on the assignment
                    // statement removes those taints from the rhs→lhs edge.
                    let removed_taints = if matches!(
                        analysis_data.data_flow_graph.kind,
                        GraphKind::WholeProgram(_)
                    ) {
                        assignment_docblock_removed_taints(analyzer, direct_span.start.offset)
                    } else {
                        vec![]
                    };

                    for parent_node in &assigned_type.parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            &parent_node.id,
                            &assignment_node.id,
                            PathKind::Default,
                            vec![],
                            removed_taints.clone(),
                        );
                    }
                }
                // Writing through an external reference (global import, by-ref
                // use/param, static var) or a local reference binding
                // consumes the binding: the write only lands where it does
                // because of it (Psalm's
                // referenceAssignmentToNonReferenceCountsAsUse).
                if (context.references_to_external_scope.contains(&var_id)
                    || context.references_in_scope.contains_key(&var_id))
                    && let Some(previous_type) = context.get_var_type(&var_id)
                    && !previous_type.parent_nodes.is_empty()
                {
                    let write_sink = DataFlowNode::get_for_unlabelled_sink(var_expr_pos);
                    add_default_dataflow_paths(
                        &mut analysis_data.data_flow_graph,
                        &previous_type.parent_nodes,
                        &write_sink,
                    );
                    analysis_data.data_flow_graph.add_node(write_sink);
                }
                // Psalm marks an external-scope reference as used when a value
                // is assigned to it (AssignmentAnalyzer's `variable-use` path):
                // the written value escapes through the reference, so the rhs
                // chain (`$new = …; $type = $new;` under `foreach (… as &$type)`)
                // counts as used.
                if has_parent_nodes && context.references_to_external_scope.contains(&var_id) {
                    let escape_sink = DataFlowNode::get_for_unlabelled_sink(var_expr_pos);
                    analysis_data.data_flow_graph.add_path(
                        &assignment_node.id,
                        &escape_sink.id,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                    analysis_data.data_flow_graph.add_node(escape_sink);
                }
                assigned_type.parent_nodes = vec![assignment_node];

                // Writing through an external reference (global import, by-ref
                // use/param, static var) consumes the imported binding: link
                // the previous parents to the write node so the declaration
                // counts as used.

                // Psalm (AssignmentAnalyzer): inside a try, keep the previous
                // assignment's parents too — an exception can interrupt at any
                // point, so a later use also uses every earlier assignment.
                if context.inside_try
                    && let Some(previous_type) = context.get_var_type(&var_id)
                {
                    for parent_node in &previous_type.parent_nodes {
                        if !assigned_type.parent_nodes.contains(parent_node) {
                            assigned_type.parent_nodes.push(parent_node.clone());
                        }
                    }
                }

                // Set the variable's type in context (this also tracks assignment)
                let var_existed = context.locals.contains_key(&var_id);
                context.set_var_type(var_id.clone(), assigned_type);
                clear_dependent_property_types(context, var_name);
                clear_array_path_types_for_base_var(context, var_name);
                clear_dependent_array_access_types(context, var_name);
                context.invalidate_dependent_types(&var_id);
                // Psalm reaches removeVarFromConflictingClauses (which seeds
                // parent_remove_vars) only for re-assignments; a first
                // assignment keeps dependent clauses like `$flag = $c !== null`.
                if var_existed {
                    remove_var_clauses_from_context(context, var_name);
                } else {
                    context.remove_var_name_clauses(var_name);
                }
            }
        }
        Expression::Access(access) => {
            use mago_syntax::ast::ast::access::Access;

            match access {
                Access::Property(prop_access) => {
                    // Top-level property assignments are handled before this function is called.
                    // We still need this branch for destructuring assignments, e.g.
                    // list($this->a, $this->b) = ["a", "b"];
                    let span = prop_access.span();
                    instance_property_assignment_analyzer::analyze_with_known_type(
                        analyzer,
                        prop_access,
                        rhs_type.clone(),
                        (span.start.offset, span.end.offset),
                        analysis_data,
                        context,
                    );
                }
                Access::NullSafeProperty(_) | Access::StaticProperty(_) => {
                    // Destructuring into nullsafe/static properties is uncommon and currently
                    // not modeled with per-element value expressions.
                }
                Access::ClassConstant(_) => {
                    // Cannot assign to class constants - this would be a parse error
                    // The PHP parser would reject this before we get here
                }
            }
        }
        Expression::List(list) => {
            // list() assignment - destructure RHS by offset/key, matching Psalm/Hakana behavior.
            for (offset, element) in list.elements.iter().enumerate() {
                analyze_destructuring_element(
                    analyzer,
                    element,
                    offset,
                    rhs_expr,
                    rhs_type,
                    assignment_offset,
                    analysis_data,
                    context,
                );
            }
        }
        Expression::Array(array) => {
            // Short destructuring syntax: [$a, $b] = $arr
            for (offset, element) in array.elements.iter().enumerate() {
                analyze_destructuring_element(
                    analyzer,
                    element,
                    offset,
                    rhs_expr,
                    rhs_type,
                    assignment_offset,
                    analysis_data,
                    context,
                );
            }
        }
        Expression::ArrayAccess(access) => {
            // A top-level `$arr[key] = value` is dispatched to
            // array_assignment_analyzer from expression_analyzer before this
            // function runs; reaching here means a destructuring target like
            // `list($a["foo"]) = $parts`, whose element type is already known.
            let span = access.span();
            array_assignment_analyzer::analyze_with_known_type(
                analyzer,
                access,
                rhs_expr,
                rhs_type.clone(),
                (span.start.offset, span.end.offset),
                analysis_data,
                context,
            );
        }
        Expression::ArrayAppend(_) => {
            // Array append - $arr[] = value
            // Handled by array_assignment_analyzer when dispatched from expression_analyzer
        }
        _ => {
            // Other expressions on LHS (invalid in most cases)
        }
    }
}

fn emit_reference_constraint_issue_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    var_id: VarName,
    assigned_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(constraints) = context.get_reference_constraints(var_id.clone()) else {
        return;
    };

    if constraints.is_empty() {
        return;
    }

    if reference_constraints_conflict(analyzer, constraints) {
        let (line, col) = analyzer.get_line_column(assignment_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::ConflictingReferenceConstraint,
            format!(
                "${} is constrained by incompatible reference types",
                var_id.trim_start_matches('$')
            ),
            analyzer.file_path,
            assignment_offset,
            assignment_offset.saturating_add(1),
            line,
            col,
        ));
        return;
    }

    let violates_constraint = constraints.iter().any(|constraint| {
        let mut comparison = TypeComparisonResult::new();
        !union_type_comparator::is_contained_by(
            analyzer.codebase,
            assigned_type,
            constraint,
            false,
            false,
            &mut comparison,
        )
    });

    if !violates_constraint {
        return;
    }

    let (line, col) = analyzer.get_line_column(assignment_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::ReferenceConstraintViolation,
        format!(
            "${} violates a by-reference type constraint",
            var_id.trim_start_matches('$')
        ),
        analyzer.file_path,
        assignment_offset,
        assignment_offset.saturating_add(1),
        line,
        col,
    ));
}

fn reference_constraints_conflict(
    analyzer: &StatementsAnalyzer<'_>,
    constraints: &[TUnion],
) -> bool {
    for i in 0..constraints.len() {
        for j in (i + 1)..constraints.len() {
            let left = &constraints[i];
            let right = &constraints[j];
            let overlaps =
                union_type_comparator::can_be_contained_by(analyzer.codebase, left, right)
                    || union_type_comparator::can_be_contained_by(analyzer.codebase, right, left);
            if !overlaps {
                return true;
            }
        }
    }

    false
}

pub(crate) fn clear_dependent_property_types(context: &mut BlockContext, var_name: &str) {
    let property_prefix = format!("{var_name}->");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.starts_with(&property_prefix))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

pub(crate) fn clear_dependent_array_access_types(context: &mut BlockContext, var_name: &str) {
    let key_fragment = format!("[{var_name}]");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.contains(&key_fragment))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

pub(crate) fn clear_array_path_types_for_base_var(context: &mut BlockContext, var_name: &str) {
    let prefix = format!("{var_name}[");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.starts_with(&prefix))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

#[derive(Clone)]
enum DestructuringLookupKey {
    Int(i64),
    String(String),
    Unknown,
}

fn analyze_destructuring_element(
    analyzer: &StatementsAnalyzer<'_>,
    element: &ArrayElement<'_>,
    offset: usize,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let (target_expr, lookup_key) = match element {
        ArrayElement::Missing(_) => return,
        ArrayElement::Variadic(_) => return,
        ArrayElement::Value(value_element) => (
            value_element.value,
            DestructuringLookupKey::Int(offset as i64),
        ),
        ArrayElement::KeyValue(kv) => (
            kv.value,
            extract_destructuring_key(kv.key).unwrap_or(DestructuringLookupKey::Unknown),
        ),
    };

    if !rhs_can_be_destructured(analyzer, rhs_type) {
        let (line, col) = analyzer.get_line_column(assignment_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArrayOffset,
            "Cannot destructure non-array value",
            analyzer.file_path,
            assignment_offset,
            assignment_offset.saturating_add(1),
            line,
            col,
        ));
    }

    // Psalm routes each destructuring element through ArrayFetchAnalyzer, so
    // a nullable source reports PossiblyNullArrayAccess per element — except
    // positional items over a list shape, which Psalm reads directly off the
    // shape's properties (the elements just gain |null instead).
    let positional_over_list_shape = matches!(lookup_key, DestructuringLookupKey::Int(_))
        && matches!(element, ArrayElement::Value(_))
        && rhs_type.types.iter().any(|atomic| {
            // A list shape: a list with known entries (old `TKeyedArray`
            // with `is_list`); a generic `list<V>` has no known entries.
            matches!(
                atomic,
                TAtomic::TArray { is_list: true, known_values, .. }
                    if !known_values.is_empty()
            )
        });
    if rhs_type.is_nullable() && !rhs_type.ignore_nullable_issues && !positional_over_list_shape {
        let (line, col) = analyzer.get_line_column(assignment_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullArrayAccess,
            "Cannot access array value on possibly null variable",
            analyzer.file_path,
            assignment_offset,
            assignment_offset.saturating_add(1),
            line,
            col,
        ));
    }

    // An optional shape property reports PossiblyUndefinedArrayOffset
    // (Psalm's list-assignment handling, which then clears the flag).
    if let Some(array_key) = lookup_key_to_array_key(&lookup_key) {
        let optional_property_hit = rhs_type.types.iter().any(|atomic| match atomic {
            // A shape with no typed fallback (old `TKeyedArray` with
            // `fallback_value_type: None`) whose named entry is optional.
            TAtomic::TArray {
                known_values,
                params: None,
                ..
            } => known_values
                .get(&array_key)
                .is_some_and(|(possibly_undefined, _)| *possibly_undefined),
            _ => false,
        });
        if optional_property_hit {
            let (line, col) = analyzer.get_line_column(assignment_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyUndefinedArrayOffset,
                "Possibly undefined array key",
                analyzer.file_path,
                assignment_offset,
                assignment_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }

    // ...and a literal offset every sealed shape member lacks reports
    // InvalidArrayOffset (`[$w, $h, $d] = size()` over `array{int, int}`).
    if let Some(array_key) = lookup_key_to_array_key(&lookup_key) {
        let mut saw_sealed_shape = false;
        let mut offset_can_exist = false;
        for atomic in &rhs_type.types {
            match atomic {
                // A generic array/list (unsealed, or with a typed fallback) can
                // always hold the offset; only a sealed shape lacking the key
                // proves it absent. A generic `array<K,V>`/`list<V>` is unsealed,
                // so `!is_sealed` keeps `offset_can_exist` true as before.
                // TODO(unify-array): the empty array `[]` is now `empty_array()`
                // (sealed, no params, no entries) and so sets `saw_sealed_shape`,
                // where the old generic `TArray{nothing,nothing}` set
                // `offset_can_exist`; `[$a] = []` now reports the missing offset.
                TAtomic::TArray {
                    known_values,
                    params,
                    is_sealed,
                    ..
                } => {
                    if known_values.contains_key(&array_key) || !*is_sealed || params.is_some() {
                        offset_can_exist = true;
                    } else {
                        saw_sealed_shape = true;
                    }
                }
                TAtomic::TNull | TAtomic::TFalse => {}
                _ => offset_can_exist = true,
            }
        }
        if saw_sealed_shape && !offset_can_exist {
            let (line, col) = analyzer.get_line_column(assignment_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!(
                    "Cannot access value on variable of type {} using offset {:?}",
                    rhs_type.get_id(Some(analyzer.interner)),
                    array_key
                ),
                analyzer.file_path,
                assignment_offset,
                assignment_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }

    // Destructuring through a type variable (e.g. `new ArrayObject([...])`
    // whose constructor templates are still unresolved) reads its accumulated
    // lower bounds — the same resolution array_fetch_analyzer applies before
    // element lookup.
    let resolved_rhs_type = crate::template::resolve_type_variables_in_union_deep(
        rhs_type,
        &analysis_data.type_variable_bounds,
    );
    let mut target_type = infer_destructured_value_type(analyzer, &resolved_rhs_type, &lookup_key);

    // Psalm's AssignmentAnalyzer widens a destructured target with `null` when
    // the assignment is under the `@` error-suppression operator and the offset
    // isn't guaranteed to be set: every element past the first (`offset > 0`),
    // plus the first element when the source array can be empty. This is what
    // leaves `$b` as `string|null` in `@[$a, $b] = explode(...)`, where the
    // source is `non-empty-list<string>` (offset 0 is guaranteed, offset 1 not).
    // Psalm gates this on `$list_var_id`, which is empty for a nested list/array
    // target (`@list($a, list($b, $c))`), so those are left to recurse untouched.
    let target_is_nested_destructure = matches!(
        target_expr.unparenthesized(),
        Expression::List(_) | Expression::Array(_)
    );
    if context.error_suppressing
        && !target_is_nested_destructure
        && (offset > 0 || rhs_can_be_empty(&resolved_rhs_type))
        && !target_type.is_nullable()
    {
        target_type = combine_union_types(&target_type, &TUnion::new(TAtomic::TNull), false);
    }

    // Hakana's list assignment connects the source array's parents to each
    // destructured value via `array_fetch_analyzer::add_array_fetch_dataflow`.
    let keyed_array_var_id =
        expression_identifier::get_expression_var_key(rhs_expr).and_then(|source_expr_id| {
            match &lookup_key {
                DestructuringLookupKey::Int(value) => {
                    Some(format!("{}['{}']", source_expr_id, value))
                }
                DestructuringLookupKey::String(value) => {
                    Some(format!("{}['{}']", source_expr_id, value))
                }
                DestructuringLookupKey::Unknown => None,
            }
        });
    let mut destructure_key_type = match &lookup_key {
        DestructuringLookupKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
        DestructuringLookupKey::String(value) => TUnion::new(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        DestructuringLookupKey::Unknown => TUnion::array_key(),
    };
    let rhs_span = rhs_expr.span();
    crate::expr::fetch::array_fetch_analyzer::add_array_fetch_dataflow(
        analyzer,
        (rhs_span.start.offset, rhs_span.end.offset),
        analysis_data,
        keyed_array_var_id,
        &mut target_type,
        &mut destructure_key_type,
    );

    analyze_assignment_lhs(
        analyzer,
        target_expr,
        rhs_expr,
        &target_type,
        assignment_offset,
        analysis_data,
        context,
    );
}

/// Psalm's list-destructuring tracks `$can_be_empty`, cleared once an offset is
/// guaranteed present. Under `@`, a still-possibly-empty source widens its first
/// target with `null`. Returns `false` only when the source guarantees an
/// element at offset 0 — a non-empty array/list, or a shape with a required `0`
/// entry — matching Psalm's `can_be_empty = !TNonEmptyArray` / property check.
fn rhs_can_be_empty(rhs_type: &TUnion) -> bool {
    !rhs_type.types.iter().any(|atomic| match atomic {
        TAtomic::TArray {
            is_nonempty,
            known_values,
            ..
        } => {
            *is_nonempty
                || known_values
                    .get(&ArrayKey::Int(0))
                    .is_some_and(|(possibly_undefined, _)| !*possibly_undefined)
        }
        _ => false,
    })
}

fn rhs_can_be_destructured(analyzer: &StatementsAnalyzer<'_>, rhs_type: &TUnion) -> bool {
    let array_access_id = StrId::ARRAY_ACCESS;

    rhs_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. } | TAtomic::TMixed | TAtomic::TNonEmptyMixed
        ) || matches!(atomic, TAtomic::TNamedObject { name, .. } if is_class_subtype_of(*name, array_access_id, analyzer.codebase))
    })
}

fn extract_destructuring_key(expr: &Expression<'_>) -> Option<DestructuringLookupKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .map(|value| DestructuringLookupKey::Int(value as i64)),
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| DestructuringLookupKey::String(value.to_string())),
        _ => None,
    }
}

fn infer_destructured_value_type(
    analyzer: &StatementsAnalyzer<'_>,
    rhs_type: &TUnion,
    lookup_key: &DestructuringLookupKey,
) -> TUnion {
    let mut inferred_type: Option<TUnion> = None;
    let mut saw_destructurable_type = false;
    let array_access_id = StrId::ARRAY_ACCESS;

    for atomic in &rhs_type.types {
        match atomic {
            // Generic array/list (no known entries): the element type is the
            // fallback `params` value (a fallback-less empty array has none).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } if known_values.is_empty() => {
                saw_destructurable_type = true;
                let value_type = params
                    .as_deref()
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(TUnion::nothing);
                add_inferred_union(&mut inferred_type, &value_type);
            }
            // Shape (known entries): read the named entry, falling back to the
            // typed fallback `params` value, then to every known value.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                saw_destructurable_type = true;
                let fallback_value_type = params.as_deref().map(|(_, v)| v);
                if let Some(array_key) = lookup_key_to_array_key(lookup_key) {
                    if let Some((_, property_type)) = known_values.get(&array_key) {
                        add_inferred_union(&mut inferred_type, property_type);
                    } else if let Some(fallback_value_type) = fallback_value_type {
                        add_inferred_union(&mut inferred_type, fallback_value_type);
                    }
                } else if let Some(fallback_value_type) = fallback_value_type {
                    add_inferred_union(&mut inferred_type, fallback_value_type);
                } else {
                    for (_, property_type) in known_values.values() {
                        add_inferred_union(&mut inferred_type, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                return TUnion::mixed();
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if is_class_subtype_of(*name, array_access_id, analyzer.codebase) {
                    saw_destructurable_type = true;
                    // Psalm's AssignmentAnalyzer destructuring: an
                    // ArrayAccess-interface rhs resolves each element through
                    // ForeachAnalyzer::getKeyValueParamsForTraversableObject
                    // (the Traversable<TKey, TValue> binding); only a class
                    // with no resolvable binding stays mixed.
                    let element_type = crate::stmt::foreach_analyzer::traversable_extended_param(
                        analyzer,
                        *name,
                        type_params.as_ref(),
                        "TValue",
                    )
                    .unwrap_or_else(TUnion::mixed);
                    add_inferred_union(&mut inferred_type, &element_type);
                }
            }
            // Destructuring a null (or false) half of the rhs union yields
            // null elements — Psalm adds `|null` to each target without an
            // issue, so `[$a, $b] = maybeReturnsShape()` keeps the targets
            // nullable when the shape is nullable.
            TAtomic::TNull | TAtomic::TFalse => {
                add_inferred_union(&mut inferred_type, &TUnion::new(TAtomic::TNull));
            }
            _ => {}
        }
    }

    if let Some(inferred_type) = inferred_type {
        inferred_type
    } else if saw_destructurable_type {
        TUnion::mixed()
    } else {
        TUnion::mixed()
    }
}

fn lookup_key_to_array_key(key: &DestructuringLookupKey) -> Option<ArrayKey> {
    match key {
        DestructuringLookupKey::Int(value) => Some(ArrayKey::Int(*value)),
        DestructuringLookupKey::String(value) => Some(ArrayKey::String(value.clone())),
        DestructuringLookupKey::Unknown => None,
    }
}

fn add_inferred_union(target: &mut Option<TUnion>, next: &TUnion) {
    if let Some(existing) = target {
        *existing = combine_union_types(existing, next, false);
    } else {
        *target = Some(next.clone());
    }
}

fn get_reference_operand<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return None;
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Reference(_)) {
        return None;
    }

    Some(unary.operand)
}

fn analyze_reference_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    lhs_expr: &Expression<'_>,
    rhs_operand: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> bool {
    let Some(lhs_key) = expression_identifier::get_expression_var_key(lhs_expr) else {
        return false;
    };
    let Some(rhs_key) = expression_identifier::get_expression_var_key(rhs_operand) else {
        return false;
    };

    // A reference to an array offset whose index is itself an array/property
    // fetch cannot be tracked (Psalm's UnsupportedReferenceUsage).
    if let Expression::ArrayAccess(array_access) = rhs_operand.unparenthesized()
        && matches!(
            array_access.index.unparenthesized(),
            Expression::ArrayAccess(_) | Expression::Access(_)
        )
    {
        return false;
    }

    let lhs_var_id = lhs_key.clone();
    let rhs_var_id = rhs_key.clone();

    if lhs_var_id == "$this" && context.get_var_type("$this").is_none() {
        if !issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            pos.0,
            "InvalidScope",
        ) {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidScope,
                "Invalid assignment to $this in a non-class context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
        analysis_data
            .expr_types
            .insert(pos, Rc::new(TUnion::mixed()));
        return true;
    }

    if let Expression::Variable(Variable::Direct(_)) = rhs_operand.unparenthesized()
        && context.get_var_type(&rhs_var_id).is_none()
    {
        // Psalm initializes newly created references to null.
        context.set_var_type_direct(rhs_var_id.clone(), TUnion::null());
    }

    let rhs_pos = expression_analyzer::analyze(analyzer, rhs_operand, analysis_data, context);
    let rhs_type = analysis_data
        .expr_types
        .get(&rhs_pos)
        .cloned()
        .map(|t| (*t).clone())
        .or_else(|| context.get_var_type(&rhs_var_id).cloned())
        .unwrap_or_else(TUnion::mixed);

    if has_unnamed_inline_var_annotation(analyzer, pos.0)
        || analysis_data
            .current_stmt_start
            .is_some_and(|start| has_unnamed_inline_var_annotation(analyzer, start))
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidDocblock,
            "Docblock type cannot be used for reference assignment",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let rhs_is_external = rhs_key.contains('[') || rhs_key.contains("->") || rhs_key.contains("::");

    // A reference whose target is an offset/property OF the source variable
    // itself (`$arr = &$arr[$key]`) cannot be soundly aliased: a later narrowing
    // of `$arr` (e.g. `isset($arr[$key])` proving it a non-empty array) would
    // propagate back into `$arr[$key]` through the reference cluster and wrongly
    // contradict a sibling `!is_array($arr[$key])`. Bind `$arr` to the offset's
    // current value type without a tracked alias, so the two no longer share a
    // type slot across loop iterations.
    let rhs_targets_lhs = rhs_key
        .strip_prefix(lhs_var_id.as_str())
        .is_some_and(|rest| rest.starts_with('[') || rest.starts_with("->"));
    if rhs_targets_lhs {
        context.set_var_type_direct(lhs_var_id.clone(), rhs_type.clone());
    } else {
        context.set_reference(
            lhs_var_id.clone(),
            rhs_var_id,
            rhs_type.clone(),
            rhs_is_external,
        );
    }

    // Psalm's AssignmentAnalyzer: a reference taken to an object property
    // (`$b = &$a->b;`) or static property (`$b = &A::$b;`) cannot be tracked
    // through the reference — UnsupportedPropertyReferenceUsage.
    if (rhs_key.contains("->") || rhs_key.contains("::"))
        && !issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            pos.0,
            "UnsupportedPropertyReferenceUsage",
        )
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::UnsupportedPropertyReferenceUsage,
            "This reference cannot be analyzed",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Reference-binding dataflow: the bind publishes the target's current
    // value (it stays reachable through the alias — Psalm reports nothing for
    // a value only read via the reference), and the binding itself is an
    // assignment to the alias variable that can be unused
    // (unusedReferenceToPreviouslyUsedVariable).
    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
        && let Expression::Variable(Variable::Direct(lhs_direct)) = lhs_expr.unparenthesized()
    {
        let lhs_span = lhs_direct.span();
        let node_pos =
            make_data_flow_node_position(analyzer, (lhs_span.start.offset, lhs_span.end.offset));

        let binding_node = DataFlowNode::get_for_variable_source(
            VariableSourceKind::Default,
            VarId(analyzer.interner.intern(&lhs_var_id)),
            node_pos,
            false,
            !rhs_type.parent_nodes.is_empty(),
            false,
            false,
            false,
        );
        // The target's value flows into the binding (NOT a use by itself:
        // `$a = 1; $b = &$a;` leaves BOTH unused in Psalm); it becomes used
        // when the alias is later read or written through.
        add_default_dataflow_paths(
            &mut analysis_data.data_flow_graph,
            &rhs_type.parent_nodes,
            &binding_node,
        );
        analysis_data.data_flow_graph.add_node(binding_node.clone());
        if let Some(lhs_type) = context.locals.get_mut(&lhs_var_id) {
            lhs_type.parent_nodes = vec![binding_node];
        }
    }

    if let Expression::Variable(Variable::Direct(direct)) = lhs_expr.unparenthesized() {
        let var_existed = context.locals.contains_key(&lhs_var_id);
        clear_dependent_property_types(context, direct.name);
        clear_array_path_types_for_base_var(context, direct.name);
        clear_dependent_array_access_types(context, direct.name);
        context.invalidate_dependent_types(&lhs_var_id);
        if var_existed {
            remove_var_clauses_from_context(context, direct.name);
        } else {
            context.remove_var_name_clauses(direct.name);
        }
    }

    analysis_data.expr_types.insert(pos, Rc::new(rhs_type));
    true
}

fn has_unnamed_inline_var_annotation(analyzer: &StatementsAnalyzer<'_>, offset: u32) -> bool {
    analyzer
        .get_inline_var_annotations(offset)
        .is_some_and(|annotations| {
            annotations
                .iter()
                .any(|annotation| annotation.var_name.is_none())
        })
}

/// Psalm parses `@psalm-taint-escape <kind>` from the docblock attached to
/// an assignment statement (`VarDocblockComment::removed_taints`) and removes
/// those kinds on the assignment's dataflow edge.
fn assignment_docblock_removed_taints(
    analyzer: &StatementsAnalyzer<'_>,
    assignment_offset: u32,
) -> Vec<pzoom_code_info::data_flow::node::SinkType> {
    let source = analyzer.source;
    let offset = (assignment_offset as usize).min(source.len());

    let bytes = source.as_bytes();
    let mut cursor = offset;
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }

    if cursor < 2 || &source[cursor - 2..cursor] != "*/" {
        return vec![];
    }

    let doc_end = cursor;
    let Some(doc_start) = source[..doc_end - 2].rfind("/**") else {
        return vec![];
    };

    let mut removed = vec![];
    for line in source[doc_start..doc_end].split('\n') {
        let Some(tag_pos) = line.find("@psalm-taint-escape") else {
            continue;
        };
        let content = &line[tag_pos + "@psalm-taint-escape".len()..];
        if let Some(kind) = content.split_whitespace().next() {
            for sink in pzoom_code_info::data_flow::node::SinkType::kinds_from_name(
                kind.trim_matches('\'').trim_matches('"'),
            ) {
                if !removed.contains(&sink) {
                    removed.push(sink);
                }
            }
        }
    }

    removed
}

/// Like [`get_inline_var_annotation_type`] but never falls back to a
/// nameless annotation.
fn get_named_inline_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    assignment_offset: u32,
    var_id: &str,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(assignment_offset)?;
    annotations
        .iter()
        .find_map(|annotation| match annotation.var_name {
            Some(name) if analyzer.interner.lookup(name).as_ref() == var_id => {
                Some(annotation.var_type.clone())
            }
            _ => None,
        })
}

fn get_inline_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    assignment_offset: u32,
    var_id: &str,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(assignment_offset)?;

    let mut unnamed_match = None;
    for annotation in annotations {
        match annotation.var_name {
            Some(name) if analyzer.interner.lookup(name).as_ref() == var_id => {
                return Some(annotation.var_type.clone());
            }
            None if unnamed_match.is_none() => unnamed_match = Some(annotation.var_type.clone()),
            _ => {}
        }
    }

    unnamed_match
}

fn handle_assignment_with_boolean_logic(
    analyzer: &StatementsAnalyzer<'_>,
    assigned_var_name: &str,
    lhs_expr: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if !is_bool_like(rhs_type) {
        return;
    }

    if !matches!(rhs_expr.unparenthesized(), Expression::Binary(_)) {
        return;
    }

    let var_object_id = (lhs_expr.start_offset() as u32, lhs_expr.end_offset() as u32);
    let cond_object_id = (rhs_expr.start_offset() as u32, rhs_expr.end_offset() as u32);

    let right_clauses = crate::formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        rhs_expr,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();
    if right_clauses.is_empty() {
        return;
    }

    let right_clauses = filter_clauses_for_assignment_target(assigned_var_name, right_clauses);
    if right_clauses.is_empty() {
        return;
    }

    let mut possibilities = BTreeMap::new();
    possibilities.insert(
        ClauseKey::Name(VarName::new(assigned_var_name)),
        pzoom_code_info::AssertionSet::from_iter([(Assertion::Falsy.to_hash(), Assertion::Falsy)]),
    );

    let assignment_clauses = combine_ored_clauses(
        vec![Clause::new(
            possibilities,
            var_object_id,
            var_object_id,
            None,
            None,
            None,
        )],
        right_clauses,
        cond_object_id,
    );

    if let Ok(assignment_clauses) = assignment_clauses {
        context
            .clauses
            .extend(assignment_clauses.into_iter().map(Rc::new));
    }
}

fn is_bool_like(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .all(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
}

fn filter_clauses_for_assignment_target(
    assigned_var_name: &str,
    clauses: Vec<Clause>,
) -> Vec<Clause> {
    clauses
        .into_iter()
        .filter_map(|clause| {
            let mut possibilities = (*clause.possibilities).clone();
            possibilities.retain(|key, _| match key {
                ClauseKey::Name(name) => {
                    name != assigned_var_name
                        && !name.starts_with(&format!("{}[", assigned_var_name))
                        && !name.starts_with(&format!("{}->", assigned_var_name))
                        && !name.contains(&format!("[{}]", assigned_var_name))
                }
                ClauseKey::Range(..) => true,
            });

            if possibilities.is_empty() {
                return None;
            }

            Some(Clause::new(
                possibilities,
                clause.creating_conditional_id,
                clause.creating_object_id,
                Some(clause.wedge),
                Some(clause.reconcilable),
                Some(clause.generated),
            ))
        })
        .collect()
}

fn remove_var_clauses_from_context(context: &mut BlockContext, assigned_var_name: &str) {
    context.remove_var_name_from_conflicting_clauses(assigned_var_name);
}

/// Whether `expr` is a direct variable (or its negation) that has only
/// *possibly* been assigned in the current scope — i.e. assigned on some but
/// not all paths.
pub(crate) fn is_possibly_undefined_direct_var(
    expr: &Expression<'_>,
    context: Option<&BlockContext>,
) -> bool {
    let Some(context) = context else {
        return false;
    };

    let var_name = match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name),
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            if let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized()
            {
                Some(direct.name)
            } else {
                None
            }
        }
        _ => None,
    };

    let Some(var_name) = var_name else {
        return false;
    };

    let var_id = VarName::new(var_name);
    context.possibly_assigned_var_ids.contains(&var_id)
        && !context.assigned_var_ids.contains_key(&var_id)
}
