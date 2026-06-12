//! Arithmetic operation helpers for binary operators.

use mago_syntax::ast::ast::binary::BinaryOperator;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub use super::arithmetic_analyzer::analyze;

/// The arithmetic operators with a numeric result, normalized from the AST.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

pub(crate) fn arith_op(operator: &BinaryOperator) -> Option<ArithOp> {
    match operator {
        BinaryOperator::Addition(_) => Some(ArithOp::Add),
        BinaryOperator::Subtraction(_) => Some(ArithOp::Sub),
        BinaryOperator::Multiplication(_) => Some(ArithOp::Mul),
        BinaryOperator::Division(_) => Some(ArithOp::Div),
        BinaryOperator::Modulo(_) => Some(ArithOp::Mod),
        BinaryOperator::Exponentiation(_) => Some(ArithOp::Pow),
        _ => None,
    }
}

fn int_range_of(atomic: &TAtomic) -> Option<(Option<i64>, Option<i64>)> {
    match atomic {
        TAtomic::TInt => Some((None, None)),
        TAtomic::TLiteralInt { value } => Some((Some(*value), Some(*value))),
        TAtomic::TIntRange { min, max } => Some((*min, *max)),
        _ => None,
    }
}

/// Build the int result of combining two bounds. `None` bounds stay open.
fn add_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => a.checked_add(b),
        _ => None,
    }
}

fn sub_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => a.checked_sub(b),
        _ => None,
    }
}

fn mul_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => a.checked_mul(b),
        _ => None,
    }
}

/// Range predicates mirroring Psalm's `TIntRange::isPositive`/`isNegative`/
/// `isPositiveOrZero`/`isNegativeOrZero`. A `None` (open) bound makes the
/// predicate false on that side.
fn r_is_positive((min, _): (Option<i64>, Option<i64>)) -> bool {
    matches!(min, Some(m) if m > 0)
}

fn r_is_negative((_, max): (Option<i64>, Option<i64>)) -> bool {
    matches!(max, Some(m) if m < 0)
}

fn r_is_positive_or_zero((min, _): (Option<i64>, Option<i64>)) -> bool {
    matches!(min, Some(m) if m >= 0)
}

fn r_is_negative_or_zero((_, max): (Option<i64>, Option<i64>)) -> bool {
    matches!(max, Some(m) if m <= 0)
}

/// The single value when a range is a literal (`min == max`), else `None`.
fn r_literal((min, max): (Option<i64>, Option<i64>)) -> Option<i64> {
    match (min, max) {
        (Some(a), Some(b)) if a == b => Some(a),
        _ => None,
    }
}

/// An int range union that never collapses to a literal (loop widening).
fn int_range_widened(min: Option<i64>, max: Option<i64>) -> TUnion {
    if min.is_none() && max.is_none() {
        TUnion::int_from_calculation()
    } else {
        TUnion::new(TAtomic::TIntRange { min, max })
    }
}

/// Build a `TUnion` for an int range, collapsing a fully-open range to plain
/// `int` (from calculation), mirroring how Psalm renders `int<min, max>`.
fn int_range_union(min: Option<i64>, max: Option<i64>) -> TUnion {
    if min.is_none() && max.is_none() {
        TUnion::int_from_calculation()
    } else if let (Some(min_value), Some(max_value)) = (min, max)
        && min_value == max_value
    {
        // A fully-determined result is Psalm's calculation-derived literal.
        literal_from_calculation(min_value)
    } else {
        TUnion::new(TAtomic::TIntRange { min, max })
    }
}

/// `int|float`, Psalm's result when an integer pow can overflow to float.
fn int_or_float() -> TUnion {
    TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat])
}

/// A literal int flagged `from_calculation` (Psalm's `Type::getInt(true, N)`).
fn literal_from_calculation(value: i64) -> TUnion {
    let mut union = TUnion::new(TAtomic::TLiteralInt { value });
    union.from_calculation = true;
    union
}

