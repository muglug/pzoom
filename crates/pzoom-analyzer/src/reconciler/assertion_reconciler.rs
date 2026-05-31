//! Main assertion reconciler.
//!
//! This module handles type reconciliation based on assertions. It dispatches
//! to simple_assertion_reconciler for positive assertions and
//! negated_assertion_reconciler for negative assertions.

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind};
use pzoom_code_info::{Assertion, CodebaseInfo, TAtomic, TUnion};
use pzoom_str::StrId;

use super::{negated_assertion_reconciler, simple_assertion_reconciler};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;
use crate::template::TemplateMap;

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
    let existing_var_type = existing_var_type
        .cloned()
        .unwrap_or_else(|| get_missing_type(assertion, inside_loop));

    // Dispatch based on whether the assertion is negated
    let reconciled_type = if assertion.has_negation() {
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
    };

    reconciled_type
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
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        if let Some(intersected) =
            intersect_atomic_with_atomic_inner(atomic, assertion_atomic, Some(analyzer.codebase))
        {
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
    intersect_atomic_with_atomic_inner(existing_atomic, assertion_atomic, None)
}

fn intersect_atomic_with_atomic_inner(
    existing_atomic: &TAtomic,
    assertion_atomic: &TAtomic,
    codebase: Option<&CodebaseInfo>,
) -> Option<TAtomic> {
    if let TAtomic::TObjectIntersection { types } = existing_atomic {
        let mut intersection_types = Vec::new();
        for atomic in types {
            if let Some(intersected) =
                intersect_atomic_with_atomic_inner(atomic, assertion_atomic, codebase)
            {
                push_intersection_atomic(&mut intersection_types, intersected);
            }
        }

        return match intersection_types.len() {
            0 => None,
            1 => intersection_types.into_iter().next(),
            _ => Some(TAtomic::TObjectIntersection {
                types: intersection_types,
            }),
        };
    }

    if let TAtomic::TObjectIntersection { types } = assertion_atomic {
        let mut intersection_types = Vec::new();
        for atomic in types {
            let intersected =
                intersect_atomic_with_atomic_inner(existing_atomic, atomic, codebase)?;
            push_intersection_atomic(&mut intersection_types, intersected);
        }

        return match intersection_types.len() {
            0 => None,
            1 => intersection_types.into_iter().next(),
            _ => Some(TAtomic::TObjectIntersection {
                types: intersection_types,
            }),
        };
    }

    // Handle mixed types - always intersect
    match existing_atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
            return Some(assertion_atomic.clone());
        }
        _ => {}
    }

    // Psalm-style static handling: `instanceof static` is not contradictory
    // for concrete object values in non-final hierarchies.
    if matches!(
        assertion_atomic,
        TAtomic::TNamedObject {
            name: StrId::STATIC,
            ..
        }
    ) && matches!(
        existing_atomic,
        TAtomic::TNamedObject { .. } | TAtomic::TObject | TAtomic::TObjectIntersection { .. }
    ) {
        return Some(existing_atomic.clone());
    }

    if matches!(
        existing_atomic,
        TAtomic::TNamedObject {
            name: StrId::STATIC,
            ..
        }
    ) && matches!(
        assertion_atomic,
        TAtomic::TNamedObject { .. } | TAtomic::TObject | TAtomic::TObjectIntersection { .. }
    ) {
        return Some(assertion_atomic.clone());
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

    if matches!(existing_atomic, TAtomic::TCallable { .. })
        && atomic_can_be_callable_representation(assertion_atomic)
    {
        return Some(assertion_atomic.clone());
    }

    if matches!(assertion_atomic, TAtomic::TCallable { .. })
        && atomic_can_be_callable_representation(existing_atomic)
    {
        return Some(existing_atomic.clone());
    }

    // Handle specific type intersections
    match (existing_atomic, assertion_atomic) {
        // int types
        (TAtomic::TInt, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TInt, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TInt, TAtomic::TIntRange { min, max }) => Some(int_bounds_to_atomic(*min, *max)),
        (TAtomic::TLiteralInt { value }, TAtomic::TInt) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralInt { value }, TAtomic::TIntRange { min, max }) => {
            if literal_in_bounds(*value, *min, *max) {
                Some(TAtomic::TLiteralInt { value: *value })
            } else {
                None
            }
        }
        (TAtomic::TIntRange { min, max }, TAtomic::TInt) => Some(int_bounds_to_atomic(*min, *max)),
        (TAtomic::TIntRange { min, max }, TAtomic::TLiteralInt { value }) => {
            if literal_in_bounds(*value, *min, *max) {
                Some(TAtomic::TLiteralInt { value: *value })
            } else {
                None
            }
        }
        (
            TAtomic::TIntRange {
                min: existing_min,
                max: existing_max,
            },
            TAtomic::TIntRange {
                min: asserted_min,
                max: asserted_max,
            },
        ) => intersect_int_bounds(*existing_min, *existing_max, *asserted_min, *asserted_max)
            .map(|(min, max)| int_bounds_to_atomic(min, max)),

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
        (TAtomic::TNumeric, TAtomic::TString) => Some(TAtomic::TNumericString),
        (TAtomic::TNumeric, TAtomic::TNonEmptyString) => Some(TAtomic::TNonEmptyNumericString),
        (TAtomic::TNumeric, TAtomic::TNumericString) => Some(TAtomic::TNumericString),
        (TAtomic::TNumeric, TAtomic::TNonEmptyNumericString) => {
            Some(TAtomic::TNonEmptyNumericString)
        }
        (TAtomic::TNumeric, TAtomic::TLiteralString { value }) => {
            if value.parse::<f64>().is_ok() {
                Some(TAtomic::TLiteralString {
                    value: value.clone(),
                })
            } else {
                None
            }
        }
        (TAtomic::TInt, TAtomic::TNumeric) => Some(TAtomic::TInt),
        (TAtomic::TFloat, TAtomic::TNumeric) => Some(TAtomic::TFloat),
        (TAtomic::TString, TAtomic::TNumeric) => Some(TAtomic::TNumericString),
        (TAtomic::TNonEmptyString, TAtomic::TNumeric) => Some(TAtomic::TNonEmptyNumericString),
        (TAtomic::TNumericString, TAtomic::TNumeric) => Some(TAtomic::TNumericString),
        (TAtomic::TNonEmptyNumericString, TAtomic::TNumeric) => {
            Some(TAtomic::TNonEmptyNumericString)
        }
        (TAtomic::TLiteralString { value }, TAtomic::TNumeric) => {
            if value.parse::<f64>().is_ok() {
                Some(TAtomic::TLiteralString {
                    value: value.clone(),
                })
            } else {
                None
            }
        }
        (TAtomic::TArrayKey, TAtomic::TNumeric) => Some(TAtomic::TNumeric),
        (TAtomic::TScalar, TAtomic::TNumeric) => Some(TAtomic::TNumeric),
        // numeric contains every int/float-valued atomic; intersecting keeps the
        // more specific existing type (mirrors reconcileNumeric narrowing).
        (TAtomic::TLiteralInt { value }, TAtomic::TNumeric) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralFloat { value }, TAtomic::TNumeric) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }
        (TAtomic::TIntRange { min, max }, TAtomic::TNumeric) => {
            Some(TAtomic::TIntRange { min: *min, max: *max })
        }

        // string types
        (TAtomic::TString, TAtomic::TString) => Some(TAtomic::TString),
        (TAtomic::TString, TAtomic::TLiteralString { value }) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TString, TAtomic::TNonEmptyString) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TString, TAtomic::TNumericString) => Some(TAtomic::TNumericString),
        // A non-empty literal string (e.g. "0") is a member of non-empty-string, so
        // `$s === "0"` on a non-empty-string keeps the literal rather than clearing.
        (TAtomic::TNonEmptyString, TAtomic::TLiteralString { value })
        | (TAtomic::TLiteralString { value }, TAtomic::TNonEmptyString)
            if !value.is_empty() =>
        {
            Some(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
        // A numeric-valued literal string is a member of (non-empty-)numeric-string,
        // so `$s === "1"` after is_numeric($s) keeps the literal rather than clearing.
        (
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString,
            TAtomic::TLiteralString { value },
        ) if value.parse::<f64>().is_ok() => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (
            TAtomic::TLiteralString { value },
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString,
        ) if value.parse::<f64>().is_ok() => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (TAtomic::TString, TAtomic::TClassString { as_type }) => Some(TAtomic::TClassString {
            as_type: as_type.clone(),
        }),
        (TAtomic::TString, TAtomic::TLiteralClassString { name }) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }
        (TAtomic::TLiteralString { value }, TAtomic::TString) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TLiteralClassString { name }, TAtomic::TString) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }
        (TAtomic::TNonEmptyString, TAtomic::TString) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TNumericString, TAtomic::TString) => Some(TAtomic::TNumericString),
        (TAtomic::TClassString { as_type }, TAtomic::TString) => Some(TAtomic::TClassString {
            as_type: as_type.clone(),
        }),
        (TAtomic::TClassString { .. }, TAtomic::TLiteralClassString { .. })
        | (TAtomic::TLiteralClassString { .. }, TAtomic::TClassString { .. })
        | (TAtomic::TClassString { .. }, TAtomic::TClassString { .. })
        | (TAtomic::TClassString { .. }, TAtomic::TTemplateParamClass { .. })
        | (TAtomic::TTemplateParamClass { .. }, TAtomic::TClassString { .. })
        | (TAtomic::TTemplateParamClass { .. }, TAtomic::TTemplateParamClass { .. }) => {
            intersect_class_string_atomics(existing_atomic, assertion_atomic, codebase)
        }
        (
            TAtomic::TLiteralClassString {
                name: existing_name,
            },
            TAtomic::TLiteralClassString {
                name: assertion_name,
            },
        ) if existing_name == assertion_name => Some(TAtomic::TLiteralClassString {
            name: existing_name.clone(),
        }),

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
        (TAtomic::TScalar, TAtomic::TTrue) => Some(TAtomic::TTrue),
        (TAtomic::TScalar, TAtomic::TFalse) => Some(TAtomic::TFalse),
        // scalar contains every literal/refined scalar, so narrowing to one keeps it.
        (TAtomic::TScalar, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TScalar, TAtomic::TLiteralFloat { value }) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }
        (TAtomic::TScalar, TAtomic::TLiteralString { value }) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TInt, TAtomic::TScalar) => Some(TAtomic::TInt),
        (TAtomic::TFloat, TAtomic::TScalar) => Some(TAtomic::TFloat),
        (TAtomic::TString, TAtomic::TScalar) => Some(TAtomic::TString),
        (TAtomic::TBool, TAtomic::TScalar) => Some(TAtomic::TBool),
        (TAtomic::TTrue, TAtomic::TScalar) => Some(TAtomic::TTrue),
        (TAtomic::TFalse, TAtomic::TScalar) => Some(TAtomic::TFalse),
        // scalar contains every literal/refined scalar; intersecting keeps the more
        // specific existing type. These reverse-order pairs were missing, so e.g.
        // is_scalar() on a literal-string|null wrongly reconciled to `never`.
        (TAtomic::TLiteralInt { value }, TAtomic::TScalar) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralFloat { value }, TAtomic::TScalar) => {
            Some(TAtomic::TLiteralFloat { value: *value })
        }
        (TAtomic::TLiteralString { value }, TAtomic::TScalar) => {
            Some(TAtomic::TLiteralString { value: value.clone() })
        }
        (TAtomic::TIntRange { min, max }, TAtomic::TScalar) => {
            Some(TAtomic::TIntRange { min: *min, max: *max })
        }
        (TAtomic::TNonEmptyString, TAtomic::TScalar) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TNumericString, TAtomic::TScalar) => Some(TAtomic::TNumericString),
        (TAtomic::TNonEmptyNumericString, TAtomic::TScalar) => {
            Some(TAtomic::TNonEmptyNumericString)
        }
        (TAtomic::TArrayKey, TAtomic::TScalar) => Some(TAtomic::TArrayKey),
        (TAtomic::TClassString { as_type }, TAtomic::TScalar) => {
            Some(TAtomic::TClassString {
                as_type: as_type.clone(),
            })
        }
        (TAtomic::TLiteralClassString { name }, TAtomic::TScalar) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }

        // array-key types
        (TAtomic::TArrayKey, TAtomic::TInt) => Some(TAtomic::TInt),
        (TAtomic::TArrayKey, TAtomic::TString) => Some(TAtomic::TString),
        (TAtomic::TArrayKey, TAtomic::TLiteralInt { value }) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TArrayKey, TAtomic::TLiteralString { value }) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TInt, TAtomic::TArrayKey) => Some(TAtomic::TInt),
        (TAtomic::TString, TAtomic::TArrayKey) => Some(TAtomic::TString),
        (TAtomic::TLiteralInt { value }, TAtomic::TArrayKey) => {
            Some(TAtomic::TLiteralInt { value: *value })
        }
        (TAtomic::TLiteralString { value }, TAtomic::TArrayKey) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TLiteralClassString { name }, TAtomic::TArrayKey) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }
        (TAtomic::TClassString { as_type }, TAtomic::TArrayKey) => Some(TAtomic::TClassString {
            as_type: as_type.clone(),
        }),
        (TAtomic::TNonEmptyString, TAtomic::TArrayKey) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TNumericString, TAtomic::TArrayKey) => Some(TAtomic::TNumericString),
        (TAtomic::TTruthyString, TAtomic::TArrayKey) => Some(TAtomic::TTruthyString),
        (TAtomic::TLowercaseString, TAtomic::TArrayKey) => Some(TAtomic::TLowercaseString),
        (TAtomic::TNonEmptyLowercaseString, TAtomic::TArrayKey) => {
            Some(TAtomic::TNonEmptyLowercaseString)
        }

        // Cross-compatible string subtypes: neither contains the other, so their
        // intersection is the combined narrower subtype (matches Psalm). Without
        // these arms the pair would fall through to `None` (an incorrect `never`).
        (TAtomic::TLowercaseString, TAtomic::TNonEmptyString)
        | (TAtomic::TNonEmptyString, TAtomic::TLowercaseString)
        | (TAtomic::TLowercaseString, TAtomic::TNonEmptyLowercaseString)
        | (TAtomic::TNonEmptyLowercaseString, TAtomic::TLowercaseString) => {
            Some(TAtomic::TNonEmptyLowercaseString)
        }
        (TAtomic::TNonEmptyString, TAtomic::TNumericString)
        | (TAtomic::TNumericString, TAtomic::TNonEmptyString)
        | (TAtomic::TTruthyString, TAtomic::TNumericString)
        | (TAtomic::TNumericString, TAtomic::TTruthyString) => {
            Some(TAtomic::TNonEmptyNumericString)
        }
        (TAtomic::TNonEmptyString, TAtomic::TTruthyString)
        | (TAtomic::TTruthyString, TAtomic::TNonEmptyString) => Some(TAtomic::TTruthyString),

        // enum types: same enum, or an enum case belonging to the enum, intersect to
        // the more specific operand (matches Psalm/Hakana). Without these the pair
        // falls through to `None` (an incorrect `never`).
        (TAtomic::TEnum { name: a }, TAtomic::TEnum { name: b }) if a == b => {
            Some(TAtomic::TEnum { name: *a })
        }
        (
            TAtomic::TEnumCase {
                enum_name,
                case_name,
            },
            TAtomic::TEnum { name },
        )
        | (
            TAtomic::TEnum { name },
            TAtomic::TEnumCase {
                enum_name,
                case_name,
            },
        ) if enum_name == name => Some(TAtomic::TEnumCase {
            enum_name: *enum_name,
            case_name: *case_name,
        }),
        (
            TAtomic::TEnumCase {
                enum_name: a_enum,
                case_name: a_case,
            },
            TAtomic::TEnumCase {
                enum_name: b_enum,
                case_name: b_case,
            },
        ) if a_enum == b_enum && a_case == b_case => Some(TAtomic::TEnumCase {
            enum_name: *a_enum,
            case_name: *a_case,
        }),

        // object types
        (TAtomic::TObject, TAtomic::TNamedObject { name, type_params , .. }) => {
            Some(TAtomic::TNamedObject {
                name: *name,
                type_params: type_params.clone(),
            is_static: false, remapped_params: false })
        }
        (TAtomic::TNamedObject { name, type_params , .. }, TAtomic::TObject) => {
            Some(TAtomic::TNamedObject {
                name: *name,
                type_params: type_params.clone(),
            is_static: false, remapped_params: false })
        }
        (
            TAtomic::TNamedObject { name: name1, .. },
            TAtomic::TNamedObject {
                name: name2,
                type_params,
            .. },
        ) => {
            if name1 == name2 {
                Some(TAtomic::TNamedObject {
                    name: *name2,
                    type_params: type_params.clone(),
                is_static: false, remapped_params: false })
            } else if let Some(codebase) = codebase {
                if object_type_comparator::is_class_subtype_of(*name1, *name2, codebase) {
                    Some(existing_atomic.clone())
                } else if object_type_comparator::is_class_subtype_of(*name2, *name1, codebase) {
                    Some(
                        specialize_assertion_named_object_from_existing(
                            existing_atomic,
                            assertion_atomic,
                            codebase,
                        )
                        .unwrap_or_else(|| assertion_atomic.clone()),
                    )
                } else if can_named_objects_intersect(*name1, *name2, codebase) {
                    // Preserve both constraints (e.g. A&I) when either side is interface-like.
                    Some(make_intersection(
                        existing_atomic.clone(),
                        assertion_atomic.clone(),
                    ))
                } else {
                    None
                }
            } else {
                // Without class hierarchy information, keep both constraints.
                Some(make_intersection(
                    existing_atomic.clone(),
                    assertion_atomic.clone(),
                ))
            }
        }

        // array types
        (
            TAtomic::TArray { .. },
            TAtomic::TArray {
                key_type,
                value_type,
            },
        ) => Some(TAtomic::TArray {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
        }),
        (
            TAtomic::TArray { .. },
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            },
        ) => Some(TAtomic::TNonEmptyArray {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
        }),
        (
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            },
            TAtomic::TArray { .. },
        ) => Some(TAtomic::TNonEmptyArray {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
        }),
        (TAtomic::TList { value_type }, TAtomic::TArray { .. }) => Some(TAtomic::TList {
            value_type: value_type.clone(),
        }),
        (TAtomic::TNonEmptyList { value_type }, TAtomic::TArray { .. }) => {
            Some(TAtomic::TNonEmptyList {
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TList { value_type }, TAtomic::TNonEmptyArray { .. }) => {
            Some(TAtomic::TNonEmptyList {
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TNonEmptyList { value_type }, TAtomic::TNonEmptyArray { .. }) => {
            Some(TAtomic::TNonEmptyList {
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
        (
            TAtomic::TIterable {
                key_type: existing_key_type,
                value_type: existing_value_type,
            },
            TAtomic::TArray {
                key_type,
                value_type,
            },
        ) => Some(TAtomic::TArray {
            key_type: Box::new(
                intersect_union_with_union(existing_key_type, key_type)
                    .unwrap_or_else(|| pick_more_specific_union(existing_key_type, key_type)),
            ),
            value_type: Box::new(
                intersect_union_with_union(existing_value_type, value_type)
                    .unwrap_or_else(|| pick_more_specific_union(existing_value_type, value_type)),
            ),
        }),
        (
            TAtomic::TArray {
                key_type,
                value_type,
            },
            TAtomic::TIterable { .. },
        ) => Some(TAtomic::TArray {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
        }),
        (
            TAtomic::TIterable {
                key_type: _,
                value_type: existing_value_type,
            },
            TAtomic::TList { value_type },
        ) => Some(TAtomic::TList {
            value_type: Box::new(
                intersect_union_with_union(existing_value_type, value_type)
                    .unwrap_or_else(|| pick_more_specific_union(existing_value_type, value_type)),
            ),
        }),
        (
            TAtomic::TIterable {
                key_type: _,
                value_type: existing_value_type,
            },
            TAtomic::TNonEmptyList { value_type },
        ) => Some(TAtomic::TNonEmptyList {
            value_type: Box::new(
                intersect_union_with_union(existing_value_type, value_type)
                    .unwrap_or_else(|| pick_more_specific_union(existing_value_type, value_type)),
            ),
        }),
        (TAtomic::TList { value_type }, TAtomic::TIterable { .. }) => Some(TAtomic::TList {
            value_type: value_type.clone(),
        }),
        (TAtomic::TNonEmptyList { value_type }, TAtomic::TIterable { .. }) => {
            Some(TAtomic::TNonEmptyList {
                value_type: value_type.clone(),
            })
        }
        (TAtomic::TIterable { .. }, keyed @ TAtomic::TKeyedArray { .. }) => Some(keyed.clone()),
        (keyed @ TAtomic::TKeyedArray { .. }, TAtomic::TIterable { .. }) => Some(keyed.clone()),
        (
            TAtomic::TIterable {
                key_type: existing_key_type,
                value_type: existing_value_type,
            },
            TAtomic::TIterable {
                key_type: asserted_key_type,
                value_type: asserted_value_type,
            },
        ) => Some(TAtomic::TIterable {
            key_type: Box::new(
                intersect_union_with_union(existing_key_type, asserted_key_type).unwrap_or_else(
                    || pick_more_specific_union(existing_key_type, asserted_key_type),
                ),
            ),
            value_type: Box::new(
                intersect_union_with_union(existing_value_type, asserted_value_type)
                    .unwrap_or_else(|| {
                        pick_more_specific_union(existing_value_type, asserted_value_type)
                    }),
            ),
        }),
        (
            TAtomic::TIterable {
                key_type,
                value_type,
            },
            TAtomic::TNamedObject { name, .. },
        ) if *name == StrId::TRAVERSABLE => Some(TAtomic::TNamedObject {
            name: *name,
            type_params: Some(vec![(**key_type).clone(), (**value_type).clone()]),
        is_static: false, remapped_params: false }),
        (named @ TAtomic::TNamedObject { name, .. }, TAtomic::TIterable { .. })
            if codebase.is_some_and(|cb| {
                object_type_comparator::is_class_subtype_of(*name, StrId::TRAVERSABLE, cb)
            }) =>
        {
            Some(named.clone())
        }

        // template parameter types
        (TAtomic::TTemplateParam { as_type, .. }, other) => {
            if as_type.is_mixed() {
                Some(other.clone())
            } else if is_objectish_atomic(other) {
                Some(make_intersection(existing_atomic.clone(), other.clone()))
            } else {
                // Try to intersect with the constraint type
                for constraint_atomic in &as_type.types {
                    if let Some(result) =
                        intersect_atomic_with_atomic_inner(constraint_atomic, other, codebase)
                    {
                        return Some(result);
                    }
                }
                Some(existing_atomic.clone())
            }
        }
        (other, TAtomic::TTemplateParam { as_type, .. }) => {
            // Asserting `is T` must narrow to the template parameter, keeping the
            // binding. For an object-ish existing type, intersect (`object&T`) so
            // both the known shape and the template are preserved; otherwise fall
            // back to the template itself rather than the (wider) existing type.
            if is_objectish_atomic(other) {
                Some(make_intersection(other.clone(), assertion_atomic.clone()))
            } else if as_type.is_mixed() {
                Some(assertion_atomic.clone())
            } else {
                for constraint_atomic in &as_type.types {
                    if let Some(result) =
                        intersect_atomic_with_atomic_inner(other, constraint_atomic, codebase)
                    {
                        return Some(result);
                    }
                }
                Some(assertion_atomic.clone())
            }
        }

        // callable/closure types
        (
            TAtomic::TCallable { .. },
            TAtomic::TClosure {
                params,
                return_type,
                is_pure,
            },
        ) => Some(TAtomic::TClosure {
            params: params.clone(),
            return_type: return_type.clone(),
            is_pure: *is_pure,
        }),
        (
            TAtomic::TClosure {
                params,
                return_type,
                is_pure,
            },
            TAtomic::TCallable { .. },
        ) => Some(TAtomic::TClosure {
            params: params.clone(),
            return_type: return_type.clone(),
            is_pure: *is_pure,
        }),

        // null type
        (TAtomic::TNull, TAtomic::TNull) => Some(TAtomic::TNull),

        // If we can't find a specific intersection, types are incompatible. (Like
        // hakana-core's intersect_atomic_with_atomic, which enumerates the
        // intersectable pairs and otherwise yields no intersection, rather than
        // falling back to a lax containment check — pzoom treats int as contained by
        // float for coercion, which must not count as a type intersection.)
        _ => None,
    }
}

fn atomic_can_be_callable_representation(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TNamedObject { .. }
            | TAtomic::TObject
    )
}

fn literal_in_bounds(value: i64, min: Option<i64>, max: Option<i64>) -> bool {
    let min_ok = min.is_none_or(|bound| value >= bound);
    let max_ok = max.is_none_or(|bound| value <= bound);
    min_ok && max_ok
}

fn intersect_int_bounds(
    min1: Option<i64>,
    max1: Option<i64>,
    min2: Option<i64>,
    max2: Option<i64>,
) -> Option<(Option<i64>, Option<i64>)> {
    let min = match (min1, min2) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };

    let max = match (max1, max2) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };

    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return None;
    }

    Some((min, max))
}

fn int_bounds_to_atomic(min: Option<i64>, max: Option<i64>) -> TAtomic {
    match (min, max) {
        (None, None) => TAtomic::TInt,
        (Some(min), Some(max)) if min == max => TAtomic::TLiteralInt { value: min },
        _ => TAtomic::TIntRange { min, max },
    }
}

fn specialize_assertion_named_object_from_existing(
    existing_atomic: &TAtomic,
    assertion_atomic: &TAtomic,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let (
        TAtomic::TNamedObject {
            name: existing_name,
            type_params: existing_type_params,
        .. },
        TAtomic::TNamedObject {
            name: assertion_name,
            type_params: assertion_type_params,
        .. },
    ) = (existing_atomic, assertion_atomic)
    else {
        return None;
    };

    if assertion_type_params.is_some() {
        return Some(assertion_atomic.clone());
    }

    let Some(existing_type_params) = existing_type_params else {
        return Some(assertion_atomic.clone());
    };
    let Some(existing_class_info) = codebase.get_class(*existing_name) else {
        return Some(assertion_atomic.clone());
    };
    let Some(assertion_class_info) = codebase.get_class(*assertion_name) else {
        return Some(assertion_atomic.clone());
    };

    let mut ancestor_template_replacements = TemplateMap::new();
    for (idx, template_type) in existing_class_info.template_types.iter().enumerate() {
        if let Some(type_param) = existing_type_params.get(idx) {
            ancestor_template_replacements.insert(
                template_type.name,
                template_type.defining_entity,
                type_param.clone(),
            );
        }
    }

    if ancestor_template_replacements.is_empty() {
        return Some(assertion_atomic.clone());
    }

    let inferred_template_replacements = infer_class_template_replacements_from_ancestors(
        assertion_class_info,
        &ancestor_template_replacements,
    );

    if assertion_class_info.template_types.is_empty() {
        return Some(assertion_atomic.clone());
    }

    let inferred_type_params = assertion_class_info
        .template_types
        .iter()
        .map(|template_type| {
            inferred_template_replacements
                .get(template_type.name, template_type.defining_entity)
                .cloned()
                .or_else(|| {
                    ancestor_template_replacements
                        .get(template_type.name, template_type.defining_entity)
                        .cloned()
                })
                .unwrap_or_else(|| template_type.as_type.clone())
        })
        .collect::<Vec<_>>();

    Some(TAtomic::TNamedObject {
        name: *assertion_name,
        type_params: Some(inferred_type_params),
    is_static: false, remapped_params: false })
}

fn infer_class_template_replacements_from_ancestors(
    class_info: &ClassLikeInfo,
    template_replacements: &TemplateMap,
) -> TemplateMap {
    let mut propagated_replacements = template_replacements.clone();

    loop {
        let mut changed = false;

        for (ancestor_class, template_map) in &class_info.template_extended_params {
            for (ancestor_template, mapped_type) in template_map {
                let Some(ancestor_replacement) = propagated_replacements
                    .get(*ancestor_template, *ancestor_class)
                    .cloned()
                else {
                    continue;
                };

                for mapped_atomic in &mapped_type.types {
                    let mapped_template = match mapped_atomic {
                        TAtomic::TTemplateParam {
                            name,
                            defining_entity,
                            ..
                        } => Some((*name, *defining_entity)),
                        TAtomic::TTemplateParamClass {
                            name,
                            defining_entity,
                            ..
                        } => Some((*name, *defining_entity)),
                        _ => None,
                    };

                    let Some((mapped_template, mapped_entity)) = mapped_template else {
                        continue;
                    };

                    let should_propagate = propagated_replacements
                        .get(mapped_template, mapped_entity)
                        .is_none_or(is_template_placeholder_union);

                    if !should_propagate {
                        continue;
                    }

                    if propagated_replacements
                        .get(mapped_template, mapped_entity)
                        .is_none_or(|existing| existing != &ancestor_replacement)
                    {
                        propagated_replacements.insert(
                            mapped_template,
                            mapped_entity,
                            ancestor_replacement.clone(),
                        );
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    let mut inferred_replacements = TemplateMap::new();
    for template_type in &class_info.template_types {
        if let Some(replacement) =
            propagated_replacements.get(template_type.name, template_type.defining_entity)
        {
            inferred_replacements.insert(
                template_type.name,
                template_type.defining_entity,
                replacement.clone(),
            );
        }
    }

    inferred_replacements
}

fn intersect_class_string_atomics(
    existing_atomic: &TAtomic,
    assertion_atomic: &TAtomic,
    codebase: Option<&CodebaseInfo>,
) -> Option<TAtomic> {
    let existing_bound = class_string_atomic_to_bound(existing_atomic, codebase)?;
    let assertion_bound = class_string_atomic_to_bound(assertion_atomic, codebase)?;

    let bound = match (existing_bound, assertion_bound) {
        (None, None) => None,
        (Some(bound), None) | (None, Some(bound)) => Some(bound),
        (Some(existing_bound), Some(asserted_bound)) => {
            let intersected =
                intersect_atomic_with_atomic_inner(&existing_bound, &asserted_bound, codebase)?;
            Some(intersected)
        }
    };

    match (existing_atomic, assertion_atomic) {
        (
            TAtomic::TLiteralClassString {
                name: existing_name,
            },
            _,
        ) => Some(TAtomic::TLiteralClassString {
            name: existing_name.clone(),
        }),
        (
            _,
            TAtomic::TLiteralClassString {
                name: assertion_name,
            },
        ) => Some(TAtomic::TLiteralClassString {
            name: assertion_name.clone(),
        }),
        _ => Some(TAtomic::TClassString {
            as_type: bound.map(Box::new),
        }),
    }
}

fn class_string_atomic_to_bound(
    atomic: &TAtomic,
    codebase: Option<&CodebaseInfo>,
) -> Option<Option<TAtomic>> {
    match atomic {
        TAtomic::TClassString { as_type } => Some(as_type.as_ref().map(|as_type| (**as_type).clone())),
        TAtomic::TTemplateParamClass { as_type, .. } => Some(Some((**as_type).clone())),
        TAtomic::TLiteralClassString { name } => {
            let class_id = codebase?.resolve_classlike_name(name)?;
            Some(Some(TAtomic::TNamedObject {
                name: class_id,
                type_params: None,
            is_static: false, remapped_params: false }))
        }
        _ => None,
    }
}

fn is_template_placeholder_union(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
            )
        })
}

fn is_objectish_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TObjectIntersection { .. }
    )
}

fn push_intersection_atomic(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    match atomic {
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                if !target.contains(&nested) {
                    target.push(nested);
                }
            }
        }
        _ => {
            if !target.contains(&atomic) {
                target.push(atomic);
            }
        }
    }
}

