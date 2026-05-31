//! Keyed-array (shape) comparator.
//!
//! Compares two keyed arrays (PHP array shapes), checking required keys, value
//! containment, list-shape compatibility, and sealed/fallback handling for extra
//! keys. Mirrors Psalm's `KeyedArrayComparator`.

use pzoom_code_info::{CodebaseInfo, TAtomic};

use super::array_type_comparator::array_key_to_literal_union;
use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

/// Returns `Some(result)` when both atomics are keyed arrays, `None` otherwise
/// (so the caller can fall through to other array handling).
pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> Option<bool> {
    let TAtomic::TKeyedArray {
        properties: container_props,
        is_list: container_is_list,
        sealed: container_sealed,
        fallback_key_type: container_fallback_key,
        fallback_value_type: container_fallback_value,
    } = container_type_part
    else {
        return None;
    };

    let TAtomic::TKeyedArray {
        properties: input_props,
        is_list: input_is_list,
        ..
    } = input_type_part
    else {
        return None;
    };

    // A list-shaped container cannot cleanly accept a non-list shape: the keys may
    // not be sequential ints from 0. Matches Psalm KeyedArrayComparator.
    if *container_is_list && !*input_is_list {
        atomic_comparison_result.type_coerced = Some(true);
        return Some(false);
    }

    // Check that input has all required keys from container
    for (key, container_value_type) in container_props {
        if let Some(input_value_type) = input_props.get(key) {
            if input_value_type.possibly_undefined && !container_value_type.possibly_undefined {
                atomic_comparison_result.type_coerced = Some(true);
                return Some(false);
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
                return Some(false);
            }
        } else if !container_value_type.possibly_undefined {
            // Input is missing a required key
            return Some(false);
        }
    }

    // Handle input keys not declared in the container shape. A sealed container
    // forbids extra keys; an unsealed container with fallback params requires the
    // extra keys/values to satisfy them. Matches Psalm.
    for (key, input_value_type) in input_props {
        if container_props.contains_key(key) {
            continue;
        }

        if *container_sealed {
            // Sealed shape: no additional keys allowed.
            return Some(false);
        }

        if let (Some(fallback_key), Some(fallback_value)) =
            (container_fallback_key, container_fallback_value)
        {
            let key_union = array_key_to_literal_union(key);
            let mut normalized_input_value = input_value_type.clone();
            normalized_input_value.possibly_undefined = false;

            if !union_type_comparator::is_contained_by(
                codebase,
                &key_union,
                fallback_key,
                false,
                false,
                atomic_comparison_result,
            ) || !union_type_comparator::is_contained_by(
                codebase,
                &normalized_input_value,
                fallback_value,
                false,
                false,
                atomic_comparison_result,
            ) {
                return Some(false);
            }
        }
    }

    Some(true)
}
