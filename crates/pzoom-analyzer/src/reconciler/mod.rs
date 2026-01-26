//! Type reconciler module.
//!
//! This module provides type narrowing based on assertions from conditional branches.
//! For example, after `if ($x instanceof Foo)`, we know `$x` is of type `Foo`.

pub mod assertion_reconciler;
mod negated_assertion_reconciler;
mod simple_assertion_reconciler;
mod simple_negated_assertion_reconciler;

use std::collections::BTreeMap;

use pzoom_code_info::{ArrayKey, Assertion, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

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
pub fn reconcile_keyed_types(
    assertions: &BTreeMap<String, Vec<Assertion>>,
    context: &mut BlockContext,
    changed_var_ids: &mut FxHashSet<StrId>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
    negated: bool,
) {
    if assertions.is_empty() {
        return;
    }

    // Process nested isset assertions
    let mut new_assertions = assertions.clone();
    add_nested_assertions(&mut new_assertions, context, analyzer);

    for (var_name, var_assertions) in &new_assertions {
        // Skip class constant assertions for now
        if var_name.contains("::") && !var_name.contains('$') && !var_name.contains('[') {
            continue;
        }

        // Determine assertion characteristics
        let has_isset = var_assertions.iter().any(|a| a.has_isset());
        let has_inverted_isset = var_assertions.iter().any(|a| matches!(a, Assertion::IsNotIsset));

        // Get the current type for this variable
        let var_id = analyzer.interner.intern(var_name);
        let mut possibly_undefined = false;

        let existing_type = if let Some(t) = context.locals.get(&var_id) {
            Some(t.clone())
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

        // Apply each assertion in sequence
        for assertion in var_assertions {
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
        }

        // Check if type changed
        let type_changed = current_type != type_before;

        // Handle nested array types
        if var_name.ends_with(']') && type_changed && !has_inverted_isset {
            let key_parts = break_up_path_into_parts(var_name);
            adjust_array_type(key_parts, context, changed_var_ids, &current_type, analyzer);
        }

        if type_changed {
            changed_var_ids.insert(var_id);
        }

        // Update the context with the narrowed type
        context.locals.insert(var_id, current_type);
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
                if brackets == 0
                    && i < char_count - 2
                    && chars[i + 1] == ':'
                    && chars[i + 2] == '$'
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
fn get_value_for_key(
    key: &str,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    has_isset: bool,
    _has_inverted_isset: bool,
    _inside_loop: bool,
    possibly_undefined: &mut bool,
) -> Option<TUnion> {
    let mut key_parts = break_up_path_into_parts(key);

    if key_parts.len() == 1 {
        let var_id = analyzer.interner.find(key)?;
        return context.locals.get(&var_id).cloned();
    }

    key_parts.reverse();

    let base_key = key_parts.pop()?;
    let base_var_id = analyzer.interner.find(&base_key)?;
    let mut base_type = context.locals.get(&base_var_id)?.clone();

    while let Some(divider) = key_parts.pop() {
        if divider == "[" {
            let array_key = key_parts.pop()?;
            key_parts.pop(); // Pop the closing "]"

            let mut new_type: Option<TUnion> = None;

            for atomic in &base_type.types {
                let candidate_type = match atomic {
                    TAtomic::TKeyedArray { properties, fallback_value_type, .. } => {
                        let dict_key = if array_key.starts_with('\'') || array_key.starts_with('"') {
                            let key_str = array_key[1..array_key.len()-1].to_string();
                            ArrayKey::String(key_str)
                        } else if let Ok(int_key) = array_key.parse::<i64>() {
                            ArrayKey::Int(int_key)
                        } else {
                            // Variable key, use fallback
                            if let Some(fallback) = fallback_value_type {
                                Some((**fallback).clone())
                            } else {
                                Some(TUnion::mixed())
                            }?;
                            continue;
                        };

                        if let Some(prop_type) = properties.get(&dict_key) {
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
                    }
                    TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                        *possibly_undefined = true;
                        Some((**value_type).clone())
                    }
                    TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                        *possibly_undefined = true;
                        Some((**value_type).clone())
                    }
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                        Some(TUnion::mixed())
                    }
                    TAtomic::TString | TAtomic::TNonEmptyString | TAtomic::TLiteralString { .. } => {
                        // String access returns string
                        Some(TUnion::string())
                    }
                    _ => None,
                };

                if let Some(t) = candidate_type {
                    new_type = Some(if let Some(existing) = new_type {
                        // Combine types
                        let mut combined = existing.types;
                        combined.extend(t.types);
                        TUnion::from_types(combined)
                    } else {
                        t
                    });
                }
            }

            base_type = new_type?;
        } else if divider == "->" {
            let _property_name = key_parts.pop()?;
            // Property access - for now return mixed
            // TODO: Look up property type from class info
            return Some(TUnion::mixed());
        } else {
            break;
        }
    }

    Some(base_type)
}

/// Adds nested assertions for isset checks.
fn add_nested_assertions(
    assertions: &mut BTreeMap<String, Vec<Assertion>>,
    _context: &BlockContext,
    _analyzer: &StatementsAnalyzer<'_>,
) {
    let mut additional_assertions: Vec<(String, Assertion)> = Vec::new();

    for (key, key_assertions) in assertions.iter() {
        if (key.contains('[') || key.contains("->"))
            && key_assertions
                .iter()
                .any(|a| matches!(a, Assertion::IsIsset | Assertion::IsEqualIsset))
        {
            let key_parts = break_up_path_into_parts(key);

            let mut base_key = String::new();
            let mut parts_iter = key_parts.iter();

            if let Some(first) = parts_iter.next() {
                base_key = first.clone();

                // Add isset assertion for base variable
                if !assertions.contains_key(&base_key) {
                    additional_assertions.push((base_key.clone(), Assertion::IsEqualIsset));
                }

                for part in parts_iter {
                    match part.as_str() {
                        "[" => {
                            // Array access follows
                        }
                        "]" => {
                            // End of array access
                        }
                        "->" => {
                            // Property access
                        }
                        _ if base_key.ends_with('[') => {
                            base_key.push_str(part);
                            base_key.push(']');
                        }
                        _ if part.starts_with('\'') || part.starts_with('"') => {
                            base_key.push('[');
                            base_key.push_str(part);
                        }
                        _ => {
                            // Other parts (property names, array keys)
                        }
                    }
                }
            }
        }
    }

    for (key, assertion) in additional_assertions {
        assertions.entry(key).or_default().push(assertion);
    }
}

/// Adjusts array types based on key narrowing.
fn adjust_array_type(
    mut key_parts: Vec<String>,
    context: &mut BlockContext,
    changed_var_ids: &mut FxHashSet<StrId>,
    result_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) {
    // Remove the closing bracket, array key, and opening bracket
    if key_parts.len() < 3 {
        return;
    }

    key_parts.pop(); // "]"
    let array_key = key_parts.pop().unwrap();
    key_parts.pop(); // "["

    if array_key.starts_with('$') {
        // Variable key - can't narrow
        return;
    }

    let base_key = key_parts.join("");
    let base_var_id = match analyzer.interner.find(&base_key) {
        Some(id) => id,
        None => return,
    };

    let existing_type = match context.locals.get(&base_var_id) {
        Some(t) => t.clone(),
        None => return,
    };

    let mut new_types = Vec::new();

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                let mut new_properties = properties.clone();

                let dict_key = if array_key.starts_with('\'') || array_key.starts_with('"') {
                    ArrayKey::String(array_key[1..array_key.len()-1].to_string())
                } else if let Ok(int_key) = array_key.parse::<i64>() {
                    ArrayKey::Int(int_key)
                } else {
                    new_types.push(atomic.clone());
                    continue;
                };

                new_properties.insert(dict_key, result_type.clone());

                new_types.push(TAtomic::TKeyedArray {
                    properties: new_properties,
                    is_list: *is_list,
                    sealed: *sealed,
                    fallback_key_type: fallback_key_type.clone(),
                    fallback_value_type: fallback_value_type.clone(),
                });
            }
            TAtomic::TArray { key_type, value_type } => {
                // Convert to keyed array with the known key
                let dict_key = if array_key.starts_with('\'') || array_key.starts_with('"') {
                    ArrayKey::String(array_key[1..array_key.len()-1].to_string())
                } else if let Ok(int_key) = array_key.parse::<i64>() {
                    ArrayKey::Int(int_key)
                } else {
                    new_types.push(atomic.clone());
                    continue;
                };

                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(dict_key, result_type.clone());

                new_types.push(TAtomic::TKeyedArray {
                    properties,
                    is_list: false,
                    sealed: false,
                    fallback_key_type: Some(key_type.clone()),
                    fallback_value_type: Some(value_type.clone()),
                });
            }
            _ => {
                new_types.push(atomic.clone());
            }
        }
    }

    if !new_types.is_empty() {
        changed_var_ids.insert(base_var_id);
        context.locals.insert(base_var_id, TUnion::from_types(new_types));
    }

    // Recursively adjust parent arrays
    if let Some(last_part) = key_parts.last() {
        if last_part == "]" {
            adjust_array_type(
                key_parts,
                context,
                changed_var_ids,
                &existing_type,
                analyzer,
            );
        }
    }
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
        return TUnion::nothing();
    }

    TUnion::from_types(acceptable_types)
}

/// Triggers an issue for impossible or redundant type checks.
pub(crate) fn trigger_issue_for_impossible(
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    _existing_var_type: &TUnion,
    key: &String,
    assertion: &Assertion,
    redundant: bool,
    negated: bool,
) {
    let assertion_string = assertion.to_string();
    let mut is_redundant = redundant;

    if negated {
        is_redundant = !is_redundant;
    }

    if is_redundant {
        // Could emit RedundantCondition issue
        let _ = (analysis_data, analyzer, key, assertion_string);
    } else {
        // Could emit TypeDoesNotContainType issue
        let _ = (analysis_data, analyzer, key, assertion_string);
    }

    // For now, we don't emit issues - this can be added later when
    // the issue reporting infrastructure is more developed
}
