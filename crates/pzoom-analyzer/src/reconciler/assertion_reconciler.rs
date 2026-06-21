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
use pzoom_code_info::TemplateResult;

/// Main entry point for assertion reconciliation.
///
/// Given an assertion and an existing type, returns the narrowed type that
/// results from applying the assertion.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: Option<&TUnion>,
    possibly_undefined: bool,
    key: Option<&str>,
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
            possibly_undefined,
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

    // In a trait body `$this` is generic: the same code runs for every using
    // class, so an `instanceof`/class assertion the concrete using class can't
    // satisfy must narrow `$this` to the asserted class (Psalm treats the trait
    // receiver as open), not to the empty `never` intersection that would make
    // every subsequent `$this->...` access look invalid.
    if analysis_data.in_trait_body
        && key == Some("$this")
        && !negated
        && reconciled_type.is_nothing()
        && !existing_var_type.is_nothing()
        && let Some(asserted) = positive_object_assertion_type(assertion)
    {
        return asserted;
    }

    reconciled_type
}

/// The asserted object type of a positive class/instanceof assertion
/// (`$x instanceof Foo`, `get_class($x) === Foo::class`), used to narrow a
/// generic trait's `$this` to the asserted class rather than `never`.
pub(crate) fn positive_object_assertion_type(assertion: &Assertion) -> Option<TUnion> {
    let atomic = match assertion {
        Assertion::IsType(atomic) | Assertion::IsEqual(atomic) | Assertion::IsLooselyEqual(atomic) => {
            atomic
        }
        _ => return None,
    };
    matches!(atomic, TAtomic::TNamedObject { .. }).then(|| TUnion::new(atomic.clone()))
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
            // Psalm's getMissingType: `Type::getMixed($inside_loop)` — inside
            // a loop the placeholder keeps its from-loop-isset flavour so the
            // type combiner can evict it once a concrete type is merged in.
            if inside_loop {
                TUnion::new(TAtomic::TMixedFromLoopIsset)
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
    // Psalm's AssertionReconciler keyed-array pre-step: when the asserted
    // shape shares NO keys with some existing keyed members, those members
    // absorb the asserted properties (sequential @psalm-assert shape calls
    // intersect: array{foo} + assert array{bar} = array{foo, bar}), and
    // they alone form the result.
    if let TAtomic::TArray {
        known_values: assertion_known_values,
        ..
    } = assertion_atomic
        && !assertion_known_values.is_empty()
    {
        // Only when NO member shares keys with the assertion (the
        // sequential-assert case); a member sharing keys means a
        // discriminated union, where the assertion selects its shape.
        let any_member_shares_keys = existing_var_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TArray { known_values, .. }
                    if !known_values.is_empty()
                        && known_values.keys().any(|key| assertion_known_values.contains_key(key))
            )
        });
        let mut merged_members = Vec::new();
        for atomic in &existing_var_type.types {
            if any_member_shares_keys {
                break;
            }
            if let TAtomic::TArray {
                known_values: existing_known_values,
                params,
                is_list,
                ..
            } = atomic
                && !existing_known_values.is_empty()
                && !existing_known_values
                    .keys()
                    .any(|key| assertion_known_values.contains_key(key))
            {
                let mut known_values = (**existing_known_values).clone();
                for (key, value) in assertion_known_values.iter() {
                    known_values.insert(key.clone(), value.clone());
                }
                merged_members.push(TAtomic::keyed_array_arc(
                    std::sync::Arc::new(known_values),
                    *is_list,
                    atomic.array_is_sealed(),
                    params.clone(),
                ));
            }
        }
        if !merged_members.is_empty() {
            return Some(TUnion::from_types(merged_members));
        }
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        // Psalm's reconcileNumeric splits array-key into its numeric halves
        // (int and numeric-string) — a per-atomic intersection can't.
        if matches!(
            (atomic, assertion_atomic),
            (TAtomic::TArrayKey, TAtomic::TNumeric)
        ) {
            acceptable_types.push(TAtomic::TInt);
            acceptable_types.push(TAtomic::TNumericString);
            continue;
        }
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

/// The element type of an array/list intersection: a mixed side defers to the
/// other, otherwise the types intersect (falling back to the assertion side
/// when they cannot).
fn intersect_array_value_unions(existing: &TUnion, assertion: &TUnion) -> TUnion {
    if assertion.is_mixed() {
        return existing.clone();
    }
    if existing.is_mixed() {
        return assertion.clone();
    }
    intersect_union_with_union(existing, assertion).unwrap_or_else(|| assertion.clone())
}

