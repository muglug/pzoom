//! Main assertion reconciler.
//!
//! This module handles type reconciliation based on assertions. It dispatches
//! to simple_assertion_reconciler for positive assertions and
//! negated_assertion_reconciler for negative assertions.

use pzoom_code_info::{Assertion, TAtomic, TUnion};

use super::{negated_assertion_reconciler, simple_assertion_reconciler};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Main entry point for assertion reconciliation.
///
/// Given an assertion and an existing type, returns the narrowed type that
/// results from applying the assertion.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: Option<&TUnion>,
    possibly_undefined: bool,
    key: Option<&String>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
    negated: bool,
) -> TUnion {
    // If we have no existing type, create a default
    let existing_var_type = existing_var_type.cloned().unwrap_or_else(|| {
        get_missing_type(assertion, inside_loop)
    });

    // Dispatch based on whether the assertion is negated
    if assertion.has_negation() {
        negated_assertion_reconciler::reconcile(
            assertion,
            &existing_var_type,
            key,
            negated,
            analysis_data,
            analyzer,
            inside_loop,
        )
    } else {
        simple_assertion_reconciler::reconcile(
            assertion,
            &existing_var_type,
            possibly_undefined,
            key,
            negated,
            analysis_data,
            analyzer,
            inside_loop,
        )
    }
}

/// Returns the type to use when a variable doesn't exist in the context.
fn get_missing_type(assertion: &Assertion, inside_loop: bool) -> TUnion {
    match assertion {
        Assertion::IsIsset | Assertion::IsEqualIsset | Assertion::IsNotIsset => {
            // For isset checks, missing variables are null
            TUnion::new(TAtomic::TNull)
        }
        Assertion::HasArrayKey(_) | Assertion::DoesNotHaveArrayKey(_) => {
            // For array key checks, start with mixed
            TUnion::mixed()
        }
        _ => {
            if inside_loop {
                TUnion::mixed()
            } else {
                TUnion::mixed()
            }
        }
    }
}

/// Intersects a union type with an atomic type.
///
/// Returns the narrowed type, or nothing if the types are incompatible.
pub fn intersect_union_with_atomic(
    existing_var_type: &TUnion,
    assertion_atomic: &TAtomic,
    _analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        if let Some(intersected) = intersect_atomic_with_atomic(atomic, assertion_atomic) {
            acceptable_types.push(intersected);
        }
    }

    if acceptable_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(acceptable_types))
    }
}

