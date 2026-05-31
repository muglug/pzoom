//! Type reconciler module.
//!
//! This module provides type narrowing based on assertions from conditional branches.
//! For example, after `if ($x instanceof Foo)`, we know `$x` is of type `Foo`.

pub mod assertion_reconciler;
mod negated_assertion_reconciler;
mod simple_assertion_reconciler;
mod simple_negated_assertion_reconciler;

use std::collections::BTreeMap;

use pzoom_code_info::{ArrayKey, Assertion, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reconciles a type based on an assertion.
///
/// This is the main entry point for type narrowing. Given an existing type and an
/// assertion, it returns the narrowed type.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    assertion_reconciler::reconcile(
        assertion,
        Some(existing_var_type),
        false,
        None,
        analyzer,
        analysis_data,
        false,
        false,
    )
}

/// Reconciles keyed types based on a map of assertions.
///
/// This processes assertions for multiple variables and updates the context accordingly.
/// Convert a flat `var -> [Assertion]` map (a conjunction of assertions per
/// variable) into the AND-of-OR-groups shape consumed by [`reconcile_keyed_types`].
/// Each assertion becomes its own singleton (one-element) clause.
pub fn to_and_groups(
    assertions: &BTreeMap<String, Vec<Assertion>>,
) -> BTreeMap<String, Vec<Vec<Assertion>>> {
    assertions
        .iter()
        .map(|(key, list)| (key.clone(), list.iter().map(|a| vec![a.clone()]).collect()))
        .collect()
}

/// Reconcile a map of per-variable assertions against the context, narrowing each
/// variable's type. Mirrors Psalm's `Reconciler::reconcileKeyedTypes` /
/// Hakana's `reconcile_keyed_types`: the value for each variable is a list of
/// AND-ed clauses, and each clause is itself a list of OR-ed assertions (a
/// disjunction is reconciled per-alternative and unioned).
#[allow(clippy::too_many_arguments)]
pub fn reconcile_keyed_types(
    assertions: &BTreeMap<String, Vec<Vec<Assertion>>>,
    context: &mut BlockContext,
    changed_var_ids: &mut FxHashSet<StrId>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
    negated: bool,
    emit_redundant_issues: bool,
    active_assertion_offsets: Option<&BTreeMap<String, FxHashSet<usize>>>,
) {
    if assertions.is_empty() {
        return;
    }

    // Flatten groups for nested-isset augmentation and flag detection.
    let mut flat: BTreeMap<String, Vec<Assertion>> = assertions
        .iter()
        .map(|(key, groups)| (key.clone(), groups.iter().flatten().cloned().collect()))
        .collect();
    add_nested_assertions(&mut flat, context, analyzer);

    // Rebuild the nested working set, appending any assertions that
    // add_nested_assertions introduced as singleton (AND) groups.
    let mut new_assertions: BTreeMap<String, Vec<Vec<Assertion>>> = assertions.clone();
    for (key, flat_assertions) in &flat {
        let original_flat_len = assertions
            .get(key)
            .map(|groups| groups.iter().map(|g| g.len()).sum::<usize>())
            .unwrap_or(0);
        let groups = new_assertions.entry(key.clone()).or_default();
        for assertion in flat_assertions.iter().skip(original_flat_len) {
            groups.push(vec![assertion.clone()]);
        }
    }

    for (var_name, var_assertions) in &new_assertions {
        // Skip class constant assertions for now
        if var_name.contains("::") && !var_name.contains('$') && !var_name.contains('[') {
            continue;
        }

        // Determine assertion characteristics
        let has_isset = var_assertions.iter().flatten().any(|a| a.has_isset());
        let has_inverted_isset = var_assertions
            .iter()
            .flatten()
            .any(|a| matches!(a, Assertion::IsNotIsset));
        let has_falsyish = var_assertions
            .iter()
            .flatten()
            .any(|a| matches!(a, Assertion::Falsy));
        let has_positive_non_isset_assertion = var_assertions.iter().flatten().any(|assertion| {
            matches!(
                assertion,
                Assertion::IsType(_)
                    | Assertion::IsNotType(_)
                    | Assertion::IsEqual(_)
                    | Assertion::IsNotEqual(_)
                    | Assertion::Truthy
                    | Assertion::InArray(_)
                    | Assertion::NotInArray(_)
                    | Assertion::HasStringArrayAccess
                    | Assertion::HasIntOrStringArrayAccess
                    | Assertion::HasArrayKey(_)
                    | Assertion::HasNonnullEntryForKey(_)
                    | Assertion::NonEmptyCountable(_)
                    | Assertion::HasExactCount(_)
            )
        });

        // Get the current type for this variable
        let var_id = analyzer.interner.intern(var_name);
        let alt_var_id = get_alternate_var_id(analyzer, var_name);
        let mut possibly_undefined = false;

        let existing_type = if let Some(t) = context.locals.get(&var_id) {
            Some(t.clone())
        } else if let Some(alt_var_id) = alt_var_id {
            context.locals.get(&alt_var_id).cloned()
        } else if var_name.contains('[') || var_name.contains("->") {
            // Try to get value for nested key
            get_value_for_key(
                var_name,
                context,
                analyzer,
                has_isset,
                has_inverted_isset,
                inside_loop,
                &mut possibly_undefined,
            )
        } else {
            None
        };

        let mut current_type = existing_type.unwrap_or_else(|| {
            if has_isset || has_inverted_isset {
                TUnion::mixed()
            } else {
                TUnion::mixed()
            }
        });

        let type_before = current_type.clone();
        let mut had_active_assertion = false;

        // Apply each assertion clause in sequence (conjunction). A clause with one
        // assertion is a plain narrowing; a clause with several is a disjunction,
        // reconciled per-alternative against the pre-clause type and unioned. The
        // clause index is the unit used for active-assertion offsets (matching
        // get_truths_from_formula and the singleton-group flat wrapper).
        for (clause_index, assertion_group) in var_assertions.iter().enumerate() {
            let is_active_assertion = active_assertion_offsets
                .and_then(|offsets_by_var| offsets_by_var.get(var_name))
                .is_some_and(|offsets| offsets.contains(&clause_index));

            if assertion_group.len() == 1 {
                let assertion = &assertion_group[0];
                let type_before_assertion = current_type.clone();
                current_type = assertion_reconciler::reconcile(
                    assertion,
                    Some(&current_type),
                    possibly_undefined,
                    Some(var_name),
                    analyzer,
                    analysis_data,
                    inside_loop,
                    negated,
                );

                // Psalm clears from_docblock only inside specific reconciles (plain
                // TString/TInt/TBool), never for a truthy/falsy narrowing — so e.g.
                // `if ($a && $a instanceof A)` on a `?static` docblock keeps the
                // docblock flag and reports RedundantConditionGivenDocblockType.
                had_active_assertion |= is_active_assertion
                    && !assertion.has_isset()
                    && !matches!(assertion, Assertion::Truthy | Assertion::Falsy)
                    && current_type != type_before_assertion;

                if emit_redundant_issues && is_active_assertion && !type_before_assertion.is_mixed() {
                    if current_type.is_nothing() {
                        trigger_issue_for_impossible(
                            analysis_data,
                            analyzer,
                            &type_before_assertion,
                            var_name,
                            assertion,
                            false,
                            negated,
                        );
                    } else if current_type == type_before_assertion
                        && should_emit_redundant_issue_for_unchanged_assertion(
                            assertion,
                            &type_before_assertion,
                            analyzer,
                        )
                    {
                        trigger_issue_for_impossible(
                            analysis_data,
                            analyzer,
                            &type_before_assertion,
                            var_name,
                            assertion,
                            true,
                            negated,
                        );
                    }
                }
            } else {
                // Disjunction: union of reconciling each alternative against the
                // type as it was before this clause.
                let base_type = current_type.clone();
                let mut result: Option<TUnion> = None;
                for assertion in assertion_group {
                    let narrowed = assertion_reconciler::reconcile(
                        assertion,
                        Some(&base_type),
                        possibly_undefined,
                        Some(var_name),
                        analyzer,
                        analysis_data,
                        inside_loop,
                        negated,
                    );
                    result = Some(match result {
                        None => narrowed,
                        Some(existing) => combine_union_types(&existing, &narrowed, false),
                    });
                }
                if let Some(result) = result {
                    had_active_assertion |= is_active_assertion && result != current_type;
                    current_type = result;
                }
            }
        }

        if had_active_assertion {
            current_type.from_docblock = false;
        }

        let is_nested_key = var_name.contains('[') || var_name.contains("->");
        if is_nested_key {
            if has_inverted_isset {
                current_type.possibly_undefined = true;
            } else if has_isset {
                current_type.possibly_undefined = false;
            } else if has_positive_non_isset_assertion {
                current_type.possibly_undefined = false;
            } else if possibly_undefined {
                current_type.possibly_undefined = true;
            }
        }

        // Check if type changed
        let type_changed = current_type != type_before;

        // Propagate a reconciled nested key back into its base array. Psalm's
        // `reconcileKeyedTypes` does this for any `…]` key that isn't an
        // inverted-isset / empty / equality assertion — notably *without* gating on
        // whether the path-local changed, so a base array stays in sync even when
        // the leaf type-local was already narrowed (e.g. `is_int($arr['foo'])`).
        if var_name.ends_with(']') && !has_inverted_isset && !has_falsyish {
            let key_parts = break_up_path_into_parts(var_name);
            adjust_tkeyed_array_type(key_parts, context, changed_var_ids, &current_type, analyzer);
        }

        if type_changed {
            changed_var_ids.insert(var_id);
            if let Some(alt_var_id) = alt_var_id {
                changed_var_ids.insert(alt_var_id);
            }
        }

        // Update the context with the narrowed type.
        // For plain variables, keep reference clusters in sync without marking
        // this narrowing as a concrete assignment.
        if !is_nested_key && !var_name.contains("::") {
            context.set_var_type_for_inference(var_id, current_type.clone());
        } else {
            context.locals.insert(var_id, current_type.clone());
        }
        if let Some(alt_var_id) = alt_var_id {
            context.locals.insert(alt_var_id, current_type.clone());
        }
    }
}

