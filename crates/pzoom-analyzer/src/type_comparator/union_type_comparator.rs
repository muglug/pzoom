//! Union type comparator.
//!
//! Handles comparison of union types (TUnion), checking if all atomic types
//! in the input are contained by at least one atomic type in the container.

use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion, TemplateBound};

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
    is_contained_by_in_context(
        codebase,
        input_type,
        container_type,
        ignore_null,
        ignore_false,
        false,
        union_comparison_result,
    )
}

/// `is_contained_by` with Psalm's `$allow_interface_equality` flag threaded
/// through (equality-tolerant contexts let template-param containers accept
/// bound-fitting inputs).
pub fn is_contained_by_in_context(
    codebase: &CodebaseInfo,
    input_type: &TUnion,
    container_type: &TUnion,
    ignore_null: bool,
    ignore_false: bool,
    allow_interface_equality: bool,
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

    // Psalm seeds `scalar_type_match_found` to true and only clears it when a
    // non-scalar mismatch is found, so a purely-scalar mismatch yields
    // `InvalidScalarArgument` rather than `InvalidArgument`.
    union_comparison_result.scalar_type_match_found = Some(true);

    // A templated container keeps input templates intact for the atomic
    // comparator's shallow template-vs-template rules (Hakana's
    // `container_has_template` gate); only a template-free container lets an
    // input template dissolve into its bound.
    let container_has_template = super::generic_type_comparator::union_has_template(container_type);

    // Track coercion state across all atomic comparisons
    let mut all_type_coerced: Option<bool> = None;
    let mut all_type_coerced_from_mixed: Option<bool> = None;
    #[allow(unused_assignments)]
    let mut _some_type_coerced = false;
    #[allow(unused_assignments)]
    let mut _some_type_coerced_from_mixed = false;

    // Check each input atomic type
    for input_type_part in &input_type.types {
        // A type variable in input position is constrained from above by the
        // container (Hakana's union comparator): record `name <: container`
        // and treat it as contained.
        if let TAtomic::TTypeVariable { name } = input_type_part {
            if let Some(TAtomic::TTypeVariable {
                name: container_name,
            }) = container_type.get_single()
                && container_name == name
            {
                continue;
            }
            union_comparison_result.type_variable_upper_bounds.push((
                name.clone(),
                TemplateBound::new(container_type.clone(), 0, None, None),
            ));

            continue;
        }

        // `scalar` is bool|int|float|string: a container spelling the members
        // out contains it even without a literal `scalar` atomic (Psalm's
        // scalar decomposition).
        if matches!(input_type_part, TAtomic::TScalar) {
            let decomposed = TUnion::from_types(vec![
                TAtomic::TBool,
                TAtomic::TInt,
                TAtomic::TFloat,
                TAtomic::TString,
            ]);
            let mut scalar_result = TypeComparisonResult::new();
            if is_contained_by_in_context(
                codebase,
                &decomposed,
                container_type,
                ignore_null,
                ignore_false,
                allow_interface_equality,
                &mut scalar_result,
            ) {
                continue;
            }
        }

        // `iterable<K, V>` is array<K, V>|Traversable<K, V>: a container
        // spelling the halves out contains it (Psalm decomposes iterable).
        if let TAtomic::TIterable {
            key_type,
            value_type,
        } = input_type_part
        {
            let decomposed = TUnion::from_types(vec![
                TAtomic::array((**key_type).clone(), (**value_type).clone()),
                TAtomic::TNamedObject {
                    name: pzoom_str::StrId::TRAVERSABLE,
                    type_params: Some(vec![(**key_type).clone(), (**value_type).clone()]),
                    is_static: false,
                    remapped_params: false,
                },
            ]);
            let mut iterable_result = TypeComparisonResult::new();
            if container_type.types.len() > 1
                && is_contained_by_in_context(
                    codebase,
                    &decomposed,
                    container_type,
                    ignore_null,
                    ignore_false,
                    allow_interface_equality,
                    &mut iterable_result,
                )
            {
                continue;
            }
        }

        // An int range can be covered by a UNION of int literals/ranges even
        // when no single member contains it (int<0, max> in 0|int<1, max>) —
        // Psalm's IntegerRangeComparator::isContainedByUnion.
        if let TAtomic::TIntRange { min, max } = input_type_part
            && container_type.types.len() > 1
            && int_range_contained_by_union(*min, *max, container_type)
        {
            continue;
        }

        // Template bounds should be compared against the full container union.
        // Comparing them atomically can produce false mismatches when the bound
        // itself is a union (e.g. T as string|null). Only when the container
        // has no template of its own (Hakana's `!container_has_template`
        // gate): template-vs-template comparisons must reach the atomic
        // comparator's shallow rules instead of dissolving into bounds.
        if let TAtomic::TTemplateParam { as_type, .. } = input_type_part
            && !container_has_template
        {
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by_in_context(
                codebase,
                as_type,
                container_type,
                ignore_null,
                ignore_false,
                allow_interface_equality,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_mixed = Some(true);
            }

            union_comparison_result.scalar_type_match_found = Some(false);
            return false;
        }

        if let TAtomic::TClassString {
            as_type: Some(input_as_type),
        } = input_type_part
            && let TAtomic::TTemplateParam {
                as_type: template_bound,
                ..
            } = input_as_type.as_ref()
            && !container_has_template
        {
            let expanded_class_strings =
                expand_class_string_union_from_template_bound(template_bound);
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by_in_context(
                codebase,
                &expanded_class_strings,
                container_type,
                ignore_null,
                ignore_false,
                allow_interface_equality,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_mixed = Some(true);
            }

            union_comparison_result.scalar_type_match_found = Some(false);
            return false;
        }

        if let TAtomic::TTemplateParamClass { as_type, .. } = input_type_part
            && !container_has_template
        {
            let class_string_bound = expand_template_param_class_union(as_type.as_ref());
            let mut template_comparison_result = TypeComparisonResult::new();
            if is_contained_by_in_context(
                codebase,
                &class_string_bound,
                container_type,
                ignore_null,
                ignore_false,
                allow_interface_equality,
                &mut template_comparison_result,
            ) {
                continue;
            }

            if template_comparison_result.type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if template_comparison_result
                .type_coerced_from_mixed
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_mixed = Some(true);
            }

            union_comparison_result.scalar_type_match_found = Some(false);
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

        // `numeric` (int|float|numeric-string) is contained by a union that covers
        // all three constituents, e.g. `int|string|float`. Matches Psalm
        // UnionTypeComparator. No single atomic contains `numeric`, so this must be
        // handled at the union level.
        if matches!(input_type_part, TAtomic::TNumeric)
            && container_type.has_int()
            && container_type.has_string()
            && container_type.has_float()
        {
            continue;
        }

        let mut type_match_found = false;
        let mut atomic_type_coerced: Option<bool> = None;
        let mut atomic_type_coerced_from_mixed: Option<bool> = None;
        let mut atomic_type_coerced_from_as_mixed: Option<bool> = None;
        // Tracks whether the failing comparisons for this input atomic were
        // scalar-vs-scalar mismatches (Psalm's per-atomic `$scalar_type_match_found`).
        let mut atomic_scalar_match = false;

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

            // A type variable in container position is constrained from below
            // by the input (Hakana's `check_atomic_contained_by_atomic`):
            // record `name >: input` and treat it as a match.
            if let TAtomic::TTypeVariable { name } = container_type_part {
                union_comparison_result.type_variable_lower_bounds.push((
                    name.clone(),
                    TemplateBound::new(input_type.clone(), 0, None, None),
                ));

                type_match_found = true;
                atomic_type_coerced = Some(false);
                atomic_type_coerced_from_mixed = Some(false);
                atomic_type_coerced_from_as_mixed = Some(false);
                break;
            }

            let mut atomic_comparison_result = TypeComparisonResult::new();

            let is_atomic_contained = atomic_type_comparator::is_contained_by_in_context(
                codebase,
                input_type_part,
                container_type_part,
                allow_interface_equality,
                &mut atomic_comparison_result,
            );

            // Mirror Psalm: the last atomic comparison with a determined
            // scalar_type_match_found wins for this input atomic.
            if let Some(scalar_match) = atomic_comparison_result.scalar_type_match_found {
                atomic_scalar_match = scalar_match;
            }
            if atomic_comparison_result
                .type_coerced_from_scalar
                .unwrap_or(false)
            {
                union_comparison_result.type_coerced_from_scalar = Some(true);
            }

            if is_atomic_contained {
                type_match_found = true;
                // Clear coercion flags since we found a direct match
                atomic_type_coerced = Some(false);
                atomic_type_coerced_from_mixed = Some(false);
                atomic_type_coerced_from_as_mixed = Some(false);

                union_comparison_result
                    .type_variable_lower_bounds
                    .extend(atomic_comparison_result.type_variable_lower_bounds);

                union_comparison_result
                    .type_variable_upper_bounds
                    .extend(atomic_comparison_result.type_variable_upper_bounds);
                break;
            }

            // Track coercion
            if atomic_comparison_result.type_coerced.unwrap_or(false) {
                atomic_type_coerced = Some(true);
                _some_type_coerced = true;
            }

            if atomic_comparison_result
                .type_coerced_from_mixed
                .unwrap_or(false)
            {
                atomic_type_coerced_from_mixed = Some(true);
                _some_type_coerced_from_mixed = true;
            }

            if atomic_comparison_result
                .type_coerced_from_as_mixed
                .unwrap_or(false)
            {
                atomic_type_coerced_from_as_mixed = Some(true);
            }
        }

        if !type_match_found {
            // An integer range can be covered by the UNION of the container's
            // int atomics even when no single atomic contains it
            // (e.g. `int<0,max>` ⊆ `0|positive-int`). Psalm handles this at the
            // union level via IntegerRangeComparator::isContainedByUnion.
            if let Some((input_range_min, input_range_max)) =
                input_int_range_bounds(input_type_part)
                && super::integer_range_comparator::is_contained_by_union(
                    input_range_min,
                    input_range_max,
                    container_type,
                )
            {
                continue;
            }

            // Update overall coercion tracking
            if atomic_type_coerced.unwrap_or(false) {
                union_comparison_result.type_coerced = Some(true);
            }
            if atomic_type_coerced_from_mixed.unwrap_or(false) {
                union_comparison_result.type_coerced_from_mixed = Some(true);
                // Psalm: a docblock-sourced template-default slot coerces from
                // *as*-mixed — downstream issue selection treats it leniently.
                if (input_type.from_template_default && input_type.from_docblock)
                    || atomic_type_coerced_from_as_mixed.unwrap_or(false)
                {
                    union_comparison_result.type_coerced_from_as_mixed = Some(true);
                }
            }

            // Psalm: clear the seeded scalar flag unless every failing container
            // comparison for this input atomic was a scalar-vs-scalar mismatch.
            if !atomic_scalar_match {
                union_comparison_result.scalar_type_match_found = Some(false);
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
        union_comparison_result.type_coerced_from_mixed = Some(true);
        if input_type.from_template_default && input_type.from_docblock {
            union_comparison_result.type_coerced_from_as_mixed = Some(true);
        }
    }

    true
}

/// Bounds for an int-range-like input atomic, used to check coverage by a union
/// of container int atomics. Only the *spanning* int types are considered here
/// (`int<a,b>`, `positive-int`, `negative-int`); a plain `int` or literal is
/// handled by the per-atomic comparison.
fn input_int_range_bounds(atomic: &TAtomic) -> Option<(Option<i64>, Option<i64>)> {
    match atomic {
        TAtomic::TIntRange { min, max } => Some((*min, *max)),
        _ => None,
    }
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
                .type_coerced_from_mixed
                .unwrap_or(false)
            {
                return true;
            }

            // A literal string naming an existing class CAN be a value of a
            // class-string (the strict comparator treats it as a coercion so
            // returns flag LessSpecific; possibility checks must not call the
            // pair disjoint).
            if let TAtomic::TLiteralString { value } = input_type_part
                && matches!(
                    container_type_part,
                    TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
                )
                && codebase.resolve_classlike_name(value).is_some()
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
    if type1.is_nullable() && type2.is_nullable() {
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

/// Psalm's `IntegerRangeComparator::isContainedByUnion`: reduce the input
/// range by carving out what the container's int literals/ranges cover until
/// it is empty (contained) or no progress can be made.
fn int_range_contained_by_union(
    min: Option<i64>,
    max: Option<i64>,
    container_type: &TUnion,
) -> bool {
    // A general `int` member contains any range.
    if container_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TInt))
    {
        return true;
    }

    let mut members: Vec<TAtomic> = container_type
        .types
        .iter()
        .filter(|atomic| {
            matches!(
                atomic,
                TAtomic::TIntRange { .. } | TAtomic::TLiteralInt { .. }
            )
        })
        .cloned()
        .collect();
    if members.len() < 2 {
        return false;
    }

    let mut reduced_min = min;
    let mut reduced_max = max;

    loop {
        let members_before = members.len();
        let mut index = 0;
        while index < members.len() {
            match members[index] {
                TAtomic::TIntRange {
                    min: container_min,
                    max: container_max,
                } => {
                    let min_in_container = container_min.is_none()
                        || (reduced_min.is_some() && container_min <= reduced_min);
                    let max_in_container = container_max.is_none()
                        || (reduced_max.is_some() && container_max >= reduced_max);
                    if min_in_container && max_in_container {
                        return true;
                    }
                    match (container_min, container_max) {
                        (Some(container_min), None) => {
                            // int<X, max> caps the reduced range at X-1.
                            let new_max = container_min - 1;
                            reduced_max = Some(match reduced_max {
                                Some(existing) => existing.min(new_max),
                                None => new_max,
                            });
                            members.remove(index);
                            continue;
                        }
                        (None, Some(container_max)) => {
                            // int<min, X> raises the reduced min to X+1.
                            let new_min = container_max + 1;
                            reduced_min = Some(match reduced_min {
                                Some(existing) => existing.max(new_min),
                                None => new_min,
                            });
                            members.remove(index);
                            continue;
                        }
                        (Some(container_min), Some(container_max)) => {
                            if let Some(current_min) = reduced_min
                                && container_min <= current_min
                                && current_min <= container_max
                            {
                                reduced_min = Some(container_max + 1);
                                members.remove(index);
                                continue;
                            }
                            if let Some(current_max) = reduced_max
                                && container_min <= current_max
                                && current_max <= container_max
                            {
                                reduced_max = Some(container_min - 1);
                                members.remove(index);
                                continue;
                            }
                        }
                        (None, None) => return true,
                    }
                }
                TAtomic::TLiteralInt { value } => {
                    let in_range = reduced_min.is_none_or(|low| low <= value)
                        && reduced_max.is_none_or(|high| value <= high);
                    if !in_range {
                        members.remove(index);
                        continue;
                    }
                    if reduced_min == Some(value) {
                        reduced_min = Some(value + 1);
                        members.remove(index);
                        continue;
                    }
                    if reduced_max == Some(value) {
                        reduced_max = Some(value - 1);
                        members.remove(index);
                        continue;
                    }
                }
                _ => unreachable!(),
            }
            index += 1;
        }

        if let (Some(low), Some(high)) = (reduced_min, reduced_max)
            && low > high
        {
            // The whole range was carved away.
            return true;
        }

        if members.is_empty() || members.len() == members_before {
            return false;
        }
    }
}
