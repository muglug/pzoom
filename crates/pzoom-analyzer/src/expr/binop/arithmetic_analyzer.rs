//! Arithmetic operation analyzer.

use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an arithmetic binary operation (+, -, *, /, %, **).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    operator: &BinaryOperator,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze both operands
    let left_pos = expr_analyzer::analyze(analyzer, left, analysis_data, context);
    let right_pos = expr_analyzer::analyze(analyzer, right, analysis_data, context);

    let left_type = analysis_data.get_expr_type(left_pos);
    let right_type = analysis_data.get_expr_type(right_pos);

    let result_type = match operator {
        BinaryOperator::Addition(_) => infer_addition_type(left_type.as_deref(), right_type.as_deref()),
        BinaryOperator::Subtraction(_) => infer_arithmetic_type(left_type.as_deref(), right_type.as_deref()),
        BinaryOperator::Multiplication(_) => infer_arithmetic_type(left_type.as_deref(), right_type.as_deref()),
        BinaryOperator::Division(_) => {
            // Division can return int or float
            TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat])
        }
        BinaryOperator::Modulo(_) => TUnion::int(),
        BinaryOperator::Exponentiation(_) => infer_arithmetic_type(left_type.as_deref(), right_type.as_deref()),
        _ => TUnion::new(TAtomic::TNumeric),
    };

    analysis_data.set_expr_type(pos, result_type);
}

/// Infer the result type of addition.
fn infer_addition_type(left: Option<&TUnion>, right: Option<&TUnion>) -> TUnion {
    // Check if either operand is an array (array union)
    let left_is_array = left.map_or(false, |t| {
        t.types.iter().any(|a| matches!(a, TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. }))
    });
    let right_is_array = right.map_or(false, |t| {
        t.types.iter().any(|a| matches!(a, TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. }))
    });

    if left_is_array && right_is_array {
        // Array union - combine the arrays
        if let Some(lt) = left {
            lt.clone()
        } else {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            })
        }
    } else {
        infer_arithmetic_type(left, right)
    }
}

/// Infer the result type of an arithmetic operation.
fn infer_arithmetic_type(left: Option<&TUnion>, right: Option<&TUnion>) -> TUnion {
    let has_float = |t: &TUnion| {
        t.types
            .iter()
            .any(|a| matches!(a, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
    };

    match (left, right) {
        (Some(lt), Some(rt)) => {
            if has_float(lt) || has_float(rt) {
                TUnion::float()
            } else {
                TUnion::int()
            }
        }
        _ => TUnion::new(TAtomic::TNumeric),
    }
}
