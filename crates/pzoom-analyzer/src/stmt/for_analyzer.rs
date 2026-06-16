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
use crate::stmt::loop_analyzer;
use crate::stmt::scope_analyzer::BreakContext;

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
    let while_true = pre_conditions
        .last()
        .map_or(true, |condition| is_always_true(condition));
    // Psalm's `doesEnterLoop`: `for ($i = 1; $i < 2; ...)` with literal-int
    // init and bound provably runs the body at least once.
    let always_enters_loop = while_true || does_enter_loop(for_stmt, &pre_conditions, context);

    let mut for_context = context.clone();
    for_context.inside_loop = true;
    for_context.inside_foreach = false;
    for_context.break_types.push(BreakContext::Loop);

    let mut loop_scope = LoopScope::new(context.locals.clone());
    // Counter variables assigned in init/increment are protected: a nested
    // foreach reassigning one reports LoopInvalidation.
    for expr in for_stmt
        .initializations
        .iter()
        .chain(for_stmt.increments.iter())
    {
        if let Some(var_name) = directly_assigned_var_name(expr) {
            loop_scope.protected_var_ids.insert(var_name);
        }
    }

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
        false,
        always_enters_loop,
        while_true,
    )?;

    // Psalm does not treat code after a break-less infinite `for` as
    // unreachable; it is analyzed with the pre-loop scope.
    let _ = &loop_scope;

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

/// Psalm's `LoopAnalyzer::doesEnterLoop` for `for` statements: a single
/// literal-int init (`$i = 1`) compared against a literal-int bound
/// (`$i < 2`, `$i <= 2`) that holds at entry proves the body runs.
fn does_enter_loop(
    for_stmt: &For<'_>,
    pre_conditions: &[&Expression<'_>],
    context: &BlockContext,
) -> bool {
    use mago_syntax::ast::ast::binary::BinaryOperator;
    use mago_syntax::ast::ast::variable::Variable;

    if for_stmt.initializations.len() != 1 || for_stmt.conditions.len() != 1 {
        return false;
    }
    let Some(condition) = pre_conditions.last() else {
        return false;
    };
    let Expression::Binary(binary) = condition.unparenthesized() else {
        return false;
    };
    let Expression::Variable(Variable::Direct(direct)) = binary.lhs.unparenthesized() else {
        return false;
    };
    let Expression::Literal(Literal::Integer(bound_literal)) = binary.rhs.unparenthesized() else {
        return false;
    };
    let Some(bound_value) = bound_literal.value else {
        return false;
    };

    // The init expressions ran in the parent context, so the counter's value
    // is its current local type.
    let init_value = context
        .locals
        .get(direct.name)
        .or_else(|| context.locals.get(direct.name.trim_start_matches('$')))
        .and_then(|var_type| match var_type.types.as_slice() {
            [pzoom_code_info::TAtomic::TLiteralInt { value }] => Some(*value),
            _ => None,
        });
    let Some(init_value) = init_value else {
        return false;
    };

    match &binary.operator {
        BinaryOperator::LessThan(_) => init_value < bound_value as i64,
        BinaryOperator::LessThanOrEqual(_) => init_value <= bound_value as i64,
        BinaryOperator::GreaterThan(_) => init_value > bound_value as i64,
        BinaryOperator::GreaterThanOrEqual(_) => init_value >= bound_value as i64,
        _ => false,
    }
}

/// The variable directly assigned/incremented by a for-init or increment
/// expression (`$i = 0`, `$i++`, `++$i`, `$i += 1`).
fn directly_assigned_var_name(expr: &Expression<'_>) -> Option<pzoom_code_info::VarName> {
    use mago_syntax::ast::ast::variable::Variable;
    let target = match expr.unparenthesized() {
        Expression::Assignment(assignment) => assignment.lhs.unparenthesized(),
        Expression::UnaryPostfix(postfix) => postfix.operand.unparenthesized(),
        Expression::UnaryPrefix(prefix) => prefix.operand.unparenthesized(),
        _ => return None,
    };
    if let Expression::Variable(Variable::Direct(direct)) = target {
        Some(pzoom_code_info::VarName::new(direct.name))
    } else {
        None
    }
}
