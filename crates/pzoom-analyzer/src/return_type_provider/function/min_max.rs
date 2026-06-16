//! `min`/`max` return-type provider.
//!
//! Mirrors Psalm's `MinMaxReturnTypeProvider`: when every argument is an
//! int-typed expression, the result is the `int<min, max>` range combining
//! the arguments' bounds — `max(1, $int)` is `int<1, max>`.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct MinMaxReturnTypeProvider;

impl FunctionReturnTypeProvider for MinMaxReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["min", "max"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        // Single array argument: the result is one of the array's values
        // (Psalm's MinMaxReturnTypeProvider returns the value type).
        if event.arg_positions.len() == 1 {
            let arg_type = analysis_data
                .expr_types
                .get(&event.arg_positions[0])
                .cloned()?;
            let mut value_types: Vec<TAtomic> = Vec::new();
            for atomic in &arg_type.types {
                let value_union = match atomic {
                    TAtomic::TArray { value_type, .. }
                    | TAtomic::TNonEmptyArray { value_type, .. }
                    | TAtomic::TList { value_type }
                    | TAtomic::TNonEmptyList { value_type } => (**value_type).clone(),
                    TAtomic::TKeyedArray {
                        properties,
                        fallback_value_type,
                        ..
                    } => {
                        let mut combined: Option<TUnion> = None;
                        for property in properties.values() {
                            combined = Some(match combined {
                                None => property.clone(),
                                Some(existing) => {
                                    pzoom_code_info::combine_union_types(&existing, property, false)
                                }
                            });
                        }
                        if let Some(fallback) = fallback_value_type {
                            combined = Some(match combined {
                                None => (**fallback).clone(),
                                Some(existing) => {
                                    pzoom_code_info::combine_union_types(&existing, fallback, false)
                                }
                            });
                        }
                        combined?
                    }
                    _ => return None,
                };
                for value_atomic in value_union.types {
                    if !value_types.contains(&value_atomic) {
                        value_types.push(value_atomic);
                    }
                }
            }
            if value_types.is_empty() {
                return None;
            }
            return Some(TUnion::from_types(value_types));
        }

        if event.arg_positions.len() < 2 {
            return None;
        }

        let is_max = event.function_id == "max";

        let mut combined: Option<(Option<i64>, Option<i64>)> = None;
        for arg_pos in event.arg_positions {
            let arg_type = analysis_data.expr_types.get(&*arg_pos).cloned()?;
            let (arg_min, arg_max) = match arg_type.get_single()? {
                TAtomic::TLiteralInt { value } => (Some(*value), Some(*value)),
                TAtomic::TIntRange { min, max } => (*min, *max),
                TAtomic::TInt => (None, None),
                _ => return None,
            };

            combined = Some(match combined {
                None => (arg_min, arg_max),
                Some((current_min, current_max)) => {
                    if is_max {
                        // max(): the result is at least the largest known lower
                        // bound, and unbounded above unless every arg is bounded.
                        (
                            max_bound(current_min, arg_min),
                            match (current_max, arg_max) {
                                (Some(a), Some(b)) => Some(a.max(b)),
                                _ => None,
                            },
                        )
                    } else {
                        // min(): symmetric.
                        (
                            match (current_min, arg_min) {
                                (Some(a), Some(b)) => Some(a.min(b)),
                                _ => None,
                            },
                            min_bound(current_max, arg_max),
                        )
                    }
                }
            });
        }

        let (min, max) = combined?;
        Some(TUnion::new(TAtomic::TIntRange { min, max }))
    }
}

/// The larger of two lower bounds, where `None` is unbounded below.
fn max_bound(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (bound, None) | (None, bound) => bound,
    }
}

/// The smaller of two upper bounds, where `None` is unbounded above.
fn min_bound(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (bound, None) | (None, bound) => bound,
    }
}