/// Propagate an int range through `+`/`-`, mirroring the generic branch of
/// Psalm's `analyzeOperandsBetweenIntRange`. Returns a `TIntRange` (or whole
/// `int` when both bounds are open).
fn add_sub_range_result(
    op: ArithOp,
    (lmin, lmax): (Option<i64>, Option<i64>),
    (rmin, rmax): (Option<i64>, Option<i64>),
) -> TUnion {
    let (min, max) = match op {
        ArithOp::Add => (add_opt(lmin, rmin), add_opt(lmax, rmax)),
        // min = lmin - rmax, max = lmax - rmin
        ArithOp::Sub => (sub_opt(lmin, rmax), sub_opt(lmax, rmin)),
        _ => (None, None),
    };
    int_range_union(min, max)
}

/// Multiplication of two int ranges, mirroring Psalm's
/// `analyzeMulBetweenIntRange`. Multiplication is a special case because of
/// sign interplay: when all four bounds are known we take the min/max over the
/// four corner products; otherwise we reason from the operand signs.
fn mul_range_result(
    left @ (lmin, lmax): (Option<i64>, Option<i64>),
    right @ (rmin, rmax): (Option<i64>, Option<i64>),
) -> TUnion {
    // Everything known: exact min/max over the four corner products.
    if let (Some(x1), Some(x2), Some(y1), Some(y2)) = (rmin, rmax, lmin, lmax) {
        let products = [
            x1.checked_mul(y1),
            x1.checked_mul(y2),
            x2.checked_mul(y1),
            x2.checked_mul(y2),
        ];
        if products.iter().any(Option::is_none) {
            return TUnion::int_from_calculation();
        }
        let values: Vec<i64> = products.into_iter().flatten().collect();
        return int_range_union(values.iter().copied().min(), values.iter().copied().max());
    }

    // Psalm swaps min/max when the sign analysis inverts them.
    let ordered = |min_v: Option<i64>, max_v: Option<i64>| match (min_v, max_v) {
        (Some(a), Some(b)) if a > b => (max_v, min_v),
        _ => (min_v, max_v),
    };

    if r_is_positive_or_zero(right) && r_is_positive_or_zero(left) {
        int_range_union(mul_opt(lmin, rmin), mul_opt(lmax, rmax))
    } else if r_is_positive_or_zero(right) && r_is_negative_or_zero(left) {
        let (min_v, max_v) = ordered(mul_opt(lmax, rmin), mul_opt(lmin, rmax));
        int_range_union(min_v, max_v)
    } else if r_is_negative_or_zero(right) && r_is_positive_or_zero(left) {
        let (min_v, max_v) = ordered(mul_opt(lmin, rmax), mul_opt(lmax, rmin));
        int_range_union(min_v, max_v)
    } else if r_is_negative_or_zero(right) && r_is_negative_or_zero(left) {
        int_range_union(mul_opt(lmax, rmax), mul_opt(lmin, rmin))
    } else {
        TUnion::int_from_calculation()
    }
}

/// Modulo of two int ranges, mirroring Psalm's `analyzeModBetweenIntRange`.
/// The sign of `%` follows the dividend (left); the magnitude is bounded by the
/// divisor (right). A literal `0` divisor yields `never` (Psalm's `NoValue`).
fn mod_range_result(
    left: (Option<i64>, Option<i64>),
    right: (Option<i64>, Option<i64>),
) -> TUnion {
    // Divisor is a single literal value: we can be precise.
    if let Some(rv) = r_literal(right) {
        if rv == 0 {
            return TUnion::nothing();
        }
        return if r_is_positive_or_zero(left) {
            if rv > 0 {
                int_range_union(Some(0), Some(rv - 1))
            } else {
                int_range_union(Some(rv + 1), Some(0))
            }
        } else if r_is_negative_or_zero(left) {
            if rv > 0 {
                int_range_union(Some(-(rv - 1)), Some(0))
            } else {
                int_range_union(Some(-(rv + 1)), Some(0))
            }
        } else {
            let max = if rv > 0 { rv - 1 } else { -rv - 1 };
            int_range_union(Some(-max), Some(max))
        };
    }

    if r_is_positive(right) {
        if r_is_positive_or_zero(left) {
            match right.1 {
                Some(rmax) => int_range_union(Some(0), Some(rmax - 1)),
                None => int_range_union(Some(0), None),
            }
        } else if r_is_negative_or_zero(left) {
            int_range_union(None, Some(0))
        } else {
            TUnion::int_from_calculation()
        }
    } else if r_is_negative(right) {
        if r_is_positive_or_zero(left) || r_is_negative_or_zero(left) {
            int_range_union(None, Some(0))
        } else {
            TUnion::int_from_calculation()
        }
    } else {
        TUnion::int_from_calculation()
    }
}