fn get_alternate_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    if var_name.contains('[') || var_name.contains("->") {
        return None;
    }

    if let Some(stripped) = var_name.strip_prefix('$') {
        analyzer.interner.find(stripped)
    } else {
        analyzer.interner.find(&format!("${}", var_name))
    }
}

fn should_emit_redundant_issue_for_unchanged_assertion(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    match assertion {
        Assertion::Truthy => existing_var_type.is_always_truthy(),
        Assertion::Falsy => existing_var_type.is_always_falsy(),
        Assertion::IsType(TAtomic::TInt)
            if existing_var_type.from_calculation && existing_var_type.has_int() =>
        {
            false
        }
        // Mirror Psalm's `SimpleAssertionReconciler::reconcileNumeric`: a numeric
        // check is only redundant when every member is already a pure int/float
        // value type. Any string component (including a literal numeric string such
        // as `"5"`), `numeric-string`, `numeric`, `scalar`, or `array-key` keeps the
        // check non-redundant, because the runtime check still discriminates.
        Assertion::IsType(TAtomic::TNumeric) => {
            !existing_var_type.types.is_empty()
                && existing_var_type.types.iter().all(|atomic| {
                    matches!(
                        atomic,
                        TAtomic::TInt
                            | TAtomic::TFloat
                            | TAtomic::TLiteralInt { .. }
                            | TAtomic::TLiteralFloat { .. }
                            | TAtomic::TIntRange { .. }
                    )
                })
        }
        Assertion::IsType(asserted_atomic) => assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            asserted_atomic,
            analyzer,
        )
        .is_some_and(|intersection| intersection == *existing_var_type),
        Assertion::IsNotType(asserted_atomic) => assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            asserted_atomic,
            analyzer,
        )
        .is_none(),
        Assertion::IsEqual(asserted_atomic) => {
            existing_var_type.types.len() == 1
                && existing_var_type
                    .types
                    .first()
                    .is_some_and(|existing_atomic| existing_atomic == asserted_atomic)
        }
        Assertion::IsNotEqual(asserted_atomic) => assertion_reconciler::intersect_union_with_atomic(
            existing_var_type,
            asserted_atomic,
            analyzer,
        )
        .is_none(),
        // Psalm's reconcileArrayKeyExists only does setPossiblyUndefined(false) and
        // never calls triggerIssueForImpossible, so array_key_exists() is never
        // reported as redundant/impossible.
        Assertion::ArrayKeyExists => false,
        // `count($x) >= n` is redundant when the value is provably already at least
        // that long (Psalm's reconcileNonEmptyCountable `$prop_min_count >= $count`).
        Assertion::HasAtLeastCount(count) => get_union_count_bounds(existing_var_type)
            .is_some_and(|(min_count, _)| min_count >= *count),
        // `count($x) < n` is redundant when the value is provably always shorter
        // (Psalm's reconcileNotNonEmptyCountable `$prop_max_count < $count`).
        Assertion::DoesNotHaveAtLeastCount(count) => get_union_count_bounds(existing_var_type)
            .is_some_and(|(_, max_count)| max_count.is_some_and(|max| max < *count)),
        Assertion::InArray(_) => false,
        Assertion::NotInArray(assertion_type) => {
            not_in_array_is_provably_redundant(existing_var_type, assertion_type)
        }
        _ => false,
    }
}

