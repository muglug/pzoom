//! `array_merge` / `array_replace` return-type provider (mirrors Psalm's
//! ArrayMergeReturnTypeProvider). Preserves keyed-array shapes through the merge.
//!
//! Only the keyed/general-array argument cases are handled here; splat arguments and
//! list-typed arguments fall through to the function stub.

use rustc_hash::FxHashMap;

use pzoom_code_info::{ArrayKey, TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayMergeReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayMergeReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_merge", "array_replace"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        if event.args.is_empty() {
            return None;
        }
        let is_replace = event.function_id.eq_ignore_ascii_case("array_replace");

        // Spread args contribute their ELEMENT types (Psalm's provider
        // unpacks them); only provably non-empty spreads are handled
        // precisely — anything else falls back to the templated stub.
        let mut arg_unit_types: Vec<TUnion> = Vec::with_capacity(event.args.len());
        for (idx, arg) in event.args.iter().enumerate() {
            let pos = event.arg_positions.get(idx).copied()?;
            let arg_type = crate::expr::call::arguments_analyzer::get_argument_value_type(
                analysis_data,
                arg,
                pos,
            )?;
            if arg.is_unpacked() {
                let spread_nonempty = arg_type.types.iter().all(|atomic| match atomic {
                    TAtomic::TNonEmptyList { .. } | TAtomic::TNonEmptyArray { .. } => true,
                    TAtomic::TKeyedArray { properties, .. } => {
                        properties.values().any(|value| !value.possibly_undefined)
                    }
                    _ => false,
                });
                if !spread_nonempty {
                    return None;
                }
                let element =
                    crate::expr::call::arguments_analyzer::unpacked_element_type_for_templates(
                        event.analyzer.codebase,
                        &arg_type,
                    )?;
                arg_unit_types.push(element);
            } else {
                arg_unit_types.push((*arg_type).clone());
            }
        }

        let mut generic: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
        let mut inner_keys: Vec<TAtomic> = Vec::new();
        let mut inner_values: Vec<TAtomic> = Vec::new();
        let mut all_keyed_arrays = true;
        let mut all_int_offsets = true;
        let mut all_nonempty_lists = true;
        let mut any_nonempty = false;
        let mut all_empty = true;
        let mut max_keyed_array_size = 0usize;

        for arg_type in &arg_unit_types {
            for atomic in &arg_type.types {
                match atomic {
                    TAtomic::TKeyedArray {
                        properties,
                        is_list,
                        fallback_key_type,
                        fallback_value_type,
                        ..
                    } => {
                        all_empty = false;
                        max_keyed_array_size = max_keyed_array_size.max(properties.len());

                        for (key, value) in properties.iter() {
                            if !value.possibly_undefined {
                                any_nonempty = true;
                            }
                            match key {
                                ArrayKey::String(_) | ArrayKey::ClassString(_) => {
                                    all_int_offsets = false;
                                    set_or_combine(&mut generic, key.clone(), value);
                                }
                                ArrayKey::Int(_) => {
                                    if is_replace {
                                        set_or_combine(&mut generic, key.clone(), value);
                                    } else {
                                        all_keyed_arrays = false;
                                        inner_keys.push(TAtomic::TInt);
                                        inner_values.extend(value.types.iter().cloned());
                                    }
                                }
                            }
                        }

                        if !is_list {
                            all_nonempty_lists = false;
                        }
                        if let Some(fk) = fallback_key_type {
                            all_keyed_arrays = false;
                            inner_keys.extend(fk.types.iter().cloned());
                        }
                        if let Some(fv) = fallback_value_type {
                            all_keyed_arrays = false;
                            inner_values.extend(fv.types.iter().cloned());
                        }
                    }
                    TAtomic::TArray {
                        key_type,
                        value_type,
                    }
                    | TAtomic::TNonEmptyArray {
                        key_type,
                        value_type,
                    } => {
                        let non_empty = matches!(atomic, TAtomic::TNonEmptyArray { .. });
                        if !non_empty && value_type.is_nothing() {
                            continue;
                        }
                        for existing in generic.values_mut() {
                            *existing = combine_union_types(existing, value_type, false);
                        }
                        all_keyed_arrays = false;
                        all_nonempty_lists = false;
                        if !union_is_all_int(key_type) {
                            all_int_offsets = false;
                        }
                        if non_empty {
                            any_nonempty = true;
                        }
                        all_empty = false;
                        inner_keys.extend(key_type.types.iter().cloned());
                        inner_values.extend(value_type.types.iter().cloned());
                    }
                    // Lists merge/replace by sequential int keys, so they keep
                    // list-ness in the result (mirrors Psalm treating a list as a
                    // keyed array with int offsets).
                    TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                        let non_empty = matches!(atomic, TAtomic::TNonEmptyList { .. });
                        all_keyed_arrays = false;
                        if non_empty {
                            any_nonempty = true;
                        }
                        all_empty = false;
                        inner_keys.push(TAtomic::TInt);
                        inner_values.extend(value_type.types.iter().cloned());
                    }
                    // A first-pass `isset($acc[$k])` artifact inside a loop:
                    // Psalm's from_loop_isset mixed never holds a real value
                    // by merge time, so it contributes nothing (keeping the
                    // other args' non-emptiness intact).
                    TAtomic::TMixedFromLoopIsset => {}
                    // mixed / anything else: defer to the stub.
                    _ => return None,
                }
            }
        }

        let inner_key = combine_atomics(inner_keys);
        let inner_value = combine_atomics(inner_values);

        let gp_count = generic.len();
        if !generic.is_empty()
            && gp_count < 64
            && (gp_count < max_keyed_array_size * 2 || gp_count < 16)
        {
            let (fallback_key_type, fallback_value_type, sealed) =
                if all_keyed_arrays || inner_key.is_none() || inner_value.is_none() {
                    (None, None, true)
                } else {
                    (inner_key.map(Box::new), inner_value.map(Box::new), false)
                };

            return Some(TUnion::new(TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(generic),
                is_list: all_nonempty_lists || all_int_offsets,
                sealed,
                fallback_key_type,
                fallback_value_type,
            }));
        }

        if all_empty {
            return Some(TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            }));
        }

        if let Some(inner_value) = inner_value {
            if all_int_offsets {
                return Some(TUnion::new(if any_nonempty {
                    TAtomic::TNonEmptyList {
                        value_type: Box::new(inner_value),
                    }
                } else {
                    TAtomic::TList {
                        value_type: Box::new(inner_value),
                    }
                }));
            }

            let inner_key = inner_key.unwrap_or_else(TUnion::array_key);
            return Some(TUnion::new(if any_nonempty {
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(inner_key),
                    value_type: Box::new(inner_value),
                }
            } else {
                TAtomic::TArray {
                    key_type: Box::new(inner_key),
                    value_type: Box::new(inner_value),
                }
            }));
        }

        None
    }
}

fn set_or_combine(generic: &mut FxHashMap<ArrayKey, TUnion>, key: ArrayKey, value: &TUnion) {
    match generic.get(&key) {
        Some(existing) => {
            let possibly_undefined = existing.possibly_undefined && value.possibly_undefined;
            let mut combined = combine_union_types(existing, value, false);
            combined.possibly_undefined = possibly_undefined;
            generic.insert(key, combined);
        }
        None => {
            generic.insert(key, value.clone());
        }
    }
}

fn combine_atomics(atomics: Vec<TAtomic>) -> Option<TUnion> {
    let mut result: Option<TUnion> = None;
    for atomic in atomics {
        let single = TUnion::new(atomic);
        result = Some(match result {
            Some(existing) => combine_union_types(&existing, &single, false),
            None => single,
        });
    }
    result
}

fn union_is_all_int(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
            )
        })
}
