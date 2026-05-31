//! Array type comparator.
//!
//! Handles comparison of array types including keyed arrays (shapes).

use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

/// Check if an input array type is contained by a container array type.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Generic array comparisons
    if let TAtomic::TArray {
        key_type: container_key,
        value_type: container_value,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TArray {
                key_type: input_key,
                value_type: input_value,
            } => {
                return compare_array_params(
                    codebase,
                    input_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TNonEmptyArray {
                key_type: input_key,
                value_type: input_value,
            } => {
                return compare_array_params(
                    codebase,
                    input_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TList {
                value_type: input_value,
            } => {
                // List has int keys
                let int_key = TUnion::int();
                return compare_array_params(
                    codebase,
                    &int_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TNonEmptyList {
                value_type: input_value,
            } => {
                let int_key = TUnion::int();
                return compare_array_params(
                    codebase,
                    &int_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TKeyedArray { properties, .. } => {
                // Keyed arrays need to have compatible value types
                // Check that all values in the keyed array are compatible with container value type
                for (key, value_type) in properties {
                    // Check key compatibility (if container has specific key type)
                    if !container_key.is_mixed() {
                        let key_type = normalize_array_key_union_for_comparison(
                            &array_key_to_literal_union(key),
                        );
                        let normalized_container_key =
                            normalize_array_key_union_for_comparison(container_key);
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &key_type,
                            &normalized_container_key,
                            false,
                            false,
                            atomic_comparison_result,
                        ) {
                            return false;
                        }
                    }

                    // Check value compatibility
                    if !container_value.is_mixed()
                        && !union_type_comparator::is_contained_by(
                            codebase,
                            value_type,
                            container_value,
                            false,
                            false,
                            atomic_comparison_result,
                        )
                    {
                        return false;
                    }
                }
                return true;
            }
            _ => {}
        }
    }

    // Non-empty array comparisons
    if let TAtomic::TNonEmptyArray {
        key_type: container_key,
        value_type: container_value,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TNonEmptyArray {
                key_type: input_key,
                value_type: input_value,
            } => {
                return compare_array_params(
                    codebase,
                    input_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TNonEmptyList {
                value_type: input_value,
            } => {
                let int_key = TUnion::int();
                return compare_array_params(
                    codebase,
                    &int_key,
                    input_value,
                    container_key,
                    container_value,
                    atomic_comparison_result,
                );
            }
            TAtomic::TArray {
                value_type: input_value,
                ..
            }
            | TAtomic::TList {
                value_type: input_value,
            } => {
                // A definitely-empty array (`array<never, never>`, e.g. the `[]`
                // literal) can never satisfy a non-empty constraint, so it is a hard
                // mismatch rather than a coercion (Psalm yields InvalidArgument here).
                // A general, possibly-empty array is a coercion (ArgumentTypeCoercion).
                if !input_value.is_nothing() {
                    atomic_comparison_result.type_coerced = Some(true);
                }
                return false;
            }
            TAtomic::TKeyedArray { properties, .. } => {
                if properties.is_empty() {
                    return false;
                }

                // A shape with only optional keys can still be empty and is therefore
                // not safely contained by non-empty array constraints.
                if properties
                    .values()
                    .all(|property_type| property_type.possibly_undefined)
                {
                    return false;
                }
                // Check that all values in the keyed array are compatible
                for (key, value_type) in properties {
                    // Check key compatibility
                    if !container_key.is_mixed() {
                        let key_type = normalize_array_key_union_for_comparison(
                            &array_key_to_literal_union(key),
                        );
                        let normalized_container_key =
                            normalize_array_key_union_for_comparison(container_key);
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &key_type,
                            &normalized_container_key,
                            false,
                            false,
                            atomic_comparison_result,
                        ) {
                            return false;
                        }
                    }
                    // Check value compatibility
                    if !container_value.is_mixed()
                        && !union_type_comparator::is_contained_by(
                            codebase,
                            value_type,
                            container_value,
                            false,
                            false,
                            atomic_comparison_result,
                        )
                    {
                        return false;
                    }
                }
                return true;
            }
            _ => {}
        }
    }

    // List comparisons
    if let TAtomic::TList {
        value_type: container_value,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TList {
                value_type: input_value,
            } => {
                return union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            }
            TAtomic::TNonEmptyList {
                value_type: input_value,
            } => {
                return union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            }
            TAtomic::TKeyedArray {
                is_list: true,
                properties,
                ..
            } => {
                // Check that all values are compatible with container value type
                for (_key, value_type) in properties {
                    if !union_type_comparator::is_contained_by(
                        codebase,
                        value_type,
                        container_value,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                }
                return true;
            }
            TAtomic::TArray {
                key_type: input_key,
                value_type: input_value,
            }
            | TAtomic::TNonEmptyArray {
                key_type: input_key,
                value_type: input_value,
            } => {
                if !input_key.is_nothing() {
                    let int_key = TUnion::int();
                    if !union_type_comparator::is_contained_by(
                        codebase,
                        input_key,
                        &int_key,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                }

                if input_value.is_nothing() {
                    return true;
                }

                return union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            }
            _ => {}
        }
    }

    // Non-empty list comparisons
    if let TAtomic::TNonEmptyList {
        value_type: container_value,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TNonEmptyList {
                value_type: input_value,
            } => {
                return union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            }
            TAtomic::TList { .. } => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            TAtomic::TKeyedArray {
                is_list: true,
                properties,
                ..
            } => {
                if properties.is_empty() {
                    return false;
                }

                // A list-shape with only optional offsets may still be empty.
                if properties
                    .values()
                    .all(|property_type| property_type.possibly_undefined)
                {
                    return false;
                }
                // Check that all values are compatible with container value type
                for (_key, value_type) in properties {
                    if !union_type_comparator::is_contained_by(
                        codebase,
                        value_type,
                        container_value,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                }
                return true;
            }
            TAtomic::TNonEmptyArray {
                key_type: input_key,
                value_type: input_value,
            } => {
                let int_key = TUnion::int();
                if !union_type_comparator::is_contained_by(
                    codebase,
                    input_key,
                    &int_key,
                    false,
                    false,
                    atomic_comparison_result,
                ) {
                    return false;
                }

                if input_value.is_nothing() {
                    return false;
                }

                return union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            }
            _ => {}
        }
    }

    // A generic array input against a keyed-array (shape) container. The
    // keyed-array comparator below only handles shape-vs-shape; a generic array
    // input has no declared per-key structure. An empty array (`array<never,
    // never>`, the `[]` literal) satisfies a shape whose keys are *all* optional,
    // because it simply omits every key. Matches Psalm, which contains
    // `array<never, never>` in any all-optional shape (a required key fails).
    if let TAtomic::TKeyedArray { properties, .. } = container_type_part {
        let input_is_empty_array = matches!(
            input_type_part,
            TAtomic::TArray { value_type, .. } | TAtomic::TList { value_type }
                if value_type.is_nothing()
        );
        if input_is_empty_array {
            return properties
                .values()
                .all(|property_type| property_type.possibly_undefined);
        }
    }

    // Keyed array (shape) comparisons — delegated to the keyed-array comparator
    // (mirrors Psalm's KeyedArrayComparator).
    if let Some(result) = super::keyed_array_comparator::is_contained_by(
        codebase,
        input_type_part,
        container_type_part,
        atomic_comparison_result,
    ) {
        return result;
    }

    false
}

pub(crate) fn array_key_to_literal_union(key: &pzoom_code_info::t_atomic::ArrayKey) -> TUnion {
    match key {
        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
            TUnion::new(TAtomic::TLiteralInt { value: *value })
        }
        pzoom_code_info::t_atomic::ArrayKey::String(value) => value
            .parse::<i64>()
            .map(|parsed_int| TUnion::new(TAtomic::TLiteralInt { value: parsed_int }))
            .unwrap_or_else(|_| {
                TUnion::new(TAtomic::TLiteralString {
                    value: value.clone(),
                })
            }),
    }
}

fn normalize_array_key_union_for_comparison(union: &TUnion) -> TUnion {
    let mut normalized = Vec::new();

    for atomic in &union.types {
        let converted = match atomic {
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .map(|as_int| TAtomic::TLiteralInt { value: as_int })
                .unwrap_or_else(|_| atomic.clone()),
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => TAtomic::TInt,
            _ => atomic.clone(),
        };

        if !normalized.contains(&converted) {
            normalized.push(converted);
        }
    }

    if normalized.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(normalized)
    }
}

/// Compare array key and value types.
fn compare_array_params(
    codebase: &CodebaseInfo,
    input_key: &TUnion,
    input_value: &TUnion,
    container_key: &TUnion,
    container_value: &TUnion,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Empty arrays/lists are compatible with any generic array constraints.
    if input_value.is_nothing() {
        return true;
    }

    let normalized_input_key = normalize_array_key_union_for_comparison(input_key);
    let normalized_container_key = normalize_array_key_union_for_comparison(container_key);

    // Check key compatibility. A `mixed` input key satisfies an `array-key`
    // (int&string) container key without further checking. Matches Psalm
    // ArrayTypeComparator (the key param is array-key by construction).
    let key_ok = normalized_container_key.is_mixed()
        || (normalized_input_key.is_mixed()
            && normalized_container_key.has_int()
            && normalized_container_key.has_string())
        || union_type_comparator::is_contained_by(
            codebase,
            &normalized_input_key,
            &normalized_container_key,
            false,
            false,
            atomic_comparison_result,
        );

    // Check value compatibility
    let value_ok = container_value.is_mixed()
        || union_type_comparator::is_contained_by(
            codebase,
            input_value,
            container_value,
            false,
            false,
            atomic_comparison_result,
        );

    key_ok && value_ok
}
