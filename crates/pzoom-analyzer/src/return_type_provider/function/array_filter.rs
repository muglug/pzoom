//! `"array_filter"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use mago_syntax::ast::ast::argument::Argument;
use crate::expr::call::function_call_analyzer as fca;
pub(super) struct ArrayFilterReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayFilterReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_filter"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_filter_return_type(event.args, event.arg_positions, analysis_data)
    }
}

fn infer_array_filter_return_type(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let callback_is_default = fca::is_default_array_filter_callback(args, arg_positions, analysis_data);

    let mut filtered_types = Vec::new();

    for atomic in &array_type.types {
        let Some(filtered_atomic) = fca::infer_array_filter_return_atomic(atomic, callback_is_default)
        else {
            continue;
        };

        if !filtered_types.contains(&filtered_atomic) {
            filtered_types.push(filtered_atomic);
        }
    }

    if filtered_types.is_empty() {
        let array_info = fca::extract_array_like_info_from_union(&array_type)?;

        let key_type = if array_info.key_type.is_nothing() {
            TUnion::array_key()
        } else {
            fca::normalize_array_key_union(&array_info.key_type)
        };

        let value_type = if callback_is_default {
            fca::narrow_union_to_truthy(&array_info.value_type)
        } else {
            array_info.value_type
        };

        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(key_type),
            value_type: Box::new(value_type),
        }));
    }

    Some(TUnion::from_types(filtered_types))
}
