//! Binary operation analyzer.

use mago_syntax::ast::ast::binary::Binary;

use pzoom_code_info::{combine_union_types, Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a binary operation expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    binop: &Binary<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::binary::BinaryOperator;

    // Analyze both operands
    let left_pos = expr_analyzer::analyze(analyzer, binop.lhs, analysis_data, context);
    let right_pos = expr_analyzer::analyze(analyzer, binop.rhs, analysis_data, context);

    let left_type = analysis_data.get_expr_type(left_pos);
    let right_type = analysis_data.get_expr_type(right_pos);

    // Check for invalid operands for arithmetic operations
    let is_arithmetic_op = matches!(
        &binop.operator,
        BinaryOperator::Addition(_)
            | BinaryOperator::Subtraction(_)
            | BinaryOperator::Multiplication(_)
            | BinaryOperator::Division(_)
            | BinaryOperator::Modulo(_)
            | BinaryOperator::Exponentiation(_)
    );

    if is_arithmetic_op {
        // Check if operands are valid for arithmetic
        if let Some(lt) = &left_type {
            if is_invalid_arithmetic_operand(lt) {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidOperand,
                    format!(
                        "Cannot use arithmetic on type {}",
                        lt.get_id()
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
        if let Some(rt) = &right_type {
            if is_invalid_arithmetic_operand(rt) {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidOperand,
                    format!(
                        "Cannot use arithmetic on type {}",
                        rt.get_id()
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    let result_type = match &binop.operator {
        // Comparison operators -> bool
        BinaryOperator::Equal(_)
        | BinaryOperator::NotEqual(_)
        | BinaryOperator::AngledNotEqual(_)
        | BinaryOperator::Identical(_)
        | BinaryOperator::NotIdentical(_)
        | BinaryOperator::LessThan(_)
        | BinaryOperator::LessThanOrEqual(_)
        | BinaryOperator::GreaterThan(_)
        | BinaryOperator::GreaterThanOrEqual(_)
        | BinaryOperator::Spaceship(_) => {
            // For spaceship, result is -1, 0, or 1 (int), but we'll simplify to int
            if matches!(binop.operator, BinaryOperator::Spaceship(_)) {
                TUnion::int()
            } else {
                TUnion::bool()
            }
        }

        // Logical operators -> bool
        BinaryOperator::And(_)
        | BinaryOperator::Or(_)
        | BinaryOperator::LowAnd(_)
        | BinaryOperator::LowOr(_)
        | BinaryOperator::LowXor(_) => TUnion::bool(),

        // Arithmetic operators
        BinaryOperator::Addition(_)
        | BinaryOperator::Subtraction(_)
        | BinaryOperator::Multiplication(_) => {
            infer_arithmetic_type(left_type.as_deref(), right_type.as_deref())
        }

        BinaryOperator::Division(_) => {
            // Division can return int or float
            TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat])
        }

        BinaryOperator::Modulo(_) => TUnion::int(),

        BinaryOperator::Exponentiation(_) => {
            infer_arithmetic_type(left_type.as_deref(), right_type.as_deref())
        }

        // Bitwise operators -> int
        BinaryOperator::BitwiseAnd(_)
        | BinaryOperator::BitwiseOr(_)
        | BinaryOperator::BitwiseXor(_)
        | BinaryOperator::LeftShift(_)
        | BinaryOperator::RightShift(_) => TUnion::int(),

        // String concatenation
        BinaryOperator::StringConcat(_) => TUnion::string(),

        // Instanceof -> bool
        BinaryOperator::Instanceof(_) => TUnion::bool(),

        // Null coalescing - returns union of left (minus null) and right
        BinaryOperator::NullCoalesce(_) => {
            match (left_type, right_type) {
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
                (Some(t), None) | (None, Some(t)) => (*t).clone(),
                (None, None) => TUnion::mixed(),
            }
        }
    };

    analysis_data.set_expr_type(pos, result_type);
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

/// Check if a type is invalid for arithmetic operations.
fn is_invalid_arithmetic_operand(t: &TUnion) -> bool {
    t.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
                | TAtomic::TNamedObject { .. }
                | TAtomic::TObject
                | TAtomic::TCallable { .. }
                | TAtomic::TClosure { .. }
                | TAtomic::TResource
                | TAtomic::TClosedResource
                | TAtomic::TVoid
                | TAtomic::TNull
                // Strings (except numeric strings) cannot be used in arithmetic
                // Note: TLiteralString could be numeric but we're conservative
                | TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
        )
    })
}
