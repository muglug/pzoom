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
        TAtomic::TArray { key_type, .. }
        | TAtomic::TNonEmptyArray { key_type, .. }
        | TAtomic::TIterable { key_type, .. } => (**key_type).clone(),
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => TUnion::int(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            ..
        } => {
            let mut key_types: Vec<TAtomic> = Vec::new();
            for key in properties.keys() {
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
            if let Some(fallback_key_type) = fallback_key_type {
                for fallback_atomic in &fallback_key_type.types {
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
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TIterable { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => (**value_type).clone(),
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut value_types: Vec<TAtomic> = Vec::new();
            for value in properties.values() {
                for value_atomic in &value.types {
                    if !value_types.contains(value_atomic) {
                        value_types.push(value_atomic.clone());
                    }
                }
            }
            if let Some(fallback_value_type) = fallback_value_type {
                for fallback_atomic in &fallback_value_type.types {
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
