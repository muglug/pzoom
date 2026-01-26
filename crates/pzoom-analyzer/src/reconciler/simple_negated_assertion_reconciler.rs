//! Simple negated assertion reconciler.
//!
//! Handles simple type subtractions like !null, !false, !true, !int, !string, etc.
//! This module provides the building blocks for more complex type subtractions.

use pzoom_code_info::{Assertion, TAtomic, TUnion};

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reconciles a simple negated assertion (type subtraction).
///
/// Returns Some(narrowed_type) if reconciliation was handled, None if it should
/// fall through to more complex reconciliation logic.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    _key: Option<&String>,
    _negated: bool,
    _analysis_data: &mut FunctionAnalysisData,
    _analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let assertion_type = assertion.get_type();

    if let Some(assertion_type) = assertion_type {
        match assertion_type {
            TAtomic::TObject => {
                return Some(subtract_object(existing_var_type));
            }
            TAtomic::TBool => {
                return Some(subtract_bool(existing_var_type));
            }
            TAtomic::TNumeric => {
                return Some(subtract_num(existing_var_type));
            }
            TAtomic::TFloat => {
                return Some(subtract_float(existing_var_type));
            }
            TAtomic::TInt => {
                return Some(subtract_int(existing_var_type));
            }
            TAtomic::TString => {
                return Some(subtract_string(existing_var_type));
            }
            TAtomic::TArrayKey => {
                return Some(subtract_arraykey(existing_var_type));
            }
            TAtomic::TNull => {
                return Some(subtract_null(existing_var_type));
            }
            TAtomic::TFalse => {
                return Some(subtract_false(existing_var_type));
            }
            TAtomic::TTrue => {
                return Some(subtract_true(existing_var_type));
            }
            TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. } => {
                return Some(subtract_array(existing_var_type));
            }
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => {
                return Some(subtract_list(existing_var_type));
            }
            _ => {}
        }
    }

    match assertion {
        Assertion::Falsy => Some(reconcile_falsy(existing_var_type)),
        Assertion::IsNotIsset => Some(reconcile_not_isset(existing_var_type)),
        Assertion::EmptyCountable => Some(reconcile_empty_countable(existing_var_type)),
        Assertion::DoesNotHaveArrayKey(key) => {
            Some(reconcile_no_array_key(existing_var_type, key))
        }
        _ => None,
    }
}

/// Subtracts object types from a union.
fn subtract_object(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TCallable { .. } => {
                // Remove object types
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_object(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts bool types from a union.
fn subtract_bool(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => {
                // Remove bool types
            }
            TAtomic::TScalar => {
                // Narrow scalar to non-bool scalars
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TInt);
                acceptable_types.push(TAtomic::TFloat);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_bool(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts numeric types (int|float) from a union.
fn subtract_num(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TNumeric => {
                // Remove numeric types
            }
            TAtomic::TScalar => {
                // Narrow to non-numeric scalars
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TArrayKey => {
                // array-key - int = string
                acceptable_types.push(TAtomic::TString);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_num(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts float types from a union.
fn subtract_float(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => {
                // Remove float types
            }
            TAtomic::TScalar => {
                // Narrow to non-float scalars
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TInt);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TNumeric => {
                // numeric - float = int
                acceptable_types.push(TAtomic::TInt);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_float(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts int types from a union.
fn subtract_int(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. } => {
                // Remove int types
            }
            TAtomic::TScalar => {
                // Narrow to non-int scalars
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TNumeric => {
                // numeric - int = float
                acceptable_types.push(TAtomic::TFloat);
            }
            TAtomic::TArrayKey => {
                // array-key - int = string
                acceptable_types.push(TAtomic::TString);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_int(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts string types from a union.
fn subtract_string(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. } => {
                // Remove string types
            }
            TAtomic::TScalar => {
                // Narrow to non-string scalars
                acceptable_types.push(TAtomic::TInt);
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TArrayKey => {
                // array-key - string = int
                acceptable_types.push(TAtomic::TInt);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_string(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts arraykey (int|string) types from a union.
fn subtract_arraykey(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TArrayKey => {
                // Remove arraykey types
            }
            TAtomic::TScalar => {
                // Narrow to non-arraykey scalars
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TNumeric => {
                // numeric - arraykey = float (since int is arraykey)
                acceptable_types.push(TAtomic::TFloat);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_arraykey(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts null from a type union.
pub fn subtract_null(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TNull => {
                // Remove null
            }
            TAtomic::TMixed => {
                // mixed - null = non-null-mixed
                acceptable_types.push(TAtomic::TNonEmptyMixed);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let subtracted = subtract_null(as_type);
                if !subtracted.is_nothing() {
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
        let mut result = TUnion::from_types(acceptable_types);
        result.is_nullable = false;
        result
    }
}

/// Subtracts false from a type union.
pub fn subtract_false(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TFalse => {
                // Remove false
            }
            TAtomic::TBool => {
                // bool - false = true
                acceptable_types.push(TAtomic::TTrue);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let subtracted = subtract_false(as_type);
                if !subtracted.is_nothing() {
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

/// Subtracts true from a type union.
pub fn subtract_true(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TTrue => {
                // Remove true
            }
            TAtomic::TBool => {
                // bool - true = false
                acceptable_types.push(TAtomic::TFalse);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let subtracted = subtract_true(as_type);
                if !subtracted.is_nothing() {
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

/// Subtracts array types from a union.
fn subtract_array(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. } => {
                // Remove array types
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_array(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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

/// Subtracts list types from a union.
fn subtract_list(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => {
                // Remove list types
            }
            TAtomic::TKeyedArray { is_list: true, .. } => {
                // Remove list-shaped keyed arrays
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_list(as_type);
                    if !subtracted.is_nothing() {
                        acceptable_types.push(atomic.clone());
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
                // Could be empty string
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
            TAtomic::TNull
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { value: 0 }
            | TAtomic::TLiteralFloat { value: _ } => {
                // These might be falsy, keep them
                if atomic.is_falsy() || !atomic.is_truthy() {
                    acceptable_types.push(atomic.clone());
                }
            }
            TAtomic::TLiteralString { value } if value.is_empty() => {
                acceptable_types.push(atomic.clone());
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
    // Otherwise, the type would narrow to nothing
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
                // Remove the key from known items if it exists
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
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                result_types.push(atomic.clone());
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
