//! If-conditional helpers.

use std::cell::RefCell;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{DataFlowNode, GraphKind, Issue, IssueKind, PathKind, TAtomic, TUnion};
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::scope::if_conditional_scope::IfConditionalScope;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an if/elseif condition, producing the conditional scope.
///
/// Adapted from hakana-core's `if_conditional_analyzer::analyze`. The condition is
/// analyzed once in a dedicated context carrying a shared `if_body_context`; the
/// `&&`/`||` analyzers narrow into that shared context with the type information the
/// if body should see (including right-operand assignments). The returned scope's
/// `if_body_context` is the post-condition fallthrough base with only the
/// operator-narrowed/-assigned locals overlaid, so simple (non-`&&`/`||`) conditions
/// reduce to the fallthrough base unchanged.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    cond: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    outer_context: &BlockContext,
) -> IfConditionalScope {
    // Psalm analyzes the sub-expression that is definitely evaluated regardless of
    // branch (e.g. the `preg_match(..., $matches)` in `if (!preg_match(...) || ...)`)
    // in the OUTER (fallthrough) context, so its assignments are seeded into scope
    // before the rest of the condition narrows things.
    let externally_applied_if_cond_expr = get_definitely_evaluated_expression_after_if(cond);
    let internally_applied_if_cond_expr = get_definitely_evaluated_expression_inside_if(cond);

    // Psalm mutates `$outer_context` in place; pzoom's callers pass the outer
    // context immutably, so work on a clone that becomes post_if_context.
    let mut outer_working = outer_context.clone();
    outer_working.if_body_context = None;
    let pre_condition_assigned = outer_context.assigned_var_ids.clone();

    // Psalm clones `$if_context` BEFORE the externally-applied analysis when the
    // internally- and externally-applied expressions differ: scope entries the
    // externally-applied sub-expression registers (e.g. isset pseudo-vars for
    // magic properties) belong to the fallthrough only, and the if path
    // re-derives the asserted keys through the reconciler.
    let mut early_if_context = if !std::ptr::eq(
        internally_applied_if_cond_expr,
        externally_applied_if_cond_expr,
    ) {
        Some(outer_working.clone())
    } else {
        None
    };

    let was_inside_conditional = outer_working.inside_conditional;
    outer_working.inside_conditional = true;
    expression_analyzer::analyze(
        analyzer,
        externally_applied_if_cond_expr,
        analysis_data,
        &mut outer_working,
    );
    outer_working.inside_conditional = was_inside_conditional;

    let if_context = early_if_context
        .take()
        .unwrap_or_else(|| outer_working.clone());

    // The shared body context the &&/|| operators narrow into (Psalm's
    // `$if_conditional_context->if_body_context = $if_context` reference).
    let if_body_rc = Rc::new(RefCell::new(if_context));
    let mut if_conditional_context = if_body_rc.borrow().clone();
    if_conditional_context.if_body_context = Some(if_body_rc.clone());

    // Psalm clones the post-if (else/fallthrough) base before the full condition
    // is analyzed, so condition-only narrowing doesn't leak there.
    let post_if_context = outer_working.clone();

    let cond_unparenthesized = cond.unparenthesized();
    if !std::ptr::eq(internally_applied_if_cond_expr, cond_unparenthesized)
        || !std::ptr::eq(externally_applied_if_cond_expr, cond_unparenthesized)
    {
        let was_inside_conditional = if_conditional_context.inside_conditional;
        if_conditional_context.inside_conditional = true;
        expression_analyzer::analyze(analyzer, cond, analysis_data, &mut if_conditional_context);
        if_conditional_context.inside_conditional = was_inside_conditional;
    }

    add_branch_dataflow(analyzer, cond, analysis_data);

    if_conditional_context.if_body_context = None;

    let mut cond_referenced_var_ids = outer_working.cond_referenced_var_ids;
    cond_referenced_var_ids.extend(
        if_conditional_context
            .cond_referenced_var_ids
            .iter()
            .cloned(),
    );

    let mut if_body_context = match Rc::try_unwrap(if_body_rc) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => rc.borrow().clone(),
    };
    if_body_context.if_body_context = None;
    // Carry the clauses the &&/||/ternary operators reconciled during condition
    // analysis so the if-body reconcile can skip re-reporting them (Hakana). `&&`
    // records into the shared body context; `||`/ternary record onto the cond
    // context.
    if_body_context
        .reconciled_expression_clauses
        .extend(if_conditional_context.reconciled_expression_clauses);

    // Psalm's assigned_in_conditional_var_ids: vars whose assignment count grew
    // while analyzing the condition — in the externally-applied pass, the full
    // condition pass, or an operator's merge into the shared body context.
    let mut assigned_in_conditional_var_ids = FxHashSet::default();
    for assigned in [
        &outer_working.assigned_var_ids,
        &if_conditional_context.assigned_var_ids,
        &if_body_context.assigned_var_ids,
    ] {
        for (var_id, count) in assigned {
            if pre_condition_assigned.get(var_id).copied().unwrap_or(0) < *count {
                assigned_in_conditional_var_ids.insert(var_id.clone());
            }
        }
    }

    IfConditionalScope {
        if_body_context,
        post_if_context,
        cond_referenced_var_ids,
        assigned_in_conditional_var_ids,
    }
}

