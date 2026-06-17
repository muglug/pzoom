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
            // A generic non-empty array/list (former TNonEmptyArray/
            // TNonEmptyList): empty known_values, typed fallback, guaranteed
            // non-empty — the element is always present.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                is_nonempty: true,
                ..
            } if known_values.is_empty() => (params.1.clone(), false),
            // A generic possibly-empty array/list (former TArray/TList): the
            // element may be missing.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                ..
            } if known_values.is_empty() => (params.1.clone(), true),
            // A keyed-array shape (former TKeyedArray), including the empty
            // array `[]` (no entries, no fallback → yields null).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                let fallback_value = params.as_deref().map(|(_, v)| v);
                let mut keys: Vec<&ArrayKey> = known_values.keys().collect();
                keys.sort();
                let chosen = if is_shift { keys.first() } else { keys.last() };

                match chosen {
                    Some(key) => {
                        let (possibly_undefined, value) = &known_values[*key];
                        let definite = !*possibly_undefined;
                        let mut value = value.clone();
                        // The removed element is always present in the result.
                        value.possibly_undefined = false;
                        if definite {
                            (value, false)
                        } else if let Some(fallback) = fallback_value {
                            (combine_union_types(&value, fallback, false), true)
                        } else {
                            (value, true)
                        }
                    }
                    None => match fallback_value {
                        Some(fallback) => (fallback.clone(), true),
                        None => (TUnion::null(), false),
                    },
                }
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
