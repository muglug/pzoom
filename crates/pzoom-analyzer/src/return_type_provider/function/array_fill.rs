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
    let count_pos = arg_positions.get(1).copied()?;
    let value_pos = arg_positions.get(2).copied()?;

    let count_type = analysis_data.get_expr_type(count_pos)?;
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let is_non_empty = count_type.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralInt { value } => *value > 0,
        TAtomic::TIntRange { min, .. } => min.is_some_and(|min| min > 0),
        _ => false,
    });

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
