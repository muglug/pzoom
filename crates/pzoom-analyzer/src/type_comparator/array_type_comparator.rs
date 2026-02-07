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
            TAtomic::TArray { .. } | TAtomic::TList { .. } => {
                // Regular array/list could be empty
                atomic_comparison_result.type_coerced = Some(true);
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

    // Keyed array (shape) comparisons
    if let TAtomic::TKeyedArray {
        properties: container_props,
        sealed: container_sealed,
        ..
    } = container_type_part
    {
        if let TAtomic::TKeyedArray {
            properties: input_props,
            ..
        } = input_type_part
        {
            // Check that input has all required keys from container
            for (key, container_value_type) in container_props {
                if let Some(input_value_type) = input_props.get(key) {
                    if input_value_type.possibly_undefined && !container_value_type.possibly_undefined
                    {
                        atomic_comparison_result.type_coerced = Some(true);
                        return false;
                    }

                    let mut normalized_input_value = input_value_type.clone();
                    normalized_input_value.possibly_undefined = false;
                    let mut normalized_container_value = container_value_type.clone();
                    normalized_container_value.possibly_undefined = false;

                    if !union_type_comparator::is_contained_by(
                        codebase,
                        &normalized_input_value,
                        &normalized_container_value,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                } else if !container_value_type.possibly_undefined {
                    // Input is missing a required key
                    return false;
                }
            }

            // Psalm treats a shape with extra known fields as compatible with a
            // shape requiring only a subset of those fields.
            let _ = container_sealed;

            return true;
        }
    }

    false
}

fn array_key_to_literal_union(key: &pzoom_code_info::t_atomic::ArrayKey) -> TUnion {
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

    // Check key compatibility
    let key_ok = normalized_container_key.is_mixed()
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
