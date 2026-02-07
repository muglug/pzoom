//! Union type comparator.
//!
//! Handles comparison of union types (TUnion), checking if all atomic types
//! in the input are contained by at least one atomic type in the container.

use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};

use super::{atomic_type_comparator, type_comparison_result::TypeComparisonResult};

/// Check if input_type is contained by container_type.
///
/// Returns true if every value of input_type is also a valid value of container_type.
/// For example: `int` is contained by `int|string`, but `int|string` is not contained by `int`.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type: &TUnion,
    container_type: &TUnion,
    ignore_null: bool,
    ignore_false: bool,
    union_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Short-circuit for identical types
    if input_type == container_type {
        return true;
    }

    // Mixed contains everything
    if container_type.is_mixed() {
        return true;
    }

    // Nothing is contained by everything (never type)
    if input_type.is_nothing() {
        return true;
    }

    // Track coercion state across all atomic comparisons
    let mut all_type_coerced: Option<bool> = None;
    let mut all_type_coerced_from_mixed: Option<bool> = None;
    #[allow(unused_assignments)]
    let mut _some_type_coerced = false;
    #[allow(unused_assignments)]
    let mut _some_type_coerced_from_mixed = false;

    // Check each input atomic type
    for input_type_part in &input_type.types {
        // Template bounds should be compared against the full container union.
        // Comparing them atomically can produce false mismatches when the bound
        // itself is a union (e.g. T as string|null).
        if let TAtomic::TTemplateParam { as_type, .. } = input_type_part {
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by(
                codebase,
                as_type,
                container_type,
                ignore_null,
                ignore_false,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }

        if let TAtomic::TClassString {
            as_type: Some(input_as_type),
        } = input_type_part
            && let TAtomic::TTemplateParam {
                as_type: template_bound,
                ..
            } = input_as_type.as_ref()
        {
            let expanded_class_strings =
                expand_class_string_union_from_template_bound(template_bound);
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by(
                codebase,
                &expanded_class_strings,
                container_type,
                ignore_null,
                ignore_false,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }

        if let TAtomic::TTemplateParamClass { as_type, .. } = input_type_part {
            let class_string_bound = expand_template_param_class_union(as_type.as_ref());
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by(
                codebase,
                &class_string_bound,
                container_type,
                ignore_null,
                ignore_false,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }

        // Skip null if requested
        if ignore_null && matches!(input_type_part, TAtomic::TNull) {
            continue;
        }

        // Skip false if requested
        if ignore_false && matches!(input_type_part, TAtomic::TFalse) {
            continue;
        }

        // Special handling for array-key type
        if matches!(input_type_part, TAtomic::TArrayKey) {
            if container_type.has_int() && container_type.has_string() {
                continue;
            }
        }

        let mut type_match_found = false;
        let mut atomic_type_coerced: Option<bool> = None;
        let mut atomic_type_coerced_from_mixed: Option<bool> = None;

        // Check against each container atomic type
        for container_type_part in &container_type.types {
            // Skip null in container if requested
            if ignore_null
                && matches!(container_type_part, TAtomic::TNull)
                && !matches!(input_type_part, TAtomic::TNull)
            {
                continue;
            }

            // Skip false in container if requested
            if ignore_false
                && matches!(container_type_part, TAtomic::TFalse)
                && !matches!(input_type_part, TAtomic::TFalse)
            {
                continue;
            }

            let mut atomic_comparison_result = TypeComparisonResult::new();

            let is_atomic_contained = atomic_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                &mut atomic_comparison_result,
            );

            if is_atomic_contained {
                type_match_found = true;
                // Clear coercion flags since we found a direct match
                atomic_type_coerced = Some(false);
                atomic_type_coerced_from_mixed = Some(false);
                break;
            }

            // Track coercion
            if atomic_comparison_result.type_coerced.unwrap_or(false) {
                atomic_type_coerced = Some(true);
                _some_type_coerced = true;
            }

            if atomic_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                atomic_type_coerced_from_mixed = Some(true);
                _some_type_coerced_from_mixed = true;
            }
        }

        if !type_match_found {
            // Update overall coercion tracking
            if atomic_type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if atomic_type_coerced_from_mixed.unwrap_or(false) {
                union_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }

        // Update all_type_coerced tracking
        match (all_type_coerced, atomic_type_coerced) {
            (None, Some(v)) => all_type_coerced = Some(v),
            (Some(true), Some(false)) => all_type_coerced = Some(false),
            _ => {}
        }

        match (all_type_coerced_from_mixed, atomic_type_coerced_from_mixed) {
            (None, Some(v)) => all_type_coerced_from_mixed = Some(v),
            (Some(true), Some(false)) => all_type_coerced_from_mixed = Some(false),
            _ => {}
        }
    }

    // Update union comparison result with coercion info
    if all_type_coerced.unwrap_or(false) {
        union_comparison_result.type_coerced = Some(true);
    }
    if all_type_coerced_from_mixed.unwrap_or(false) {
        union_comparison_result.type_coerced_from_nested_mixed = Some(true);
    }

    true
}

fn expand_template_param_class_union(as_type: &TAtomic) -> TUnion {
    if let TAtomic::TTemplateParam {
        as_type: template_bound,
        ..
    } = as_type
    {
        let mut expanded = Vec::with_capacity(template_bound.types.len());
        for bound_atomic in &template_bound.types {
            let class_string_atomic = TAtomic::TClassString {
                as_type: Some(Box::new(bound_atomic.clone())),
            };

            if !expanded.contains(&class_string_atomic) {
                expanded.push(class_string_atomic);
            }
        }

        if !expanded.is_empty() {
            return TUnion::from_types(expanded);
        }
    }

    TUnion::new(TAtomic::TClassString {
        as_type: Some(Box::new(as_type.clone())),
    })
}

fn expand_class_string_union_from_template_bound(template_bound: &TUnion) -> TUnion {
    let mut expanded = Vec::with_capacity(template_bound.types.len());

    for bound_atomic in &template_bound.types {
        let class_string_atomic = TAtomic::TClassString {
            as_type: Some(Box::new(bound_atomic.clone())),
        };

        if !expanded.contains(&class_string_atomic) {
            expanded.push(class_string_atomic);
        }
    }

    if expanded.is_empty() {
        TUnion::new(TAtomic::TClassString { as_type: None })
    } else {
        TUnion::from_types(expanded)
    }
}

/// Check if any value of input_type could be a valid value of container_type.
///
/// More permissive than is_contained_by - returns true if there's any overlap.
pub fn can_be_contained_by(
    codebase: &CodebaseInfo,
    input_type: &TUnion,
    container_type: &TUnion,
) -> bool {
    // Mixed can contain anything
    if container_type.is_mixed() {
        return true;
    }

    // Nothing can be contained by anything
    if input_type.is_nothing() {
        return true;
    }

    // Check if any input atomic can be contained by any container atomic
    for container_type_part in &container_type.types {
        for input_type_part in &input_type.types {
            let mut atomic_comparison_result = TypeComparisonResult::new();

            if atomic_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                &mut atomic_comparison_result,
            ) {
                return true;
            }

            // Also accept if coercion would work
            if atomic_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                return true;
            }
        }
    }

    false
}

/// Check if two union types can have identical values.
pub fn can_expression_types_be_identical(
    codebase: &CodebaseInfo,
    type1: &TUnion,
    type2: &TUnion,
) -> bool {
    // Mixed can be identical to anything
    if type1.is_mixed() || type2.is_mixed() {
        return true;
    }

    // Both nullable means null could match
    if type1.is_nullable && type2.is_nullable {
        return true;
    }

    // Check if any atomic type pair can be identical
    for type1_part in &type1.types {
        for type2_part in &type2.types {
            if atomic_type_comparator::can_be_identical(codebase, type1_part, type2_part) {
                return true;
            }
        }
    }

    false
}
