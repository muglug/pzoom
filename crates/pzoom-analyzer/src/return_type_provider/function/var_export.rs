//! `"var_export"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
pub(super) struct VarExportReturnTypeProvider;

impl FunctionReturnTypeProvider for VarExportReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["var_export"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_var_export_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_var_export_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let Some(return_flag_pos) = arg_positions.get(1).copied() else {
        return Some(TUnion::void());
    };

    let return_flag_type = analysis_data.expr_types.get(&return_flag_pos).cloned()?;
    match fca::get_literal_bool_from_union(&return_flag_type) {
        Some(true) => Some(TUnion::string()),
        Some(false) => Some(TUnion::void()),
        None => Some(TUnion::from_types(vec![TAtomic::TString, TAtomic::TVoid])),
    }
}
