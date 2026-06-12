//! `"preg_replace"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::expr::call::function_call_analyzer as fca;
pub(super) struct PregReplaceReturnTypeProvider;

impl FunctionReturnTypeProvider for PregReplaceReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["preg_replace"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_preg_replace_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_preg_replace_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let subject_pos = arg_positions.get(2).copied()?;
    let subject_type = analysis_data.expr_types.get(&subject_pos).cloned()?;
    let mut inferred = fca::infer_string_transform_return_type(&subject_type)?;
    inferred.add_type(TAtomic::TNull);
    Some(inferred)
}
