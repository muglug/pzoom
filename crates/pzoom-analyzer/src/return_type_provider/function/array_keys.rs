//! `"array_keys"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::expr::call::function_call_analyzer as fca;
pub(super) struct ArrayKeysReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayKeysReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_keys"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_keys_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_array_keys_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let array_info = fca::extract_array_like_info_from_union(&array_type)?;

    let key_type = fca::normalize_array_key_union(&array_info.key_type);
    let atomic = if array_info.is_non_empty {
        TAtomic::TNonEmptyList {
            value_type: Box::new(key_type),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(key_type),
        }
    };

    Some(TUnion::new(atomic))
}
