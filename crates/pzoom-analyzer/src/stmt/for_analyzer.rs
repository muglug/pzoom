//! For loop statement analyzer.
//!
//! Delegates to the shared [`loop_analyzer`] fixpoint, mirroring Hakana's
//! `for_analyzer`.

use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::r#for::For;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};
use crate::stmt::loop_analyzer;

/// Analyze a for loop statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    for_stmt: &For<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Initialization expressions run once, in the parent context.
    for init_expr in for_stmt.initializations.iter() {
        let _ = expression_analyzer::analyze(analyzer, init_expr, analysis_data, context);
    }

    // Only the last condition acts as the loop guard in PHP; the earlier ones
    // are evaluated for their side effects but do not gate the loop.
    let pre_conditions: Vec<&Expression<'_>> = match for_stmt.conditions.iter().last() {
        Some(condition) => vec![condition],
        None => vec![],
    };
    let post_expressions: Vec<&Expression<'_>> = for_stmt.increments.iter().collect();

    // A `for` loop is infinite when it has no guard or its guard is always true
    // (`for (;;)`, `for (; true;)`, `for ($i = 0;; $i++)`). In that case the body
    // is guaranteed to run at least once and the loop only exits via `break` —
    // matching how Psalm derives `always_enters_loop`.
    let always_enters_loop = pre_conditions
        .last()
        .map_or(true, |condition| is_always_true(condition));
    let while_true = always_enters_loop;

    let mut for_context = context.clone();
    for_context.inside_loop = true;
    for_context.inside_foreach = false;
    for_context.break_types.push(BreakContext::Loop);

    let loop_scope = LoopScope::new(context.locals.clone());

    let body_stmts = for_stmt.body.statements();

    let (loop_scope, _inner) = loop_analyzer::analyze(
        analyzer,
        body_stmts,
        pre_conditions,
        post_expressions,
        loop_scope,
        &mut for_context,
        context,
        analysis_data,
        always_enters_loop,
        while_true,
    )?;

    // An infinite loop with no reachable `break` never exits normally, so any
    // code after it is unreachable.
    let exits_via_break = loop_scope.final_actions.contains(&ControlAction::Break);
    if while_true && !exits_via_break {
        context.control_actions.insert(ControlAction::End);
        context.has_returned = true;
    }

    Ok(())
}

/// Returns true if the guard expression is a literal `true`, making the loop
/// body always execute (matching Psalm's always-enters detection).
fn is_always_true(condition: &Expression<'_>) -> bool {
    matches!(
        condition.unparenthesized(),
        Expression::Literal(Literal::True(_))
    )
}