/// Computes the `(min_count, Option<max_count>)` bounds of a union's array
/// members, mirroring Psalm's `TKeyedArray::getMinCount()`/`getMaxCount()`.
///
/// Returns `None` when the union contains a non-countable atomic (so the count is
/// unknown), or when there are no atomics. `max_count` is `None` when an unsealed
/// or unbounded array is present.
pub(crate) fn get_union_count_bounds(union: &TUnion) -> Option<(usize, Option<usize>)> {
    if union.types.is_empty() {
        return None;
    }

    let mut min_count = usize::MAX;
    let mut max_count = Some(0usize);

    for atomic in &union.types {
        let (atomic_min, atomic_max) = match atomic {
            TAtomic::TArray { .. } | TAtomic::TList { .. } => (0, None),
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => (1, None),
            // Defer to the keyed-array bounds defined on the type itself, mirroring
            // Psalm's TKeyedArray::getMinCount()/getMaxCount().
            TAtomic::TKeyedArray { .. } => {
                (atomic.get_min_count().unwrap_or(0), atomic.get_max_count())
            }
            _ => return None,
        };

        min_count = min_count.min(atomic_min);
        max_count = match (max_count, atomic_max) {
            (Some(existing_max), Some(next_max)) => Some(existing_max.max(next_max)),
            _ => None,
        };
    }

    if min_count == usize::MAX {
        None
    } else {
        Some((min_count, max_count))
    }
}

