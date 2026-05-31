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
        if event.args.is_empty() || event.args.iter().any(|arg| arg.is_unpacked()) {
            return None;
        }
        let is_replace = event.function_id.eq_ignore_ascii_case("array_replace");

        let mut generic: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
        let mut inner_keys: Vec<TAtomic> = Vec::new();
        let mut inner_values: Vec<TAtomic> = Vec::new();
        let mut all_keyed_arrays = true;
        let mut all_int_offsets = true;
        let mut all_nonempty_lists = true;
        let mut any_nonempty = false;
        let mut all_empty = true;
        let mut max_keyed_array_size = 0usize;

        for pos in event.arg_positions {
            let arg_type = analysis_data.get_expr_type(*pos)?;
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

                        for (key, value) in properties {
                            if !value.possibly_undefined {
                                any_nonempty = true;
                            }
                            match key {
                                ArrayKey::String(_) => {
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
                    TAtomic::TList { value_type }
                    | TAtomic::TNonEmptyList { value_type } => {
                        let non_empty = matches!(atomic, TAtomic::TNonEmptyList { .. });
                        all_keyed_arrays = false;
                        if non_empty {
                            any_nonempty = true;
                        }
                        all_empty = false;
                        inner_keys.push(TAtomic::TInt);
                        inner_values.extend(value_type.types.iter().cloned());
                    }
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
                    (
                        inner_key.map(Box::new),
                        inner_value.map(Box::new),
                        false,
                    )
                };

            return Some(TUnion::new(TAtomic::TKeyedArray {
                properties: generic,
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
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
            )
        })
}
