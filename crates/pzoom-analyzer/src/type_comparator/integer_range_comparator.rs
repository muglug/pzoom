//! Integer range comparator.
//!
//! Mirrors Psalm's `IntegerRangeComparator`: whether one `int<min,max>` range is
//! fully contained by another (`is_contained_by`), or covered by a *union* of
//! int atomics (`is_contained_by_union`). `None` bounds denote an open end
//! (`min` / `max`).

use pzoom_code_info::{TAtomic, TUnion};

/// Returns true when the input range `[input_min, input_max]` is a subset of the
/// container range `[container_min, container_max]`. A `None` container bound
/// accepts any input on that side; a `None` input bound against a bounded
/// container fails (the input is wider than the container there).
pub(crate) fn is_contained_by(
    input_min: Option<i64>,
    input_max: Option<i64>,
    container_min: Option<i64>,
    container_max: Option<i64>,
) -> bool {
    let min_ok = match (container_min, input_min) {
        (Some(c), Some(i)) => i >= c,
        (Some(_), None) => false,
        (None, _) => true,
    };
    let max_ok = match (container_max, input_max) {
        (Some(c), Some(i)) => i <= c,
        (Some(_), None) => false,
        (None, _) => true,
    };
    min_ok && max_ok
}

/// An int-like container atomic, normalized for range reduction.
#[derive(Clone, Copy)]
enum IntAtomic {
    /// `int<min, max>` (`None` = open end). `positive-int` = `(Some(1), None)`,
    /// `negative-int` = `(None, Some(-1))`, whole `int` = `(None, None)`.
    Range(Option<i64>, Option<i64>),
    Literal(i64),
}

fn normalize_int_atomic(atomic: &TAtomic) -> Option<IntAtomic> {
    match atomic {
        TAtomic::TInt => Some(IntAtomic::Range(None, None)),
        TAtomic::TLiteralInt { value } => Some(IntAtomic::Literal(*value)),
        TAtomic::TIntRange { min, max } => Some(IntAtomic::Range(*min, *max)),
        _ => None,
    }
}

fn range_contains(min: Option<i64>, max: Option<i64>, value: i64) -> bool {
    min.is_none_or(|m| value >= m) && max.is_none_or(|m| value <= m)
}

/// Whether an `int<input_min, input_max>` range is fully covered by the union of
/// the int atomics in `container`. Faithful port of Psalm's
/// `IntegerRangeComparator::isContainedByUnion` + `reduceRangeIncrementally`:
/// reduce the input range using each container int atomic until it empties
/// (covered) or no further reduction is possible (inconclusive => not covered).
pub(crate) fn is_contained_by_union(
    input_min: Option<i64>,
    input_max: Option<i64>,
    container: &TUnion,
) -> bool {
    // A whole `int` in the container covers any integer range (Psalm's `int`
    // key short-circuit).
    if container
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TInt))
    {
        return true;
    }

    let mut atomics: Vec<IntAtomic> = container
        .types
        .iter()
        .filter_map(normalize_int_atomic)
        .collect();

    let mut reduced_min = input_min;
    let mut reduced_max = input_max;

    // Loop until stable or a definite result, mirroring Psalm's do-while.
    loop {
        let before = atomics.len();
        if let Some(result) =
            reduce_range_incrementally(&mut atomics, &mut reduced_min, &mut reduced_max)
        {
            return result;
        }
        if atomics.len() == before {
            break;
        }
    }

    // Inconclusive and no atomics left to apply => not contained.
    false
}

/// One reduction pass. Returns `Some(true)`/`Some(false)` for a definite
/// result, or `None` when inconclusive for this round. Mirrors Psalm's
/// `reduceRangeIncrementally` (including its early `false` when a bounded
/// container range does not contain the reduced range).
fn reduce_range_incrementally(
    atomics: &mut Vec<IntAtomic>,
    reduced_min: &mut Option<i64>,
    reduced_max: &mut Option<i64>,
) -> Option<bool> {
    let mut kept: Vec<IntAtomic> = Vec::with_capacity(atomics.len());

    for atomic in std::mem::take(atomics) {
        match atomic {
            IntAtomic::Range(cmin, cmax) => {
                if is_contained_by(*reduced_min, *reduced_max, cmin, cmax) {
                    match (cmin, cmax) {
                        // Covers any integer.
                        (None, None) => return Some(true),
                        // `int<X, max>`: X-1 becomes the max of the reduced range
                        // if it was higher.
                        (Some(x), None) => {
                            let candidate = x - 1;
                            *reduced_max =
                                Some(reduced_max.map_or(candidate, |m| m.min(candidate)));
                        }
                        // `int<min, X>`: X+1 becomes the min of the reduced range
                        // if it was lower.
                        (None, Some(x)) => {
                            let candidate = x + 1;
                            *reduced_min =
                                Some(reduced_min.map_or(candidate, |m| m.max(candidate)));
                        }
                        // Fully-bounded container range: trim whichever reduced
                        // bound it contains.
                        (Some(cmin_v), Some(cmax_v)) => {
                            if let Some(rmin) = *reduced_min
                                && range_contains(Some(cmin_v), Some(cmax_v), rmin)
                            {
                                *reduced_min = Some(cmax_v + 1);
                            } else if let Some(rmax) = *reduced_max
                                && range_contains(Some(cmin_v), Some(cmax_v), rmax)
                            {
                                *reduced_max = Some(cmin_v - 1);
                            }
                        }
                    }
                    // consumed: not kept
                } else {
                    // The reduced range is wider than this container range.
                    return Some(false);
                }
            }
            IntAtomic::Literal(value) => {
                if !range_contains(*reduced_min, *reduced_max, value) {
                    // outside the reduced range: drop it
                } else if *reduced_min == Some(value) {
                    *reduced_min = Some(value + 1);
                } else if *reduced_max == Some(value) {
                    *reduced_max = Some(value - 1);
                } else {
                    kept.push(atomic);
                }
            }
        }
    }

    *atomics = kept;

    // If the reduced range's min exceeds its max, the container covered it all.
    if let (Some(lo), Some(hi)) = (*reduced_min, *reduced_max)
        && lo > hi
    {
        return Some(true);
    }

    None
}
