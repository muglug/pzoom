//! Simple assertion reconciler.
//!
//! Handles positive assertions like `truthy`, `isset`, and basic type checks.

use pzoom_code_info::{ArrayKey, Assertion, TAtomic, TUnion, combine_union_types};
use rustc_hash::FxHashMap;

use super::{assertion_reconciler, get_acceptable_type};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reconciles a positive assertion with an existing type.
///
/// Returns the narrowed type after applying the assertion.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    inside_loop: bool,
) -> TUnion {
    // Get the assertion type if any
    let assertion_type = assertion.get_type();

    // Handle type assertions with intersection
    if let Some(assertion_atomic) = assertion_type {
        // Check for TMixed with non-null flag
        match assertion_atomic {
            TAtomic::TNonEmptyMixed => {
                // mixed - null
                return with_docblock_from(
                    super::simple_negated_assertion_reconciler::subtract_null(existing_var_type),
                    existing_var_type,
                );
            }
            _ => {}
        }

        // Match Psalm: asserting a concrete runtime type against mixed yields a fresh
        // runtime type (not docblock-derived).
        if matches!(assertion, Assertion::IsType(_)) && existing_var_type.is_mixed() {
            return TUnion::new(assertion_atomic.clone());
        }

        // Mirror Psalm's overflow quirk: int results from arithmetic can become float.
        if matches!(assertion_atomic, TAtomic::TFloat)
            && existing_var_type.from_calculation
            && existing_var_type.has_int()
        {
            let mut float_type = TUnion::float();
            float_type.from_docblock = existing_var_type.from_docblock;
            return float_type;
        }

        // Try to intersect with the assertion type
        if let Some(result) = assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            assertion_atomic,
            analyzer,
        ) {
            return with_docblock_from(result, existing_var_type);
        }

        // If intersection is empty, the assertion branch is impossible.
        return with_docblock_from(TUnion::nothing(), existing_var_type);
    }

    // Handle specific assertions
    match assertion {
        Assertion::Truthy => reconcile_truthy(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::IsIsset | Assertion::IsEqualIsset => reconcile_isset(
            assertion,
            existing_var_type,
            possibly_undefined,
            key,
            negated,
            analysis_data,
            analyzer,
            inside_loop,
        ),
        Assertion::ArrayKeyExists => {
            if existing_var_type.is_nothing() {
                let _ = inside_loop;
                TUnion::mixed()
            } else {
                let mut existing = existing_var_type.clone();
                existing.possibly_undefined = false;
                existing
            }
        }
        Assertion::NonEmptyCountable(_) => reconcile_non_empty_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::HasExactCount(count) => reconcile_exact_count(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            analyzer,
            *count,
        ),
        Assertion::InArray(array_type) => with_docblock_from(
            reconcile_in_array(existing_var_type, array_type),
            existing_var_type,
        ),
        Assertion::HasArrayKey(array_key) => reconcile_has_array_key(
            assertion,
            existing_var_type,
            array_key,
            possibly_undefined,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::HasNonnullEntryForKey(array_key) => reconcile_has_nonnull_entry_for_key(
            assertion,
            existing_var_type,
            array_key,
            possibly_undefined,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::HasStringArrayAccess => reconcile_array_access(
            assertion,
            existing_var_type,
            false,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::HasIntOrStringArrayAccess => reconcile_array_access(
            assertion,
            existing_var_type,
            true,
            key,
            negated,
            analysis_data,
            analyzer,
        ),
        Assertion::IsType(atomic) => {
            // Handle type assertion with intersection
            if let Some(result) = assertion_reconciler::intersect_union_with_atomic(
                existing_var_type,
                atomic,
                analyzer,
            ) {
                return with_docblock_from(result, existing_var_type);
            }
            with_docblock_from(TUnion::nothing(), existing_var_type)
        }
        Assertion::IsEqual(atomic) => {
            // For equality, the type becomes exactly that literal
            let mut result = TUnion::new(atomic.clone());
            result.from_docblock = existing_var_type.from_docblock;
            result
        }
        _ => existing_var_type.clone(),
    }
}

fn push_narrowed_template_type(
    target: &mut Vec<TAtomic>,
    template_atomic: &TAtomic,
    narrowed_as_type: TUnion,
) {
    if narrowed_as_type.is_nothing() {
        return;
    }

    match template_atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        } => target.push(TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(narrowed_as_type),
        }),
        _ => target.push(template_atomic.clone()),
    }
}

