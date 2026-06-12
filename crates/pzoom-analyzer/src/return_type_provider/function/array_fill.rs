//! `"array_fill"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
pub(super) struct ArrayFillReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayFillReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_fill"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_fill_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_array_fill_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let start_pos = arg_positions.first().copied()?;
    let count_pos = arg_positions.get(1).copied()?;
    let value_pos = arg_positions.get(2).copied()?;

    let count_type = analysis_data.expr_types.get(&count_pos).cloned()?;
    let value_type = analysis_data
        .expr_types.get(&value_pos).cloned()
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let is_non_empty = count_type.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralInt { value } => *value > 0,
        TAtomic::TIntRange { min, .. } => min.is_some_and(|min| min > 0),
        _ => false,
    });

    // Psalm's ArrayFillReturnTypeProvider: a literal-0 start index yields a
    // (non-empty-)list of the value type.
    let starts_at_zero = analysis_data
        .expr_types.get(&start_pos).cloned()
        .is_some_and(|start_type| {
            matches!(start_type.get_single(), Some(TAtomic::TLiteralInt { value: 0 }))
        });
    if starts_at_zero {
        return Some(TUnion::new(if is_non_empty {
            TAtomic::TNonEmptyList {
                value_type: Box::new(value_type),
            }
        } else {
            TAtomic::TList {
                value_type: Box::new(value_type),
            }
        }));
    }

    Some(TUnion::new(if is_non_empty {
        TAtomic::TNonEmptyArray {
            key_type: Box::new(TUnion::int()),
            value_type: Box::new(value_type),
        }
    } else {
        TAtomic::TArray {
            key_type: Box::new(TUnion::int()),
            value_type: Box::new(value_type),
        }
    }))
}