/// Exponentiation of two int ranges, mirroring Psalm's
/// `analyzePowBetweenIntRange`. A negative exponent forces a float result; an
/// exponent of `0` yields `1` (or `-1` for a negative base of magnitude 1 — see
/// Psalm). Even/odd literal exponents decide the sign for a negative base.
fn pow_range_result(
    left: (Option<i64>, Option<i64>),
    right: (Option<i64>, Option<i64>),
) -> TUnion {
    let right_zero = right == (Some(0), Some(0));
    let left_zero = left == (Some(0), Some(0));
    let right_even_literal = matches!(r_literal(right), Some(v) if v % 2 == 0);

    if r_is_positive(left) {
        if r_is_positive(right) {
            int_range_union(Some(1), None)
        } else if r_is_negative(right) {
            TUnion::float()
        } else if right_zero {
            literal_from_calculation(1)
        } else {
            int_or_float()
        }
    } else if r_is_negative(left) {
        if r_is_positive(right) {
            if r_literal(right).is_some() {
                if right_even_literal {
                    int_range_union(Some(1), None)
                } else {
                    int_range_union(None, Some(-1))
                }
            } else {
                TUnion::int_from_calculation()
            }
        } else if r_is_negative(right) {
            TUnion::float()
        } else if right_zero {
            literal_from_calculation(-1)
        } else {
            int_or_float()
        }
    } else if left_zero {
        if r_is_positive(right) {
            literal_from_calculation(0)
        } else if right_zero {
            literal_from_calculation(1)
        } else if r_is_negative(right) {
            TUnion::float()
        } else {
            // 0 ** (mix of negative/0/positive) => float | 0 | 1
            let mut union = TUnion::from_types(vec![
                TAtomic::TFloat,
                TAtomic::TLiteralInt { value: 0 },
                TAtomic::TLiteralInt { value: 1 },
            ]);
            union.from_calculation = true;
            union
        }
    } else {
        // Base spans zero.
        if r_is_positive(right) {
            if right_even_literal {
                int_range_union(Some(1), None)
            } else {
                TUnion::int_from_calculation()
            }
        } else if r_is_negative(right) {
            TUnion::float()
        } else if right_zero {
            literal_from_calculation(1)
        } else {
            int_or_float()
        }
    }
}

/// Compute a precise arithmetic result when both operands are single atomics.
///
/// Int-range propagation is applied for `+`, `-`, `*`, `%` and `**`, mirroring
/// Psalm's `ArithmeticOpAnalyzer` (`analyzeOperandsBetweenIntRange` and its
/// `analyzeMul/Mod/PowBetweenIntRange` helpers). `/` is left to the generic
/// `int|float` inference. Literal+literal operands fold to a literal flagged
/// `from_calculation` (Psalm's `Type::getInt(true, N)`); the comparison checks
/// skip calculation-derived operands, so `5 + 3 === 8` stays silent like Psalm.
/// Returns `None` to fall back to the generic per-operator inference.
pub(crate) fn infer_precise_arithmetic_result(
    op: ArithOp,
    left: Option<&TUnion>,
    right: Option<&TUnion>,
    inside_loop: bool,
) -> Option<TUnion> {
    let (left, right) = (left?, right?);
    let (left_atomic, right_atomic) = (left.get_single()?, right.get_single()?);

    let (Some(lr), Some(rr)) = (int_range_of(left_atomic), int_range_of(right_atomic)) else {
        return None;
    };

    // Inside a loop, literal +/- literal widens to a half-open range (Psalm's
    // ArithmeticOpAnalyzer: a loop-carried `$a = $a + 1` must not stay a
    // literal across iterations).
    if inside_loop
        && matches!(left_atomic, TAtomic::TLiteralInt { .. })
        && matches!(right_atomic, TAtomic::TLiteralInt { .. })
    {
        match op {
            ArithOp::Add => {
                return Some(int_range_widened(add_opt(lr.0, rr.0), None));
            }
            ArithOp::Sub => {
                return Some(int_range_widened(None, sub_opt(lr.1, rr.0)));
            }
            _ => {}
        }
    }

    match op {
        ArithOp::Add | ArithOp::Sub => Some(add_sub_range_result(op, lr, rr)),
        ArithOp::Mul => Some(mul_range_result(lr, rr)),
        ArithOp::Mod => Some(mod_range_result(lr, rr)),
        ArithOp::Pow => Some(pow_range_result(lr, rr)),
        ArithOp::Div => None,
    }
}

