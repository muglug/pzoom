//! Negated assertion reconciler.
//!
//! Handles negated assertions like `!truthy` (falsy), `!isset`, and type negations.

use pzoom_code_info::{Assertion, TAtomic, TUnion};

use super::simple_negated_assertion_reconciler;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reconciles a negated assertion with an existing type.
///
/// Subtracts the asserted type from the existing type.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    _inside_loop: bool,
) -> TUnion {
    // First, try the simple negated assertion reconciler
    if let Some(result) = simple_negated_assertion_reconciler::reconcile(
        assertion,
        existing_var_type,
        key,
        negated,
        analysis_data,
        analyzer,
    ) {
        return result;
    }

    // Fall through to more specific handling
    match assertion {
        Assertion::Falsy => reconcile_falsy(existing_var_type),
        Assertion::IsNotIsset => reconcile_not_isset(existing_var_type),
        Assertion::IsNotType(atomic) => subtract_type(existing_var_type, atomic),
        Assertion::IsNotEqual(atomic) => subtract_literal(existing_var_type, atomic),
        Assertion::EmptyCountable => reconcile_empty_countable(existing_var_type),
        Assertion::NotInArray(array_type) => reconcile_not_in_array(existing_var_type, array_type),
        Assertion::DoesNotHaveArrayKey(key) => {
            reconcile_no_array_key(existing_var_type, key)
        }
        Assertion::DoesNotHaveExactCount(_) => existing_var_type.clone(),
        Assertion::DoesNotHaveNonnullEntryForKey(_) => existing_var_type.clone(),
        Assertion::ArrayKeyDoesNotExist => existing_var_type.clone(),
        _ => existing_var_type.clone(),
    }
}

/// Reconciles a falsy assertion (the negation of truthy).
///
/// Keeps only falsy types (null, false, 0, "", []).
fn reconcile_falsy(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        // If the type is always truthy, exclude it
        if atomic.is_truthy() {
            continue;
        }

        // For types that might be truthy, narrow to falsy variants
        match atomic {
            TAtomic::TBool => {
                acceptable_types.push(TAtomic::TFalse);
            }
            TAtomic::TString => {
                // Could be empty string - add literal empty string
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
            }
            TAtomic::TInt => {
                // Could be 0
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
            }
            TAtomic::TArray { .. } => {
                // Could be empty array
                acceptable_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::nothing()),
                    value_type: Box::new(TUnion::nothing()),
                });
            }
            TAtomic::TList { .. } => {
                // Could be empty list (which is empty array)
                acceptable_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::nothing()),
                    value_type: Box::new(TUnion::nothing()),
                });
            }
            TAtomic::TMixed => {
                // Mixed could be any falsy value
                acceptable_types.push(TAtomic::TNull);
                acceptable_types.push(TAtomic::TFalse);
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
            }
            TAtomic::TNonEmptyMixed => {
                // Non-empty mixed but can still be falsy (0, "", false)
                acceptable_types.push(TAtomic::TFalse);
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
            }
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TLiteralInt { value: 0 } => {
                // These are falsy, keep them
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TLiteralString { value } if value.is_empty() => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TLiteralFloat { value } if *value == 0.0 => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TFloat => {
                // Could be 0.0
                acceptable_types.push(TAtomic::TLiteralFloat { value: 0.0 });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = reconcile_falsy(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
                    }
                }
            }
            _ => {
                // Other types - check if they could be falsy
                if atomic.is_falsy() || !atomic.is_truthy() {
                    acceptable_types.push(atomic.clone());
                }
            }
        }
    }

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(acceptable_types)
    }
}

/// Reconciles a !isset assertion.
///
/// Returns null type (the variable is not set).
fn reconcile_not_isset(existing_var_type: &TUnion) -> TUnion {
    // If the type already includes null, return just null
    let has_null = existing_var_type
        .types
        .iter()
        .any(|t| matches!(t, TAtomic::TNull));

    if has_null || existing_var_type.is_nullable {
        TUnion::new(TAtomic::TNull)
    } else if existing_var_type.is_mixed() {
        // Mixed can include null
        TUnion::new(TAtomic::TNull)
    } else {
        // The variable wasn't set - this is an impossible type
        TUnion::nothing()
    }
}

