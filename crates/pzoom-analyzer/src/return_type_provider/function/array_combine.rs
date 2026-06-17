//! `array_combine` return-type provider (mirrors Psalm's
//! ArrayCombineReturnTypeProvider): emits InvalidArgument when the keys and values
//! arrays have different lengths.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayCombineReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayCombineReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_combine"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let keys_pos = event.arg_positions.first().copied()?;
        let values_pos = event.arg_positions.get(1).copied()?;
        let keys_type = analysis_data.expr_types.get(&keys_pos).cloned()?;
        let values_type = analysis_data.expr_types.get(&values_pos).cloned()?;

        let keys_len = known_tuple_len(&keys_type)?;
        let values_len = known_tuple_len(&values_type)?;

        if keys_len != values_len {
            let (line, col) = event.analyzer.get_line_column(keys_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                format!(
                    "The keys array {} must have exactly the same number of elements as the \
                     values array {}",
                    keys_type.get_id(Some(event.analyzer.interner)),
                    values_type.get_id(Some(event.analyzer.interner)),
                ),
                event.analyzer.file_path,
                keys_pos.0,
                keys_pos.1,
                line,
                col,
            ));
        }

        None
    }
}

/// The fixed element count of a tuple-like array literal, or None if it has unknown
/// length (a general array, a fallback, or possibly-undefined elements).
fn known_tuple_len(union: &TUnion) -> Option<usize> {
    match union.get_single() {
        // A sealed keyed-array shape (former TKeyedArray with no fallback),
        // including the empty array `[]`: a fixed length unless some entry is
        // possibly-undefined.
        Some(TAtomic::TArray {
            known_values,
            params: None,
            ..
        }) => {
            if known_values
                .values()
                .any(|(possibly_undefined, _)| *possibly_undefined)
            {
                None
            } else {
                Some(known_values.len())
            }
        }
        // A generic `array<never, never>` is provably length 0.
        Some(TAtomic::TArray {
            params: Some(params),
            ..
        }) if params.1.is_nothing() => Some(0),
        _ => None,
    }
}