fn with_docblock_from(mut new_var_type: TUnion, existing_var_type: &TUnion) -> TUnion {
    new_var_type.from_docblock = existing_var_type.from_docblock;
    new_var_type.from_calculation = existing_var_type.from_calculation;
    new_var_type.ignore_nullable_issues = existing_var_type.ignore_nullable_issues;
    new_var_type.ignore_falsable_issues = existing_var_type.ignore_falsable_issues;
    new_var_type
}

fn finalize_reconciliation(
    acceptable_types: Vec<TAtomic>,
    did_remove_type: bool,
    existing_var_type: &TUnion,
    assertion: &Assertion,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        None,
        false,
        assertion,
        analyzer,
        analysis_data,
    )
}

/// Reconciles a truthy assertion.
///
/// Removes falsy types (null, false, 0, "", []) from the union.
fn reconcile_truthy(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        // Skip always-falsy types
        if atomic.is_falsy() {
            did_remove_type = true;
            continue;
        }

        if !atomic.is_truthy() {
            did_remove_type = true;
        }

        // For types that might be falsy, narrow them
        match atomic {
            TAtomic::TBool => {
                acceptable_types.push(TAtomic::TTrue);
            }
            TAtomic::TString => {
                acceptable_types.push(TAtomic::TNonEmptyString);
            }
            TAtomic::TInt => {
                // Keep int but note that 0 could be removed in strict mode
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TFloat => {
                // Keep float but note that 0.0 could be removed in strict mode
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TArray {
                key_type,
                value_type,
            } => {
                // Narrow to non-empty array
                acceptable_types.push(TAtomic::TNonEmptyArray {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                });
            }
            TAtomic::TList { value_type } => {
                acceptable_types.push(TAtomic::TNonEmptyList {
                    value_type: value_type.clone(),
                });
            }
            TAtomic::TKeyedArray {
                properties,
                sealed,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                if properties.is_empty() && *sealed && fallback_value_type.is_none() {
                    did_remove_type = true;
                    continue;
                }

                // If all shape keys are optional, the keyed array can still be empty.
                // On a truthy assertion, model this as a non-empty generic array.
                if atomic.is_truthy() {
                    acceptable_types.push(atomic.clone());
                } else if let Some(narrowed) = keyed_array_to_non_empty_array(
                    properties,
                    fallback_key_type.as_deref(),
                    fallback_value_type.as_deref(),
                ) {
                    acceptable_types.push(narrowed);
                }
            }
            TAtomic::TMixed => {
                acceptable_types.push(TAtomic::TNonEmptyMixed);
            }
            TAtomic::TNonEmptyMixed => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TLiteralInt { value: 0 } => {
                // Skip - falsy
                did_remove_type = true;
            }
            TAtomic::TLiteralFloat { value } if *value == 0.0 => {
                // Skip - falsy
                did_remove_type = true;
            }
            TAtomic::TLiteralString { value } => {
                if value.is_empty() {
                    // Skip empty string - falsy
                    did_remove_type = true;
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            TAtomic::TNull | TAtomic::TFalse => {
                // These are falsy, skip them
                did_remove_type = true;
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let narrowed =
                        reconcile_truthy(assertion, as_type, None, false, analysis_data, analyzer);
                    if narrowed.is_nothing() {
                        did_remove_type = true;
                    } else {
                        push_narrowed_template_type(&mut acceptable_types, atomic, narrowed);
                    }
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    let _ = (key, negated);
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

/// Reconciles an isset assertion.
///
/// Removes null from the union.
fn reconcile_isset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    inside_loop: bool,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = possibly_undefined;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TNull => {
                did_remove_type = true;
            }
            TAtomic::TMixed => {
                // Psalm preserves mixed on isset checks while clearing undefined-ness.
                acceptable_types.push(TAtomic::TMixed);
                did_remove_type = true;
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let narrowed = reconcile_isset(
                    assertion,
                    as_type,
                    false,
                    None,
                    false,
                    analysis_data,
                    analyzer,
                    inside_loop,
                );
                push_narrowed_template_type(&mut acceptable_types, atomic, narrowed);
                did_remove_type = true;
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    let _ = (key, negated);
    let result = finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    );

    if result.is_nothing() && (existing_var_type.is_nothing() || possibly_undefined) {
        let _ = inside_loop;
        return TUnion::mixed();
    }

    let mut result = result;
    result.is_nullable = false;
    result
}

/// Reconciles a non-empty countable assertion.
///
/// Narrows arrays and countable types to their non-empty variants.
fn reconcile_non_empty_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            } => {
                did_remove_type = true;
                if !value_type.is_nothing() {
                    acceptable_types.push(TAtomic::TNonEmptyArray {
                        key_type: key_type.clone(),
                        value_type: value_type.clone(),
                    });
                }
            }
            TAtomic::TList { value_type } => {
                did_remove_type = true;
                if !value_type.is_nothing() {
                    acceptable_types.push(TAtomic::TNonEmptyList {
                        value_type: value_type.clone(),
                    });
                }
            }
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TKeyedArray { properties, .. } => {
                if !properties.is_empty() {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                acceptable_types.push(TAtomic::TNonEmptyMixed);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_non_empty_countable(
                        assertion,
                        as_type,
                        None,
                        false,
                        analysis_data,
                        analyzer,
                    );
                    push_narrowed_template_type(&mut acceptable_types, atomic, narrowed);
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            _ => {
                // Non-countable types pass through
                did_remove_type = true;
                acceptable_types.push(atomic.clone());
            }
        }
    }

    let _ = (key, negated);
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

/// Reconciles an exact count assertion.
///
/// Narrows arrays to have exactly the specified count.
fn reconcile_exact_count(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    count: usize,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray { properties, .. } => {
                if properties.len() == count {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TArray {
                key_type,
                value_type,
            } => {
                did_remove_type = true;
                if count == 0 {
                    // Empty array
                    acceptable_types.push(TAtomic::TArray {
                        key_type: Box::new(TUnion::nothing()),
                        value_type: Box::new(TUnion::nothing()),
                    });
                } else if !value_type.is_nothing() {
                    // Non-empty array
                    acceptable_types.push(TAtomic::TNonEmptyArray {
                        key_type: key_type.clone(),
                        value_type: value_type.clone(),
                    });
                }
            }
            TAtomic::TNonEmptyArray { .. } => {
                if count > 0 {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                did_remove_type = true;
                if count == 0 {
                    acceptable_types.push(TAtomic::TArray {
                        key_type: Box::new(TUnion::nothing()),
                        value_type: Box::new(TUnion::nothing()),
                    });
                } else if !value_type.is_nothing() {
                    let mut properties = rustc_hash::FxHashMap::default();
                    for i in 0..count {
                        properties.insert(ArrayKey::Int(i as i64), (**value_type).clone());
                    }
                    acceptable_types.push(TAtomic::TKeyedArray {
                        properties,
                        is_list: true,
                        sealed: true,
                        fallback_key_type: None,
                        fallback_value_type: None,
                    });
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                acceptable_types.push(atomic.clone());
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    let _ = (key, negated);
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

/// Reconciles an in_array assertion.
///
/// Narrows the type to values that could be in the array.
fn reconcile_in_array(existing_var_type: &TUnion, assertion_type: &TUnion) -> TUnion {
    let Some(possible_union) = normalize_in_array_assertion_union(assertion_type) else {
        return existing_var_type.clone();
    };

    if let Some(intersection) =
        assertion_reconciler::intersect_union_with_union(existing_var_type, &possible_union)
    {
        return intersection;
    }

    // Fallback - intersect with existing type
    let mut acceptable_types = Vec::new();

    for existing_atomic in &existing_var_type.types {
        for possible_value in &possible_union.types {
            if types_might_match(existing_atomic, possible_value) {
                // Use the more specific type
                if is_more_specific(possible_value, existing_atomic) {
                    acceptable_types.push(possible_value.clone());
                } else {
                    acceptable_types.push(existing_atomic.clone());
                }
                break;
            }
        }
    }

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

fn normalize_in_array_assertion_union(assertion_type: &TUnion) -> Option<TUnion> {
    let mut value_union: Option<TUnion> = None;
    let mut saw_array_like = false;

    for atomic in &assertion_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                saw_array_like = true;
                value_union = Some(match value_union {
                    Some(existing) => combine_union_types(&existing, value_type, false),
                    None => (**value_type).clone(),
                });
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                saw_array_like = true;

                for property_type in properties.values() {
                    value_union = Some(match value_union {
                        Some(existing) => combine_union_types(&existing, property_type, false),
                        None => property_type.clone(),
                    });
                }

                if let Some(fallback_value_type) = fallback_value_type {
                    value_union = Some(match value_union {
                        Some(existing) => {
                            combine_union_types(&existing, fallback_value_type, false)
                        }
                        None => (**fallback_value_type).clone(),
                    });
                }
            }
            _ => {}
        }
    }

    if saw_array_like {
        value_union
    } else {
        Some(assertion_type.clone())
    }
}

/// Reconciles a has_array_key assertion.
fn reconcile_has_array_key(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    array_key: &ArrayKey,
    possibly_undefined: bool,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut result_types = Vec::new();
    let mut did_remove_type = possibly_undefined;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                // If the key exists and is optional, make it required
                if properties.contains_key(array_key) {
                    // Key exists - keep as is
                    result_types.push(atomic.clone());
                } else if let Some(fallback) = fallback_value_type {
                    // Add the key with the fallback type
                    did_remove_type = true;
                    let mut new_properties = properties.clone();
                    new_properties.insert(array_key.clone(), (**fallback).clone());
                    result_types.push(TAtomic::TKeyedArray {
                        properties: new_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                } else if !sealed {
                    // Open array - key could exist
                    did_remove_type = true;
                    result_types.push(atomic.clone());
                } else {
                    // Sealed array without the key - this is impossible
                    did_remove_type = true;
                }
            }
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => {
                if !key_type_allows_array_key(key_type, array_key) {
                    did_remove_type = true;
                    continue;
                }

                // General array with a known key - promote to open keyed-array with fallback.
                did_remove_type = true;
                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(array_key.clone(), (**value_type).clone());
                result_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(key_type.clone()),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                if !matches!(array_key, ArrayKey::Int(_)) {
                    did_remove_type = true;
                    continue;
                }

                did_remove_type = true;
                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(array_key.clone(), (**value_type).clone());
                result_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(Box::new(TUnion::int())),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;

                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(
                    array_key.clone(),
                    TUnion::new(TAtomic::TNonEmptyMixed),
                );

                result_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(Box::new(TUnion::array_key())),
                    fallback_value_type: Some(Box::new(TUnion::mixed())),
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_has_array_key(
                        assertion,
                        as_type,
                        array_key,
                        possibly_undefined,
                        None,
                        false,
                        analysis_data,
                        analyzer,
                    );
                    push_narrowed_template_type(&mut result_types, atomic, narrowed);
                } else {
                    result_types.push(atomic.clone());
                }
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    let _ = (key, negated);
    finalize_reconciliation(
        result_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

fn key_type_allows_array_key(key_type: &TUnion, array_key: &ArrayKey) -> bool {
    key_type
        .types
        .iter()
        .any(|atomic| atomic_allows_array_key(atomic, array_key))
}

fn atomic_allows_array_key(atomic: &TAtomic, array_key: &ArrayKey) -> bool {
    match (atomic, array_key) {
        (
            TAtomic::TMixed
            | TAtomic::TNonEmptyMixed
            | TAtomic::TArrayKey
            | TAtomic::TScalar
            | TAtomic::TNumeric,
            _,
        ) => true,
        (TAtomic::TString, ArrayKey::String(_)) => true,
        (TAtomic::TString, ArrayKey::Int(_)) => true,
        (TAtomic::TInt, ArrayKey::Int(_)) => true,
        (TAtomic::TLiteralInt { value }, ArrayKey::Int(expected)) => value == expected,
        (TAtomic::TLiteralString { value }, ArrayKey::String(expected)) => value == expected,
        (TAtomic::TLiteralString { value }, ArrayKey::Int(expected)) => value
            .parse::<i64>()
            .ok()
            .is_some_and(|int_value| int_value == *expected),
        (TAtomic::TIntRange { min, max }, ArrayKey::Int(expected)) => {
            min.is_none_or(|lower| *expected >= lower) && max.is_none_or(|upper| *expected <= upper)
        }
        (TAtomic::TTemplateParam { as_type, .. }, _) => {
            key_type_allows_array_key(as_type, array_key)
        }
        _ => false,
    }
}

/// Reconciles a has_nonnull_entry_for_key assertion.
fn reconcile_has_nonnull_entry_for_key(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    array_key: &ArrayKey,
    possibly_undefined: bool,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut result_types = Vec::new();
    let mut did_remove_type = possibly_undefined;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                if let Some(prop_type) = properties.get(array_key).cloned() {
                    // Narrow the property to non-null
                    let narrowed =
                        super::simple_negated_assertion_reconciler::subtract_null(&prop_type);
                    if narrowed != prop_type {
                        did_remove_type = true;
                    }
                    let mut new_properties = properties.clone();
                    new_properties.insert(array_key.clone(), narrowed);
                    result_types.push(TAtomic::TKeyedArray {
                        properties: new_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                } else if let Some(fallback) = fallback_value_type {
                    // Add the key with non-null fallback type
                    did_remove_type = true;
                    let narrowed =
                        super::simple_negated_assertion_reconciler::subtract_null(fallback);
                    let mut new_properties = properties.clone();
                    new_properties.insert(array_key.clone(), narrowed);
                    result_types.push(TAtomic::TKeyedArray {
                        properties: new_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                } else if !sealed {
                    did_remove_type = true;
                    result_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => {
                did_remove_type = true;
                let mut properties = rustc_hash::FxHashMap::default();
                let narrowed_value =
                    super::simple_negated_assertion_reconciler::subtract_null(value_type);
                properties.insert(array_key.clone(), narrowed_value);
                result_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(key_type.clone()),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                did_remove_type = true;
                let mut properties = rustc_hash::FxHashMap::default();
                let narrowed_value =
                    super::simple_negated_assertion_reconciler::subtract_null(value_type);
                properties.insert(array_key.clone(), narrowed_value);
                result_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(Box::new(TUnion::int())),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            TAtomic::TString => {
                did_remove_type = true;
                if matches!(array_key, ArrayKey::Int(_)) {
                    result_types.push(TAtomic::TNonEmptyString);
                }
            }
            TAtomic::TNonEmptyString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. } => {
                did_remove_type = true;
                if matches!(array_key, ArrayKey::Int(_)) {
                    result_types.push(atomic.clone());
                }
            }
            TAtomic::TLiteralString { value } => {
                did_remove_type = true;
                if !value.is_empty() && matches!(array_key, ArrayKey::Int(_)) {
                    result_types.push(atomic.clone());
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                result_types.push(atomic.clone());
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_has_nonnull_entry_for_key(
                        assertion,
                        as_type,
                        array_key,
                        possibly_undefined,
                        None,
                        false,
                        analysis_data,
                        analyzer,
                    );
                    push_narrowed_template_type(&mut result_types, atomic, narrowed);
                } else {
                    result_types.push(atomic.clone());
                }
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    let _ = (key, negated);
    finalize_reconciliation(
        result_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

/// Reconciles an array access assertion.
///
/// Ensures the type can be accessed as an array.
fn reconcile_array_access(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    allow_int_key: bool,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    if existing_var_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
    {
        return existing_var_type.clone();
    }

    if existing_var_type.is_mixed() {
        if allow_int_key {
            return existing_var_type.clone();
        }

        let mut reconciled_type = TUnion::from_types(vec![
            TAtomic::TNonEmptyArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            TAtomic::TNamedObject {
                name: analyzer.interner.intern("ArrayAccess"),
                type_params: None,
            },
        ]);
        reconciled_type.from_docblock = existing_var_type.from_docblock;
        return reconciled_type;
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        if can_be_array_accessed(atomic, allow_int_key) {
            acceptable_types.push(atomic.clone());
        }
    }

    let did_remove_type = acceptable_types.len() != existing_var_type.types.len();

    let _ = (key, negated);
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        analysis_data,
        analyzer,
    )
}

fn keyed_array_to_non_empty_array(
    properties: &FxHashMap<ArrayKey, TUnion>,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,
) -> Option<TAtomic> {
    let mut key_union: Option<TUnion> = None;
    let mut value_union: Option<TUnion> = None;

    for (key, value_type) in properties {
        let key_atomic = match key {
            ArrayKey::Int(value) => TAtomic::TLiteralInt { value: *value },
            ArrayKey::String(value) => TAtomic::TLiteralString {
                value: value.clone(),
            },
        };

        let key_type = TUnion::new(key_atomic);
        key_union = Some(match key_union.take() {
            Some(existing) => combine_union_types(&existing, &key_type, false),
            None => key_type,
        });

        value_union = Some(match value_union.take() {
            Some(existing) => combine_union_types(&existing, value_type, false),
            None => value_type.clone(),
        });
    }

    if let Some(extra_key_type) = fallback_key_type {
        key_union = Some(match key_union.take() {
            Some(existing) => combine_union_types(&existing, extra_key_type, false),
            None => extra_key_type.clone(),
        });
    }

    if let Some(extra_value_type) = fallback_value_type {
        value_union = Some(match value_union.take() {
            Some(existing) => combine_union_types(&existing, extra_value_type, false),
            None => extra_value_type.clone(),
        });
    }

    let key_type = key_union.unwrap_or_else(TUnion::array_key);
    let value_type = value_union?;

    Some(TAtomic::TNonEmptyArray {
        key_type: Box::new(key_type),
        value_type: Box::new(value_type),
    })
}

/// Checks if two types might match (for in_array checks).
fn types_might_match(a: &TAtomic, b: &TAtomic) -> bool {
    match (a, b) {
        (
            TAtomic::TInt,
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt,
        ) => true,
        (
            TAtomic::TLiteralInt { .. },
            TAtomic::TInt | TAtomic::TPositiveInt | TAtomic::TNegativeInt,
        ) => true,
        (TAtomic::TLiteralInt { value: v1 }, TAtomic::TLiteralInt { value: v2 }) => v1 == v2,

        (
            TAtomic::TString,
            TAtomic::TString | TAtomic::TLiteralString { .. } | TAtomic::TNonEmptyString,
        ) => true,
        (TAtomic::TLiteralString { .. }, TAtomic::TString | TAtomic::TNonEmptyString) => true,
        (TAtomic::TLiteralString { value: v1 }, TAtomic::TLiteralString { value: v2 }) => v1 == v2,

        (TAtomic::TBool, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse) => true,
        (TAtomic::TTrue, TAtomic::TBool | TAtomic::TTrue) => true,
        (TAtomic::TFalse, TAtomic::TBool | TAtomic::TFalse) => true,

        (TAtomic::TMixed | TAtomic::TNonEmptyMixed, _) => true,
        (_, TAtomic::TMixed | TAtomic::TNonEmptyMixed) => true,

        _ => a == b,
    }
}

/// Checks if type a is more specific than type b.
fn is_more_specific(a: &TAtomic, b: &TAtomic) -> bool {
    match (a, b) {
        (
            TAtomic::TLiteralInt { .. },
            TAtomic::TInt | TAtomic::TPositiveInt | TAtomic::TNegativeInt,
        ) => true,
        (TAtomic::TLiteralString { .. }, TAtomic::TString | TAtomic::TNonEmptyString) => true,
        (TAtomic::TTrue | TAtomic::TFalse, TAtomic::TBool) => true,
        (_, TAtomic::TMixed | TAtomic::TNonEmptyMixed) => true,
        _ => false,
    }
}

/// Checks if a type can be accessed as an array.
fn can_be_array_accessed(atomic: &TAtomic, allow_int_key: bool) -> bool {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => true,

        TAtomic::TString | TAtomic::TNonEmptyString | TAtomic::TLiteralString { .. } => {
            // String access with int key
            allow_int_key
        }

        TAtomic::TNamedObject { .. } => {
            // Could implement ArrayAccess
            true
        }

        TAtomic::TMixed | TAtomic::TNonEmptyMixed => true,

        TAtomic::TTemplateParam { as_type, .. } => {
            as_type.is_mixed()
                || as_type
                    .types
                    .iter()
                    .any(|t| can_be_array_accessed(t, allow_int_key))
        }

        _ => false,
    }
}
