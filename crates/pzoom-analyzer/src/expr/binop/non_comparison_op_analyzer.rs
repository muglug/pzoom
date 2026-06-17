//! Non-comparison binary operator analyzer (Psalm `NonComparisonOpAnalyzer` equivalent).

use mago_syntax::ast::ast::binary::BinaryOperator;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::expr::binop::{arithmetic_op_analyzer, concat_analyzer};
use crate::expr::binop_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    operator: &BinaryOperator,
    left_type: Option<&TUnion>,
    right_type: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
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

    // Precise literal-folding / int-range propagation (Psalm
    // ArithmeticOpAnalyzer), falling back to the generic per-operator result.
    if !addition_is_array_union
        && let Some(op) = arithmetic_op_analyzer::arith_op(operator)
        && let Some(precise) = arithmetic_op_analyzer::infer_precise_arithmetic_result(
            op,
            left_type,
            right_type,
            inside_loop,
        )
    {
        return precise;
    }

    match operator {
        BinaryOperator::LowXor(_) => TUnion::bool(),
        BinaryOperator::Addition(_) => {
            if addition_is_array_union {
                // Psalm's pre-atomic operand checks still flag a nullable
                // operand of an array `+` (its own config suppresses the
                // issue repo-wide).
                for operand in [left_type, right_type].into_iter().flatten() {
                    if operand
                        .types
                        .iter()
                        .any(|atomic| matches!(atomic, TAtomic::TNull))
                        && !operand.ignore_nullable_issues
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::PossiblyNullOperand,
                            format!(
                                "Cannot use arithmetic on possibly null type {}",
                                operand.get_id(Some(analyzer.interner))
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
                match (left_type, right_type) {
                    (Some(lt), Some(rt)) => {
                        let left_operand = array_union_operand(lt);
                        let right_operand = array_union_operand(rt);
                        if let Some(merged) = keyed_array_plus(&left_operand, &right_operand) {
                            merged
                        } else {
                            combine_union_types(&left_operand, &right_operand, true)
                        }
                    }
                    (Some(lt), None) => lt.clone(),
                    (None, Some(rt)) => rt.clone(),
                    (None, None) => {
                        TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed()))
                    }
                }
            } else {
                arithmetic_op_analyzer::infer_arithmetic_type(left_type, right_type)
            }
        }
        BinaryOperator::Subtraction(_) | BinaryOperator::Multiplication(_) => {
            arithmetic_op_analyzer::infer_arithmetic_type(left_type, right_type)
        }
        BinaryOperator::Division(_) => {
            arithmetic_op_analyzer::infer_division_type(left_type, right_type)
        }
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
            // Psalm's ConcatAnalyzer: stringifying an object operand in a
            // mutation-free context flags a non-mutation-free __toString.
            if crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer) {
                for operand_type in [left_type, right_type].into_iter().flatten() {
                    binop_analyzer::emit_impure_to_string_for_union(
                        analyzer,
                        operand_type,
                        pos,
                        analysis_data,
                    );
                }
            }

            concat_analyzer::infer_concat_type(analyzer, left_type, right_type)
        }
        BinaryOperator::Instanceof(_) => {
            // Psalm types an `instanceof` expression as plain bool even when
            // it is statically certain; the always-true/always-false verdict
            // is the reconciler's to report ("Type X for $e is always Y").
            // Folding to `true` here double-reported as a truthy operand.
            TUnion::bool()
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

/// Whether `+` on this operand is the array-union `+`. Psalm's
/// ArithmeticOpAnalyzer analyzes per-atomic and skips null/false atomics
/// (they only feed the Possibly{Null,False}Operand checks), so
/// `array|null + array` still array-unions rather than collapsing to
/// numeric addition.
fn union_is_array_like(t: &TUnion) -> bool {
    let mut has_array = false;
    for atomic in &t.types {
        if atomic.is_array() {
            has_array = true;
        } else if matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing) {
            // null/false/never operands only feed the Possibly* operand checks.
        } else {
            return false;
        }
    }
    has_array
}

/// The array atomics of an operand already vetted by
/// [`union_is_array_like`]: null/false members are dropped from the
/// array-union result (Psalm skips those atomics).
/// Psalm's ArithmeticOpAnalyzer `+` over two keyed shapes: left's properties
/// win; right-only keys are added; a left key that is possibly undefined but
/// defined on the right combines both types and takes the right's
/// definedness.
fn keyed_array_plus(left: &TUnion, right: &TUnion) -> Option<TUnion> {
    // TODO(unify-array): the pre-unification version fired only when both
    // operands were `TKeyedArray` (shapes), routing generic `array`/`list`
    // operands to `combine_union_types`. Under the unified model every array is
    // a `TArray`, so this now also handles generic-array pairs through the same
    // known_values/params merge — which agrees with `combine_union_types` for
    // those cases (identical operands clone; generic + generic re-combines the
    // params).
    let (
        TAtomic::TArray {
            known_values: left_known,
            params: left_params,
            is_list: left_is_list,
            is_sealed: left_sealed,
            ..
        },
        TAtomic::TArray {
            known_values: right_known,
            params: right_params,
            is_list: right_is_list,
            is_sealed: right_sealed,
            ..
        },
    ) = (left.get_single()?, right.get_single()?)
    else {
        return None;
    };

    let mut known_values = (**left_known).clone();
    for (key, (right_undefined, right_value)) in right_known.iter() {
        match known_values.get(key) {
            None => {
                // A left side with fallback params may already hold the key
                // with any value, so a right-only key combines with mixed
                // (Psalm's definitely_existing_mixed_right_properties). The
                // entry keeps the right side's possibly-undefined flag.
                if left_params.is_some() {
                    let combined = combine_union_types(&TUnion::mixed(), right_value, false);
                    known_values.insert(key.clone(), (*right_undefined, combined));
                } else {
                    known_values.insert(key.clone(), (*right_undefined, right_value.clone()));
                }
            }
            Some((left_undefined, left_value)) if *left_undefined => {
                let combined = combine_union_types(left_value, right_value, false);
                known_values.insert(key.clone(), (*right_undefined, combined));
            }
            _ => {}
        }
    }

    // Old `fallback_key_type`/`fallback_value_type` travelled separately; in the
    // unified model the key/value fallback is a single `params` pair (both
    // present or neither), so combining is symmetric across the pair.
    let combined_params = match (left_params, right_params) {
        (None, None) => None,
        (Some(left_fb), Some(right_fb)) => Some(Box::new((
            combine_union_types(&left_fb.0, &right_fb.0, false),
            combine_union_types(&left_fb.1, &right_fb.1, false),
        ))),
        (Some(fb), None) | (None, Some(fb)) => Some(fb.clone()),
    };

    Some(TUnion::new(TAtomic::keyed_array_arc(
        std::sync::Arc::new(known_values),
        *left_is_list && *right_is_list,
        *left_sealed && *right_sealed,
        combined_params,
    )))
}

fn array_union_operand(t: &TUnion) -> TUnion {
    let arrays: Vec<TAtomic> = t
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing))
        .cloned()
        .collect();
    if arrays.is_empty() {
        t.clone()
    } else {
        TUnion::from_types(arrays)
    }
}