fn not_in_array_is_provably_redundant(
    existing_var_type: &TUnion,
    assertion_type: &TUnion,
) -> bool {
    let Some(assertion_value_union) = normalize_in_array_assertion_union(assertion_type) else {
        return false;
    };

    assertion_reconciler::intersect_union_with_union(existing_var_type, &assertion_value_union)
        .is_none()
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

/// Breaks up a key path like `$a['foo']->bar` into parts.
fn break_up_path_into_parts(path: &str) -> Vec<String> {
    let chars: Vec<char> = path.chars().collect();
    let mut string_char: Option<char> = None;
    let mut escape_char = false;
    let mut brackets = 0;
    let mut parts = BTreeMap::new();
    parts.insert(0, String::new());
    let mut parts_offset = 0;
    let mut i = 0;
    let char_count = chars.len();

    while i < char_count {
        let ichar = chars[i];

        if let Some(string_char_inner) = string_char {
            if ichar == string_char_inner && !escape_char {
                string_char = None;
            }

            if ichar == '\\' {
                escape_char = !escape_char;
            }

            parts.entry(parts_offset).or_default().push(ichar);
            i += 1;
            continue;
        }

        match ichar {
            '[' | ']' => {
                parts_offset += 1;
                parts.insert(parts_offset, ichar.to_string());
                parts_offset += 1;
                brackets += if ichar == '[' { 1 } else { -1 };
                i += 1;
                continue;
            }
            '\'' | '"' => {
                parts.entry(parts_offset).or_default().push(ichar);
                string_char = Some(ichar);
                i += 1;
                continue;
            }
            ':' => {
                if brackets == 0 && i < char_count - 2 && chars[i + 1] == ':' && chars[i + 2] == '$'
                {
                    parts_offset += 1;
                    parts.insert(parts_offset, "::$".to_string());
                    parts_offset += 1;
                    i += 3;
                    continue;
                }
            }
            '-' => {
                if brackets == 0 && i < char_count - 1 && chars[i + 1] == '>' {
                    parts_offset += 1;
                    parts.insert(parts_offset, "->".to_string());
                    parts_offset += 1;
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }

        parts.entry(parts_offset).or_default().push(ichar);
        i += 1;
    }

    parts.into_values().collect()
}

/// Gets the value type for a nested key path.
/// Resolve the current type for a variable/property-path key (e.g. `$this->foo`),
/// falling back to the declared property/static type when the key isn't yet a
/// local. Thin public wrapper over [`get_value_for_key`] for callers (such as
/// method-call assertion application) that need to seed a property path before
/// narrowing it. Mirrors the resolution Psalm performs in `Reconciler::reconcile`.
pub(crate) fn resolve_key_type(
    key: &str,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let mut possibly_undefined = false;
    get_value_for_key(key, context, analyzer, false, false, false, &mut possibly_undefined)
}

fn get_value_for_key(
    key: &str,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    has_isset: bool,
    has_inverted_isset: bool,
    _inside_loop: bool,
    possibly_undefined: &mut bool,
) -> Option<TUnion> {
    if key.ends_with(']')
        && key.matches('[').count() > 1
        && let Some((base_key, array_key)) = split_last_array_access(key)
    {
        let base_type = get_value_for_key(
            &base_key,
            context,
            analyzer,
            has_isset,
            has_inverted_isset,
            false,
            possibly_undefined,
        )?;

        if let Some(resolved_type) = apply_array_access_to_base_type(
            &base_type,
            &array_key,
            context,
            analyzer,
            has_isset,
            has_inverted_isset,
            possibly_undefined,
        ) {
            return Some(resolved_type);
        }
    }

    let mut key_parts = break_up_path_into_parts(key);

    if key_parts.len() == 1 {
        let var_id = analyzer
            .interner
            .find(key)
            .or_else(|| get_alternate_var_id(analyzer, key))?;
        return context.locals.get(&var_id).cloned();
    }

    key_parts.reverse();

    let base_key = key_parts.pop()?;
    let mut base_type = if let Some(base_var_id) = analyzer
        .interner
        .find(&base_key)
        .or_else(|| get_alternate_var_id(analyzer, &base_key))
    {
        context.locals.get(&base_var_id).cloned()
    } else {
        None
    }
    .or_else(|| resolve_class_constant_type_from_key(&base_key, analyzer))
    .or_else(|| resolve_static_property_type_from_key(&base_key, analyzer))?;

    while let Some(divider) = key_parts.pop() {
        if divider == "[" {
            let array_key = key_parts.pop()?;
            key_parts.pop(); // Pop the closing "]"
            base_type = apply_array_access_to_base_type(
                &base_type,
                &array_key,
                context,
                analyzer,
                has_isset,
                has_inverted_isset,
                possibly_undefined,
            )?;
        } else if divider == "->" {
            let property_name = key_parts.pop()?;
            let property_id = analyzer.interner.intern(&property_name);
            let mut new_type: Option<TUnion> = None;

            for atomic in &base_type.types {
                let candidate_type = match atomic {
                    TAtomic::TNamedObject { name, .. } => analyzer
                        .codebase
                        .get_class(*name)
                        .and_then(|class_info| class_info.properties.get(&property_id))
                        .map(|property_info| {
                            property_info
                                .get_type()
                                .cloned()
                                .unwrap_or_else(TUnion::mixed)
                        }),
                    TAtomic::TObject => Some(TUnion::mixed()),
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed => Some(TUnion::mixed()),
                    _ => None,
                };

                if let Some(mut t) = candidate_type {
                    if base_type.from_docblock {
                        t.from_docblock = true;
                    }

                    new_type = Some(if let Some(existing) = new_type {
                        let mut combined = combine_union_types(&existing, &t, false);
                        combined.from_docblock = existing.from_docblock || t.from_docblock;
                        combined
                    } else {
                        t
                    });
                }
            }

            base_type = new_type?;
        } else {
            break;
        }
    }

    Some(base_type)
}

fn apply_array_access_to_base_type(
    base_type: &TUnion,
    array_key: &str,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    has_isset: bool,
    has_inverted_isset: bool,
    possibly_undefined: &mut bool,
) -> Option<TUnion> {
    let mut new_type: Option<TUnion> = None;

    for atomic in &base_type.types {
        let candidate_type = match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                fallback_value_type,
                ..
            } => {
                if let Some(dict_key) = if array_key.starts_with('\'') || array_key.starts_with('"') {
                    let key_str = array_key[1..array_key.len() - 1].to_string();
                    Some(ArrayKey::String(key_str))
                } else if let Ok(int_key) = array_key.parse::<i64>() {
                    Some(ArrayKey::Int(int_key))
                } else {
                    None
                } {
                    if let Some(prop_type) = lookup_property_type_by_runtime_key(properties, &dict_key)
                    {
                        Some(prop_type.clone())
                    } else if let Some(fallback) = fallback_value_type {
                        *possibly_undefined = true;
                        Some((**fallback).clone())
                    } else if has_isset {
                        *possibly_undefined = true;
                        Some(TUnion::mixed())
                    } else {
                        None
                    }
                } else if let Some((resolved_literal_type, used_literal_keys)) =
                    resolve_keyed_array_value_for_variable_key(
                        array_key,
                        properties,
                        fallback_value_type.as_deref(),
                        context,
                        analyzer,
                        possibly_undefined,
                    )
                {
                    if used_literal_keys {
                        resolved_literal_type
                    } else {
                        *possibly_undefined = true;
                        if let Some(fallback) = fallback_value_type {
                            Some((**fallback).clone())
                        } else if (*is_list
                            || properties.keys().all(|key| matches!(key, ArrayKey::Int(_))))
                            && !properties.is_empty()
                        {
                            let mut combined = Vec::new();
                            for prop_type in properties.values() {
                                combined.extend(prop_type.types.clone());
                            }
                            Some(TUnion::from_types(combined))
                        } else if !properties.is_empty() || has_isset {
                            Some(TUnion::mixed())
                        } else {
                            None
                        }
                    }
                } else {
                    *possibly_undefined = true;
                    if let Some(fallback) = fallback_value_type {
                        Some((**fallback).clone())
                    } else if (*is_list
                        || properties.keys().all(|key| matches!(key, ArrayKey::Int(_))))
                        && !properties.is_empty()
                    {
                        let mut combined = Vec::new();
                        for prop_type in properties.values() {
                            combined.extend(prop_type.types.clone());
                        }
                        Some(TUnion::from_types(combined))
                    } else if !properties.is_empty() || has_isset {
                        Some(TUnion::mixed())
                    } else {
                        None
                    }
                }
            }
            TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                *possibly_undefined = true;
                Some((**value_type).clone())
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                *possibly_undefined = true;
                Some((**value_type).clone())
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => Some(TUnion::mixed()),
            TAtomic::TString | TAtomic::TNonEmptyString | TAtomic::TLiteralString { .. } => {
                Some(TUnion::string())
            }
            _ => {
                if has_isset || has_inverted_isset {
                    *possibly_undefined = true;
                    Some(TUnion::mixed())
                } else {
                    None
                }
            }
        };

        if let Some(mut t) = candidate_type {
            if base_type.from_docblock {
                t.from_docblock = true;
            }

            new_type = Some(if let Some(existing) = new_type {
                let mut combined = combine_union_types(&existing, &t, false);
                combined.from_docblock = existing.from_docblock || t.from_docblock;
                combined
            } else {
                t
            });
        }
    }

    new_type
}

fn resolve_class_constant_type_from_key(
    key: &str,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let (class_name, constant_name) = key.split_once("::")?;
    let class_id = resolve_class_id_from_key(class_name, analyzer)?;
    let const_id = analyzer.interner.intern(constant_name);

    find_class_constant_in_hierarchy(analyzer, class_id, const_id, &mut FxHashSet::default())
}

fn resolve_static_property_type_from_key(
    key: &str,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    let (class_name, property_name) = key.split_once("::$")?;
    let class_id = resolve_class_id_from_key(class_name, analyzer)?;
    let property_id = analyzer.interner.intern(property_name);

    find_static_property_in_hierarchy(analyzer, class_id, property_id, &mut FxHashSet::default())
}

fn resolve_class_id_from_key(class_name: &str, analyzer: &StatementsAnalyzer<'_>) -> Option<StrId> {
    let normalized = class_name.trim_start_matches('\\');

    if normalized.eq_ignore_ascii_case("self") || normalized.eq_ignore_ascii_case("static") {
        return analyzer.get_declaring_class();
    }

    if normalized.eq_ignore_ascii_case("parent") {
        return analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        });
    }

    if let Some(class_id) = analyzer
        .interner
        .find(class_name)
        .or_else(|| analyzer.interner.find(normalized))
        .or_else(|| analyzer.interner.find(&format!("\\{}", normalized)))
    {
        if analyzer.codebase.get_class(class_id).is_some() {
            return Some(class_id);
        }
    }

    let mut matched_class: Option<StrId> = None;

    for class_id in analyzer.codebase.classlike_infos.keys() {
        let fq_class_name = analyzer.interner.lookup(*class_id);
        let normalized_fq = fq_class_name.trim_start_matches('\\');
        let short_name = normalized_fq.rsplit('\\').next().unwrap_or(normalized_fq);

        if normalized_fq.eq_ignore_ascii_case(normalized) || short_name.eq_ignore_ascii_case(normalized)
        {
            if matched_class.is_some_and(|existing| existing != *class_id) {
                return None;
            }
            matched_class = Some(*class_id);
        }
    }

    matched_class
}

