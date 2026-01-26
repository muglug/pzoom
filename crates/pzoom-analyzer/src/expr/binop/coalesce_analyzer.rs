//! Null coalesce (??) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{combine_union_types, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a null coalesce expression (??).
///
/// The ?? operator returns the left operand if it exists and is not null,
/// otherwise returns the right operand.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the left side with isset context to suppress undefined variable warnings
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    let left_pos = expr_analyzer::analyze(analyzer, left, analysis_data, context);
    let left_type = analysis_data.get_expr_type(left_pos);

    context.inside_isset = was_inside_isset;

    // Analyze the right side
    let right_pos = expr_analyzer::analyze(analyzer, right, analysis_data, context);
    let right_type = analysis_data.get_expr_type(right_pos);

    // Combine the types: left type (minus null) + right type
    let result_type = match (left_type, right_type) {
        (Some(lt), Some(rt)) => {
            // Remove null from left type
            let left_without_null: Vec<_> = lt.types.iter()
                .filter(|t| !matches!(t, TAtomic::TNull))
                .cloned()
                .collect();

            if left_without_null.is_empty() {
                // Left was only null, result is just right type
                (*rt).clone()
            } else {
                // Combine non-null left types with right types
                let left_non_null = TUnion::from_types(left_without_null);
                combine_union_types(&left_non_null, &rt, false)
            }
        }
        (Some(lt), None) => {
            // Remove null from left type
            let left_without_null: Vec<_> = lt.types.iter()
                .filter(|t| !matches!(t, TAtomic::TNull))
                .cloned()
                .collect();

            if left_without_null.is_empty() {
                TUnion::mixed()
            } else {
                TUnion::from_types(left_without_null)
            }
        }
        (None, Some(rt)) => (*rt).clone(),
        (None, None) => TUnion::mixed(),
    };

    analysis_data.set_expr_type(pos, result_type);
}
