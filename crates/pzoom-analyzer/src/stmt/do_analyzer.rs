//! Do-while statement analyzer (Psalm `DoAnalyzer` / Hakana `do_analyzer`).
//!
//! A `do { ... } while (cond)` body always executes at least once and evaluates its
//! condition *after* the body. This delegates to the shared [`loop_analyzer`]
//! fixpoint with `is_do = true`, mirroring Hakana's do-loop handling.

use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::do_while::DoWhile;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};
use crate::stmt::loop_analyzer;
use crate::stmt::while_analyzer::get_and_expressions;

/// Analyze a do-while statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    do_while: &DoWhile<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let condition = do_while.condition;

    let mut loop_context = context.clone();
    loop_context.inside_loop = true;
    loop_context.inside_foreach = false;
    loop_context.break_types.push(BreakContext::Loop);

    let loop_scope = LoopScope::new(context.locals.clone());

    // The body of a do-while always runs at least once.
    let body_stmts = std::slice::from_ref(do_while.statement);
    let pre_conditions = get_and_expressions(condition);

    let (loop_scope, _inner) = loop_analyzer::analyze(
        analyzer,
        body_stmts,
        pre_conditions,
        vec![],
        loop_scope,
        &mut loop_context,
        context,
        analysis_data,
        true,
        true,
    )?;

    let while_true = matches!(
        condition.unparenthesized(),
        Expression::Literal(Literal::True(_))
    );
    let can_leave_loop = !while_true || loop_scope.final_actions.contains(&ControlAction::Break);

    if !can_leave_loop {
        context.control_actions.insert(ControlAction::End);
        context.has_returned = true;
    }

    Ok(())
}