fn find_class_constant_in_hierarchy(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    const_id: StrId,
    seen_classes: &mut FxHashSet<StrId>,
) -> Option<TUnion> {
    if !seen_classes.insert(class_id) {
        return None;
    }

    let class_info = analyzer.codebase.get_class(class_id)?;

    if let Some(const_info) = class_info.constants.get(&const_id) {
        return Some(const_info.constant_type.clone());
    }

    if let Some(parent_class) = class_info.parent_class {
        if let Some(parent_const_type) =
            find_class_constant_in_hierarchy(analyzer, parent_class, const_id, seen_classes)
        {
            return Some(parent_const_type);
        }
    }

    for interface_id in &class_info.interfaces {
        if let Some(interface_const_type) =
            find_class_constant_in_hierarchy(analyzer, *interface_id, const_id, seen_classes)
        {
            return Some(interface_const_type);
        }
    }

    None
}

fn find_static_property_in_hierarchy(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    property_id: StrId,
    seen_classes: &mut FxHashSet<StrId>,
) -> Option<TUnion> {
    if !seen_classes.insert(class_id) {
        return None;
    }

    let class_info = analyzer.codebase.get_class(class_id)?;

    if let Some(property_info) = class_info.properties.get(&property_id) {
        if property_info.is_static {
            return Some(
                property_info
                    .get_type()
                    .cloned()
                    .unwrap_or_else(TUnion::mixed),
            );
        }
    }

    if let Some(parent_class) = class_info.parent_class {
        if let Some(parent_property_type) = find_static_property_in_hierarchy(
            analyzer,
            parent_class,
            property_id,
            seen_classes,
        ) {
            return Some(parent_property_type);
        }
    }

    None
}

fn resolve_keyed_array_value_for_variable_key(
    array_key_var: &str,
    properties: &FxHashMap<ArrayKey, TUnion>,
    fallback_value_type: Option<&TUnion>,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    possibly_undefined: &mut bool,
) -> Option<(Option<TUnion>, bool)> {
    let Some(var_type) = resolve_variable_key_type(
        array_key_var,
        context,
        analyzer,
        possibly_undefined,
    ) else {
        return None;
    };

    let literal_keys = extract_literal_array_keys_from_union(&var_type);

    if literal_keys.is_empty() {
        return Some((None, false));
    }

    let mut resolved: Option<TUnion> = None;
    let mut saw_missing = false;
    let mut processed_keys: Vec<ArrayKey> = Vec::new();

    for key in literal_keys {
        if processed_keys
            .iter()
            .any(|processed_key| array_keys_are_equivalent(processed_key, &key))
        {
            continue;
        }
        processed_keys.push(key.clone());

        if let Some(property_type) = lookup_property_type_by_runtime_key(properties, &key) {
            resolved = Some(match resolved {
                Some(existing) => combine_union_types(&existing, property_type, false),
                None => property_type.clone(),
            });
        } else if let Some(fallback_type) = fallback_value_type {
            *possibly_undefined = true;
            resolved = Some(match resolved {
                Some(existing) => combine_union_types(&existing, fallback_type, false),
                None => fallback_type.clone(),
            });
        } else {
            saw_missing = true;
        }
    }

    if saw_missing {
        *possibly_undefined = true;
    }

    Some((resolved, true))
}

