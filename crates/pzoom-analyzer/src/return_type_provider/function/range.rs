//! `"range"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
pub(super) struct RangeReturnTypeProvider;

impl FunctionReturnTypeProvider for RangeReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["range"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_range_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_range_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let start_pos = arg_positions.first().copied()?;
    let end_pos = arg_positions.get(1).copied()?;

    let start_type = analysis_data.expr_types.get(&start_pos).cloned()?;
    let end_type = analysis_data.expr_types.get(&end_pos).cloned()?;
    let mut value_type = combine_union_types(&start_type, &end_type, false);

    // A float step yields float elements regardless of the bounds
    // (Psalm's RangeReturnTypeProvider).
    if let Some(step_pos) = arg_positions.get(2).copied()
        && let Some(step_type) = analysis_data.expr_types.get(&step_pos).cloned()
        && step_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
    {
        value_type = combine_union_types(&value_type, &TUnion::float(), false);
    }

    let mut normalized = Vec::new();
    for atomic in &value_type.types {
        let mapped = match atomic {
            TAtomic::TInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TLiteralInt { .. } => TAtomic::TInt,
            TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => TAtomic::TFloat,
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString => TAtomic::TString,
            _ => TAtomic::TMixed,
        };

        if !normalized.contains(&mapped) {
            normalized.push(mapped);
        }
    }

    if normalized.is_empty() {
        value_type = TUnion::mixed();
    } else {
        value_type = TUnion::from_types(normalized);
    }

    Some(TUnion::new(TAtomic::TNonEmptyList {
        value_type: Box::new(value_type),
    }))
}
