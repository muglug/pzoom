//! While statement analyzer.
//!
//! Delegates to the shared [`loop_analyzer`] fixpoint, mirroring Hakana's
//! `while_analyzer`.

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::r#loop::r#while::While;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};
use crate::stmt::loop_analyzer;

/// Analyze a while statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    while_stmt: &While<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let condition = while_stmt.condition;
    let while_true = matches!(
        condition.unparenthesized(),
        Expression::Literal(mago_syntax::ast::ast::literal::Literal::True(_))
    );

    let mut while_context = context.clone();
    while_context.inside_loop = true;
    while_context.inside_foreach = false;
    while_context.break_types.push(BreakContext::Loop);

    let loop_scope = LoopScope::new(context.locals.clone());

    let cond_pos = (
        condition.start_offset() as u32,
        condition.end_offset() as u32,
    );
    let always_enters_loop = while_true
        || analysis_data
            .get_expr_type(cond_pos)
            .is_some_and(|t| t.is_always_truthy());

    let body_stmts = while_stmt.body.statements();
    let pre_conditions = get_and_expressions(condition);

    let (loop_scope, _inner_loop_context) = loop_analyzer::analyze(
        analyzer,
        body_stmts,
        pre_conditions,
        vec![],
        loop_scope,
        &mut while_context,
        context,
        analysis_data,
        false,
        always_enters_loop,
    )?;

    let can_leave_loop = !while_true || loop_scope.final_actions.contains(&ControlAction::Break);

    if always_enters_loop && !can_leave_loop {
        // `while (true)` with no reachable break never exits normally.
        context.control_actions.insert(ControlAction::End);
        context.has_returned = true;
    }

    Ok(())
}

/// Split a condition `a && b && c` into its conjuncts `[a, b, c]`, mirroring
/// Hakana's `get_and_expressions`.
pub fn get_and_expressions<'a, 'arena>(
    condition: &'a Expression<'arena>,
) -> Vec<&'a Expression<'arena>> {
    if let Expression::Binary(binary) = condition.unparenthesized() {
        if matches!(
            binary.operator,
            BinaryOperator::And(_) | BinaryOperator::LowAnd(_)
        ) {
            let mut anded = get_and_expressions(binary.lhs);
            anded.extend(get_and_expressions(binary.rhs));
            return anded;
        }
    }

    vec![condition]
}