/// Mirrors Psalm `IfConditionalAnalyzer::getDefinitelyEvaluatedExpressionAfterIf`:
/// reduces a condition to the sub-expression that is definitely evaluated
/// regardless of which branch is taken — stripping `=== true`, taking the left
/// operand of `&&`/`and`/`xor`, and descending through `!` (which swaps to the
/// inside-if reduction). This keeps assignments such as `$matches` in
/// `if (!preg_match($re, $s, $matches))` defined after the `if`.
fn get_definitely_evaluated_expression_after_if<'a>(
    stmt: &'a Expression<'a>,
) -> &'a Expression<'a> {
    match stmt.unparenthesized() {
        Expression::Binary(binary)
            if matches!(
                binary.operator,
                BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
            ) =>
        {
            if is_true_literal(binary.lhs) {
                return get_definitely_evaluated_expression_after_if(binary.rhs);
            }
            if is_true_literal(binary.rhs) {
                return get_definitely_evaluated_expression_after_if(binary.lhs);
            }
            stmt
        }
        Expression::Binary(binary) => {
            if matches!(
                binary.operator,
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_) | BinaryOperator::LowXor(_)
            ) {
                return get_definitely_evaluated_expression_after_if(binary.lhs);
            }
            stmt
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = get_definitely_evaluated_expression_inside_if(unary.operand);
            if std::ptr::eq(inner, unary.operand) {
                stmt
            } else {
                inner
            }
        }
        _ => stmt,
    }
}

/// Mirrors Psalm `IfConditionalAnalyzer::getDefinitelyEvaluatedExpressionInsideIf`:
/// the expression definitely evaluated before any statements inside the `if`
/// body — like the above but taking the left operand of `||`/`or`/`xor`.
fn get_definitely_evaluated_expression_inside_if<'a>(
    stmt: &'a Expression<'a>,
) -> &'a Expression<'a> {
    match stmt.unparenthesized() {
        Expression::Binary(binary)
            if matches!(
                binary.operator,
                BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
            ) =>
        {
            if is_true_literal(binary.lhs) {
                return get_definitely_evaluated_expression_inside_if(binary.rhs);
            }
            if is_true_literal(binary.rhs) {
                return get_definitely_evaluated_expression_inside_if(binary.lhs);
            }
            stmt
        }
        Expression::Binary(binary) => {
            if matches!(
                binary.operator,
                BinaryOperator::Or(_) | BinaryOperator::LowOr(_) | BinaryOperator::LowXor(_)
            ) {
                return get_definitely_evaluated_expression_inside_if(binary.lhs);
            }
            stmt
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = get_definitely_evaluated_expression_after_if(unary.operand);
            if std::ptr::eq(inner, unary.operand) {
                stmt
            } else {
                inner
            }
        }
        _ => stmt,
    }
}

fn is_true_literal(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Literal(Literal::True(_))
    )
}

/// Hakana `if_conditional_analyzer::add_branch_dataflow`: the condition's parents
/// flow into an unlabelled branch sink (function-body graphs only).
pub fn add_branch_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    cond: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        // todo maybe useful in the future
        return;
    }

    let cond_span = cond.span();
    let cond_pos: Pos = (cond_span.start.offset, cond_span.end.offset);

    let Some(conditional_type) = analysis_data.expr_types.get(&cond_pos).cloned() else {
        return;
    };

    if !conditional_type.parent_nodes.is_empty() {
        let branch_node =
            DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(analyzer, cond_pos));

        for parent_node in &conditional_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &branch_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }

        analysis_data.data_flow_graph.add_node(branch_node);
    }
}

