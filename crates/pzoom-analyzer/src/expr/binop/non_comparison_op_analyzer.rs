//! Non-comparison binary operator analyzer (Psalm `NonComparisonOpAnalyzer` equivalent).

use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::expr::binop::{arithmetic_op_analyzer, concat_analyzer};
use crate::expr::binop_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    operator: &BinaryOperator,
    _left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
    left_type: Option<&TUnion>,
    right_type: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> TUnion {
    let addition_is_array_union = matches!(operator, BinaryOperator::Addition(_))
        && left_type.is_some_and(union_is_array_like)
        && right_type.is_some_and(union_is_array_like);

    let is_arithmetic_op = matches!(
        operator,
        BinaryOperator::Subtraction(_)
            | BinaryOperator::Multiplication(_)
            | BinaryOperator::Division(_)
            | BinaryOperator::Modulo(_)
            | BinaryOperator::Exponentiation(_)
    ) || matches!(operator, BinaryOperator::Addition(_))
        && !addition_is_array_union;

    if is_arithmetic_op {
        arithmetic_op_analyzer::emit_arithmetic_operand_issue(
            analyzer,
            left_type,
            pos,
            analysis_data,
        );
        arithmetic_op_analyzer::emit_arithmetic_operand_issue(
            analyzer,
            right_type,
            pos,
            analysis_data,
        );
    }

    if matches!(
        operator,
        BinaryOperator::BitwiseAnd(_)
            | BinaryOperator::BitwiseOr(_)
            | BinaryOperator::BitwiseXor(_)
            | BinaryOperator::LeftShift(_)
            | BinaryOperator::RightShift(_)
    ) {
        binop_analyzer::emit_bitwise_operand_issue(analyzer, left_type, pos, analysis_data);
        binop_analyzer::emit_bitwise_operand_issue(analyzer, right_type, pos, analysis_data);

        if matches!(
            operator,
            BinaryOperator::BitwiseAnd(_)
                | BinaryOperator::BitwiseOr(_)
                | BinaryOperator::BitwiseXor(_)
        ) && let (Some(left_union), Some(right_union)) = (left_type, right_type)
            && ((binop_analyzer::union_is_string_like_for_bitwise(left_union)
                && binop_analyzer::union_is_numeric_like_for_bitwise(right_union))
                || (binop_analyzer::union_is_numeric_like_for_bitwise(left_union)
                    && binop_analyzer::union_is_string_like_for_bitwise(right_union)))
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidOperand,
                format!(
                    "Cannot use bitwise operation on types {} and {}",
                    left_union.get_id(Some(analyzer.interner)),
                    right_union.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    if matches!(operator, BinaryOperator::StringConcat(_)) {
        concat_analyzer::emit_concat_operand_issue(analyzer, left_type, pos, analysis_data);
        concat_analyzer::emit_concat_operand_issue(analyzer, right_type, pos, analysis_data);
    }

    match operator {
        BinaryOperator::LowXor(_) => TUnion::bool(),
        BinaryOperator::Addition(_) => {
            if addition_is_array_union {
                match (left_type, right_type) {
                    (Some(lt), Some(rt)) => combine_union_types(lt, rt, true),
                    (Some(lt), None) => lt.clone(),
                    (None, Some(rt)) => rt.clone(),
                    (None, None) => TUnion::new(TAtomic::TArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(TUnion::mixed()),
                    }),
                }
            } else {
                arithmetic_op_analyzer::infer_arithmetic_type(left_type, right_type)
            }
        }
        BinaryOperator::Subtraction(_) | BinaryOperator::Multiplication(_) => {
            arithmetic_op_analyzer::infer_arithmetic_type(left_type, right_type)
        }
        BinaryOperator::Division(_) => TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat]),
        BinaryOperator::Modulo(_) => TUnion::int(),
        BinaryOperator::Exponentiation(_) => {
            arithmetic_op_analyzer::infer_arithmetic_type(left_type, right_type)
        }
        BinaryOperator::BitwiseAnd(_)
        | BinaryOperator::BitwiseOr(_)
        | BinaryOperator::BitwiseXor(_)
        | BinaryOperator::LeftShift(_)
        | BinaryOperator::RightShift(_) => {
            binop_analyzer::infer_bitwise_type(operator, left_type, right_type)
        }
        BinaryOperator::StringConcat(_) => {
            concat_analyzer::infer_concat_type(analyzer, left_type, right_type)
        }
        BinaryOperator::Instanceof(_) => {
            if let Some(left_union) = left_type
                && let Some(asserted_class_id) =
                    binop_analyzer::resolve_instanceof_class_id(analyzer, right_expr)
            {
                let (can_be_instance, always_instance) =
                    binop_analyzer::evaluate_instanceof_possibility(
                        analyzer,
                        left_union,
                        asserted_class_id,
                    );

                let rhs_is_explicit_class = matches!(right_expr.unparenthesized(), Expression::Identifier(_));

                if always_instance && rhs_is_explicit_class && !context.inside_loop {
                    let mut result = TUnion::new(TAtomic::TTrue);
                    result.from_docblock = left_union.from_docblock;
                    result
                } else if can_be_instance {
                    TUnion::bool()
                } else {
                    TUnion::bool()
                }
            } else {
                TUnion::bool()
            }
        }
        BinaryOperator::NullCoalesce(_) => match (left_type, right_type) {
            (Some(lt), Some(rt)) => {
                let left_without_null: Vec<_> = lt
                    .types
                    .iter()
                    .filter(|t| !matches!(t, TAtomic::TNull))
                    .cloned()
                    .collect();

                if left_without_null.is_empty() {
                    rt.clone()
                } else {
                    let left_non_null = TUnion::from_types(left_without_null);
                    combine_union_types(&left_non_null, rt, false)
                }
            }
            (Some(t), None) | (None, Some(t)) => t.clone(),
            (None, None) => TUnion::mixed(),
        },
        _ => TUnion::mixed(),
    }
}

/// Infer the result type of an arithmetic operation.
fn union_is_array_like(t: &TUnion) -> bool {
    !t.types.is_empty()
        && t.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TArray { .. }
                    | TAtomic::TNonEmptyArray { .. }
                    | TAtomic::TList { .. }
                    | TAtomic::TNonEmptyList { .. }
                    | TAtomic::TKeyedArray { .. }
            )
        })
}