fn make_intersection(left: TAtomic, right: TAtomic) -> TAtomic {
    let mut types = Vec::new();
    push_intersection_atomic(&mut types, left);
    push_intersection_atomic(&mut types, right);

    if types.len() == 1 {
        types.into_iter().next().unwrap()
    } else {
        TAtomic::TObjectIntersection { types }
    }
}

fn pick_more_specific_union(existing: &TUnion, asserted: &TUnion) -> TUnion {
    if existing.is_mixed() && !asserted.is_mixed() {
        asserted.clone()
    } else if asserted.is_mixed() && !existing.is_mixed() {
        existing.clone()
    } else {
        existing.clone()
    }
}

fn can_named_objects_intersect(
    name_1: StrId,
    name_2: StrId,
    codebase: &pzoom_code_info::CodebaseInfo,
) -> bool {
    let left_is_interface = codebase
        .get_class(name_1)
        .is_some_and(|class_info| class_info.kind == ClassLikeKind::Interface);
    let right_is_interface = codebase
        .get_class(name_2)
        .is_some_and(|class_info| class_info.kind == ClassLikeKind::Interface);

    left_is_interface || right_is_interface
}

/// Intersects two union types.
pub fn intersect_union_with_union(type1: &TUnion, type2: &TUnion) -> Option<TUnion> {
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
        let mut union = TUnion::from_types(result_types);
        union.from_docblock = type1.from_docblock || type2.from_docblock;
        Some(union)
    }
}
