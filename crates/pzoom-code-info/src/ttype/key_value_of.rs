//! Resolution of `key-of<T>` / `value-of<T>` against a concrete type.
//!
//! Mirrors the key/value extraction Psalm performs when expanding `TKeyOf` /
//! `TValueOf` (and the template variants once their bound is known). Kept here so the
//! comparator, template replacers and the type expander all share one implementation.

use crate::t_atomic::{ArrayKey, TAtomic};
use crate::t_union::TUnion;

/// The keys of `union`, as `key-of<union>` would resolve to.
pub fn get_key_of_union(union: &TUnion) -> TUnion {
    let mut key_types = Vec::new();
    for atomic in &union.types {
        for key_atomic in get_key_of_atomic(atomic).types {
            if !key_types.contains(&key_atomic) {
                key_types.push(key_atomic);
            }
        }
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        TUnion::from_types(key_types)
    }
}

fn get_key_of_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TIterable { key_type, .. } => (**key_type).clone(),
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let mut key_types: Vec<TAtomic> = Vec::new();
            for key in known_values.keys() {
                let key_atomic = match key {
                    ArrayKey::Int(value) => TAtomic::TLiteralInt { value: *value },
                    ArrayKey::String(value) => TAtomic::TLiteralString {
                        value: value.clone(),
                    },
                    ArrayKey::ClassString(value) => TAtomic::TLiteralClassString {
                        name: value.clone(),
                    },
                };
                if !key_types.contains(&key_atomic) {
                    key_types.push(key_atomic);
                }
            }
            if let Some(params) = params {
                for fallback_atomic in &params.0.types {
                    if !key_types.contains(fallback_atomic) {
                        key_types.push(fallback_atomic.clone());
                    }
                }
            }
            if key_types.is_empty() {
                TUnion::array_key()
            } else {
                TUnion::from_types(key_types)
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => get_key_of_union(as_type),
        _ => TUnion::array_key(),
    }
}

/// The values of `union`, as `value-of<union>` would resolve to.
pub fn get_value_of_union(union: &TUnion) -> TUnion {
    let mut value_types = Vec::new();
    for atomic in &union.types {
        for value_atomic in get_value_of_atomic(atomic).types {
            if !value_types.contains(&value_atomic) {
                value_types.push(value_atomic);
            }
        }
    }

    if value_types.is_empty() {
        TUnion::mixed()
    } else {
        TUnion::from_types(value_types)
    }
}

fn get_value_of_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TIterable { value_type, .. } => (**value_type).clone(),
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let mut value_types: Vec<TAtomic> = Vec::new();
            for (_, value) in known_values.values() {
                for value_atomic in &value.types {
                    if !value_types.contains(value_atomic) {
                        value_types.push(value_atomic.clone());
                    }
                }
            }
            if let Some(params) = params {
                for fallback_atomic in &params.1.types {
                    if !value_types.contains(fallback_atomic) {
                        value_types.push(fallback_atomic.clone());
                    }
                }
            }
            if value_types.is_empty() {
                TUnion::mixed()
            } else {
                TUnion::from_types(value_types)
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => get_value_of_union(as_type),
        _ => TUnion::mixed(),
    }
}
