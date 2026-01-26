//! String concatenation (.) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a string concatenation expression (.).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze both operands
    let left_pos = expr_analyzer::analyze(analyzer, left, analysis_data, context);
    let right_pos = expr_analyzer::analyze(analyzer, right, analysis_data, context);

    let left_type = analysis_data.get_expr_type(left_pos);
    let right_type = analysis_data.get_expr_type(right_pos);

    // Try to compute a literal string result if both operands are literal strings
    let result_type = match (left_type, right_type) {
        (Some(left_t), Some(right_t)) => {
            // Check if both are literal strings
            if let (Some(left_str), Some(right_str)) =
                (get_literal_string_value(&left_t), get_literal_string_value(&right_t))
            {
                // Concatenate literal strings
                let combined = format!("{}{}", left_str, right_str);
                TUnion::new(TAtomic::TLiteralString { value: combined })
            } else if is_non_empty_string(&left_t) || is_non_empty_string(&right_t) {
                // If either side is non-empty, result is non-empty string
                // For now we don't have TNonEmptyString, just return string
                TUnion::string()
            } else {
                TUnion::string()
            }
        }
        _ => TUnion::string(),
    };

    analysis_data.set_expr_type(pos, result_type);
}

/// Try to extract a literal string value from a TUnion.
fn get_literal_string_value(t: &TUnion) -> Option<String> {
    if !t.is_single() {
        return None;
    }

    match t.get_single()? {
        TAtomic::TLiteralString { value } => {
            Some(value.clone())
        }
        _ => None,
    }
}

/// Check if a type is a non-empty string.
fn is_non_empty_string(t: &TUnion) -> bool {
    t.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TLiteralString { .. } | TAtomic::TLiteralClassString { .. }
        )
    })
}
