//! `"array_rand"` return-type provider (mirrors Psalm's
//! ArrayRandReturnTypeProvider): the result is the array's key type (one key),
//! a list of keys (num > 1), or the union of both when num is unknown.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayRandReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayRandReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_rand"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let array_pos = event.arg_positions.first().copied()?;
        let array_type = analysis_data.expr_types.get(&array_pos).cloned()?;
        let array_info = fca::extract_array_like_info_from_union(&array_type)?;
        let key_type = fca::normalize_array_key_union(&array_info.key_type);

        let Some(num_pos) = event.arg_positions.get(1).copied() else {
            // No second arg: exactly one key.
            return Some(key_type);
        };

        let num_literal = analysis_data
            .expr_types
            .get(&num_pos)
            .cloned()
            .and_then(|num_type| match num_type.get_single() {
                Some(TAtomic::TLiteralInt { value }) => Some(*value),
                _ => None,
            });

        if num_literal == Some(1) {
            return Some(key_type);
        }

        let keys_list = TUnion::new(TAtomic::TList {
            value_type: Box::new(key_type.clone()),
        });

        if num_literal.is_some() {
            return Some(keys_list);
        }

        Some(combine_union_types(&key_type, &keys_list, false))
    }
}
