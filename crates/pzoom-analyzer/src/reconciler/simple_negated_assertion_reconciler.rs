//! Simple negated assertion reconciler.
//!
//! Handles simple type subtractions like !null, !false, !true, !int, !string, etc.
//! This module provides the building blocks for more complex type subtractions.

use pzoom_code_info::{Assertion, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reconciles a simple negated assertion (type subtraction).
///
/// Returns Some(narrowed_type) if reconciliation was handled, None if it should
/// fall through to more complex reconciliation logic.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&str>,
    negated: bool,
    possibly_undefined: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let assertion_type = assertion.get_type();

    if let Some(assertion_type) = assertion_type {
        match assertion_type {
            TAtomic::TObject => {
                return Some(subtract_object(existing_var_type));
            }
            TAtomic::TScalar => {
                return Some(subtract_scalar(existing_var_type));
            }
            TAtomic::TResource => {
                return Some(subtract_resource(existing_var_type));
            }
            TAtomic::TCallable { .. } => {
                return Some(subtract_callable(existing_var_type, analyzer));
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
                // Psalm's SimpleNegatedAssertionReconciler reports a `!null`
                // reconcile that removes nothing (the variable was never
                // null); a negated formula flips it into the contradiction
                // wording ("Docblock-defined type int for $x is never null").
                // `possibly_undefined` exempts isset-derived `!null`
                // assertions: the reconcile also asserts definedness, so it
                // is not redundant (Psalm routes isset through a separate
                // reconciler that never reports here).
                if let Some(key) = key
                    && !possibly_undefined
                    && !existing_var_type.is_mixed()
                    && !existing_var_type.is_nullable()
                    && !assertion.has_equality()
                {
                    super::trigger_issue_for_impossible(
                        analysis_data,
                        analyzer,
                        existing_var_type,
                        key,
                        assertion,
                        true,
                        negated,
                    );
                }
                return Some(subtract_null(existing_var_type));
            }
            TAtomic::TFalse => {
                return Some(subtract_false(existing_var_type));
            }
            TAtomic::TTrue => {
                return Some(subtract_true(existing_var_type));
            }
            // Only the *general* array/list assertion (`!is_array($x)`)
            // removes every array; a negated specific array
            // (`@psalm-assert-if-false array<string, string>` negated on the
            // true path) subtracts just the matching atomics downstream. A
            // shape (known entries) is never general, so it falls through.
            //
            // A general *list* assertion (`is_list`). A missing fallback (the
            // empty array `[]`) counts as general (its `never` value is
            // vacuously general).
            TAtomic::TArray {
                known_values,
                params,
                is_list: true,
                ..
            } if known_values.is_empty()
                && params
                    .as_deref()
                    .is_none_or(|(_, value)| negated_array_param_is_general(value)) =>
            {
                return Some(subtract_list(existing_var_type));
            }
            // A general *array* assertion (not a list).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } if known_values.is_empty()
                && params.as_deref().is_none_or(|(key, value)| {
                    negated_array_param_is_general(value)
                        && (matches!(key.get_single(), Some(TAtomic::TArrayKey) | None)
                            || negated_array_param_is_general(key))
                }) =>
            {
                return Some(subtract_array(existing_var_type));
            }
            _ => {}
        }
    }

    match assertion {
        Assertion::Falsy | Assertion::Empty => Some(reconcile_falsy(existing_var_type)),
        Assertion::IsNotIsset => Some(reconcile_not_isset(
            existing_var_type,
            possibly_undefined,
            key,
            analyzer,
            analysis_data,
        )),
        Assertion::EmptyCountable => Some(reconcile_empty_countable(existing_var_type)),
        Assertion::DoesNotHaveAtLeastCount(count) => Some(reconcile_does_not_have_at_least_count(
            existing_var_type,
            *count,
        )),
        Assertion::DoesNotHaveArrayKey(key) => Some(reconcile_no_array_key(existing_var_type, key)),
        _ => None,
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

/// Subtracts object types from a union.
fn subtract_object(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TObject | TAtomic::TNamedObject { .. } | TAtomic::TClosure { .. } => {
                // Remove object types
            }
            TAtomic::TCallable { .. } => {
                // A callable is not necessarily an object: it can be a
                // callable-string or a callable-array. Narrow to those rather
                // than removing the type entirely. Matches Psalm reconcileObject.
                acceptable_types.push(TAtomic::TCallableString);
                acceptable_types.push(TAtomic::array(TUnion::array_key(), TUnion::mixed()));
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_object(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Returns true if the atomic type belongs to Psalm's `Scalar` hierarchy
/// (int/float/string/bool families plus numeric, array-key and scalar itself).
fn is_scalar_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TNumeric
            | TAtomic::TArrayKey
            | TAtomic::TScalar
    )
}

/// Subtracts all scalar types from a union (`!scalar`). Mirrors Psalm
/// `reconcileScalar`: keep only non-scalar atomics. Gated on non-mixed, like
/// Psalm's dispatch (`!$existing_var_type->hasMixed()`).
fn subtract_scalar(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            _ if is_scalar_atomic(atomic) => {
                // Remove scalar types
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_scalar(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Subtracts resource types from a union (`!resource`). Mirrors Psalm
/// `reconcileResource`: remove the `resource` atomic, keep everything else.
fn subtract_resource(existing_var_type: &TUnion) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TResource => {
                // Remove resource
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_resource(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Subtracts callable types from a union (`!callable`). Mirrors Psalm
/// `reconcileCallable`: remove atomics that are themselves callable types
/// (`callable`, closures) and literal strings naming a known function or
/// `Class::method`, keeping the rest.
fn subtract_callable(existing_var_type: &TUnion, analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => {
                // Remove callable types
            }
            // `Closure` as a named object, and classes declaring __invoke,
            // are callable too (Psalm's reconcileCallable).
            TAtomic::TNamedObject { name, .. }
                if *name == StrId::CLOSURE
                    || analyzer
                        .codebase
                        .get_class(*name)
                        .is_some_and(|class_info| {
                            class_info.methods.contains_key(&StrId::INVOKE)
                        }) => {}
            TAtomic::TLiteralString { value } if literal_string_is_callable(value, analyzer) => {
                // Psalm removes literal strings found in the callmap (and
                // `Class::method` strings resolvable to a real method).
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_callable(as_type, analyzer);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Whether a literal string names a known function (`"strlen"`) or a real
/// `Class::method` (the lookups behind Psalm's `reconcileCallable`).
fn literal_string_is_callable(value: &str, analyzer: &StatementsAnalyzer<'_>) -> bool {
    if let Some((class_name, method_name)) = value.split_once("::") {
        let class_id = analyzer
            .interner
            .intern(class_name.trim_start_matches('\\'));
        let method_id = analyzer.interner.intern(method_name);
        return analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| class_info.methods.contains_key(&method_id));
    }

    let function_id = analyzer.interner.intern(value.trim_start_matches('\\'));
    analyzer.codebase.get_function(function_id).is_some()
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
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNumeric => {
                // Remove numeric types (Psalm's isNumericType includes
                // numeric-string and numeric literal strings).
            }
            TAtomic::TLiteralString { value }
                if !value.trim().is_empty() && value.trim().parse::<f64>().is_ok() => {}
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
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
                // numeric - float = int | numeric-string; Psalm coarsens the
                // numeric-string residue to `string`.
                acceptable_types.push(TAtomic::TInt);
                acceptable_types.push(TAtomic::TString);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_float(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
            TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. } => {
                // Remove int types
            }
            TAtomic::TScalar => {
                // Narrow to non-int scalars
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TBool);
            }
            TAtomic::TNumeric => {
                // numeric - int = float | numeric-string; Psalm coarsens the
                // numeric-string residue to `string`.
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TString);
            }
            TAtomic::TArrayKey => {
                // array-key - int = string
                acceptable_types.push(TAtomic::TString);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_int(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
            TAtomic::TCallable { .. } => {
                // callable - string => array|object
                acceptable_types.push(TAtomic::array(TUnion::array_key(), TUnion::mixed()));
                acceptable_types.push(TAtomic::TObject);
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
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
                push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        // Removing null from a docblock-provided type keeps it docblock-derived
        // (Psalm preserves `from_docblock` through reconcileNotNull), so a later
        // redundancy on the narrowed value is the `*GivenDocblockType` variant.
        let mut result = TUnion::from_types(acceptable_types);
        result.from_docblock = existing_var_type.from_docblock;
        result.from_calculation = existing_var_type.from_calculation;
        result.ignore_nullable_issues = existing_var_type.ignore_nullable_issues;
        result.ignore_falsable_issues = existing_var_type.ignore_falsable_issues;
        result.sync_docblock_bits_from_union_flag();
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
                push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
                push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
            TAtomic::TArray { .. } => {
                // Remove array types
            }
            TAtomic::TCallable {
                params,
                return_type,
                is_pure,
            } => {
                // callable - array => callable-string|callable-object: both
                // remaining halves stay callable (Psalm's TCallableString /
                // TCallableObject).
                acceptable_types.push(TAtomic::TCallableString);
                // A *typed* callable (`callable():R`) keeps its signature on the
                // object half as a Closure, so a later invocation still resolves
                // the return type (Psalm preserves the callable signature).
                if params.is_some() || return_type.is_some() {
                    acceptable_types.push(TAtomic::TClosure {
                        params: params.clone(),
                        return_type: return_type.clone(),
                        is_pure: *is_pure,
                    });
                } else {
                    acceptable_types.push(TAtomic::TObjectWithProperties {
                        properties: Default::default(),
                        is_stringable: false,
                        is_invokable: true,
                    });
                }
            }
            TAtomic::TIterable {
                key_type,
                value_type,
            } => {
                // iterable is array|Traversable; removing array leaves Traversable
                acceptable_types.push(TAtomic::TNamedObject {
                    name: StrId::TRAVERSABLE,
                    type_params: Some(vec![(**key_type).clone(), (**value_type).clone()]),
                    is_static: false,
                    remapped_params: false,
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_array(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
            // Remove every list (generic lists and list-shaped keyed arrays).
            TAtomic::TArray { is_list: true, .. } => {}
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = subtract_list(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Whether a negated array assertion's param is "general": mixed or a bare
/// template param (is_array() builds its assertion from the existing
/// iterable's template params). Specific concrete params come from
/// `@psalm-assert` docblocks and subtract param-aware downstream.
fn negated_array_param_is_general(param: &TUnion) -> bool {
    param.is_mixed()
        || param
            .types
            .iter()
            .all(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
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
                // The only falsy strings are "" and "0". Matches Psalm.
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: "0".to_string(),
                });
            }
            TAtomic::TNonEmptyString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TTruthyString => {
                // The only falsy non-empty string is "0". (A truthy-string cannot
                // be "0", so it narrows to nothing here.) Matches Psalm.
                if !matches!(atomic, TAtomic::TTruthyString) {
                    acceptable_types.push(TAtomic::TLiteralString {
                        value: "0".to_string(),
                    });
                }
            }
            TAtomic::TLowercaseString => {
                // A lowercase-string can be "" or "0".
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: "0".to_string(),
                });
            }
            TAtomic::TInt => {
                // Could be 0
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
            }
            TAtomic::TIntRange { min, max } => {
                // The only falsy int is 0; keep it only if the range contains 0
                // (Psalm reconcileFalsyOrEmpty narrows an int range to literal 0).
                let contains_zero = min.is_none_or(|m| m <= 0) && max.is_none_or(|m| m >= 0);
                if contains_zero {
                    acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                }
            }
            // A *shape* (known entries present) is falsy only when empty; if it
            // can be empty (no required keys), narrow to the empty array.
            // Matches Psalm.
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                let has_required = known_values
                    .values()
                    .any(|(possibly_undefined, _)| !*possibly_undefined);
                if !has_required {
                    acceptable_types.push(TAtomic::array(TUnion::nothing(), TUnion::nothing()));
                }
            }
            // A generic array/list — could be the empty array.
            TAtomic::TArray { .. } => {
                acceptable_types.push(TAtomic::array(TUnion::nothing(), TUnion::nothing()));
            }
            TAtomic::TMixed => {
                // Mixed could be any falsy value
                acceptable_types.push(TAtomic::TNull);
                acceptable_types.push(TAtomic::TFalse);
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                acceptable_types.push(TAtomic::TLiteralFloat { value: 0.0 });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: "0".to_string(),
                });
            }
            TAtomic::TNonEmptyMixed => {
                // Non-empty mixed but can still be falsy (0, "", false)
                acceptable_types.push(TAtomic::TFalse);
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                acceptable_types.push(TAtomic::TLiteralFloat { value: 0.0 });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: "0".to_string(),
                });
            }
            TAtomic::TScalar => {
                acceptable_types.push(TAtomic::TFalse);
                acceptable_types.push(TAtomic::TLiteralInt { value: 0 });
                acceptable_types.push(TAtomic::TLiteralFloat { value: 0.0 });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
                acceptable_types.push(TAtomic::TLiteralString {
                    value: "0".to_string(),
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
                if as_type.is_mixed() {
                    // A mixed-bounded template can be falsy; keep it as-is
                    // (Psalm's template reconciliation leniency —
                    // allowTemplateReconciliation).
                    acceptable_types.push(atomic.clone());
                } else {
                    let subtracted = reconcile_falsy(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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
fn reconcile_not_isset(
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&str>,
    _analyzer: &StatementsAnalyzer<'_>,
    _analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    let _ = possibly_undefined;
    // For nested paths (`$a[0]`, `$obj->prop`), forcing the type to `null` bleeds
    // nullability through branch merges and causes false positives after guarded writes.
    // Keep the existing nested value type and model the unset state through clauses.
    if key.is_some_and(|k| k.contains('[') || k.contains("->")) {
        return existing_var_type.clone();
    }

    // Plain variables use the historical null fallback for !isset checks.
    TUnion::new(TAtomic::TNull)
}

/// Reconciles an empty countable assertion.
///
/// Narrows to empty arrays.
fn reconcile_empty_countable(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            // A *shape* (known entries present). All-optional properties mean the
            // shape may be empty (Psalm's reconcileNotNonEmptyCountable:
            // !isNonEmpty() → empty array); a required property means it can't.
            // TODO(unify-array): the old code kept an *empty-properties* keyed
            // array verbatim; under unification that case is a generic/empty
            // array and is normalised to `array<never, never>` (same meaning).
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                if known_values
                    .values()
                    .all(|(possibly_undefined, _)| *possibly_undefined)
                {
                    acceptable_types.push(TAtomic::array(TUnion::nothing(), TUnion::nothing()));
                }
                // else: has a required property, can't be empty — skip.
            }
            // A generic non-empty LIST can't be empty, skip (Psalm's keyed-list
            // branch removes the type and reports the impossibility).
            TAtomic::TArray {
                is_list: true,
                is_nonempty: true,
                ..
            } => {}
            // Every other generic array/list narrows to the empty array. Psalm's
            // reconcileNotNonEmptyCountable TArray branch covers non-empty arrays
            // too: it silently substitutes array<never, never>.
            TAtomic::TArray { .. } => {
                acceptable_types.push(TAtomic::array(TUnion::nothing(), TUnion::nothing()));
            }
            TAtomic::TMixed => {
                // Could be empty array
                acceptable_types.push(TAtomic::array(TUnion::nothing(), TUnion::nothing()));
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if !as_type.is_mixed() {
                    let subtracted = reconcile_empty_countable(as_type);
                    push_narrowed_template_type(&mut acceptable_types, atomic, subtracted);
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

/// Reconciles a `count($x) < count` assertion (`DoesNotHaveAtLeastCount`).
///
/// Mirrors Psalm's `SimpleNegatedAssertionReconciler::reconcileNotNonEmptyCountable`
/// with a non-null `$count`: removes sealed shapes that always have at least `count`
/// elements (an impossible `< count`), keeps the rest. The centralized redundant-issue
/// path reports the impossible/redundant cases.
fn reconcile_does_not_have_at_least_count(existing_var_type: &TUnion, count: usize) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            // A *shape* (known entries present).
            TAtomic::TArray {
                known_values,
                params,
                is_list,
                ..
            } if !known_values.is_empty() => {
                // Mirror Psalm's reconcileNotNonEmptyCountable: a shape whose
                // getMinCount() already meets the bound can never be shorter.
                let prop_min_count = atomic.get_min_count().unwrap_or(0);

                if prop_min_count >= count {
                    // count($a) < count is impossible: the shape is always at least
                    // `count` long. Drop it (yielding a contradiction).
                    did_remove_type = true;
                } else if *is_list && !atomic.array_is_sealed() && count >= 2 && count <= 32 {
                    // A list shape with an open fallback tail is capped by
                    // `count($a) < count`: it holds at most `count - 1` elements,
                    // so the tail is dropped and the shape sealed to that length
                    // (`list{0: T, 1?: T, ...<T>}` under `count($a) < 3` becomes
                    // `list{0: T, 1?: T}`). Known entries keep their type and
                    // definedness; positions the tail would have supplied become
                    // possibly-undefined.
                    did_remove_type = true;
                    let tail_value = params
                        .as_deref()
                        .map(|(_, value)| value.clone())
                        .unwrap_or_else(TUnion::nothing);
                    let mut new_known_values: rustc_hash::FxHashMap<
                        pzoom_code_info::ArrayKey,
                        (bool, TUnion),
                    > = rustc_hash::FxHashMap::default();
                    for index in 0..(count - 1) {
                        let array_key = pzoom_code_info::ArrayKey::Int(index as i64);
                        let (possibly_undefined, value) = match known_values.get(&array_key) {
                            Some((pu, value)) => (*pu, value.clone()),
                            None => (true, tail_value.clone()),
                        };
                        if value.is_nothing() {
                            continue;
                        }
                        new_known_values.insert(array_key, (possibly_undefined, value));
                    }
                    acceptable_types.push(TAtomic::keyed_array(
                        new_known_values,
                        true,
                        true,
                        None,
                        None,
                    ));
                } else {
                    // Redundant (always shorter) or possible: keep the shape.
                    acceptable_types.push(atomic.clone());
                }
            }
            // Psalm reshapes a generic list under count($a) < N into a sealed
            // sized shape: the first element keeps its definedness, the rest
            // become possibly-undefined (list{0: T, 1?: T} for N = 3).
            TAtomic::TArray {
                params,
                is_list: true,
                is_nonempty,
                ..
            } => {
                // The reshape is capped: a guard like `count($a) > 60_000`
                // would produce a shape with 60k properties that every
                // downstream operation re-walks (Psalm's own simplifyCNF
                // comments on the same cliff). Above the cap the list is
                // kept verbatim.
                if count <= 1 || count > 32 {
                    acceptable_types.push(atomic.clone());
                    continue;
                }
                did_remove_type = true;
                let value_type = params
                    .as_deref()
                    .map(|(_, value)| value.clone())
                    .unwrap_or_else(TUnion::nothing);
                let mut known_values: rustc_hash::FxHashMap<
                    pzoom_code_info::ArrayKey,
                    (bool, TUnion),
                > = rustc_hash::FxHashMap::default();
                let first_defined = *is_nonempty;
                for index in 0..(count - 1) {
                    let possibly_undefined = index > 0 || !first_defined;
                    known_values.insert(
                        pzoom_code_info::ArrayKey::Int(index as i64),
                        (possibly_undefined, value_type.clone()),
                    );
                }
                acceptable_types.push(TAtomic::keyed_array(known_values, true, true, None, None));
            }
            _ => {
                acceptable_types.push(atomic.clone());
            }
        }
    }

    if acceptable_types.is_empty() {
        return TUnion::nothing();
    }
    // Nothing dropped: keep the type verbatim (preserving data-flow nodes) so the
    // centralized redundant-issue path can detect the no-op via equality.
    if !did_remove_type {
        return existing_var_type.clone();
    }
    TUnion::from_types(acceptable_types)
}

/// Reconciles a DoesNotHaveArrayKey assertion.
fn reconcile_no_array_key(existing_var_type: &TUnion, key: &pzoom_code_info::ArrayKey) -> TUnion {
    let mut result_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            // A *shape* (known entries present).
            TAtomic::TArray {
                known_values,
                params,
                is_list,
                ..
            } if !known_values.is_empty() => {
                // Remove the key from known items if it exists
                let mut new_known_values = (**known_values).clone();
                new_known_values.remove(key);

                result_types.push(TAtomic::keyed_array_arc(
                    std::sync::Arc::new(new_known_values),
                    *is_list,
                    atomic.array_is_sealed(),
                    params.clone(),
                ));
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                let subtracted = reconcile_no_array_key(as_type, key);
                push_narrowed_template_type(&mut result_types, atomic, subtracted);
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
