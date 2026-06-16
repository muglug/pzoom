//! `array_shift` / `array_pop` return-type provider.
//!
//! Psalm returns the *removed* element — the first (shift) or last (pop) — which
//! for a shape/list with known offsets is the specific element type, not the
//! array's combined value type the call map would infer.

use pzoom_code_info::{ArrayKey, TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayShiftPopReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayShiftPopReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_shift", "array_pop"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let is_shift = event.function_id == "array_shift";
        let array_pos = event.arg_positions.first().copied()?;
        let array_type = analysis_data.expr_types.get(&array_pos).cloned()?;
        infer_array_shift_pop_return(&array_type, is_shift)
    }
}

fn infer_array_shift_pop_return(array_type: &TUnion, is_shift: bool) -> Option<TUnion> {
    let mut result: Option<TUnion> = None;

    for atomic in &array_type.types {
        // `(value, may_be_missing)` — when the slot may be absent the element
        // type is unioned with null (an empty array yields null).
        let (value, may_be_missing) = match atomic {
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                let mut keys: Vec<&ArrayKey> = properties.keys().collect();
                keys.sort();
                let chosen = if is_shift { keys.first() } else { keys.last() };

                match chosen {
                    Some(key) => {
                        let mut value = properties[*key].clone();
                        let definite = !value.possibly_undefined;
                        value.possibly_undefined = false;
                        if definite {
                            (value, false)
                        } else if let Some(fallback) = fallback_value_type {
                            (combine_union_types(&value, fallback, false), true)
                        } else {
                            (value, true)
                        }
                    }
                    None => match fallback_value_type {
                        Some(fallback) => ((**fallback).clone(), true),
                        None => (TUnion::null(), false),
                    },
                }
            }
            TAtomic::TNonEmptyList { value_type } | TAtomic::TNonEmptyArray { value_type, .. } => {
                ((**value_type).clone(), false)
            }
            TAtomic::TList { value_type } | TAtomic::TArray { value_type, .. } => {
                ((**value_type).clone(), true)
            }
            // Not statically an array — defer to the call map.
            _ => return None,
        };

        let value = if may_be_missing {
            combine_union_types(&value, &TUnion::null(), false)
        } else {
            value
        };

        result = Some(match result {
            Some(existing) => combine_union_types(&existing, &value, false),
            None => value,
        });
    }

    result
}
