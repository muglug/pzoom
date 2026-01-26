//! And (&&) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a logical AND expression (&&, 'and').
///
/// The AND operator short-circuits: if the left side is falsy, the right side
/// is not evaluated. This analyzer handles type narrowing through the left side.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Create a context for the left side analysis
    let mut left_context = context.clone();

    // Analyze the left side
    let left_pos = expr_analyzer::analyze(analyzer, left, analysis_data, &mut left_context);

    // Apply basic type narrowing based on the left expression
    // For $x && ..., if $x is nullable, in the right context it won't be null
    apply_truthiness_narrowing(analyzer, left, analysis_data, &mut left_context, left_pos);

    // Analyze the right side with the narrowed context
    let _right_pos = expr_analyzer::analyze(analyzer, right, analysis_data, &mut left_context);

    // Merge any variable changes back to the original context
    for (var_id, var_type) in &left_context.locals {
        if left_context.assigned_var_ids.contains_key(var_id) {
            context.locals.insert(*var_id, var_type.clone());
        }
    }

    // The result type is always bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}

/// Apply truthiness-based type narrowing.
///
/// When we know an expression is truthy (e.g., left side of &&),
/// we can narrow nullable types to non-null.
fn apply_truthiness_narrowing(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
    expr_pos: Pos,
) {
    // Handle simple variable checks: $x && ... means $x is truthy
    if let Expression::Variable(var) = expr {
        if let mago_syntax::ast::ast::variable::Variable::Direct(direct) = var {
            let var_id = analyzer.interner.intern(direct.name);

            if let Some(var_type) = analysis_data.get_expr_type(expr_pos) {
                // Remove null and false from the type since we know it's truthy
                let narrowed_types: Vec<_> = var_type
                    .types
                    .iter()
                    .filter(|t| !matches!(t, TAtomic::TNull | TAtomic::TFalse))
                    .cloned()
                    .collect();

                if !narrowed_types.is_empty() {
                    context.locals.insert(var_id, TUnion::from_types(narrowed_types));
                }
            }
        }
    }

    // Handle isset($x) or !empty($x) style checks
    // This would require more complex AST inspection
}
