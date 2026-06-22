//! `max`/`min` single-argument callmap variant (Psalm's CallMap `max'0`):
//! `max($array)` takes a non-empty-array — passing a possibly-empty array is
//! an ArgumentTypeCoercion.

use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::StrId;

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

        // A spread argument unpacks into positional values, so `max(...$list)`
        // uses the variadic `max(mixed, mixed, ...)` form (the default stub),
        // not the single-array `max(non-empty-array)` variant — mirroring how
        // Psalm spreads into the variadic overload. Without this, the unpacked
        // element type would be checked against `non-empty-array`.
        if event.args[0].is_unpacked() {
            return None;
        }

        // Psalm picks the matching CallMap variant: a single NON-array
        // argument selects the variadic `min'1`/`max'1` variant
        // (`min($value, $value2, ...$rest)`), which needs at least two
        // arguments — so a lone scalar is TooFewArguments, not InvalidArgument.
        if let Some(arg_pos) = event.arg_positions.first().copied()
            && let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned()
            && !arg_type.types.iter().any(|atomic| {
                atomic.is_array() || matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
            })
        {
            let mixed = TUnion::mixed();
            let value_param = |variadic: bool| ParamInfo {
                name: StrId::VALUE,
                param_type: Some(mixed.clone()),
                signature_type: Some(mixed.clone()),
                is_optional: variadic,
                is_variadic: variadic,
                ..ParamInfo::default()
            };
            return Some(FunctionParamsProviderResult::Params(vec![
                value_param(false),
                value_param(false),
                value_param(true),
            ]));
        }

        let non_empty_array = TUnion::new(TAtomic::non_empty_array(
            TUnion::array_key(),
            TUnion::mixed(),
        ));

        Some(FunctionParamsProviderResult::Params(vec![ParamInfo {
            name: StrId::VALUE,
            param_type: Some(non_empty_array.clone()),
            signature_type: Some(non_empty_array),
            ..ParamInfo::default()
        }]))
    }
}
