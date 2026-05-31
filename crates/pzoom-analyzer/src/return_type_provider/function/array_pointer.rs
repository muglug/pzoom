//! `current` / `reset` / `end` return-type provider.
//!
//! An empty array yields `false`; a non-empty array yields the value type; a
//! possibly-empty array yields `value|false` with falsable issues ignored (so
//! returning the result where a non-false type is expected is not flagged).

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayPointerReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayPointerReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["current", "reset", "end"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let pos = event.arg_positions.first().copied()?;
        let arr_type = analysis_data.get_expr_type(pos)?;
        let info = fca::extract_array_like_info_from_union(&arr_type)?;

        let value_type = info.value_type;
        if info.is_non_empty {
            return Some(value_type);
        }

        if value_type.is_nothing() {
            return Some(TUnion::new(TAtomic::TFalse));
        }

        let mut result = value_type;
        result.add_type(TAtomic::TFalse);
        result.ignore_falsable_issues = true;
        Some(result)
    }
}
