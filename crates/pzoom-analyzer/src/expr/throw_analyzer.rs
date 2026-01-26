//! Throw expression analyzer.
//!
//! Modeled after Psalm's ThrowAnalyzer - handles throw expressions and sets
//! the appropriate context flags for control flow analysis.

use mago_syntax::ast::ast::throw::Throw;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a throw expression.
///
/// This sets `context.has_returned = true` to indicate that control flow
/// will exit at this point (similar to a return statement).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    throw: &Throw<'_>,
    pos: (u32, u32),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Set inside_throw flag while analyzing the thrown expression
    context.inside_throw = true;

    // Analyze the exception expression
    let _exception_pos = expr_analyzer::analyze(analyzer, throw.exception, analysis_data, context);

    context.inside_throw = false;

    // Mark that control flow has exited (like Psalm's $context->has_returned = true)
    context.has_returned = true;

    // TODO: Handle finally_scope - combine types with finally scope vars
    // if context.finally_scope.is_some() { ... }

    // TODO: Validate that the thrown expression is Throwable
    // if let Some(throw_type) = analysis_data.get_expr_type(exception_pos) {
    //     // Check if throw_type is a subtype of Throwable
    // }

    // Throw expression has type `never` (nothing)
    analysis_data.set_expr_type(pos, TUnion::nothing());
}
