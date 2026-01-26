//! Simple assertion reconciler.
//!
//! Handles positive assertions like `truthy`, `isset`, and basic type checks.

use pzoom_code_info::{ArrayKey, Assertion, TAtomic, TUnion};

use super::assertion_reconciler;
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
    _analysis_data: &mut FunctionAnalysisData,
    _analyzer: &StatementsAnalyzer<'_>,
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
                return super::simple_negated_assertion_reconciler::subtract_null(existing_var_type);
            }
            _ => {}
        }

        // Try to intersect with the assertion type
        if let Some(result) = assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            assertion_atomic,
            _analyzer,
        ) {
            return result;
        }

        // If intersection is empty, the assertion is impossible
        return TUnion::new(assertion_atomic.clone());
    }

    // Handle specific assertions
    match assertion {
        Assertion::Truthy => reconcile_truthy(existing_var_type),
        Assertion::IsIsset | Assertion::IsEqualIsset => {
            reconcile_isset(existing_var_type, possibly_undefined, inside_loop)
        }
        Assertion::ArrayKeyExists => {
            if existing_var_type.is_nothing() {
                if inside_loop {
                    TUnion::mixed()
                } else {
                    TUnion::mixed()
                }
            } else {
                existing_var_type.clone()
            }
        }
        Assertion::NonEmptyCountable(_) => reconcile_non_empty_countable(existing_var_type),
        Assertion::HasExactCount(count) => reconcile_exact_count(existing_var_type, *count),
        Assertion::InArray(array_type) => reconcile_in_array(existing_var_type, array_type),
        Assertion::HasArrayKey(array_key) => {
            reconcile_has_array_key(existing_var_type, array_key, possibly_undefined)
        }
        Assertion::HasNonnullEntryForKey(array_key) => {
            reconcile_has_nonnull_entry_for_key(existing_var_type, array_key, possibly_undefined)
        }
        Assertion::HasStringArrayAccess => reconcile_array_access(existing_var_type, false),
        Assertion::HasIntOrStringArrayAccess => reconcile_array_access(existing_var_type, true),
        Assertion::IsType(atomic) => {
            // Handle type assertion with intersection
            if let Some(result) = assertion_reconciler::intersect_union_with_atomic(
                existing_var_type,
                atomic,
                _analyzer,
            ) {
                return result;
            }
            TUnion::new(atomic.clone())
        }
        Assertion::IsEqual(atomic) => {
            // For equality, the type becomes exactly that literal
            TUnion::new(atomic.clone())
        }
        _ => existing_var_type.clone(),
    }
}