/// Mirrors Psalm's `IfConditionalAnalyzer::handleParadoxicalCondition`.
pub fn handle_paradoxical_condition(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_redundant_with_assignment: bool,
    context: Option<&BlockContext>,
) {
    let Some(expr_type) = analysis_data
        .expr_types
        .get(&expr_pos)
        .cloned()
        .map(|union| (*union).clone())
    else {
        return;
    };

    if crate::expr::assignment_analyzer::is_possibly_undefined_direct_var(expr, context) {
        return;
    }

    if expr_type.is_always_falsy() {
        // Inside a loop pzoom's iteration widening can transiently type a
        // condition as literal `false` where Psalm would have widened (e.g. a
        // chain over a loop-reassigned var); skip only the falsy half there —
        // the truthy half is stable (a loop-exit guard proven true, which
        // Psalm reports too).
        if context.is_some_and(|context| context.inside_loop) {
            return;
        }
        let issue_kind = if expr_type.from_docblock {
            IssueKind::DocblockTypeContradiction
        } else {
            IssueKind::TypeDoesNotContainType
        };
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        let type_id = expr_type.get_id(Some(analyzer.interner));
        analysis_data.add_issue(
            Issue::new(
                issue_kind,
                format!("Operand of type {} is always falsy", type_id),
                analyzer.file_path,
                expr_pos.0,
                expr_pos.1,
                line,
                col,
            )
            // Psalm's handleParadoxicalCondition dupe key (the reconciler's
            // "Type X for $y" report at the same spot carries the same key).
            .with_dupe_key(format!("{} falsy", type_id)),
        );
        return;
    }

    if expr_type.is_always_truthy()
        && (!matches!(expr.unparenthesized(), Expression::Assignment(_))
            || emit_redundant_with_assignment)
    {
        let issue_kind = if expr_type.from_docblock {
            IssueKind::RedundantConditionGivenDocblockType
        } else {
            IssueKind::RedundantCondition
        };
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        let type_id = expr_type.get_id(Some(analyzer.interner));
        analysis_data.add_issue(
            Issue::new(
                issue_kind,
                format!("Operand of type {} is always truthy", type_id),
                analyzer.file_path,
                expr_pos.0,
                expr_pos.1,
                line,
                col,
            )
            // Psalm passes "<type> falsy" as the dupe key for the truthy
            // paradox too (handleParadoxicalCondition).
            .with_dupe_key(format!("{} falsy", type_id)),
        );
        return;
    }

    // Psalm: otherwise flag a risky truthy/falsy comparison
    // (`ExpressionAnalyzer::checkRiskyTruthyFalsyComparison`), skipped for the
    // `===` / `!==` / `!` forms that already compare explicitly.
    if !is_assignment_or_negated_assignment(expr)
        && should_check_risky_truthy_falsy(expr, analyzer)
        && get_truthy_falsy_target_union(expr, expr_type.clone(), analysis_data)
            .is_some_and(|target_union| is_risky_truthy_falsy_union(&target_union))
    {
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RiskyTruthyFalsyComparison,
            format!(
                "Operand of type {} may evaluate differently under truthy/falsy checks",
                expr_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            expr_pos.0,
            expr_pos.1,
            line,
            col,
        ));
    }
}

fn is_risky_truthy_falsy_union(union: &TUnion) -> bool {
    if !union.is_nullable()
        || union
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
    {
        return false;
    }

    union.types.iter().any(is_ambiguous_array_like_atomic)
}

fn get_truthy_falsy_target_union(
    expr: &Expression<'_>,
    expr_type: TUnion,
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return Some(expr_type);
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Not(_)) {
        return Some(expr_type);
    }

    analysis_data
        .expr_types
        .get(&(
            unary.operand.start_offset() as u32,
            unary.operand.end_offset() as u32,
        ))
        .cloned()
        .map(|union| (*union).clone())
        .or(Some(expr_type))
}

fn is_array_like_atomic(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => as_type.types.iter().any(is_array_like_atomic),
        _ => false,
    }
}

fn is_ambiguous_array_like_atomic(atomic: &TAtomic) -> bool {
    if !is_array_like_atomic(atomic) {
        return false;
    }

    if atomic.is_truthy() || atomic.is_falsy() {
        return false;
    }

    match atomic {
        TAtomic::TTemplateParam { as_type, .. } => {
            as_type.types.iter().any(is_ambiguous_array_like_atomic)
        }
        _ => true,
    }
}

fn is_assignment_or_negated_assignment(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Assignment(_) => true,
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            matches!(unary.operand.unparenthesized(), Expression::Assignment(_))
        }
        _ => false,
    }
}

fn should_check_risky_truthy_falsy(
    expr: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            analyzer.interner.find(direct.name).is_some_and(|var_id| {
                analyzer.function_info.is_some_and(|function_info| {
                    function_info.params.iter().any(|p| p.name == var_id)
                })
            })
        }
        Expression::Call(Call::Function(_)) => true,
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            should_check_risky_truthy_falsy(unary.operand, analyzer)
        }
        _ => false,
    }
}