pub(crate) fn infer_arithmetic_type(left: Option<&TUnion>, right: Option<&TUnion>) -> TUnion {
    match (left, right) {
        (Some(lt), Some(rt)) => {
            if union_has_float(lt) || union_has_float(rt) {
                TUnion::float()
            } else {
                TUnion::int_from_calculation()
            }
        }
        _ => TUnion::new(TAtomic::TNumeric),
    }
}

fn union_has_float(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
}

/// Result type of `/`. Division yields `int|float` for integer operands (PHP
/// returns int when evenly divisible, float otherwise), but a `float` operand
/// forces a `float` result. Mirrors Psalm's ArithmeticOpAnalyzer division
/// handling.
pub(crate) fn infer_division_type(left: Option<&TUnion>, right: Option<&TUnion>) -> TUnion {
    // Psalm divides int literals exactly: an even division stays a literal
    // int (`67108864 / 1024 / 1024` is 64), otherwise the result is float.
    if let (Some(lt), Some(rt)) = (left, right)
        && let Some(TAtomic::TLiteralInt { value: lhs }) = lt.get_single()
        && let Some(TAtomic::TLiteralInt { value: rhs }) = rt.get_single()
        && *rhs != 0
    {
        return if lhs % rhs == 0 {
            TUnion::new(TAtomic::TLiteralInt { value: lhs / rhs })
        } else {
            TUnion::float()
        };
    }
    match (left, right) {
        (Some(lt), Some(rt)) if union_has_float(lt) || union_has_float(rt) => TUnion::float(),
        _ => TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat]),
    }
}

pub(crate) fn emit_arithmetic_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    union: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(union) = union else {
        return;
    };

    if union.is_null() {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::NullOperand,
            "Cannot use arithmetic on null".to_string(),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    if union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNull))
        && !union.ignore_nullable_issues
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullOperand,
            format!(
                "Cannot use arithmetic on possibly null type {}",
                union.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if union.is_single() && matches!(union.get_single(), Some(TAtomic::TFalse)) {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::FalseOperand,
            "Cannot use arithmetic on false".to_string(),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    // Psalm's ArithmeticOpAnalyzer reports a mixed operand as MixedOperand
    // and skips the remaining validation for it.
    if union
        .types
        .iter()
        .any(|atomic| matches!(
            atomic,
            TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset
        ))
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedOperand,
            "Operand cannot be mixed".to_string(),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    let mut has_valid = false;
    let mut has_invalid = false;
    for atomic in &union.types {
        if matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing) {
            continue;
        }

        if is_valid_arithmetic_atomic(atomic) {
            has_valid = true;
        } else {
            has_invalid = true;
        }
    }

    if !has_invalid {
        return;
    }

    let kind = if has_valid {
        IssueKind::PossiblyInvalidOperand
    } else {
        IssueKind::InvalidOperand
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        format!(
            "Cannot use arithmetic on type {}",
            union.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn is_valid_arithmetic_atomic(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TIntRange { .. }
        | TAtomic::TFloat
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TNumeric
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString => true,
        TAtomic::TLiteralString { value } => value.parse::<f64>().is_ok(),
        TAtomic::TTemplateParam { as_type, .. } => {
            !as_type.types.is_empty() && as_type.types.iter().all(is_valid_arithmetic_atomic)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => is_valid_arithmetic_atomic(as_type),
        _ => false,
    }
}