/// Reconciles a truthy assertion.
///
/// Removes falsy types (null, false, 0, "", []) from the union.
fn reconcile_truthy(existing_var_type: &TUnion) -> TUnion {
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
            TAtomic::TArray { key_type, value_type } => {
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
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                // Keyed arrays are truthy if they have properties
                if !properties.is_empty() || !sealed {
                    acceptable_types.push(atomic.clone());
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
                    let narrowed = reconcile_truthy(as_type);
                    if !narrowed.is_nothing() {
                        acceptable_types.push(atomic.clone());
                    } else {
                        did_remove_type = true;
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

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

/// Reconciles an isset assertion.
///
/// Removes null from the union.
fn reconcile_isset(existing_var_type: &TUnion, possibly_undefined: bool, inside_loop: bool) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = possibly_undefined;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TNull => {
                did_remove_type = true;
            }
            TAtomic::TMixed => {
                // mixed - null = non-null mixed
                acceptable_types.push(TAtomic::TNonEmptyMixed);
                did_remove_type = true;
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let narrowed = reconcile_isset(as_type, false, inside_loop);
                if !narrowed.is_nothing() {
                    acceptable_types.push(atomic.clone());
                }
                did_remove_type = true;
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    if acceptable_types.is_empty() {
        if existing_var_type.is_nothing() || possibly_undefined {
            // Variable wasn't in scope - return mixed in loop context
            if inside_loop {
                TUnion::mixed()
            } else {
                TUnion::mixed()
            }
        } else {
            TUnion::nothing()
        }
    } else {
        let mut result = TUnion::from_types(acceptable_types);
        result.is_nullable = false;
        result
    }
}

/// Reconciles a non-empty countable assertion.
///
/// Narrows arrays and countable types to their non-empty variants.
fn reconcile_non_empty_countable(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TArray { key_type, value_type } => {
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
                    let narrowed = reconcile_non_empty_countable(as_type);
                    if !narrowed.is_nothing() {
                        acceptable_types.push(atomic.clone());
                    }
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

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

/// Reconciles an exact count assertion.
///
/// Narrows arrays to have exactly the specified count.
fn reconcile_exact_count(existing_var_type: &TUnion, count: usize) -> TUnion {
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
            TAtomic::TArray { key_type, value_type } => {
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
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => {
                if count > 0 {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TList { value_type } => {
                did_remove_type = true;
                if count == 0 {
                    acceptable_types.push(TAtomic::TArray {
                        key_type: Box::new(TUnion::nothing()),
                        value_type: Box::new(TUnion::nothing()),
                    });
                } else if !value_type.is_nothing() {
                    acceptable_types.push(TAtomic::TNonEmptyList {
                        value_type: value_type.clone(),
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

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

/// Reconciles an in_array assertion.
///
/// Narrows the type to values that could be in the array.
fn reconcile_in_array(existing_var_type: &TUnion, array_type: &TUnion) -> TUnion {
    // Get the value types from the array
    let mut possible_values = Vec::new();

    for atomic in &array_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                for v in &value_type.types {
                    possible_values.push(v.clone());
                }
            }
            TAtomic::TKeyedArray { properties, .. } => {
                for prop_type in properties.values() {
                    for v in &prop_type.types {
                        possible_values.push(v.clone());
                    }
                }
            }
            _ => {}
        }
    }

    if possible_values.is_empty() {
        return existing_var_type.clone();
    }

    // Try to intersect types
    let possible_union = TUnion::from_types(possible_values);

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

/// Reconciles a has_array_key assertion.
fn reconcile_has_array_key(
    existing_var_type: &TUnion,
    key: &ArrayKey,
    possibly_undefined: bool,
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
                if properties.contains_key(key) {
                    // Key exists - keep as is
                    result_types.push(atomic.clone());
                } else if let Some(fallback) = fallback_value_type {
                    // Add the key with the fallback type
                    did_remove_type = true;
                    let mut new_properties = properties.clone();
                    new_properties.insert(key.clone(), (**fallback).clone());
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
            TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. } => {
                // General array - key might exist
                did_remove_type = true;
                result_types.push(atomic.clone());
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                result_types.push(atomic.clone());
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_has_array_key(as_type, key, possibly_undefined);
                    if !narrowed.is_nothing() {
                        result_types.push(atomic.clone());
                    }
                } else {
                    result_types.push(atomic.clone());
                }
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    if result_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(result_types)
    }
}

/// Reconciles a has_nonnull_entry_for_key assertion.
fn reconcile_has_nonnull_entry_for_key(
    existing_var_type: &TUnion,
    key: &ArrayKey,
    possibly_undefined: bool,
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
                if let Some(prop_type) = properties.get(key).cloned() {
                    // Narrow the property to non-null
                    let narrowed = super::simple_negated_assertion_reconciler::subtract_null(&prop_type);
                    if narrowed != prop_type {
                        did_remove_type = true;
                    }
                    let mut new_properties = properties.clone();
                    new_properties.insert(key.clone(), narrowed);
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
                    let narrowed = super::simple_negated_assertion_reconciler::subtract_null(fallback);
                    let mut new_properties = properties.clone();
                    new_properties.insert(key.clone(), narrowed);
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
            TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. } => {
                did_remove_type = true;
                result_types.push(atomic.clone());
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                result_types.push(atomic.clone());
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_has_nonnull_entry_for_key(as_type, key, possibly_undefined);
                    if !narrowed.is_nothing() {
                        result_types.push(atomic.clone());
                    }
                } else {
                    result_types.push(atomic.clone());
                }
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    if result_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(result_types)
    }
}

/// Reconciles an array access assertion.
///
/// Ensures the type can be accessed as an array.
fn reconcile_array_access(existing_var_type: &TUnion, allow_int_key: bool) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        if can_be_array_accessed(atomic, allow_int_key) {
            acceptable_types.push(atomic.clone());
        }
    }

    if acceptable_types.is_empty() {
        // If nothing can be array accessed, return mixed (might be ArrayAccess)
        TUnion::mixed()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

/// Checks if two types might match (for in_array checks).
fn types_might_match(a: &TAtomic, b: &TAtomic) -> bool {
    match (a, b) {
        (
            TAtomic::TInt,
            TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TPositiveInt | TAtomic::TNegativeInt,
        ) => true,
        (TAtomic::TLiteralInt { .. }, TAtomic::TInt | TAtomic::TPositiveInt | TAtomic::TNegativeInt) => {
            true
        }
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
            as_type.is_mixed() || as_type.types.iter().any(|t| can_be_array_accessed(t, allow_int_key))
        }

        _ => false,
    }
}
