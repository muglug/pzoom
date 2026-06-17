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
                for (key, value_type) in properties.iter() {
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
                for (key, value_type) in properties.iter() {
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
                for (_key, value_type) in properties.iter() {
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
                // Psalm (ArrayTypeComparator:87-99): a generic array is never
                // *contained* in a list — list-ness is information the array
                // lacks — but it is always *coercible* to one. The empty
                // array (`array<never, never>`) is the exception: it is a
                // valid (empty) list.
                if input_key.is_nothing() && input_value.is_nothing() {
                    return true;
                }
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            TAtomic::TKeyedArray { is_list: false, .. } => {
                // Same Psalm rule for non-list shapes vs list containers.
                atomic_comparison_result.type_coerced = Some(true);
                return false;
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
                // Psalm reports InvalidArgument / InvalidReturnStatement for
                // list -> non-empty-list (its keyed-array comparator does not
                // mark the emptiness mismatch as a coercion), unlike the
                // generic array -> non-empty-array case which coerces.
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
                for (_key, value_type) in properties.iter() {
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
                // Psalm: a generic array is never *contained* in a list —
                // list-ness is information the array lacks — but a compatible
                // one is *coercible* (ArgumentTypeCoercion at call sites).
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

                if union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                ) {
                    atomic_comparison_result.type_coerced = Some(true);
                }
                return false;
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

    // A generic array against a shape container: Psalm converts the shape to
    // its generic array form (TKeyedArray::getGenericArrayType — non-empty when
    // it has required keys) and compares params, so coercion flags (e.g.
    // array-key vs the shape's literal keys, a from-mixed coercion) flow out;
    // a maybe-empty input against a required-key shape is then an emptiness
    // coercion.
    if let TAtomic::TKeyedArray {
        properties,
        fallback_key_type,
        fallback_value_type,
        ..
    } = container_type_part
    {
        let input_params = match input_type_part {
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => Some(((**key_type).clone(), (**value_type).clone())),
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                Some((TUnion::int(), (**value_type).clone()))
            }
            _ => None,
        };

        if let Some((input_key, input_value)) = input_params {
            let mut container_key: Option<TUnion> =
                fallback_key_type.as_ref().map(|k| (**k).clone());
            let mut container_value: Option<TUnion> =
                fallback_value_type.as_ref().map(|v| (**v).clone());
            let mut has_required_key = false;

            for (key, value) in properties.iter() {
                if !value.possibly_undefined {
                    has_required_key = true;
                }
                let key_union = array_key_to_literal_union(key);
                container_key = Some(match container_key {
                    Some(existing) => {
                        pzoom_code_info::combine_union_types(&existing, &key_union, false)
                    }
                    None => key_union,
                });
                container_value = Some(match container_value {
                    Some(existing) => pzoom_code_info::combine_union_types(&existing, value, false),
                    None => value.clone(),
                });
            }

            let (Some(container_key), Some(container_value)) = (container_key, container_value)
            else {
                return false;
            };

            // Psalm's KeyedArrayComparator: an unsized generic input can
            // never prove a shape's required keys — InvalidArgument, not a
            // coercion, even for non-empty inputs (non-empty-list<T> vs
            // list{T, T} is a plain mismatch). Checked before the param
            // comparison so its internal coercion flags don't leak out.
            if has_required_key
                && matches!(
                    input_type_part,
                    TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. }
                )
            {
                return false;
            }

            if !compare_array_params(
                codebase,
                &input_key,
                &input_value,
                &container_key,
                &container_value,
                atomic_comparison_result,
            ) {
                return false;
            }

            // Key/value-compatible generic array against the shape: required
            // keys the array cannot prove are a plain mismatch (Psalm:
            // "different due to additional array shape fields", no coercion);
            // an all-optional shape contains it (Psalm passes
            // array<'from'|'to', bool> into array{from?: bool, to?: bool}).
            if has_required_key {
                return false;
            }

            return true;
        }
    }

    false
}

pub(crate) fn array_key_to_literal_union(key: &pzoom_code_info::t_atomic::ArrayKey) -> TUnion {
    match key {
        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
            TUnion::new(TAtomic::TLiteralInt { value: *value })
        }
        pzoom_code_info::t_atomic::ArrayKey::String(value)
        | pzoom_code_info::t_atomic::ArrayKey::ClassString(value) => value
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

    let mut all_params_ok = true;

    // Check key compatibility. A `mixed` input key satisfies an `array-key`
    // (int&string) container key without further checking. Matches Psalm
    // ArrayTypeComparator (the key param is array-key by construction).
    let key_pre_ok = normalized_container_key.is_mixed()
        || (normalized_input_key.is_mixed()
            && normalized_container_key.has_int()
            && normalized_container_key.has_string());
    if !key_pre_ok {
        let mut param_comparison_result = TypeComparisonResult::new();
        // Psalm's ArrayTypeComparator passes each input param's own
        // ignore-nullable/ignore-falsable flags into the nested comparison,
        // so a falsable CallMap return inside an array element stays exempt.
        if !union_type_comparator::is_contained_by(
            codebase,
            &normalized_input_key,
            &normalized_container_key,
            normalized_input_key.ignore_nullable_issues,
            normalized_input_key.ignore_falsable_issues,
            &mut param_comparison_result,
        ) {
            and_combine_param_result(atomic_comparison_result, &param_comparison_result);
            all_params_ok = false;
        }
    }

    // Check value compatibility
    if !container_value.is_mixed() {
        let mut param_comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            codebase,
            input_value,
            container_value,
            input_value.ignore_nullable_issues,
            input_value.ignore_falsable_issues,
            &mut param_comparison_result,
        ) {
            and_combine_param_result(atomic_comparison_result, &param_comparison_result);
            all_params_ok = false;
        }
    }

    all_params_ok
}

/// Psalm's ArrayTypeComparator param-loop flag combining (lines 158-175): a
/// coercion flag survives only while *every* failing param sets it — one hard
/// param mismatch makes the whole array comparison a hard mismatch.
fn and_combine_param_result(
    atomic_comparison_result: &mut TypeComparisonResult,
    param_comparison_result: &TypeComparisonResult,
) {
    atomic_comparison_result.type_coerced = Some(
        param_comparison_result.type_coerced == Some(true)
            && atomic_comparison_result.type_coerced != Some(false),
    );
    atomic_comparison_result.type_coerced_from_mixed = Some(
        param_comparison_result.type_coerced_from_mixed == Some(true)
            && atomic_comparison_result.type_coerced_from_mixed != Some(false),
    );
    atomic_comparison_result.type_coerced_from_as_mixed = Some(
        param_comparison_result.type_coerced_from_as_mixed == Some(true)
            && atomic_comparison_result.type_coerced_from_as_mixed != Some(false),
    );
    atomic_comparison_result.type_coerced_from_scalar = Some(
        param_comparison_result.type_coerced_from_scalar == Some(true)
            && atomic_comparison_result.type_coerced_from_scalar != Some(false),
    );
}