/// Intersects two atomic types.
///
/// Returns Some(narrowed_type) if the types can be intersected, None if incompatible.
pub fn intersect_atomic_with_atomic(
    existing_atomic: &TAtomic,
    assertion_atomic: &TAtomic,
) -> Option<TAtomic> {
    // Handle mixed types - always intersect
    match existing_atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
            return Some(assertion_atomic.clone());
        }
        _ => {}
    }

    // Check for identical types
    if existing_atomic == assertion_atomic {
        return Some(existing_atomic.clone());
    }

    // Handle assertion type being mixed
    match assertion_atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
            return Some(existing_atomic.clone());
        }
        _ => {}
    }

    // Handle specific type intersections
    match (existing_atomic, assertion_atomic) {
        // int types
        (TAtomic::TInt, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TInt, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TInt, TAtomic::TPositiveInt) => Some(TAtomic::TPositiveInt),
        (TAtomic::TInt, TAtomic::TNegativeInt) => Some(TAtomic::TNegativeInt),
        (TAtomic::TLiteralInt { value }, TAtomic::TInt) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralInt { value }, TAtomic::TPositiveInt) if *value > 0 => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralInt { value }, TAtomic::TNegativeInt) if *value < 0 => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TPositiveInt, TAtomic::TInt) => Some(TAtomic::TPositiveInt),
        (TAtomic::TNegativeInt, TAtomic::TInt) => Some(TAtomic::TNegativeInt),

        // float types
        (TAtomic::TFloat, TAtomic::TFloat) => Some(TAtomic::TFloat),
        (TAtomic::TFloat, TAtomic::TLiteralFloat { value }) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }
        (TAtomic::TLiteralFloat { value }, TAtomic::TFloat) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }

        // numeric types
        (TAtomic::TNumeric, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TNumeric, TAtomic::TFloat) => Some(TAtomic::TFloat),
        (TAtomic::TNumeric, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TNumeric, TAtomic::TLiteralFloat { value }) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }
        (TAtomic::TInt, TAtomic::TNumeric) => Some(TAtomic::TInt),
        (TAtomic::TFloat, TAtomic::TNumeric) => Some(TAtomic::TFloat),

        // string types
        (TAtomic::TString, TAtomic::TString) => Some(TAtomic::TString),
        (TAtomic::TString, TAtomic::TLiteralString { value }) => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (TAtomic::TString, TAtomic::TNonEmptyString) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TString, TAtomic::TNumericString) => Some(TAtomic::TNumericString),
        (TAtomic::TString, TAtomic::TClassString { as_type }) => {
            Some(TAtomic::TClassString { as_type: as_type.clone() })
        }
        (TAtomic::TLiteralString { value }, TAtomic::TString) => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (TAtomic::TNonEmptyString, TAtomic::TString) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TNumericString, TAtomic::TString) => Some(TAtomic::TNumericString),
        (TAtomic::TClassString { as_type }, TAtomic::TString) => {
            Some(TAtomic::TClassString { as_type: as_type.clone() })
        }

        // bool types
        (TAtomic::TBool, TAtomic::TBool) => Some(TAtomic::TBool),
        (TAtomic::TBool, TAtomic::TTrue) => Some(TAtomic::TTrue),
        (TAtomic::TBool, TAtomic::TFalse) => Some(TAtomic::TFalse),
        (TAtomic::TTrue, TAtomic::TBool) => Some(TAtomic::TTrue),
        (TAtomic::TFalse, TAtomic::TBool) => Some(TAtomic::TFalse),
        (TAtomic::TTrue, TAtomic::TTrue) => Some(TAtomic::TTrue),
        (TAtomic::TFalse, TAtomic::TFalse) => Some(TAtomic::TFalse),

        // scalar types
        (TAtomic::TScalar, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TScalar, TAtomic::TFloat) => Some(TAtomic::TFloat),
        (TAtomic::TScalar, TAtomic::TString) => Some(TAtomic::TString),
        (TAtomic::TScalar, TAtomic::TBool) => Some(TAtomic::TBool),
        (TAtomic::TInt, TAtomic::TScalar) => Some(TAtomic::TInt),
        (TAtomic::TFloat, TAtomic::TScalar) => Some(TAtomic::TFloat),
        (TAtomic::TString, TAtomic::TScalar) => Some(TAtomic::TString),
        (TAtomic::TBool, TAtomic::TScalar) => Some(TAtomic::TBool),

        // array-key types
        (TAtomic::TArrayKey, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TArrayKey, TAtomic::TString) => Some(TAtomic::TString),
        (TAtomic::TArrayKey, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TArrayKey, TAtomic::TLiteralString { value }) => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (TAtomic::TInt, TAtomic::TArrayKey) => Some(TAtomic::TInt),
        (TAtomic::TString, TAtomic::TArrayKey) => Some(TAtomic::TString),

        // object types
        (TAtomic::TObject, TAtomic::TNamedObject { name, type_params }) => {
            Some(TAtomic::TNamedObject {
                name: *name,
                type_params: type_params.clone(),
            })
        }
        (TAtomic::TNamedObject { name, type_params }, TAtomic::TObject) => {
            Some(TAtomic::TNamedObject {
                name: *name,
                type_params: type_params.clone(),
            })
        }
        (
            TAtomic::TNamedObject { name: name1, .. },
            TAtomic::TNamedObject { name: name2, type_params },
        ) => {
            // TODO: Check class hierarchy to determine which is more specific
            if name1 == name2 {
                Some(TAtomic::TNamedObject {
                    name: *name2,
                    type_params: type_params.clone(),
                })
            } else {
                // For now, assume they could be related through inheritance
                Some(TAtomic::TNamedObject {
                    name: *name2,
                    type_params: type_params.clone(),
                })
            }
        }

        // array types
        (TAtomic::TArray { .. }, TAtomic::TArray { key_type, value_type }) => {
            Some(TAtomic::TArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TArray { .. }, TAtomic::TNonEmptyArray { key_type, value_type }) => {
            Some(TAtomic::TNonEmptyArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TNonEmptyArray { key_type, value_type }, TAtomic::TArray { .. }) => {
            Some(TAtomic::TNonEmptyArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }

        // list types
        (TAtomic::TList { .. }, TAtomic::TList { value_type }) => Some(TAtomic::TList {
            value_type: value_type.clone(),
        }),
        (TAtomic::TList { .. }, TAtomic::TNonEmptyList { value_type }) => {
            Some(TAtomic::TNonEmptyList {
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TNonEmptyList { value_type }, TAtomic::TList { .. }) => {
            Some(TAtomic::TNonEmptyList {
                value_type: value_type.clone(),
            })
        }

        // keyed array types
        (TAtomic::TKeyedArray { .. }, assertion_keyed @ TAtomic::TKeyedArray { .. }) => {
            // For keyed arrays, intersection requires careful property merging
            // For now, just return the assertion type
            Some(assertion_keyed.clone())
        }
        (TAtomic::TArray { .. }, assertion_keyed @ TAtomic::TKeyedArray { .. }) => {
            Some(assertion_keyed.clone())
        }
        (existing_keyed @ TAtomic::TKeyedArray { .. }, TAtomic::TArray { .. }) => {
            Some(existing_keyed.clone())
        }

        // iterable types
        (TAtomic::TIterable { .. }, TAtomic::TArray { key_type, value_type }) => {
            Some(TAtomic::TArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TArray { key_type, value_type }, TAtomic::TIterable { .. }) => {
            Some(TAtomic::TArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }

        // template parameter types
        (TAtomic::TTemplateParam { as_type, .. }, other) => {
            if as_type.is_mixed() {
                Some(other.clone())
            } else {
                // Try to intersect with the constraint type
                for constraint_atomic in &as_type.types {
                    if let Some(result) = intersect_atomic_with_atomic(constraint_atomic, other) {
                        return Some(result);
                    }
                }
                Some(existing_atomic.clone())
            }
        }
        (other, TAtomic::TTemplateParam { as_type, .. }) => {
            if as_type.is_mixed() {
                Some(other.clone())
            } else {
                for constraint_atomic in &as_type.types {
                    if let Some(result) = intersect_atomic_with_atomic(other, constraint_atomic) {
                        return Some(result);
                    }
                }
                Some(assertion_atomic.clone())
            }
        }

        // callable/closure types
        (TAtomic::TCallable { .. }, TAtomic::TClosure { params, return_type }) => {
            Some(TAtomic::TClosure {
                params: params.clone(),
                return_type: return_type.clone(),
            })
        }
        (TAtomic::TClosure { params, return_type }, TAtomic::TCallable { .. }) => {
            Some(TAtomic::TClosure {
                params: params.clone(),
                return_type: return_type.clone(),
            })
        }

        // null type
        (TAtomic::TNull, TAtomic::TNull) => Some(TAtomic::TNull),

        // If we can't find a specific intersection, types are incompatible
        _ => None,
    }
}

/// Intersects two union types.
pub fn intersect_union_with_union(
    type1: &TUnion,
    type2: &TUnion,
) -> Option<TUnion> {
    let mut result_types = Vec::new();

    for atomic1 in &type1.types {
        for atomic2 in &type2.types {
            if let Some(intersected) = intersect_atomic_with_atomic(atomic1, atomic2) {
                // Avoid duplicates
                if !result_types.iter().any(|t| t == &intersected) {
                    result_types.push(intersected);
                }
            }
        }
    }

    if result_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(result_types))
    }
}