/// Reconciles an empty countable assertion.
///
/// Narrows to empty arrays.
fn reconcile_empty_countable(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TArray { .. } => {
                // Narrow to empty array
                acceptable_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::nothing()),
                    value_type: Box::new(TUnion::nothing()),
                });
            }
            TAtomic::TList { .. } => {
                acceptable_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::nothing()),
                    value_type: Box::new(TUnion::nothing()),
                });
            }
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => {
                // Non-empty can't be empty, skip
            }
            TAtomic::TKeyedArray { properties, .. } if properties.is_empty() => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TKeyedArray { .. } => {
                // Has properties, can't be empty
            }
            TAtomic::TMixed => {
                // Could be empty array
                acceptable_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::nothing()),
                    value_type: Box::new(TUnion::nothing()),
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = reconcile_empty_countable(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
                    }
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            _ => {
                // Keep other types (they're not countable anyway)
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

/// Reconciles a not-in-array assertion.
fn reconcile_not_in_array(existing_var_type: &TUnion, array_type: &TUnion) -> TUnion {
    // Get the literal values from the array type
    let mut values_to_remove = Vec::new();

    for atomic in &array_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                for value_atomic in &value_type.types {
                    if matches!(
                        value_atomic,
                        TAtomic::TLiteralInt { .. }
                            | TAtomic::TLiteralString { .. }
                            | TAtomic::TLiteralFloat { .. }
                            | TAtomic::TTrue
                            | TAtomic::TFalse
                    ) {
                        values_to_remove.push(value_atomic.clone());
                    }
                }
            }
            TAtomic::TKeyedArray { properties, .. } => {
                for value in properties.values() {
                    for value_atomic in &value.types {
                        if matches!(
                            value_atomic,
                            TAtomic::TLiteralInt { .. }
                                | TAtomic::TLiteralString { .. }
                                | TAtomic::TLiteralFloat { .. }
                                | TAtomic::TTrue
                                | TAtomic::TFalse
                        ) {
                            values_to_remove.push(value_atomic.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if values_to_remove.is_empty() {
        return existing_var_type.clone();
    }

    // Remove the literal values from the existing type
    let mut result = existing_var_type.clone();
    for value in &values_to_remove {
        result = subtract_literal(&result, value);
    }

    result
}

/// Reconciles a DoesNotHaveArrayKey assertion.
fn reconcile_no_array_key(
    existing_var_type: &TUnion,
    key: &pzoom_code_info::ArrayKey,
) -> TUnion {
    let mut result_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                // Remove the key from known items
                let mut new_properties = properties.clone();
                new_properties.remove(key);

                result_types.push(TAtomic::TKeyedArray {
                    properties: new_properties,
                    is_list: *is_list,
                    sealed: *sealed,
                    fallback_key_type: fallback_key_type.clone(),
                    fallback_value_type: fallback_value_type.clone(),
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let subtracted = reconcile_no_array_key(as_type, key);
                if !subtracted.is_nothing() {
                    result_types.push(atomic.clone());
                }
            }
            _ => {
                result_types.push(atomic.clone());
            }
        }
    }

    if result_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(result_types)
    }
}

/// Subtracts a type from a union.
///
/// Removes the specified atomic type from the union.
pub fn subtract_type(existing_var_type: &TUnion, type_to_remove: &TAtomic) -> TUnion {
    let mut remaining_types = Vec::new();

    for atomic in &existing_var_type.types {
        if let Some(narrowed) = narrow_after_subtraction(atomic, type_to_remove) {
            remaining_types.push(narrowed);
        } else if !should_subtract(atomic, type_to_remove) {
            remaining_types.push(atomic.clone());
        }
    }

    if remaining_types.is_empty() {
        TUnion::nothing()
    } else {
        let mut result = TUnion::from_types(remaining_types);

        // Update nullable flag
        if matches!(type_to_remove, TAtomic::TNull) {
            result.is_nullable = false;
        }

        result
    }
}

/// Subtracts a literal value from a union.
fn subtract_literal(existing_var_type: &TUnion, literal: &TAtomic) -> TUnion {
    let mut remaining_types = Vec::new();

    for atomic in &existing_var_type.types {
        match (atomic, literal) {
            // Same literal values
            (
                TAtomic::TLiteralInt { value: v1 },
                TAtomic::TLiteralInt { value: v2 },
            ) if v1 == v2 => {
                // Remove
            }
            (
                TAtomic::TLiteralString { value: v1 },
                TAtomic::TLiteralString { value: v2 },
            ) if v1 == v2 => {
                // Remove
            }
            (
                TAtomic::TLiteralFloat { value: v1 },
                TAtomic::TLiteralFloat { value: v2 },
            ) if v1 == v2 => {
                // Remove
            }
            (TAtomic::TTrue, TAtomic::TTrue) => {
                // Remove
            }
            (TAtomic::TFalse, TAtomic::TFalse) => {
                // Remove
            }
            (TAtomic::TNull, TAtomic::TNull) => {
                // Remove
            }
            // Bool narrowing
            (TAtomic::TBool, TAtomic::TTrue) => {
                remaining_types.push(TAtomic::TFalse);
            }
            (TAtomic::TBool, TAtomic::TFalse) => {
                remaining_types.push(TAtomic::TTrue);
            }
            // Keep everything else
            _ => {
                remaining_types.push(atomic.clone());
            }
        }
    }

    if remaining_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(remaining_types)
    }
}

/// Returns a narrowed type after subtraction, if narrowing is possible.
fn narrow_after_subtraction(existing: &TAtomic, to_remove: &TAtomic) -> Option<TAtomic> {
    match (existing, to_remove) {
        // Bool - true = false
        (TAtomic::TBool, TAtomic::TTrue) => Some(TAtomic::TFalse),
        // Bool - false = true
        (TAtomic::TBool, TAtomic::TFalse) => Some(TAtomic::TTrue),

        // Scalar narrowing
        (TAtomic::TScalar, TAtomic::TBool) => None, // Can't easily represent int|float|string
        (TAtomic::TScalar, TAtomic::TInt) => None,
        (TAtomic::TScalar, TAtomic::TFloat) => None,
        (TAtomic::TScalar, TAtomic::TString) => None,

        // array-key - int = string
        (TAtomic::TArrayKey, TAtomic::TInt) => Some(TAtomic::TString),
        // array-key - string = int
        (TAtomic::TArrayKey, TAtomic::TString) => Some(TAtomic::TInt),

        // numeric - int = float
        (TAtomic::TNumeric, TAtomic::TInt) => Some(TAtomic::TFloat),
        // numeric - float = int
        (TAtomic::TNumeric, TAtomic::TFloat) => Some(TAtomic::TInt),

        // Template params
        (TAtomic::TTemplateParam { name, defining_entity, as_type }, _) => {
            let subtracted = subtract_type(as_type, to_remove);
            if subtracted.is_nothing() {
                None
            } else if subtracted == *as_type.as_ref() {
                None // No change
            } else {
                Some(TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(subtracted),
                })
            }
        }

        _ => None,
    }
}

/// Determines if an existing type should be completely subtracted.
fn should_subtract(existing: &TAtomic, to_remove: &TAtomic) -> bool {
    match (existing, to_remove) {
        // Exact matches
        (a, b) if a == b => true,

        // Null removal
        (TAtomic::TNull, TAtomic::TNull) => true,

        // Bool variants
        (TAtomic::TTrue, TAtomic::TTrue) => true,
        (TAtomic::TFalse, TAtomic::TFalse) => true,
        (TAtomic::TBool, TAtomic::TBool) => true,
        (TAtomic::TTrue, TAtomic::TBool) => true,
        (TAtomic::TFalse, TAtomic::TBool) => true,

        // Integer removal
        (TAtomic::TInt, TAtomic::TInt) => true,
        (TAtomic::TLiteralInt { value: v1 }, TAtomic::TLiteralInt { value: v2 }) => v1 == v2,
        (TAtomic::TLiteralInt { .. }, TAtomic::TInt) => true,
        (TAtomic::TPositiveInt, TAtomic::TPositiveInt) => true,
        (TAtomic::TPositiveInt, TAtomic::TInt) => true,
        (TAtomic::TNegativeInt, TAtomic::TNegativeInt) => true,
        (TAtomic::TNegativeInt, TAtomic::TInt) => true,

        // String removal
        (TAtomic::TString, TAtomic::TString) => true,
        (TAtomic::TLiteralString { value: v1 }, TAtomic::TLiteralString { value: v2 }) => v1 == v2,
        (TAtomic::TLiteralString { .. }, TAtomic::TString) => true,
        (TAtomic::TNonEmptyString, TAtomic::TNonEmptyString) => true,
        (TAtomic::TNonEmptyString, TAtomic::TString) => true,
        (TAtomic::TNumericString, TAtomic::TString) => true,
        (TAtomic::TClassString { .. }, TAtomic::TString) => true,

        // Float removal
        (TAtomic::TFloat, TAtomic::TFloat) => true,
        (TAtomic::TLiteralFloat { value: v1 }, TAtomic::TLiteralFloat { value: v2 }) => v1 == v2,
        (TAtomic::TLiteralFloat { .. }, TAtomic::TFloat) => true,

        // Array removal
        (TAtomic::TArray { .. }, TAtomic::TArray { .. }) => true,
        (TAtomic::TNonEmptyArray { .. }, TAtomic::TArray { .. }) => true,
        (TAtomic::TList { .. }, TAtomic::TArray { .. }) => true,
        (TAtomic::TNonEmptyList { .. }, TAtomic::TArray { .. }) => true,
        (TAtomic::TKeyedArray { .. }, TAtomic::TArray { .. }) => true,

        // Object removal
        (TAtomic::TObject, TAtomic::TObject) => true,
        (TAtomic::TNamedObject { name: n1, .. }, TAtomic::TNamedObject { name: n2, .. }) => n1 == n2,
        (TAtomic::TNamedObject { .. }, TAtomic::TObject) => true,
        (TAtomic::TClosure { .. }, TAtomic::TObject) => true,

        // Resource removal
        (TAtomic::TResource, TAtomic::TResource) => true,
        (TAtomic::TClosedResource, TAtomic::TClosedResource) => true,

        // Callable removal
        (TAtomic::TCallable { .. }, TAtomic::TCallable { .. }) => true,
        (TAtomic::TClosure { .. }, TAtomic::TClosure { .. }) => true,
        (TAtomic::TClosure { .. }, TAtomic::TCallable { .. }) => true,

        // Numeric removal
        (TAtomic::TNumeric, TAtomic::TNumeric) => true,
        (TAtomic::TInt, TAtomic::TNumeric) => true,
        (TAtomic::TFloat, TAtomic::TNumeric) => true,

        // Scalar removal
        (TAtomic::TScalar, TAtomic::TScalar) => true,
        (TAtomic::TInt, TAtomic::TScalar) => true,
        (TAtomic::TFloat, TAtomic::TScalar) => true,
        (TAtomic::TString, TAtomic::TScalar) => true,
        (TAtomic::TBool, TAtomic::TScalar) => true,

        // ArrayKey removal
        (TAtomic::TArrayKey, TAtomic::TArrayKey) => true,
        (TAtomic::TInt, TAtomic::TArrayKey) => true,
        (TAtomic::TString, TAtomic::TArrayKey) => true,

        _ => false,
    }
}

/// Subtracts null from a type union.
pub fn subtract_null(existing_var_type: &TUnion) -> TUnion {
    subtract_type(existing_var_type, &TAtomic::TNull)
}

/// Subtracts false from a type union.
pub fn subtract_false(existing_var_type: &TUnion) -> TUnion {
    subtract_type(existing_var_type, &TAtomic::TFalse)
}

/// Subtracts true from a type union.
pub fn subtract_true(existing_var_type: &TUnion) -> TUnion {
    subtract_type(existing_var_type, &TAtomic::TTrue)
}