fn resolve_variable_key_type(
    array_key_var: &str,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    possibly_undefined: &mut bool,
) -> Option<TUnion> {
    if let Some(var_id) = analyzer
        .interner
        .find(array_key_var)
        .or_else(|| get_alternate_var_id(analyzer, array_key_var))
        && let Some(var_type) = context.locals.get(&var_id)
    {
        return Some(var_type.clone());
    }

    if array_key_var.contains('[') || array_key_var.contains("->") {
        return get_value_for_key(
            array_key_var,
            context,
            analyzer,
            false,
            false,
            false,
            possibly_undefined,
        );
    }

    None
}

fn extract_literal_array_keys_from_union(var_type: &TUnion) -> Vec<ArrayKey> {
    let mut literal_keys = Vec::new();

    for atomic in &var_type.types {
        match atomic {
            TAtomic::TLiteralInt { value } => {
                let int_key = ArrayKey::Int(*value);
                if !literal_keys.contains(&int_key) {
                    literal_keys.push(int_key);
                }

                let str_key = ArrayKey::String(value.to_string());
                if !literal_keys.contains(&str_key) {
                    literal_keys.push(str_key);
                }
            }
            TAtomic::TLiteralString { value } => {
                let str_key = ArrayKey::String(value.clone());
                if !literal_keys.contains(&str_key) {
                    literal_keys.push(str_key);
                }

                if let Some(int_value) = parse_canonical_int_string(value) {
                    let int_key = ArrayKey::Int(int_value);
                    if !literal_keys.contains(&int_key) {
                        literal_keys.push(int_key);
                    }
                }
            }
            _ => {}
        }
    }

    literal_keys
}

fn lookup_property_type_by_runtime_key<'a>(
    properties: &'a FxHashMap<ArrayKey, TUnion>,
    key: &ArrayKey,
) -> Option<&'a TUnion> {
    if let Some(property_type) = properties.get(key) {
        return Some(property_type);
    }

    match key {
        ArrayKey::Int(value) => properties.get(&ArrayKey::String(value.to_string())),
        ArrayKey::String(value) => parse_canonical_int_string(value)
            .and_then(|int_value| properties.get(&ArrayKey::Int(int_value))),
    }
}

fn array_keys_are_equivalent(a: &ArrayKey, b: &ArrayKey) -> bool {
    match (a, b) {
        (ArrayKey::Int(a_int), ArrayKey::Int(b_int)) => a_int == b_int,
        (ArrayKey::String(a_str), ArrayKey::String(b_str)) => {
            if a_str == b_str {
                return true;
            }

            parse_canonical_int_string(a_str)
                .zip(parse_canonical_int_string(b_str))
                .is_some_and(|(a_int, b_int)| a_int == b_int)
        }
        (ArrayKey::Int(int_value), ArrayKey::String(str_value))
        | (ArrayKey::String(str_value), ArrayKey::Int(int_value)) => {
            parse_canonical_int_string(str_value).is_some_and(|parsed| parsed == *int_value)
        }
    }
}

fn canonicalize_array_key(key: &ArrayKey) -> ArrayKey {
    match key {
        ArrayKey::Int(value) => ArrayKey::Int(*value),
        ArrayKey::String(value) => parse_canonical_int_string(value)
            .map(ArrayKey::Int)
            .unwrap_or_else(|| ArrayKey::String(value.clone())),
    }
}

fn deduplicate_runtime_array_keys(literal_keys: &[ArrayKey]) -> Vec<ArrayKey> {
    let mut unique = Vec::new();

    for key in literal_keys {
        let canonical = canonicalize_array_key(key);
        if unique
            .iter()
            .any(|existing| array_keys_are_equivalent(existing, &canonical))
        {
            continue;
        }

        unique.push(canonical);
    }

    unique
}

fn parse_canonical_int_string(value: &str) -> Option<i64> {
    if value.is_empty() {
        return None;
    }

    if value.starts_with('+') {
        return None;
    }

    let body = if let Some(rest) = value.strip_prefix('-') {
        rest
    } else {
        value
    };

    if body.is_empty() {
        return None;
    }

    if body.len() > 1 && body.starts_with('0') {
        return None;
    }

    if !body.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    value.parse::<i64>().ok()
}

/// Adds nested assertions for isset checks.
fn add_nested_assertions(
    assertions: &mut BTreeMap<String, Vec<Assertion>>,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
) {
    let original_assertions = assertions.clone();

    for (nested_key, nested_key_assertions) in original_assertions {
        if !(nested_key.contains('[') || nested_key.contains("->")) {
            continue;
        }

        let Some(first_assertion) = nested_key_assertions.first() else {
            continue;
        };

        if !matches!(
            first_assertion,
            Assertion::IsEqualIsset | Assertion::IsIsset | Assertion::NonEmptyCountable(_)
        ) {
            continue;
        }

        let mut key_parts = break_up_path_into_parts(&nested_key);
        if key_parts.is_empty() {
            continue;
        }

        let mut base_key = key_parts.remove(0);

        if !base_key.starts_with('$')
            && key_parts.len() > 2
            && key_parts.first().is_some_and(|part| part == "::$")
        {
            base_key.push_str(key_parts.remove(0).as_str());
            base_key.push_str(key_parts.remove(0).as_str());
        }

        let base_is_set = analyzer
            .interner
            .find(&base_key)
            .and_then(|base_var_id| context.locals.get(&base_var_id))
            .is_some_and(|base_type| !base_type.is_nullable);

        if !base_is_set {
            assertions
                .entry(base_key.clone())
                .or_default()
                .push(Assertion::IsEqualIsset);
        }

        let mut i = 0;
        while i < key_parts.len() {
            match key_parts[i].as_str() {
                "[" => {
                    if i + 2 >= key_parts.len() || key_parts[i + 2] != "]" {
                        break;
                    }

                    let array_key = normalize_array_key_literal(&key_parts[i + 1]);
                    let new_base_key = format!("{}[{}]", base_key, array_key);

                    let array_access_assertion = if array_key.contains('\'') {
                        Assertion::HasStringArrayAccess
                    } else {
                        Assertion::HasIntOrStringArrayAccess
                    };

                    assertions
                        .entry(base_key.clone())
                        .or_default()
                        .push(array_access_assertion);

                    base_key = new_base_key;
                    i += 3;
                }
                "->" => {
                    if i + 1 >= key_parts.len() {
                        break;
                    }

                    let property_name = key_parts[i + 1].clone();
                    let new_base_key = format!("{}->{}", base_key, property_name);

                    assertions
                        .entry(base_key.clone())
                        .or_default()
                        .push(Assertion::IsEqualIsset);

                    base_key = new_base_key;
                    i += 2;
                }
                _ => break,
            }
        }
    }
}

