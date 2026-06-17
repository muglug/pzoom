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
    let TAtomic::TArray {
        known_values: container_props,
        params: container_params,
        is_list: container_is_list,
        is_sealed: container_sealed,
        ..
    } = container_type_part
    else {
        return None;
    };

    let TAtomic::TArray {
        known_values: input_props,
        params: input_params,
        is_list: input_is_list,
        is_sealed: input_sealed,
        ..
    } = input_type_part
    else {
        return None;
    };

    // Only shape-vs-shape comparisons belong here (Psalm's KeyedArrayComparator);
    // a generic array (no known entries) is handled by the caller. The fallback
    // `params` carry the unsealed extra-key key/value, replacing the old
    // `fallback_key_type` / `fallback_value_type`.
    if container_props.is_empty() || input_props.is_empty() {
        return None;
    }
    let container_fallback_key = container_params.as_deref().map(|(key, _)| key);
    let container_fallback_value = container_params.as_deref().map(|(_, value)| value);
    let input_fallback_value = input_params.as_deref().map(|(_, value)| value);

    // A sealed container forbids extra keys; an *unsealed* input may carry
    // arbitrary ones, so it is not contained (Psalm reports a plain
    // InvalidArgument for sealed shapes refusing unsealed inputs).
    if *container_sealed && !*input_sealed && input_fallback_value.is_some() {
        return Some(false);
    }

    // A list-shaped container cannot cleanly accept a non-list shape: the keys may
    // not be sequential ints from 0. Matches Psalm KeyedArrayComparator.
    if *container_is_list && !*input_is_list {
        atomic_comparison_result.type_coerced = Some(true);
        return Some(false);
    }

    // Check that input has all required keys from container. Psalm walks every
    // property (no early return), merging each property comparison's flags
    // overwrite-style so the final coercion verdict reflects the whole shape.
    let mut all_types_contain = true;

    for (key, (container_possibly_undefined, container_value_type)) in container_props.iter() {
        if let Some((input_possibly_undefined, input_value_type)) = input_props.get(key) {
            // A possibly-undefined input key against a required container key
            // is a plain mismatch — Psalm sets no coercion flag, so this
            // reports InvalidArgument/InvalidReturnStatement rather than the
            // less-specific variants.
            if *input_possibly_undefined && !*container_possibly_undefined {
                all_types_contain = false;
                continue;
            }

            let mut normalized_input_value = input_value_type.clone();
            normalized_input_value.possibly_undefined = false;
            let mut normalized_container_value = container_value_type.clone();
            normalized_container_value.possibly_undefined = false;

            let mut property_type_comparison = TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                codebase,
                &normalized_input_value,
                &normalized_container_value,
                false,
                false,
                &mut property_type_comparison,
            ) {
                atomic_comparison_result.type_coerced = Some(
                    property_type_comparison.type_coerced == Some(true)
                        && atomic_comparison_result.type_coerced != Some(false),
                );

                // Psalm: if no coercion was detected, compare the other way
                // around — a container property contained by the input property
                // means the input is merely wider, i.e. coercible.
                if atomic_comparison_result.type_coerced != Some(true) {
                    let mut inverse_property_type_comparison = TypeComparisonResult::new();
                    if union_type_comparator::is_contained_by(
                        codebase,
                        &normalized_container_value,
                        &normalized_input_value,
                        false,
                        false,
                        &mut inverse_property_type_comparison,
                    ) {
                        atomic_comparison_result.type_coerced = Some(true);
                    }
                }

                atomic_comparison_result.type_coerced_from_mixed = Some(
                    property_type_comparison.type_coerced_from_mixed == Some(true)
                        && atomic_comparison_result.type_coerced_from_mixed != Some(false),
                );
                // Psalm propagates the scalar-mismatch flag (gated on the
                // container property's docblock provenance) so a shape whose
                // only failing property is a scalar mismatch reports
                // InvalidScalarArgument, not a coercion.
                atomic_comparison_result.scalar_type_match_found = Some(
                    !container_value_type.from_docblock
                        && property_type_comparison.scalar_type_match_found == Some(true)
                        && atomic_comparison_result.scalar_type_match_found != Some(false),
                );

                all_types_contain = false;
            }
        } else if !*container_possibly_undefined {
            // Input is missing a required key
            all_types_contain = false;
        }
    }

    if !all_types_contain {
        return Some(false);
    }

    // Handle input keys not declared in the container shape. A sealed container
    // forbids extra keys; an unsealed container with fallback params requires the
    // extra keys/values to satisfy them. Matches Psalm.
    for (key, (_input_possibly_undefined, input_value_type)) in input_props.iter() {
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
