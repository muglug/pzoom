//! Unary operation analyzer.

use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::{UnaryPostfix, UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a unary prefix expression.
pub fn analyze_prefix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let operand_pos = expr_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let operand_type = analysis_data.get_expr_type(operand_pos);

    let expr_type = match &unary.operator {
        // Boolean not -> bool
        UnaryPrefixOperator::Not(_) => TUnion::bool(),

        // Arithmetic negation/plus
        UnaryPrefixOperator::Negation(_) => {
            if let Some(op_type) = operand_type {
                // If operand is a literal int, negate it
                if let Some(TAtomic::TLiteralInt { value }) = op_type.types.first() {
                    if op_type.types.len() == 1 {
                        return analysis_data
                            .set_expr_type(pos, TUnion::new(TAtomic::TLiteralInt { value: -value }));
                    }
                }
                if op_type.types.iter().any(|t| {
                    matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
                }) {
                    TUnion::float()
                } else {
                    TUnion::int()
                }
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }

        UnaryPrefixOperator::Plus(_) => {
            if let Some(op_type) = operand_type {
                if op_type.types.iter().any(|t| {
                    matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
                }) {
                    TUnion::float()
                } else {
                    TUnion::int()
                }
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }

        // Bitwise not -> int
        UnaryPrefixOperator::BitwiseNot(_) => TUnion::int(),

        // Pre-increment/decrement - returns the modified value
        UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_) => {
            // Update the variable's type in context if this is a variable
            let result_type = get_increment_result_type(operand_type.as_deref());
            update_var_type_for_increment(analyzer, unary.operand, &result_type, context);
            result_type
        }

        // Error control (@) - type is same as operand
        UnaryPrefixOperator::ErrorControl(_) => operand_type
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed),

        // Reference (&) - type is same as operand
        UnaryPrefixOperator::Reference(_) => operand_type
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed),

        // Type casts - delegate to cast_analyzer
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            TUnion::int()
        }
        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),
        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            TUnion::string()
        }
        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }
        UnaryPrefixOperator::ArrayCast(_, _) => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        UnaryPrefixOperator::ObjectCast(_, _) => TUnion::new(TAtomic::TObject),
        UnaryPrefixOperator::UnsetCast(_, _) => TUnion::null(),
        UnaryPrefixOperator::VoidCast(_, _) => TUnion::void(),
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze a unary postfix expression.
pub fn analyze_postfix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPostfix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let operand_pos = expr_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let operand_type = analysis_data.get_expr_type(operand_pos);

    // Post increment/decrement returns the original value before modification
    let expr_type = operand_type
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Update the variable's type in context (the variable gets the incremented value)
    let new_var_type = get_increment_result_type(operand_type.as_deref());
    update_var_type_for_increment(analyzer, unary.operand, &new_var_type, context);

    analysis_data.set_expr_type(pos, expr_type);
}

/// Get the result type after incrementing/decrementing a value.
fn get_increment_result_type(operand_type: Option<&TUnion>) -> TUnion {
    match operand_type {
        Some(t) => {
            // If it's a literal int, we can't preserve the literal value since we don't
            // know if it's increment or decrement. Just return int.
            if t.types.iter().all(|a| {
                matches!(
                    a,
                    TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TNumeric
                )
            }) {
                TUnion::int()
            } else if t.types.iter().any(|a| {
                matches!(a, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
            }) {
                TUnion::float()
            } else {
                // PHP's increment behavior on non-numeric types is complex,
                // fall back to numeric
                TUnion::new(TAtomic::TNumeric)
            }
        }
        None => TUnion::new(TAtomic::TNumeric),
    }
}

/// Update a variable's type in the context after increment/decrement.
fn update_var_type_for_increment(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    new_type: &TUnion,
    context: &mut BlockContext,
) {
    if let Expression::Variable(var) = expr {
        if let Variable::Direct(direct) = var {
            let var_id = analyzer.interner.intern(direct.name);
            context.locals.insert(var_id, new_type.clone());
        }
    }
}