fn normalize_array_key_literal(array_key: &str) -> String {
    if (array_key.starts_with('\'') || array_key.starts_with('"')) && array_key.len() >= 2 {
        let unquoted = &array_key[1..array_key.len() - 1];
        if let Ok(int_key) = unquoted.parse::<i64>() {
            return int_key.to_string();
        }
    }

    array_key.to_string()
}

/// Propagates a reconciled nested key (`$base[offset]`) into the base array's
/// type. Mirrors Psalm's `Reconciler::adjustTKeyedArrayType`: it updates the
/// first array-like atomic's known item for the offset (converting a list / plain
/// array as needed), keeps the array a list where it can (filling gaps from the
/// fallback or otherwise dropping list-ness), marks `$base[offset]` undefined when
/// it lands on more than one offset, and recurses into the parent for deeper
/// paths with the freshly-updated base type.
fn adjust_tkeyed_array_type(
    key_parts: Vec<String>,
    context: &mut BlockContext,
    changed_var_ids: &mut FxHashSet<StrId>,
    result_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) {
    let full_key = key_parts.join("");
    let Some((base_key, array_key)) = split_last_array_access(&full_key) else {
        return;
    };

    let array_key_offsets = if array_key.starts_with('$') {
        let mut nested_possibly_undefined = false;
        let Some(key_type) = resolve_variable_key_type(
            &array_key,
            context,
            analyzer,
            &mut nested_possibly_undefined,
        ) else {
            return;
        };

        let literal_keys =
            deduplicate_runtime_array_keys(&extract_literal_array_keys_from_union(&key_type));
        if literal_keys.is_empty() {
            return;
        }

        literal_keys
    } else if array_key.starts_with('\'') || array_key.starts_with('"') {
        vec![ArrayKey::String(array_key[1..array_key.len() - 1].to_string())]
    } else if let Ok(int_key) = array_key.parse::<i64>() {
        vec![ArrayKey::Int(int_key)]
    } else {
        return;
    };

    let Some(base_var_id) = analyzer
        .interner
        .find(&base_key)
        .or_else(|| get_alternate_var_id(analyzer, &base_key))
    else {
        return;
    };

    // The result is possibly-undefined when it can land on more than one offset.
    let mut result_type = result_type.clone();
    if array_key_offsets.len() > 1 {
        result_type.possibly_undefined = true;
    }

    let nested_path_id = analyzer
        .interner
        .find(&format!("{}[{}]", base_key, array_key));

    for offset in &array_key_offsets {
        let Some(existing_type) = context.locals.get(&base_var_id).cloned() else {
            return;
        };

        // Psalm updates the first matching array-like atomic and stops.
        let mut new_atomics = existing_type.types.clone();
        let mut updated = false;
        for atomic in new_atomics.iter_mut() {
            let replacement = match atomic {
                TAtomic::TKeyedArray {
                    properties,
                    is_list,
                    sealed,
                    fallback_key_type,
                    fallback_value_type,
                } => Some(set_keyed_array_offset(
                    properties,
                    *is_list,
                    *sealed,
                    fallback_key_type.as_deref(),
                    fallback_value_type.as_deref(),
                    offset,
                    &result_type,
                )),
                // A generic list (`list<T>`) keeps its uniform element type rather
                // than being turned into a keyed array — pzoom resolves a dynamic
                // offset against the element type, not the fallback of a keyed list.
                TAtomic::TArray {
                    key_type,
                    value_type,
                } => {
                    // A plain array becomes a keyed array with the offset known and
                    // the original params as the unsealed fallback.
                    let mut properties = FxHashMap::default();
                    properties.insert(offset.clone(), result_type.clone());
                    Some(TAtomic::TKeyedArray {
                        properties,
                        is_list: false,
                        sealed: false,
                        fallback_key_type: Some(key_type.clone()),
                        fallback_value_type: Some(value_type.clone()),
                    })
                }
                _ => None,
            };

            if let Some(replacement) = replacement {
                *atomic = replacement;
                updated = true;
                break;
            }
        }

        if !updated {
            continue;
        }

        let new_base = TUnion::from_types(new_atomics);
        changed_var_ids.insert(base_var_id);
        if let Some(nested_path_id) = nested_path_id {
            changed_var_ids.insert(nested_path_id);
        }
        context.locals.insert(base_var_id, new_base.clone());

        // Recurse into the parent for deeper paths, with the updated base type.
        if base_key.ends_with(']') {
            adjust_tkeyed_array_type(
                break_up_path_into_parts(&base_key),
                context,
                changed_var_ids,
                &new_base,
                analyzer,
            );
        }
    }
}

