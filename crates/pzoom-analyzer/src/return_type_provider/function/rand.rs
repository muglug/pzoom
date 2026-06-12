//! `rand`/`mt_rand`/`random_int` return-type provider.
//!
//! Mirrors Psalm's `RandReturnTypeProvider`: with two arguments whose bounds are
//! known literal ints (or int ranges), the result is the corresponding
//! `int<min, max>` range.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct RandReturnTypeProvider;

impl FunctionReturnTypeProvider for RandReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["rand", "mt_rand", "random_int"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        if event.arg_positions.is_empty() {
            return Some(TUnion::int());
        }

        if event.arg_positions.len() != 2 {
            return None;
        }

        let min_value = bound_from_arg(analysis_data, event, 0, true);
        let max_value = bound_from_arg(analysis_data, event, 1, false);

        Some(TUnion::new(TAtomic::TIntRange {
            min: min_value,
            max: max_value,
        }))
    }
}

/// Extract the relevant bound (min for arg 0, max for arg 1) from a single-atomic
/// literal int or int range argument; `None` means the bound is unconstrained.
fn bound_from_arg(
    analysis_data: &FunctionAnalysisData,
    event: &FunctionReturnTypeProviderEvent<'_, '_>,
    index: usize,
    is_min: bool,
) -> Option<i64> {
    let arg_type = analysis_data.expr_types.get(&event.arg_positions[index]).cloned()?;
    let atomic = arg_type.get_single()?;
    match atomic {
        TAtomic::TLiteralInt { value } => Some(*value),
        TAtomic::TIntRange { min, max } => {
            if is_min {
                *min
            } else {
                *max
            }
        }
        _ => None,
    }
}
