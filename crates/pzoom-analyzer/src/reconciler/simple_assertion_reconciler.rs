//! Simple assertion reconciler.
//!
//! Handles positive assertions like `truthy`, `isset`, and basic type checks.

use pzoom_code_info::{ArrayKey, Assertion, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

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
    key: Option<&str>,
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

        // `$x === <literal>` when $x is exactly that literal — Psalm's
        // handleLiteralEquality reports this as redundant. Template params are
        // exempt (an identity binding is not a runtime guarantee).
        if assertion.has_equality()
            && let Some(key) = key
            && existing_var_type.types.len() == 1
            && existing_var_type.types[0] == *assertion_atomic
            && matches!(
                assertion_atomic,
                TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TTrue
                    | TAtomic::TFalse
                    | TAtomic::TEnumCase { .. }
            )
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

        // Simple-type assertions follow Hakana's `intersect_simple!` model:
        // keep subtypes, short-circuit on supertypes, and report
        // redundancy/impossibility inside (where `did_remove_type` is known).
        match assertion_atomic {
            TAtomic::TScalar => {
                return intersect_simple!(
                    TAtomic::TScalar
                        | TAtomic::TNonEmptyScalar
                        | TAtomic::TInt
                        | TAtomic::TLiteralInt { .. }
                        | TAtomic::TIntRange { .. }
                        | TAtomic::TFloat
                        | TAtomic::TLiteralFloat { .. }
                        | TAtomic::TString
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TNonEmptyString
                        | TAtomic::TNumericString
                        | TAtomic::TNonEmptyNumericString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString
                        | TAtomic::TTruthyString
                        | TAtomic::TClassString { .. }
                        | TAtomic::TLiteralClassString { .. }
                        | TAtomic::TTemplateParamClass { .. }
                        | TAtomic::TDependentGetClass { .. }
                        | TAtomic::TDependentGetType { .. }
                        | TAtomic::TBool
                        | TAtomic::TTrue
                        | TAtomic::TFalse
                        | TAtomic::TArrayKey
                        | TAtomic::TNumeric,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed,
                    TUnion::new(TAtomic::TScalar),
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    analysis_data,
                    analyzer,
                    assertion.has_equality(),
                );
            }
            TAtomic::TBool => {
                let mut result = intersect_simple!(
                    TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TScalar,
                    TUnion::bool(),
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    analysis_data,
                    analyzer,
                    assertion.has_equality(),
                );
                // Psalm's reconcileBool calls `setFromDocblock(false)` on every
                // kept bool atomic — an `is_bool` check verifies the value at
                // runtime, so a later assertion on it reports the plain
                // (non-docblock) issue kind.
                if result.types.len() <= 32 {
                    for index in 0..result.types.len() {
                        if matches!(
                            result.types[index],
                            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse
                        ) {
                            result.set_atomic_from_docblock(index, false);
                        }
                    }
                    result.from_docblock =
                        result.docblock_bits_valid() && result.from_docblock_bits != 0;
                }
                return result;
            }
            TAtomic::TTrue => {
                return intersect_simple!(
                    TAtomic::TTrue,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TScalar | TAtomic::TBool,
                    TUnion::new(TAtomic::TTrue),
                    assertion,
                    existing_var_type,
                    if inside_loop { None } else { key },
                    negated,
                    analysis_data,
                    analyzer,
                    assertion.has_equality(),
                );
            }
            TAtomic::TFalse => {
                return intersect_simple!(
                    TAtomic::TFalse,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TScalar | TAtomic::TBool,
                    TUnion::new(TAtomic::TFalse),
                    assertion,
                    existing_var_type,
                    if inside_loop { None } else { key },
                    negated,
                    analysis_data,
                    analyzer,
                    assertion.has_equality(),
                );
            }
            TAtomic::TFloat => {
                return intersect_simple!(
                    TAtomic::TFloat | TAtomic::TLiteralFloat { .. },
                    TAtomic::TMixed
                        | TAtomic::TNonEmptyMixed
                        | TAtomic::TScalar
                        | TAtomic::TNumeric,
                    TUnion::float(),
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    analysis_data,
                    analyzer,
                    assertion.has_equality(),
                );
            }
            _ => {}
        }

        // Complex assertion types keep the intersection machinery, with the
        // same inner emission: redundant when the intersection removed
        // nothing (structurally), impossible when it is empty.
        if let Some(result) = assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            assertion_atomic,
            analyzer,
        ) {
            // Redundant when the intersection removed nothing — for the
            // assertion kinds where that equivalence is reliable in pzoom:
            // is_resource / is_numeric and instanceof on a concrete class.
            // Skipped for `static` (never certain in an open hierarchy),
            // interfaces (Psalm's `$new_type_has_interface` skip), strings /
            // class-strings / literals (Psalm's per-type reconcilers apply
            // extra conditions pzoom does not model yet).
            let report_redundancy = match assertion_atomic {
                TAtomic::TResource | TAtomic::TNumeric => true,
                // A range assertion that removes nothing is redundant
                // (Psalm's range reconcile: `assert($a < 10)` on int<min, 5>).
                TAtomic::TIntRange { .. } => true,
                // A scalar check that removes nothing is redundant. Psalm's
                // reconcileInt/String/Bool report it whether or not the type is
                // docblock-sourced — the provenance only picks the issue *kind*
                // (RedundantConditionGivenDocblockType vs RedundantCondition) in
                // triggerIssueForImpossible. A loop-narrowed value is exempt
                // (its type is still settling across iterations).
                TAtomic::TInt | TAtomic::TString | TAtomic::TFloat | TAtomic::TBool => {
                    !inside_loop
                }
                TAtomic::TNamedObject {
                    name: StrId::STATIC,
                    ..
                }
                | TAtomic::TNamedObject {
                    is_static: true, ..
                } => false,
                TAtomic::TNamedObject { name, .. } => {
                    analyzer.codebase.get_class(*name).is_some_and(|info| {
                        info.kind != pzoom_code_info::class_like_info::ClassLikeKind::Interface
                    })
                }
                // A repeated `$x === Enum::Case` check is always redundant
                // (enum cases are singletons).
                TAtomic::TEnumCase { .. } => true,
                _ => false,
            };
            if let Some(key) = key
                && !assertion.has_equality()
                && report_redundancy
                && result.types == existing_var_type.types
                && super::should_emit_redundant_issue_for_unchanged_assertion(
                    assertion,
                    existing_var_type,
                    analyzer,
                )
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
            let mut result = with_docblock_from(result, existing_var_type);
            // A runtime type check that narrows to exactly the asserted type
            // verifies the value: Psalm's filterTypeWithAnother substitutes
            // the assertion-derived (non-docblock) atomic, so a second
            // identical check reports plain RedundantCondition. A wider check
            // (e.g. is_object on A|B) keeps the existing atomics and their
            // docblock provenance. Value-equality narrowing (`$x === 2`,
            // enum cases) is exempt: Psalm's handleLiteralEquality keeps the
            // existing atomic and its docblock provenance.
            if matches!(assertion, Assertion::IsType(_))
                && result.types.len() == 1
                && result.types[0] == *assertion_atomic
                && !matches!(
                    assertion_atomic,
                    TAtomic::TLiteralInt { .. }
                        | TAtomic::TLiteralFloat { .. }
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TTrue
                        | TAtomic::TFalse
                        | TAtomic::TEnumCase { .. }
                )
            {
                result.from_docblock = false;
                result.sync_docblock_bits_from_union_flag();
            }
            return result;
        }

        // If intersection is empty, the assertion branch is impossible.
        if let Some(key) = key
            && !existing_var_type.is_nothing()
        {
            super::trigger_issue_for_impossible(
                analysis_data,
                analyzer,
                existing_var_type,
                key,
                assertion,
                false,
                negated,
            );
        }
        return with_docblock_from(TUnion::nothing(), existing_var_type);
    }

    // Handle specific assertions
    match assertion {
        Assertion::Truthy | Assertion::NonEmpty => reconcile_truthy(
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
                // Psalm: `Type::getMixed($inside_loop)` (see getMissingType).
                if inside_loop {
                    return TUnion::new(TAtomic::TMixedFromLoopIsset);
                }
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
        Assertion::HasAtLeastCount(count) => reconcile_has_at_least_count(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            analyzer,
            *count,
        ),
        Assertion::InArray(array_type) => {
            let result = reconcile_in_array(existing_var_type, array_type, analyzer);
            if let Some(key) = key
                && result.is_nothing()
                && !existing_var_type.is_nothing()
                // `in_array($x, [], true)`: never intersects everything
                // (Psalm's intersectUnionTypes returns never, not null), so
                // an empty haystack narrows silently with no contradiction.
                && !array_type.is_nothing()
            {
                super::trigger_issue_for_impossible(
                    analysis_data,
                    analyzer,
                    existing_var_type,
                    key,
                    assertion,
                    false,
                    negated,
                );
            }
            with_docblock_from(result, existing_var_type)
        }
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
                let mut result = with_docblock_from(result, existing_var_type);
                // Runtime-verified when narrowed to exactly the asserted type
                // (see above).
                if result.types.len() == 1 && result.types[0] == *atomic {
                    result.from_docblock = false;
                    result.sync_docblock_bits_from_union_flag();
                }
                return result;
            }
            // An empty intersection is an impossible assertion (Psalm:
            // "string does not contain null" → TypeDoesNotContainNull/Type).
            if key.is_some() && !existing_var_type.is_mixed() {
                super::trigger_issue_for_impossible(
                    analysis_data,
                    analyzer,
                    existing_var_type,
                    key.unwrap_or_default(),
                    assertion,
                    false,
                    negated,
                );
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
    // Keep the per-atomic provenance consistent with the copied union flag —
    // stale bits from before the narrowing would misreport issue kinds.
    new_var_type.sync_docblock_bits_from_union_flag();
    new_var_type
}

fn finalize_reconciliation(
    acceptable_types: Vec<TAtomic>,
    did_remove_type: bool,
    existing_var_type: &TUnion,
    assertion: &Assertion,
    key: Option<&str>,
    negated: bool,
    report_redundant: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    // Suppress the redundant branch where pzoom's `did_remove_type`
    // bookkeeping is not yet Psalm-faithful (isset/array-access paths seed it
    // approximately); impossibility (empty result) always reports.
    let key = if report_redundant || acceptable_types.is_empty() {
        key
    } else {
        None
    };

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        key,
        negated,
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
    key: Option<&str>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    // Psalm's reconcileTruthyOrVerifyTrue: a possibly-undefined type (incl.
    // from a try block whose assignment may not have run) is never a
    // redundant truthy check.
    let mut did_remove_type =
        existing_var_type.possibly_undefined || existing_var_type.possibly_undefined_from_try;

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
                // A truthy string excludes both "" and "0", i.e. truthy-string
                // (Psalm's non-falsy-string), not merely non-empty-string.
                did_remove_type = true;
                acceptable_types.push(TAtomic::TTruthyString);
            }
            TAtomic::TNonEmptyString => {
                // non-empty-string still admits "0" (falsy); narrow to truthy-string.
                did_remove_type = true;
                acceptable_types.push(TAtomic::TTruthyString);
            }
            TAtomic::TNumericString => {
                // Psalm's truthy narrowing only rewrites the exact TString /
                // TNonEmptyString / TLowercaseString classes; numeric-string
                // passes through unchanged (still nominally admits "0").
                did_remove_type = true;
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TLowercaseString => {
                // Removes "" (pzoom has no truthy-lowercase variant; "0" is kept).
                did_remove_type = true;
                acceptable_types.push(TAtomic::TNonEmptyLowercaseString);
            }
            TAtomic::TScalar => {
                // Psalm narrows truthy scalar to non-empty-scalar.
                did_remove_type = true;
                acceptable_types.push(TAtomic::TNonEmptyScalar);
            }
            TAtomic::TInt => {
                // Keep int but note that 0 could be removed in strict mode
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TIntRange { min, max } => {
                // Psalm splits a range containing 0 into its negative and
                // positive halves (int<0, max> truthy → int<1, max>).
                let contains_zero =
                    min.is_none_or(|low| low <= 0) && max.is_none_or(|high| high >= 0);
                if contains_zero {
                    did_remove_type = true;
                    if min.is_none_or(|low| low <= -1) {
                        acceptable_types.push(TAtomic::TIntRange {
                            min: *min,
                            max: Some(-1),
                        });
                    }
                    if max.is_none_or(|high| high >= 1) {
                        acceptable_types.push(TAtomic::TIntRange {
                            min: Some(1),
                            max: *max,
                        });
                    }
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            TAtomic::TFloat => {
                // Keep float but note that 0.0 could be removed in strict mode
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TArray {
                key_type,
                value_type,
            } => {
                // A definitely-empty array is never truthy — drop it (Psalm).
                if value_type.is_nothing() {
                    did_remove_type = true;
                    continue;
                }
                // Narrow to non-empty array
                acceptable_types.push(TAtomic::TNonEmptyArray {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                });
            }
            TAtomic::TList { value_type } => {
                if value_type.is_nothing() {
                    did_remove_type = true;
                    continue;
                }
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
                if properties.is_empty() && *sealed && fallback_value_type.is_none() {
                    did_remove_type = true;
                    continue;
                }

                if atomic.is_truthy() {
                    acceptable_types.push(atomic.clone());
                } else if *is_list
                    && properties
                        .get(&ArrayKey::Int(0))
                        .is_some_and(|first| first.possibly_undefined)
                {
                    // Psalm's `reconcileTruthyOrNonEmpty`: a possibly-empty list
                    // becomes non-empty by making its first element definite.
                    let mut narrowed_properties = (**properties).clone();
                    if let Some(first) = narrowed_properties.get_mut(&ArrayKey::Int(0)) {
                        first.possibly_undefined = false;
                    }
                    acceptable_types.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(narrowed_properties),
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                } else {
                    // Psalm keeps a non-list shape unchanged on a truthy
                    // assertion (its all-optional keys stay optional); the
                    // check simply isn't redundant. Degrading to a generic
                    // non-empty-array would lose the shape's keys.
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

    // Psalm's reconcileTruthyOrNonEmpty: an empty()-derived NonEmpty assertion
    // never reports redundancy/impossibility ("empty is used a lot to check
    // for array offset existence, so we have to silent errors a lot").
    let report_redundant = !matches!(assertion, Assertion::NonEmpty);
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        report_redundant,
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
    key: Option<&str>,
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

    let result = finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
        analysis_data,
        analyzer,
    );

    if result.is_nothing() && (existing_var_type.is_nothing() || possibly_undefined) {
        // Psalm's reconcileIsset returns `Type::getMixed($inside_loop)` here:
        // inside a loop the placeholder keeps its from-loop-isset flavour so
        // the type combiner can evict it once a concrete type is merged in.
        return if inside_loop {
            TUnion::new(TAtomic::TMixedFromLoopIsset)
        } else {
            TUnion::mixed()
        };
    }

    let result = result;
    result
}

/// Reconciles a non-empty countable assertion.
///
/// Narrows arrays and countable types to their non-empty variants.
fn reconcile_non_empty_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&str>,
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
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                if !properties.is_empty() {
                    // A non-empty LIST shape has its first element defined
                    // (Psalm's reconcileNonEmptyCountable list branch).
                    if *is_list
                        && properties
                            .get(&ArrayKey::Int(0))
                            .is_some_and(|first| first.possibly_undefined)
                    {
                        did_remove_type = true;
                        let mut narrowed_properties = (**properties).clone();
                        if let Some(first) = narrowed_properties.get_mut(&ArrayKey::Int(0)) {
                            first.possibly_undefined = false;
                        }
                        acceptable_types.push(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(narrowed_properties),
                            is_list: *is_list,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    } else {
                        acceptable_types.push(atomic.clone());
                    }
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

    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
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
    key: Option<&str>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    count: usize,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                if properties.len() == count {
                    // Psalm's reconcileExactlyCountable: the first `count`
                    // entries of a list are now definitely present.
                    let needs_defining = *is_list
                        && properties
                            .values()
                            .any(|property| property.possibly_undefined);
                    if needs_defining {
                        did_remove_type = true;
                        let mut defined_properties = (**properties).clone();
                        for property in defined_properties.values_mut() {
                            property.possibly_undefined = false;
                        }
                        acceptable_types.push(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(defined_properties),
                            is_list: *is_list,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    } else {
                        acceptable_types.push(atomic.clone());
                    }
                } else if *is_list
                    && properties.len() > count
                    && properties
                        .values()
                        .filter(|property| !property.possibly_undefined)
                        .count()
                        <= count
                {
                    // A sized shape with optional tail entries (list{0: T, 1?: T})
                    // under count === N keeps the first N defined and drops the
                    // rest (Psalm reshapes through min/max count bounds).
                    did_remove_type = true;
                    let mut defined_properties: rustc_hash::FxHashMap<
                        pzoom_code_info::ArrayKey,
                        TUnion,
                    > = rustc_hash::FxHashMap::default();
                    for index in 0..count {
                        if let Some(property) =
                            properties.get(&pzoom_code_info::ArrayKey::Int(index as i64))
                        {
                            let mut property = property.clone();
                            property.possibly_undefined = false;
                            defined_properties
                                .insert(pzoom_code_info::ArrayKey::Int(index as i64), property);
                        }
                    }
                    if defined_properties.len() == count {
                        acceptable_types.push(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(defined_properties),
                            is_list: true,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    }
                } else {
                    did_remove_type = true;
                    // A shape can still have exactly `count` entries when its
                    // required keys fit within the count and either an open
                    // fallback or optional keys can make up (or absent
                    // themselves down to) the difference.
                    let required_count = properties
                        .values()
                        .filter(|property| !property.possibly_undefined)
                        .count();
                    let max_count = if fallback_value_type.is_some() {
                        usize::MAX
                    } else {
                        properties.len()
                    };
                    if required_count <= count && count <= max_count {
                        acceptable_types.push(atomic.clone());
                    }
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
                        properties: std::sync::Arc::new(properties),
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

    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
        analysis_data,
        analyzer,
    )
}

/// Reconciles a `count($x) >= count` assertion (`HasAtLeastCount`).
///
/// Mirrors Psalm's `SimpleAssertionReconciler::reconcileNonEmptyCountable` when the
/// assertion is a `HasAtLeastCount`: narrows arrays to be non-empty and removes
/// sealed shapes that can never reach `count` elements (which the centralized
/// redundant-issue path then reports as a contradiction).
fn reconcile_has_at_least_count(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&str>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    count: usize,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TKeyedArray { .. } => {
                // Mirror Psalm's reconcileNonEmptyCountable: compare the bound against
                // the shape's own getMinCount()/getMaxCount().
                let prop_min_count = atomic.get_min_count().unwrap_or(0);
                let prop_max_count = atomic.get_max_count();

                if prop_max_count.is_some_and(|max_count| max_count < count) {
                    // count($a) >= count is impossible for this sealed shape.
                    did_remove_type = true;
                } else if prop_min_count >= count {
                    // Already guaranteed: redundant, keep the type unchanged.
                    acceptable_types.push(atomic.clone());
                } else if let TAtomic::TKeyedArray {
                    properties,
                    is_list: true,
                    sealed,
                    fallback_key_type,
                    fallback_value_type,
                } = atomic
                {
                    // Psalm's reconcileNonEmptyCountable list branch: entries
                    // 0..count become definite (materialized from the
                    // fallback when absent).
                    did_remove_type = true;
                    let mut new_properties = (**properties).clone();
                    let mut complete = true;
                    for index in 0..count {
                        let key = pzoom_code_info::t_atomic::ArrayKey::Int(index as i64);
                        match new_properties.get_mut(&key) {
                            Some(entry) => entry.possibly_undefined = false,
                            None => match fallback_value_type {
                                Some(fallback) => {
                                    new_properties.insert(key, (**fallback).clone());
                                }
                                None => {
                                    complete = false;
                                    break;
                                }
                            },
                        }
                    }
                    if complete {
                        acceptable_types.push(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(new_properties),
                            is_list: true,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    } else {
                        acceptable_types.push(atomic.clone());
                    }
                } else {
                    // Possible: keep the shape (conservatively unchanged).
                    did_remove_type = true;
                    acceptable_types.push(atomic.clone());
                }
            }
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
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;
                acceptable_types.push(TAtomic::TNonEmptyMixed);
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                did_remove_type = true;
                if !as_type.is_mixed() {
                    let narrowed = reconcile_has_at_least_count(
                        assertion,
                        as_type,
                        None,
                        false,
                        analysis_data,
                        analyzer,
                        count,
                    );
                    push_narrowed_template_type(&mut acceptable_types, atomic, narrowed);
                } else {
                    acceptable_types.push(atomic.clone());
                }
            }
            _ => {
                did_remove_type = true;
                acceptable_types.push(atomic.clone());
            }
        }
    }

    // When nothing was narrowed the assertion is redundant; report it here
    // (Psalm's reconcileNonEmptyCountable `$prop_min_count >= $count` branch)
    // and return the type verbatim, preserving data-flow nodes.
    if !did_remove_type {
        // Report only when provably redundant (Psalm's reconcileNonEmptyCountable
        // `$prop_min_count >= $count`), not merely un-narrowed.
        if let Some(key) = key
            && super::should_emit_redundant_issue_for_unchanged_assertion(
                assertion,
                existing_var_type,
                analyzer,
            )
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
        return existing_var_type.clone();
    }
    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
        analysis_data,
        analyzer,
    )
}

/// Reconciles an in_array assertion.
///
/// Narrows the type to values that could be in the array.
fn reconcile_in_array(
    existing_var_type: &TUnion,
    assertion_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let Some(possible_union) = normalize_in_array_assertion_union(assertion_type) else {
        return existing_var_type.clone();
    };

    if let Some(intersection) = assertion_reconciler::intersect_union_with_union_with_codebase(
        existing_var_type,
        &possible_union,
        Some(analyzer.codebase),
    ) {
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
    key: Option<&str>,
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
                    let mut new_properties = (**properties).clone();
                    new_properties.insert(array_key.clone(), (**fallback).clone());
                    result_types.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(new_properties),
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
                    properties: std::sync::Arc::new(properties),
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
                    properties: std::sync::Arc::new(properties),
                    is_list: matches!(array_key, ArrayKey::Int(0)),
                    sealed: false,
                    fallback_key_type: Some(Box::new(TUnion::int())),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                did_remove_type = true;

                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(array_key.clone(), TUnion::new(TAtomic::TNonEmptyMixed));

                result_types.push(TAtomic::TKeyedArray {
                    properties: std::sync::Arc::new(properties),
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

    finalize_reconciliation(
        result_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
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
    key: Option<&str>,
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
                    let mut new_properties = (**properties).clone();
                    new_properties.insert(array_key.clone(), narrowed);
                    result_types.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(new_properties),
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
                    let mut new_properties = (**properties).clone();
                    new_properties.insert(array_key.clone(), narrowed);
                    result_types.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(new_properties),
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
                    properties: std::sync::Arc::new(properties),
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
                    properties: std::sync::Arc::new(properties),
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

    finalize_reconciliation(
        result_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
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
    key: Option<&str>,
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
                name: StrId::ARRAY_ACCESS,
                type_params: None,
                is_static: false,
                remapped_params: false,
            },
        ]);
        reconciled_type.from_docblock = existing_var_type.from_docblock;
        return reconciled_type;
    }

    let mut acceptable_types = Vec::new();
    let mut narrowed_to_non_empty = false;

    for atomic in &existing_var_type.types {
        if can_be_array_accessed(atomic, allow_int_key) {
            // Array access (`$a[...]`) implies the array is non-empty, so narrow a
            // generic array/list to its non-empty form. Matches Psalm.
            match atomic {
                TAtomic::TArray {
                    key_type,
                    value_type,
                } => {
                    acceptable_types.push(TAtomic::TNonEmptyArray {
                        key_type: key_type.clone(),
                        value_type: value_type.clone(),
                    });
                    narrowed_to_non_empty = true;
                }
                TAtomic::TList { value_type } => {
                    acceptable_types.push(TAtomic::TNonEmptyList {
                        value_type: value_type.clone(),
                    });
                    narrowed_to_non_empty = true;
                }
                _ => acceptable_types.push(atomic.clone()),
            }
        }
    }

    let did_remove_type =
        narrowed_to_non_empty || acceptable_types.len() != existing_var_type.types.len();

    finalize_reconciliation(
        acceptable_types,
        did_remove_type,
        existing_var_type,
        assertion,
        key,
        negated,
        false,
        analysis_data,
        analyzer,
    )
}

/// Checks if two types might match (for in_array checks).
fn types_might_match(a: &TAtomic, b: &TAtomic) -> bool {
    match (a, b) {
        (
            TAtomic::TInt,
            TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. },
        ) => true,
        (TAtomic::TLiteralInt { .. }, TAtomic::TInt | TAtomic::TIntRange { .. }) => true,
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
        (TAtomic::TLiteralInt { .. }, TAtomic::TInt | TAtomic::TIntRange { .. }) => true,
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
        | TAtomic::TKeyedArray { .. }
        | TAtomic::TClassStringMap { .. } => true,

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
