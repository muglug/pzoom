//! `"array_key_first", "array_key_last"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
pub(super) struct ArrayKeyFirstLastReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayKeyFirstLastReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_key_first", "array_key_last"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_key_first_last_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_array_key_first_last_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.expr_types.get(&array_pos).cloned()?;
    let array_info = fca::extract_array_like_info_from_union(&array_type)?;

    let key_type = fca::normalize_array_key_union(&array_info.key_type);
    if array_info.is_non_empty {
        return Some(key_type);
    }

    Some(combine_union_types(
        &key_type,
        &TUnion::new(TAtomic::TNull),
        false,
    ))
}
