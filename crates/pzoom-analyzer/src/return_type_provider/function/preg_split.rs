//! `"preg_split"` return-type provider.

use pzoom_code_info::TUnion;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::expr::call::function_call_return_type_fetcher as fcrf;
pub(super) struct PregSplitReturnTypeProvider;

impl FunctionReturnTypeProvider for PregSplitReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["preg_split"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        fcrf::fetch_preg_split_return_type(event.arg_positions, analysis_data)
    }
}
