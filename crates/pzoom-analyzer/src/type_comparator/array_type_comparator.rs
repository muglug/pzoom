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
                        let key_type = match key {
                            pzoom_code_info::t_atomic::ArrayKey::Int(_) => TUnion::int(),
                            pzoom_code_info::t_atomic::ArrayKey::String(_) => TUnion::string(),
                        };
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &key_type,
                            container_key,
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
                // Check that all values in the keyed array are compatible
                for (key, value_type) in properties {
                    // Check key compatibility
                    if !container_key.is_mixed() {
                        let key_type = match key {
                            pzoom_code_info::t_atomic::ArrayKey::Int(_) => TUnion::int(),
                            pzoom_code_info::t_atomic::ArrayKey::String(_) => TUnion::string(),
                        };
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &key_type,
                            container_key,
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
                    if !union_type_comparator::is_contained_by(
                        codebase,
                        input_value_type,
                        container_value_type,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                } else {
                    // Input is missing a required key
                    return false;
                }
            }

            // If container is sealed, input must have exactly the same keys
            if *container_sealed && input_props.len() != container_props.len() {
                return false;
            }

            return true;
        }
    }

    false
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
    // Check key compatibility
    let key_ok = container_key.is_mixed()
        || union_type_comparator::is_contained_by(
            codebase,
            input_key,
            container_key,
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
