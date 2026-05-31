//! `"count"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::expr::call::function_call_analyzer as fca;
pub(super) struct CountReturnTypeProvider;

impl FunctionReturnTypeProvider for CountReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["count"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_count_return_type(event.arg_positions, analysis_data)
    }
}

fn infer_count_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let value_pos = arg_positions.first().copied()?;
    let value_type = analysis_data.get_expr_type(value_pos)?;

    let mut saw_array_like = false;
    let mut saw_non_empty = false;
    let mut exact_count: Option<i64> = None;

    for atomic in &value_type.types {
        match atomic {
            TAtomic::TArray { .. } | TAtomic::TList { .. } => {
                saw_array_like = true;
                exact_count = None;
            }
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => {
                saw_array_like = true;
                saw_non_empty = true;
                exact_count = None;
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                saw_array_like = true;

                if fallback_key_type.is_some() || fallback_value_type.is_some() {
                    exact_count = None;
                } else {
                    let mut fixed_count = 0i64;
                    let mut has_optional = false;

                    for property_type in properties.values() {
                        if property_type.possibly_undefined {
                            has_optional = true;
                        } else {
                            fixed_count += 1;
                        }
                    }

                    if fixed_count > 0 {
                        saw_non_empty = true;
                    }

                    if has_optional {
                        exact_count = None;
                    } else {
                        exact_count = match exact_count {
                            None => Some(fixed_count),
                            Some(existing) if existing == fixed_count => Some(existing),
                            Some(_) => None,
                        };
                    }
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(info) = fca::extract_array_like_info_from_union(as_type) {
                    saw_array_like = true;
                    if info.is_non_empty {
                        saw_non_empty = true;
                    }
                    exact_count = None;
                }
            }
            _ => {}
        }
    }

    if !saw_array_like {
        return None;
    }

    if let Some(count) = exact_count {
        return Some(TUnion::new(TAtomic::TLiteralInt { value: count }));
    }

    if saw_non_empty {
        return Some(TUnion::new(TAtomic::TIntRange {
            min: Some(1),
            max: None,
        }));
    }

    Some(TUnion::new(TAtomic::TIntRange {
        min: Some(0),
        max: None,
    }))
}
