//! Or (||) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a logical OR expression (||, 'or').
///
/// The OR operator short-circuits: if the left side is truthy, the right side
/// is not evaluated. This analyzer handles type narrowing through negation.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Create a context for analysis
    let mut or_context = context.clone();

    // Analyze the left side
    let left_pos = expr_analyzer::analyze(analyzer, left, analysis_data, &mut or_context);

    // Apply negated type narrowing for the right side
    // For $x || ..., the right side only executes if $x is falsy
    // So in the right context, we narrow to falsy types
    apply_falsiness_narrowing(analyzer, left, analysis_data, &mut or_context, left_pos);

    // Analyze the right side
    let _right_pos = expr_analyzer::analyze(analyzer, right, analysis_data, &mut or_context);

    // The result type is always bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}

/// Apply falsiness-based type narrowing.
///
/// When we know an expression is falsy (e.g., left side of || when right executes),
/// we can narrow types accordingly.
fn apply_falsiness_narrowing(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
    expr_pos: Pos,
) {
    // Handle simple variable checks: $x || ... means $x is falsy in right branch
    if let Expression::Variable(var) = expr {
        if let mago_syntax::ast::ast::variable::Variable::Direct(direct) = var {
            let var_id = analyzer.interner.intern(direct.name);

            if let Some(var_type) = analysis_data.get_expr_type(expr_pos) {
                // In the right branch, the variable could be null, false, 0, "", etc.
                // This is complex to narrow precisely, so we keep the falsy subset
                let narrowed_types: Vec<_> = var_type
                    .types
                    .iter()
                    .filter(|t| t.is_falsable())
                    .cloned()
                    .collect();

                // Only update if we have falsy types, otherwise keep original
                if !narrowed_types.is_empty() {
                    context.locals.insert(var_id, TUnion::from_types(narrowed_types));
                } else {
                    // If no falsy types, the right side should be unreachable
                    // For simplicity, we keep the original type
                }
            }
        }
    }
}
