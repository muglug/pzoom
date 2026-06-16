//! `"microtime"` return-type provider.

use pzoom_code_info::TUnion;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_return_type_fetcher as fcrf;
use crate::function_analysis_data::FunctionAnalysisData;
pub(super) struct MicrotimeReturnTypeProvider;

impl FunctionReturnTypeProvider for MicrotimeReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["microtime"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        fcrf::fetch_microtime_return_type(event.arg_positions, analysis_data)
    }
}