/// Psalm's `callable-array{0: class-string|object, 1: non-empty-string}`.
fn callable_array_shape() -> TAtomic {
    let mut known_values: rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, (bool, TUnion)> =
        rustc_hash::FxHashMap::default();
    known_values.insert(
        pzoom_code_info::ArrayKey::Int(0),
        (
            false,
            TUnion::from_types(vec![
                TAtomic::TClassString { as_type: None },
                TAtomic::TObject,
            ]),
        ),
    );
    known_values.insert(
        pzoom_code_info::ArrayKey::Int(1),
        (false, TUnion::new(TAtomic::TNonEmptyString)),
    );
    TAtomic::keyed_array(known_values, true, true, None, None)
}

/// Whether an array key union could hold int keys (so the array could be a
/// list). `never` (empty array) qualifies; a string-only key type does not.
fn array_key_union_allows_int(key_type: &TUnion) -> bool {
    key_type.is_nothing()
        || key_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
                    | TAtomic::TArrayKey
                    | TAtomic::TMixed
                    | TAtomic::TNonEmptyMixed
            )
        })
}

fn intersect_atomic_with_atomic_inner(
    existing_atomic: &TAtomic,
    assertion_atomic: &TAtomic,
    codebase: Option<&CodebaseInfo>,
) -> Option<TAtomic> {
    // Hakana's assertion reconciler keeps a type variable through any
    // intersection (its bound recording happens where analysis data is in
    // scope; the variable's constraints reconcile at function end).
    if matches!(existing_atomic, TAtomic::TTypeVariable { .. }) {
        return Some(existing_atomic.clone());
    }

    if matches!(assertion_atomic, TAtomic::TTypeVariable { .. }) {
        return Some(assertion_atomic.clone());
    }

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
        // `is_callable($cb)` asserts a *bare* `callable`. When the value is
        // already typed with a callable signature (`callable():T`), keep that
        // signature rather than collapsing it to a bare `callable` that
        // returns mixed — Psalm's reconcileCallable keeps `$type` unchanged
        // when `$type->isCallableType()`. (The TClosure side is already kept
        // by the assertion-is-callable arm below.)
        if matches!(
            assertion_atomic,
            TAtomic::TCallable {
                params: None,
                return_type: None,
                ..
            }
        ) {
            return Some(existing_atomic.clone());
        }
        // `is_array` on a callable narrows to Psalm's callable-array shape
        // `{0: class-string|object, 1: non-empty-string}` — a sealed
        // two-element list (so `count()` knows it is exactly 2).
        if matches!(assertion_atomic, TAtomic::TArray { .. }) {
            return Some(callable_array_shape());
        }
        // `is_string` on a callable narrows to callable-string (Psalm's
        // reconcileString pushes TCallableString for a TCallable part).
        if matches!(assertion_atomic, TAtomic::TString) {
            return Some(TAtomic::TCallableString);
        }
        return Some(assertion_atomic.clone());
    }

    if matches!(assertion_atomic, TAtomic::TCallable { .. })
        && atomic_can_be_callable_representation(existing_atomic)
    {
        // Psalm's reconcileCallable narrows a plain string to callable-string.
        if matches!(
            existing_atomic,
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TTruthyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
        ) {
            return Some(TAtomic::TCallableString);
        }
        // A KNOWN class is only callable when it declares __invoke (Psalm's
        // reconcileCallable drops the rest); unknown classes stay.
        if let TAtomic::TNamedObject { name, .. } = existing_atomic
            && let Some(codebase) = codebase
            && let Some(class_info) = codebase.get_class(*name)
            && !class_info.methods.contains_key(&pzoom_str::StrId::INVOKE)
        {
            return None;
        }
        return Some(existing_atomic.clone());
    }

    // Psalm's intersectAtomicTypes: the empty array (`array<never, never>`)
    // and a non-empty array-like are disjoint — neither is contained by the
    // other, so Type::intersectUnionTypes finds no intersection and returns
    // null (this is what resolves `TArray is array<never, never>` to the
    // else branch for a non-empty bound in conditional return types).
    let is_empty_array_atomic = |atomic: &TAtomic| match atomic {
        TAtomic::TArray {
            known_values,
            params,
            is_nonempty,
            ..
        } => {
            known_values.is_empty()
                && !*is_nonempty
                && params
                    .as_deref()
                    .is_none_or(|(key, value)| key.is_nothing() && value.is_nothing())
        }
        _ => false,
    };
    // Only a *generic* non-empty array/list (no known entries), matching the old
    // `TNonEmptyArray`/`TNonEmptyList` variants — keyed shapes have their own
    // intersection arms below.
    let is_non_empty_array_like = |atomic: &TAtomic| {
        matches!(
            atomic,
            TAtomic::TArray { known_values, is_nonempty: true, .. } if known_values.is_empty()
        )
    };
    if (is_empty_array_atomic(existing_atomic) && is_non_empty_array_like(assertion_atomic))
        || (is_non_empty_array_like(existing_atomic) && is_empty_array_atomic(assertion_atomic))
    {
        return None;
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
        // array-key ∩ numeric is int|numeric-string; the union-level loop in
        // intersect_union_with_atomic splits it (a single atomic can't).
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
        // `gettype($x)` results are string subtypes (Psalm's
        // TDependentGetType): `$t === 'object'` narrows to the literal —
        // but only the strings gettype can actually return ('bool' can't).
        (TAtomic::TDependentGetType { .. }, TAtomic::TLiteralString { value })
            if matches!(
                value.as_str(),
                "boolean"
                    | "integer"
                    | "double"
                    | "string"
                    | "array"
                    | "object"
                    | "resource"
                    | "resource (closed)"
                    | "NULL"
                    | "unknown type"
            ) =>
        {
            Some(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
        (TAtomic::TDependentGetClass { .. }, TAtomic::TLiteralString { value }) => {
            Some(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
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
        // A lowercase literal string is a member of (non-empty-)lowercase-string
        // and a truthy literal a member of truthy-string, so `$s === 'foo'` on
        // those refinements keeps the literal rather than clearing.
        (
            TAtomic::TLowercaseString | TAtomic::TNonEmptyLowercaseString,
            TAtomic::TLiteralString { value },
        )
        | (
            TAtomic::TLiteralString { value },
            TAtomic::TLowercaseString | TAtomic::TNonEmptyLowercaseString,
        ) if !value.is_empty()
            && !value.chars().any(|c| c.is_uppercase()) =>
        {
            Some(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
        (TAtomic::TTruthyString, TAtomic::TLiteralString { value })
        | (TAtomic::TLiteralString { value }, TAtomic::TTruthyString)
            if !value.is_empty() && value != "0" =>
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
        (TAtomic::TString, TAtomic::TClassString { as_type })
        // A class-string is a non-empty (and non-falsy) string, so the
        // narrower string-family types intersect to class-string too
        // (class_exists on a non-falsy-string — Psalm's reconcileString).
        | (TAtomic::TNonEmptyString, TAtomic::TClassString { as_type })
        | (TAtomic::TTruthyString, TAtomic::TClassString { as_type })
        | (TAtomic::TCallableString, TAtomic::TClassString { as_type })
        | (TAtomic::TLowercaseString, TAtomic::TClassString { as_type })
        | (TAtomic::TNonEmptyLowercaseString, TAtomic::TClassString { as_type }) => {
            Some(TAtomic::TClassString {
                as_type: as_type.clone(),
            })
        }
        (TAtomic::TString, TAtomic::TLiteralClassString { name }) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }
        // callable-string is a subtype of the general string family, so
        // `function_exists($s)` narrows a plain string to callable-string.
        (
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString,
            TAtomic::TCallableString,
        ) => Some(TAtomic::TCallableString),
        // A literal string naming a known function (or a Class::method whose
        // class resolves) is a member of callable-string, so
        // `function_exists($name)` on a literal keeps the literal.
        (TAtomic::TLiteralString { value }, TAtomic::TCallableString)
        | (TAtomic::TCallableString, TAtomic::TLiteralString { value }) => {
            let resolves = codebase.is_some_and(|codebase| {
                if let Some((class_name, _)) = value.split_once("::") {
                    codebase.resolve_classlike_name(class_name).is_some()
                } else {
                    codebase.resolve_functionlike_name(value).is_some()
                }
            });
            if resolves {
                Some(TAtomic::TLiteralString {
                    value: value.clone(),
                })
            } else {
                None
            }
        }
        // Psalm's handleLiteralEqualityWithString: any non-literal string
        // subtype (class-string included) asserted equal to a literal
        // becomes that literal — `$class_name === 'SoapFault'` on a
        // class-string narrows to 'SoapFault' without complaint.
        (TAtomic::TClassString { .. }, TAtomic::TLiteralString { value }) => {
            Some(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
        (TAtomic::TLiteralString { value }, TAtomic::TString) => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        (TAtomic::TLiteralClassString { name }, TAtomic::TString) => {
            Some(TAtomic::TLiteralClassString { name: name.clone() })
        }
        (TAtomic::TNonEmptyString, TAtomic::TString) => Some(TAtomic::TNonEmptyString),
        (TAtomic::TTruthyString, TAtomic::TString) => Some(TAtomic::TTruthyString),
        // callable-string is a non-falsy-string subtype (Psalm TCallableString)
        (TAtomic::TCallableString, TAtomic::TString)
        | (TAtomic::TCallableString, TAtomic::TNonEmptyString)
        | (TAtomic::TCallableString, TAtomic::TTruthyString)
        | (TAtomic::TCallableString, TAtomic::TCallableString) => Some(TAtomic::TCallableString),
        (TAtomic::TString, TAtomic::TTruthyString) => Some(TAtomic::TTruthyString),
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
        // An enum-typed value often appears as a plain named object (the
        // signature scanner doesn't resolve enum-ness); it still intersects
        // with the enum's cases and with the enum itself — including through
        // an interface the enum implements (`I ∩ E::A = E::A`).
        (
            TAtomic::TNamedObject { name, .. },
            TAtomic::TEnumCase {
                enum_name,
                case_name,
            },
        )
        | (
            TAtomic::TEnumCase {
                enum_name,
                case_name,
            },
            TAtomic::TNamedObject { name, .. },
        ) if name == enum_name
            || codebase.is_some_and(|codebase| {
                crate::type_comparator::object_type_comparator::is_class_subtype_of(
                    *enum_name, *name, codebase,
                )
            }) =>
        {
            Some(TAtomic::TEnumCase {
                enum_name: *enum_name,
                case_name: *case_name,
            })
        }
        (TAtomic::TNamedObject { name, .. }, TAtomic::TEnum { name: enum_name })
        | (TAtomic::TEnum { name: enum_name }, TAtomic::TNamedObject { name, .. })
            if name == enum_name
                || codebase.is_some_and(|codebase| {
                    crate::type_comparator::object_type_comparator::is_class_subtype_of(
                        *enum_name, *name, codebase,
                    )
                }) =>
        {
            Some(TAtomic::TEnum { name: *enum_name })
        }

        // A bare `object` narrowed by an object-with-properties assertion
        // (e.g. method_exists(.., '__toString') -> stringable-object) keeps
        // the more specific shape; the reverse keeps the existing shape.
        (TAtomic::TObject, TAtomic::TObjectWithProperties { .. }) => {
            Some(assertion_atomic.clone())
        }
        (TAtomic::TObjectWithProperties { .. }, TAtomic::TObject) => {
            Some(existing_atomic.clone())
        }
        // A named object asserted to an object shape stays itself when a
        // subclass could satisfy the shape; a FINAL class must already
        // declare the asserted properties/__toString or the intersection is
        // empty (Psalm: "Type Foo for $x is never").
        (
            TAtomic::TNamedObject { name, .. },
            TAtomic::TObjectWithProperties {
                properties,
                is_stringable,
                ..
            },
        ) => {
            let satisfiable = match codebase.and_then(|codebase| codebase.get_class(*name)) {
                Some(class_info) if class_info.is_final => {
                    let properties_ok = properties.is_empty() || !class_info.properties.is_empty();
                    let stringable_ok =
                        !*is_stringable || class_info.methods.contains_key(&StrId::TO_STRING);
                    properties_ok && stringable_ok
                }
                _ => true,
            };
            if satisfiable {
                if *is_stringable || properties.is_empty() {
                    Some(existing_atomic.clone())
                } else {
                    // Psalm intersects the class with the asserted shape
                    // (`stdClass&object{status: ...}`), so the shape's
                    // properties read through the result.
                    Some(TAtomic::TObjectIntersection {
                        types: vec![existing_atomic.clone(), assertion_atomic.clone()],
                    })
                }
            } else {
                None
            }
        }

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

        // array/list/keyed-array intersections. The unified `TArray` covers
        // every old array sort, so this single arm reproduces Psalm's
        // filterTypeWithAnotherType matrix, dispatching on whether each side is
        // a shape (known entries), a list, and whether it is non-empty.
        (
            TAtomic::TArray {
                known_values: e_known,
                params: e_params,
                is_list: e_is_list,
                is_nonempty: e_nonempty,
                ..
            },
            TAtomic::TArray {
                known_values: a_known,
                params: a_params,
                is_list: a_is_list,
                is_nonempty: a_nonempty,
                ..
            },
        ) => {
            let e_shape = !e_known.is_empty();
            let a_shape = !a_known.is_empty();

            // Shapes: the union-level pre-step (the keyed-array merge above)
            // covers Psalm's disjoint shape-merge; a member sharing keys with
            // the assertion narrows to the assertion shape. A shape paired with
            // a *generic list* assertion had no old arm, so it stays disjoint.
            if e_shape && a_shape {
                // Two shapes: narrow to the assertion shape.
                return Some(assertion_atomic.clone());
            }
            if e_shape {
                // Existing shape ∩ generic. Keep the (narrower) shape — even
                // against a generic *list*, when the shape is itself a list
                // (sequential int keys). Only a non-list shape (string keys)
                // paired with a generic list is disjoint: a list can't carry
                // string keys.
                return if *a_is_list && !*e_is_list {
                    None
                } else {
                    Some(existing_atomic.clone())
                };
            }
            if a_shape {
                // Generic ∩ assertion shape: narrow to the (narrower) shape —
                // including a generic *list* against a list shape. Only a
                // generic list paired with a non-list shape is disjoint.
                return if *e_is_list && !*a_is_list {
                    None
                } else {
                    Some(assertion_atomic.clone())
                };
            }

            // Both sides are generic (no known entries). Param key/value, with
            // a missing fallback treated as `never` (the empty array).
            let never = TUnion::nothing();
            let e_key = e_params.as_deref().map(|(key, _)| key).unwrap_or(&never);
            let e_value = e_params
                .as_deref()
                .map(|(_, value)| value)
                .unwrap_or(&never);
            let a_key = a_params.as_deref().map(|(key, _)| key).unwrap_or(&never);
            let a_value = a_params
                .as_deref()
                .map(|(_, value)| value)
                .unwrap_or(&never);
            let result_nonempty = *e_nonempty || *a_nonempty;

            match (*e_is_list, *a_is_list) {
                // array ∩ array — intersect both params, dropping the atomic
                // when a param is provably disjoint (Psalm's
                // filterTypeWithAnotherType).
                (false, false) => {
                    // A missing param intersection falls back to the
                    // more-specific side UNLESS the params are provably disjoint
                    // (no containment in either direction) — then the arrays
                    // can't intersect at all.
                    let params_disjoint = |a: &TUnion, b: &TUnion| {
                        if a.is_mixed()
                            || b.is_mixed()
                            || a.types
                                .iter()
                                .chain(b.types.iter())
                                .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
                        {
                            return false;
                        }
                        let Some(codebase) = codebase else {
                            return false;
                        };
                        !crate::type_comparator::union_type_comparator::can_be_contained_by(
                            codebase, a, b,
                        ) && !crate::type_comparator::union_type_comparator::can_be_contained_by(
                            codebase, b, a,
                        )
                    };
                    let intersected_key =
                        match intersect_union_with_union_with_codebase(e_key, a_key, codebase) {
                            Some(intersected) => intersected,
                            None if params_disjoint(e_key, a_key) => return None,
                            None => pick_more_specific_union(e_key, a_key),
                        };
                    let intersected_value =
                        match intersect_union_with_union_with_codebase(e_value, a_value, codebase)
                        {
                            Some(intersected) => intersected,
                            None if params_disjoint(e_value, a_value) => return None,
                            None => pick_more_specific_union(e_value, a_value),
                        };
                    if result_nonempty {
                        Some(TAtomic::non_empty_array(intersected_key, intersected_value))
                    } else {
                        Some(TAtomic::array(intersected_key, intersected_value))
                    }
                }
                // list ∩ array — a list has int keys, so a string-only-keyed
                // array assertion is disjoint. Result is a list.
                (true, false) => {
                    if !array_key_union_allows_int(a_key) {
                        return None;
                    }
                    let value = intersect_union_with_union_with_codebase(e_value, a_value, codebase)
                        .unwrap_or_else(|| pick_more_specific_union(e_value, a_value));
                    if result_nonempty {
                        Some(TAtomic::non_empty_list(value))
                    } else {
                        Some(TAtomic::list(value))
                    }
                }
                // array ∩ list — narrow to a list when the existing key type
                // allows int keys (a string-only-keyed array can't be a list).
                // A possibly-empty array asserted `non-empty-list` narrows to
                // `non-empty-list` (Psalm's filterTypeWithAnotherType); the
                // string-keyed case is still ruled out by the int-key check.
                (false, true) => {
                    if !array_key_union_allows_int(e_key) {
                        return None;
                    }
                    let value = intersect_union_with_union_with_codebase(e_value, a_value, codebase)
                        .unwrap_or_else(|| pick_more_specific_union(e_value, a_value));
                    if result_nonempty {
                        Some(TAtomic::non_empty_list(value))
                    } else {
                        Some(TAtomic::list(value))
                    }
                }
                // list ∩ list — the value type is the intersection of both
                // sides. NB: two *non-empty* generic lists with differing value
                // types had no old arm, so they stay disjoint here (identical
                // atomics were already handled above).
                // TODO(unify-array): the absent (non-empty-list, non-empty-list)
                // arm looks like a latent gap in the pre-unification matrix;
                // preserved verbatim.
                (true, true) => {
                    if *e_nonempty && *a_nonempty {
                        return None;
                    }
                    let value = intersect_array_value_unions(e_value, a_value);
                    if result_nonempty {
                        Some(TAtomic::non_empty_list(value))
                    } else {
                        Some(TAtomic::list(value))
                    }
                }
            }
        }

        // iterable types — the assertion/existing array side is the unified
        // `TArray`, so dispatch on shape / list / non-empty internally.
        (
            TAtomic::TIterable {
                key_type: existing_key_type,
                value_type: existing_value_type,
            },
            TAtomic::TArray {
                known_values: a_known,
                params: a_params,
                is_list: a_is_list,
                is_nonempty: a_nonempty,
                ..
            },
        ) => {
            if !a_known.is_empty() {
                // iterable ∩ shape → the shape.
                return Some(assertion_atomic.clone());
            }
            let never = TUnion::nothing();
            let a_key = a_params.as_deref().map(|(key, _)| key).unwrap_or(&never);
            let a_value = a_params
                .as_deref()
                .map(|(_, value)| value)
                .unwrap_or(&never);
            if *a_is_list {
                let value =
                    intersect_union_with_union_with_codebase(existing_value_type, a_value, codebase)
                        .unwrap_or_else(|| pick_more_specific_union(existing_value_type, a_value));
                if *a_nonempty {
                    Some(TAtomic::non_empty_list(value))
                } else {
                    Some(TAtomic::list(value))
                }
            } else if *a_nonempty {
                // No old `(TIterable, TNonEmptyArray)` arm — disjoint.
                None
            } else {
                Some(TAtomic::array(
                    intersect_union_with_union_with_codebase(existing_key_type, a_key, codebase)
                        .unwrap_or_else(|| pick_more_specific_union(existing_key_type, a_key)),
                    intersect_union_with_union_with_codebase(existing_value_type, a_value, codebase)
                        .unwrap_or_else(|| pick_more_specific_union(existing_value_type, a_value)),
                ))
            }
        }
        (
            TAtomic::TArray {
                known_values: e_known,
                params: e_params,
                is_list: e_is_list,
                is_nonempty: e_nonempty,
                ..
            },
            TAtomic::TIterable {
                key_type: a_key_type,
                value_type: a_value_type,
            },
        ) => {
            if !e_known.is_empty() {
                // shape ∩ iterable → the shape.
                return Some(existing_atomic.clone());
            }
            let never = TUnion::nothing();
            let e_value = e_params
                .as_deref()
                .map(|(_, value)| value)
                .unwrap_or(&never);
            // The iterable's value type refines the array's (Psalm's
            // filterTypeWithAnother: `list<mixed>` asserted `iterable<_, int>`
            // is `list<int>`).
            let value = intersect_union_with_union_with_codebase(e_value, a_value_type, codebase)
                .unwrap_or_else(|| pick_more_specific_union(e_value, a_value_type));
            if *e_is_list {
                if *e_nonempty {
                    Some(TAtomic::non_empty_list(value))
                } else {
                    Some(TAtomic::list(value))
                }
            } else if *e_nonempty {
                // No old `(TNonEmptyArray, TIterable)` arm — disjoint.
                None
            } else {
                let e_key = e_params.as_deref().map(|(key, _)| key).unwrap_or(&never);
                let key = intersect_union_with_union_with_codebase(e_key, a_key_type, codebase)
                    .unwrap_or_else(|| pick_more_specific_union(e_key, a_key_type));
                Some(TAtomic::array(key, value))
            }
        }
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
                intersect_union_with_union_with_codebase(existing_key_type, asserted_key_type, codebase).unwrap_or_else(
                    || pick_more_specific_union(existing_key_type, asserted_key_type),
                ),
            ),
            value_type: Box::new(
                intersect_union_with_union_with_codebase(existing_value_type, asserted_value_type, codebase)
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
        // `iterable<K, V>` is `array<K, V> | Traversable<K, V>`, so its object
        // part is `Traversable<K, V>` (Psalm: `is_object` on an iterable).
        (
            TAtomic::TIterable {
                key_type,
                value_type,
            },
            TAtomic::TObject,
        )
        | (
            TAtomic::TObject,
            TAtomic::TIterable {
                key_type,
                value_type,
            },
        ) => Some(TAtomic::TNamedObject {
            name: StrId::TRAVERSABLE,
            type_params: Some(vec![(**key_type).clone(), (**value_type).clone()]),
            is_static: false,
            remapped_params: false,
        }),
        (
            named @ TAtomic::TNamedObject {
                name, type_params, ..
            },
            TAtomic::TIterable {
                key_type: asserted_key_type,
                value_type: asserted_value_type,
            },
        ) if codebase.is_some_and(|cb| {
            object_type_comparator::is_class_subtype_of(*name, StrId::TRAVERSABLE, cb)
        }) =>
        {
            // Asserting `iterable<V>` on a Traversable implementor narrows its
            // key/value params (Psalm's filterTypeWithAnother:
            // `ArrayIterator<string, mixed>` asserted `iterable<string>` is
            // `ArrayIterator<string, string>`) when the class's params map
            // 1:1 onto Traversable's.
            let codebase = codebase.unwrap();
            if let Some(params) = type_params
                && params.len() == 2
                && object_type_comparator::get_mapped_generic_type_params(
                    codebase,
                    *name,
                    params,
                    StrId::TRAVERSABLE,
                )
                // Flag bits (from_docblock etc.) differ on the remapped copies;
                // identity holds when the atomics match.
                .is_some_and(|mapped| {
                    mapped.len() == params.len()
                        && mapped
                            .iter()
                            .zip(params.iter())
                            .all(|(mapped_param, param)| mapped_param.types == param.types)
                })
            {
                let narrowed_key = intersect_union_with_union_with_codebase(
                    &params[0],
                    asserted_key_type,
                    Some(codebase),
                )
                .unwrap_or_else(|| pick_more_specific_union(&params[0], asserted_key_type));
                let narrowed_value = intersect_union_with_union_with_codebase(
                    &params[1],
                    asserted_value_type,
                    Some(codebase),
                )
                .unwrap_or_else(|| pick_more_specific_union(&params[1], asserted_value_type));
                Some(TAtomic::TNamedObject {
                    name: *name,
                    type_params: Some(vec![narrowed_key, narrowed_value]),
                    is_static: false,
                    remapped_params: false,
                })
            } else {
                Some(named.clone())
            }
        }

        // template parameter types
        (
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            },
            other,
        ) => {
            // A template already inside the asserted type stays the template
            // (Psalm's refineArrayKey / filterAtomicWithAnother keep the
            // TTemplateParam, refining only its bound): asserting
            // `is array-key` on `T as array-key` is a no-op on T.
            if let Some(codebase) = codebase
                && crate::type_comparator::atomic_type_comparator::is_contained_by_in_context(
                    codebase,
                    existing_atomic,
                    other,
                    true,
                    &mut crate::type_comparator::type_comparison_result::TypeComparisonResult::new(
                    ),
                )
            {
                return Some(existing_atomic.clone());
            }

            if as_type.is_mixed() {
                // Psalm's SimpleAssertionReconciler narrows the template's
                // *bound*, keeping the template:
                // `$type->replaceAs(self::reconcile…($type->as))`.
                Some(TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(TUnion::new(other.clone())),
                })
            } else if is_objectish_atomic(other)
                && matches!(other, TAtomic::TTemplateParam { .. })
            {
                // Asserting one template against another (`is_a($item, $type)`
                // binding T against S) keeps both as an intersection (T&S).
                Some(make_intersection(existing_atomic.clone(), other.clone()))
            } else {
                // Intersect with the constraint type, keeping the template
                // wrapper around the narrowed bound (Psalm's replaceAs):
                // `T of Node|null` after `instanceof NullableType` reads
                // `T as NullableType`, not an intersection.
                for constraint_atomic in &as_type.types {
                    if let Some(result) =
                        intersect_atomic_with_atomic_inner(constraint_atomic, other, codebase)
                    {
                        return Some(TAtomic::TTemplateParam {
                            name: *name,
                            defining_entity: *defining_entity,
                            as_type: Box::new(TUnion::new(result)),
                        });
                    }
                }
                if is_objectish_atomic(other) {
                    // The bound can't absorb the asserted object. An
                    // intersection (Psalm's addIntersectionType) is only
                    // possible when an interface is involved — two unrelated
                    // concrete classes can't both apply, so the member drops.
                    let other_is_interface = match other {
                        TAtomic::TNamedObject { name, .. } => codebase
                            .and_then(|codebase| codebase.get_class(*name))
                            .is_some_and(|info| {
                                info.kind
                                    == pzoom_code_info::class_like_info::ClassLikeKind::Interface
                            }),
                        _ => true,
                    };
                    let bound_allows_intersection = as_type.types.iter().any(|constraint| {
                        match constraint {
                            TAtomic::TNamedObject { name, .. } => codebase
                                .and_then(|codebase| codebase.get_class(*name))
                                .is_none_or(|info| {
                                    info.kind
                                        == pzoom_code_info::class_like_info::ClassLikeKind::Interface
                                }),
                            TAtomic::TObject | TAtomic::TObjectWithProperties { .. } => true,
                            _ => false,
                        }
                    });
                    if other_is_interface || bound_allows_intersection {
                        Some(make_intersection(existing_atomic.clone(), other.clone()))
                    } else {
                        None
                    }
                } else {
                    Some(existing_atomic.clone())
                }
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

        // String-refinement pairs not covered by a dedicated arm intersect to
        // the more specific side (Psalm's filterAtomicWithAnother containment
        // probes): asserting `=non-empty-string` on a
        // `non-empty-lowercase-string` keeps the lowercase string. Strings
        // carry no int→float-style coercion hazard, so the comparator's
        // verdict is safe here.
        (existing, asserted)
            if codebase.is_some()
                && atomic_is_string_refinement(existing)
                && atomic_is_string_refinement(asserted) =>
        {
            let codebase = codebase.unwrap();
            let mut comparison_result =
                crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
            if crate::type_comparator::atomic_type_comparator::is_contained_by(
                codebase,
                existing,
                asserted,
                &mut comparison_result,
            ) {
                Some(existing.clone())
            } else if crate::type_comparator::atomic_type_comparator::is_contained_by(
                codebase,
                asserted,
                existing,
                &mut comparison_result,
            ) {
                Some(asserted.clone())
            } else {
                None
            }
        }

        // If we can't find a specific intersection, types are incompatible. (Like
        // hakana-core's intersect_atomic_with_atomic, which enumerates the
        // intersectable pairs and otherwise yields no intersection, rather than
        // falling back to a lax containment check — pzoom treats int as contained by
        // float for coercion, which must not count as a type intersection.)
        _ => None,
    }
}

/// The non-literal string refinements (literal/class-string pairs have
/// dedicated intersection arms above).
fn atomic_is_string_refinement(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
    )
}

fn atomic_can_be_callable_representation(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TArray { .. }
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
            ..
        },
        TAtomic::TNamedObject {
            name: assertion_name,
            type_params: assertion_type_params,
            ..
        },
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

    let mut ancestor_template_replacements = TemplateResult::default();
    for (idx, template_type) in existing_class_info.template_types.iter().enumerate() {
        if let Some(type_param) = existing_type_params.get(idx) {
            crate::template::lower_bounds_insert(
                &mut ancestor_template_replacements,
                template_type.name,
                template_type.defining_entity,
                type_param.clone(),
            );
        }
    }

    if ancestor_template_replacements.lower_bounds.is_empty() {
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
            crate::template::lower_bounds_get(
                &inferred_template_replacements,
                template_type.name,
                template_type.defining_entity,
            )
            .or_else(|| {
                crate::template::lower_bounds_get(
                    &ancestor_template_replacements,
                    template_type.name,
                    template_type.defining_entity,
                )
            })
            .unwrap_or_else(|| template_type.as_type.clone())
        })
        .collect::<Vec<_>>();

    Some(TAtomic::TNamedObject {
        name: *assertion_name,
        type_params: Some(inferred_type_params),
        is_static: false,
        remapped_params: false,
    })
}

fn infer_class_template_replacements_from_ancestors(
    class_info: &ClassLikeInfo,
    template_replacements: &TemplateResult,
) -> TemplateResult {
    let mut propagated_replacements = template_replacements.clone();

    loop {
        let mut changed = false;

        for (ancestor_class, template_map) in &class_info.template_extended_params {
            for (ancestor_template, mapped_type) in template_map {
                let Some(ancestor_replacement) = crate::template::lower_bounds_get(
                    &propagated_replacements,
                    *ancestor_template,
                    pzoom_code_info::GenericParent::ClassLike(*ancestor_class),
                ) else {
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

                    let existing = crate::template::lower_bounds_get(
                        &propagated_replacements,
                        mapped_template,
                        mapped_entity,
                    );
                    let should_propagate =
                        existing.as_ref().is_none_or(is_template_placeholder_union);

                    if !should_propagate {
                        continue;
                    }

                    if existing.is_none_or(|existing| existing != ancestor_replacement) {
                        crate::template::lower_bounds_insert(
                            &mut propagated_replacements,
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

    let mut inferred_replacements = TemplateResult::default();
    for template_type in &class_info.template_types {
        if let Some(replacement) = crate::template::lower_bounds_get(
            &propagated_replacements,
            template_type.name,
            template_type.defining_entity,
        ) {
            crate::template::lower_bounds_insert(
                &mut inferred_replacements,
                template_type.name,
                template_type.defining_entity,
                replacement,
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
    let existing_bound = class_string_atomic_to_bound(existing_atomic, codebase);
    let assertion_bound = class_string_atomic_to_bound(assertion_atomic, codebase);

    // When both bounds resolve, an empty bound intersection rules the pair out
    // (e.g. `A::class` vs `class-string<UnrelatedB>`). A literal whose class
    // cannot be resolved (no codebase available) stays permissive rather than
    // failing the whole intersection.
    let bound = match (existing_bound, assertion_bound) {
        (Some(Some(existing_bound)), Some(Some(asserted_bound))) => {
            let intersected =
                intersect_atomic_with_atomic_inner(&existing_bound, &asserted_bound, codebase)?;
            Some(intersected)
        }
        (Some(Some(bound)), _) | (_, Some(Some(bound))) => Some(bound),
        _ => None,
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
        TAtomic::TClassString { as_type } => {
            Some(as_type.as_ref().map(|as_type| (**as_type).clone()))
        }
        TAtomic::TTemplateParamClass { as_type, .. } => Some(Some((**as_type).clone())),
        TAtomic::TLiteralClassString { name } => {
            let class_id = codebase?.resolve_classlike_name(name)?;
            Some(Some(TAtomic::TNamedObject {
                name: class_id,
                type_params: None,
                is_static: false,
                remapped_params: false,
            }))
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
    intersect_union_with_union_with_codebase(type1, type2, None)
}

/// Like [`intersect_union_with_union`], with a codebase for resolving literal
/// class-string bounds during the intersection.
pub fn intersect_union_with_union_with_codebase(
    type1: &TUnion,
    type2: &TUnion,
    codebase: Option<&CodebaseInfo>,
) -> Option<TUnion> {
    let mut result_types = Vec::new();

    for atomic1 in &type1.types {
        for atomic2 in &type2.types {
            if let Some(intersected) =
                intersect_atomic_with_atomic_inner(atomic1, atomic2, codebase)
            {
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
