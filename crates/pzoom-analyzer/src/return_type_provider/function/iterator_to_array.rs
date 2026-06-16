//! `"iterator_to_array"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
pub(super) struct IteratorToArrayReturnTypeProvider;

impl FunctionReturnTypeProvider for IteratorToArrayReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["iterator_to_array"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_iterator_to_array_return_type(event.analyzer, event.arg_positions, analysis_data)
    }
}

fn infer_iterator_to_array_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let iterator_pos = arg_positions.first().copied()?;
    let iterator_type = analysis_data.expr_types.get(&iterator_pos).cloned()?;
    let iterable_info = fca::extract_iterable_like_info_from_union(analyzer, &iterator_type)?;

    let preserve_keys = arg_positions
        .get(1)
        .and_then(|pos| analysis_data.expr_types.get(&*pos).cloned())
        .and_then(|ty| fca::get_literal_bool_from_union(&ty));

    match preserve_keys {
        Some(false) => Some(TUnion::new(TAtomic::TList {
            value_type: Box::new(iterable_info.value_type),
        })),
        Some(true) | None => Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(fca::normalize_array_key_union(&iterable_info.key_type)),
            value_type: Box::new(iterable_info.value_type),
        })),
    }
}