/// Sets `offset` to `result_type` in a keyed array, preserving list-ness when the
/// offset extends the list contiguously, filling an int gap from the fallback, or
/// otherwise dropping list-ness — mirroring Psalm's list fixup in
/// `adjustTKeyedArrayType`.
fn set_keyed_array_offset(
    properties: &FxHashMap<ArrayKey, TUnion>,
    is_list: bool,
    sealed: bool,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,
    offset: &ArrayKey,
    result_type: &TUnion,
) -> TAtomic {
    let mut new_properties = properties.clone();
    new_properties.insert(offset.clone(), result_type.clone());

    let mut new_is_list = is_list;
    if is_list {
        // A non-contiguous insert (a string key, or an int that leaves a hole)
        // drops list-ness. Psalm additionally fills an int hole from the fallback
        // when one exists, but doing so here over-eagerly invents elements during
        // reconciliation (e.g. inflating a list's `count`), so pzoom drops to a
        // keyed array instead.
        let breaks_list = match offset {
            ArrayKey::String(_) => true,
            ArrayKey::Int(index) => {
                *index != 0 && !properties.contains_key(&ArrayKey::Int(index - 1))
            }
        };
        if breaks_list {
            new_is_list = false;
        }
    }

    TAtomic::TKeyedArray {
        properties: new_properties,
        is_list: new_is_list,
        sealed,
        fallback_key_type: fallback_key_type.map(|t| Box::new(t.clone())),
        fallback_value_type: fallback_value_type.map(|t| Box::new(t.clone())),
    }
}

fn split_last_array_access(path: &str) -> Option<(String, String)> {
    if !path.ends_with(']') {
        return None;
    }

    let mut depth = 0_i32;
    let mut current_start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut escape = false;
    let last_index = path.len() - 1;

    for (idx, ch) in path.char_indices() {
        if let Some(active_quote) = quote {
            if ch == '\\' && !escape {
                escape = true;
                continue;
            }

            if ch == active_quote && !escape {
                quote = None;
            }

            escape = false;
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
            }
            '[' => {
                if depth == 0 {
                    current_start = Some(idx);
                }
                depth += 1;
            }
            ']' => {
                if depth == 0 {
                    return None;
                }

                depth -= 1;
                if depth == 0 && idx == last_index {
                    let start = current_start?;
                    let base = path[..start].to_string();
                    let key = path[start + 1..last_index].to_string();
                    return Some((base, key));
                }
            }
            _ => {}
        }
    }

    None
}

/// Helper function to get acceptable type after reconciliation.
pub(crate) fn get_acceptable_type(
    acceptable_types: Vec<TAtomic>,
    did_remove_type: bool,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    assertion: &Assertion,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    if acceptable_types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            trigger_issue_for_impossible(
                analysis_data,
                analyzer,
                existing_var_type,
                key,
                assertion,
                !did_remove_type,
                negated,
            );
        }
    }

    if acceptable_types.is_empty() {
        let mut nothing_type = TUnion::nothing();
        nothing_type.from_docblock = existing_var_type.from_docblock;
        nothing_type.from_calculation = existing_var_type.from_calculation;
        nothing_type.ignore_nullable_issues = existing_var_type.ignore_nullable_issues;
        nothing_type.ignore_falsable_issues = existing_var_type.ignore_falsable_issues;
        return nothing_type;
    }

    let mut result_type = TUnion::from_types(acceptable_types);
    result_type.from_docblock = existing_var_type.from_docblock;
    result_type.from_calculation = existing_var_type.from_calculation;
    result_type.ignore_nullable_issues = existing_var_type.ignore_nullable_issues;
    result_type.ignore_falsable_issues = existing_var_type.ignore_falsable_issues;
    result_type
}

/// Triggers an issue for impossible or redundant type checks.
pub(crate) fn trigger_issue_for_impossible(
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    existing_var_type: &TUnion,
    key: &String,
    assertion: &Assertion,
    redundant: bool,
    negated: bool,
) {
    let mut assertion_string = assertion.to_string();
    let mut not = assertion_string.starts_with('!');
    if not {
        assertion_string = assertion_string[1..].to_string();
    }

    if let Some(rest) = assertion_string.strip_prefix('>') {
        assertion_string = format!(">= {}", rest);
    } else if let Some(rest) = assertion_string.strip_prefix('<') {
        assertion_string = format!("<= {}", rest);
    }

    let mut is_redundant = redundant;

    if negated {
        is_redundant = !is_redundant;
        not = !not;
    }

    let old_var_type_string = existing_var_type.get_id(Some(analyzer.interner));
    let from_docblock = existing_var_type.from_docblock;

    let (issue_kind, message) = if is_redundant {
        if from_docblock {
            (
                IssueKind::RedundantConditionGivenDocblockType,
                format!(
                    "Docblock-defined type {} for {} is {}{}",
                    old_var_type_string,
                    key,
                    if not { "never " } else { "always " },
                    assertion_string
                ),
            )
        } else {
            (
                IssueKind::RedundantCondition,
                format!(
                    "Type {} for {} is {}{}",
                    old_var_type_string,
                    key,
                    if not { "never " } else { "always " },
                    assertion_string
                ),
            )
        }
    } else {
        if from_docblock {
            (
                IssueKind::DocblockTypeContradiction,
                format!(
                    "Docblock-defined type {} for {} is {}{}",
                    old_var_type_string,
                    key,
                    if not { "always " } else { "never " },
                    assertion_string
                ),
            )
        } else if assertion_string == "null" && !not {
            (
                IssueKind::TypeDoesNotContainNull,
                format!("Type {} for {} is never null", old_var_type_string, key),
            )
        } else {
            (
                IssueKind::TypeDoesNotContainType,
                format!(
                    "Type {} for {} is {}{}",
                    old_var_type_string,
                    key,
                    if not { "always " } else { "never " },
                    assertion_string
                ),
            )
        }
    };

    let start = analysis_data.current_stmt_start.unwrap_or(0);
    let end = analysis_data.current_stmt_end.unwrap_or(start);

    if analysis_data.issues.iter().any(|issue| {
        issue.kind == issue_kind
            && issue.location.start_offset == start
            && issue.location.end_offset == end
            && issue.message == message
    }) {
        return;
    }

    let (line, col) = analyzer.get_line_column(start);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        message,
        analyzer.file_path,
        start,
        end,
        line,
        col,
    ));
}
