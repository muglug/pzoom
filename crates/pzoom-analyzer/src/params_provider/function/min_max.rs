//! `max`/`min` single-argument callmap variant (Psalm's CallMap `max'0`):
//! `max($array)` takes a non-empty-array — passing a possibly-empty array is
//! an ArgumentTypeCoercion.

use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_str::StrId;
use pzoom_code_info::{TAtomic, TUnion};

use crate::function_analysis_data::FunctionAnalysisData;

use super::{FunctionParamsProvider, FunctionParamsProviderEvent, FunctionParamsProviderResult};

pub(super) struct MinMaxParamsProvider;

impl FunctionParamsProvider for MinMaxParamsProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["max", "min"]
    }

    fn get_function_params(
        &self,
        event: &FunctionParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<FunctionParamsProviderResult> {
        if event.args.len() != 1 {
            return None;
        }

        // Psalm picks the matching CallMap variant: a single NON-array
        // argument falls back to `min($value, ...$values)` (reported as
        // TooFewArguments, not InvalidArgument).
        if let Some(arg_pos) = event.arg_positions.first().copied()
            && let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned()
            && !arg_type.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TArray { .. }
                        | TAtomic::TNonEmptyArray { .. }
                        | TAtomic::TList { .. }
                        | TAtomic::TNonEmptyList { .. }
                        | TAtomic::TKeyedArray { .. }
                        | TAtomic::TMixed
                        | TAtomic::TNonEmptyMixed
                )
            })
        {
            return None;
        }

        let non_empty_array = TUnion::new(TAtomic::TNonEmptyArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        });

        Some(FunctionParamsProviderResult::Params(vec![ParamInfo {
            name: StrId::VALUE,
            param_type: Some(non_empty_array.clone()),
            signature_type: Some(non_empty_array),
            ..ParamInfo::default()
        }]))
    }
}
