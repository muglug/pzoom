//! Type combiner - combines multiple atomic types into a simplified union.
//!
//! This handles cases like:
//! - `int + string = int|string`
//! - `array<int> + array<string> = array<int|string>`
//! - `true + false = bool`
//! - `array<never> + array<string> = array<string>`

use pzoom_str::StrId;
use rustc_hash::FxHashMap;

use crate::TAtomic;
use crate::t_atomic::ArrayKey;
use crate::t_union::TUnion;

use super::{extend_dataflow_uniquely, type_combination::TypeCombination};

/// The maximum number of literal values before collapsing to a general type
const LITERAL_LIMIT: usize = 500;

/// Combine multiple atomic types into a simplified list of atomic types.
pub fn combine(types: Vec<TAtomic>, overwrite_empty_array: bool) -> Vec<TAtomic> {
    if types.len() == 1 {
        return types;
    }

    let mut combination = TypeCombination::new();

    for atomic in types {
        let result = scrape_type_properties(
            atomic,
            &mut combination,
            overwrite_empty_array,
            LITERAL_LIMIT,
        );

        // If scrape returns Some, we should return early (e.g., for mixed)
        if let Some(early_return) = result {
            return early_return;
        }
    }

    // Handle void + other types -> null
    if combination.value_types.contains_key("void") {
        combination.value_types.remove("void");
        if !combination.value_types.contains_key("null") {
            combination
                .value_types
                .insert("null".to_string(), TAtomic::TNull);
        }
    }

    // Combine true + false = bool
    if combination.value_types.contains_key("true") && combination.value_types.contains_key("false")
    {
        combination.value_types.remove("true");
        combination.value_types.remove("false");
        combination
            .value_types
            .insert("bool".to_string(), TAtomic::TBool);
    }

    // Handle mixed tracking
    if combination.empty_mixed && combination.non_empty_mixed {
        combination
            .value_types
            .insert("mixed".to_string(), TAtomic::TMixed);
    } else if combination.non_empty_mixed {
        combination
            .value_types
            .insert("non-empty-mixed".to_string(), TAtomic::TNonEmptyMixed);
    }

    // Handle simple single-value-type case (must be after mixed handling)
    if combination.is_simple() && !combination.has_mixed {
        if combination.value_types.contains_key("false") {
            return vec![TAtomic::TFalse];
        }
        if combination.value_types.contains_key("true") {
            return vec![TAtomic::TTrue];
        }
        return combination.value_types.into_values().collect();
    }

    let mut new_types = Vec::new();

    // Handle keyed arrays (shapes)
    if !combination.objectlike_entries.is_empty() {
        new_types.extend(handle_keyed_array_entries(
            &mut combination,
            overwrite_empty_array,
        ));
    }

    // Handle generic arrays
    if let Some((key_type, value_type)) = combination.array_type_params.take() {
        new_types.push(get_array_type_from_generic_params(
            &combination,
            key_type,
            value_type,
            overwrite_empty_array,
        ));
    }

    // Handle builtin type params (iterable, Traversable, etc.)
    for (generic_type, generic_type_params) in combination.builtin_type_params {
        if generic_type == "iterable" && generic_type_params.len() == 2 {
            let mut params_iter = generic_type_params.into_iter();
            new_types.push(TAtomic::TIterable {
                key_type: Box::new(params_iter.next().unwrap()),
                value_type: Box::new(params_iter.next().unwrap_or_else(TUnion::mixed)),
            });
        } else {
            // Use well-known StrId constants for Traversable/Generator
            let name = if generic_type == "Traversable" {
                StrId::TRAVERSABLE
            } else if generic_type == "Generator" {
                StrId::GENERATOR
            } else {
                // For other types, we'd need an interner - fallback to EMPTY for now
                // This is a limitation without global interner access
                StrId::EMPTY
            };
            if name != StrId::EMPTY {
                new_types.push(TAtomic::TNamedObject {
                    name,
                    type_params: Some(generic_type_params),
                    is_static: false,
                    remapped_params: false,
                });
            }
        }
    }

    // Handle generic object type params
    for (_type_key, (name, type_params)) in combination.object_type_params {
        new_types.push(TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            is_static: false,
            remapped_params: false,
        });
    }

    // Handle class-string types
    if !combination.class_string_types.is_empty() {
        let has_non_specific_string = combination
            .value_types
            .get("string")
            .map_or(false, |t| matches!(t, TAtomic::TString));

        if !has_non_specific_string {
            for (_as_type, atomic) in combination.class_string_types {
                if let TAtomic::TNamedObject { name, .. } = atomic {
                    new_types.push(TAtomic::TClassString {
                        as_type: Some(Box::new(TAtomic::TNamedObject {
                            name,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        })),
                    });
                } else if matches!(atomic, TAtomic::TObject) {
                    new_types.push(TAtomic::TClassString { as_type: None });
                }
            }
        }
    }

    // Add literal strings
    if let Some(strings) = combination.strings {
        new_types.extend(strings.into_values());
    }

    // Add literal ints
    if let Some(ints) = combination.ints {
        new_types.extend(ints.into_values());
    }

    // Add literal floats
    if let Some(floats) = combination.floats {
        new_types.extend(floats.into_values());
    }

    // Combine scalar types
    if combination.value_types.contains_key("string")
        && combination.value_types.contains_key("int")
        && combination.value_types.contains_key("bool")
        && combination.value_types.contains_key("float")
    {
        combination.value_types.remove("string");
        combination.value_types.remove("int");
        combination.value_types.remove("bool");
        combination.value_types.remove("float");
        combination
            .value_types
            .insert("scalar".to_string(), TAtomic::TScalar);
    }

    // Add named object types
    if let Some(named_object_types) = combination.named_object_types {
        // Remove enum cases if the full enum is present
        for (_key, atomic) in &named_object_types {
            if let TAtomic::TEnum { name } = atomic {
                let enum_name = *name;
                combination.value_types.retain(|_k, v| {
                    if let TAtomic::TEnumCase { enum_name: en, .. } = v {
                        *en != enum_name
                    } else {
                        true
                    }
                });
            }
        }
        combination.value_types.extend(named_object_types);
    }

    let has_never = combination.value_types.contains_key("never");

    // Add remaining value types
    for (_key, atomic) in combination.value_types {
        // Skip mixed if we have other types and it's from loop isset
        if matches!(&atomic, TAtomic::TMixed)
            && combination.mixed_from_loop_isset == Some(true)
            && (!new_types.is_empty() || has_never)
        {
            continue;
        }

        // Skip never if we have other types
        if matches!(&atomic, TAtomic::TNothing) && (!new_types.is_empty() || has_never) {
            continue;
        }

        new_types.push(atomic);
    }

    if new_types.is_empty() {
        return vec![TAtomic::TNothing];
    }

    new_types
}

/// Scrape properties from an atomic type into the combination state.
/// Returns Some(types) if we should return early, None to continue processing.
fn scrape_type_properties(
    atomic: TAtomic,
    combination: &mut TypeCombination,
    overwrite_empty_array: bool,
    literal_limit: usize,
) -> Option<Vec<TAtomic>> {
    match atomic {
        // Handle never/nothing type - just track it, don't add to value_types
        // It will be filtered out later if there are other types
        TAtomic::TNothing => {
            combination
                .value_types
                .insert("never".to_string(), TAtomic::TNothing);
            None
        }

        TAtomic::TMixed => {
            combination.mixed_from_loop_isset = Some(false);
            combination.empty_mixed = true;
            combination.non_empty_mixed = true;
            combination.has_mixed = true;
            // We don't return early for mixed in allow_mixed_union mode
            None
        }

        TAtomic::TNonEmptyMixed => {
            combination.non_empty_mixed = true;
            if combination.empty_mixed {
                return None;
            }
            combination.has_mixed = true;
            None
        }

        // Handle bool variants
        TAtomic::TFalse | TAtomic::TTrue => {
            if combination.value_types.contains_key("bool") {
                return None;
            }
            let key = if matches!(atomic, TAtomic::TFalse) {
                "false"
            } else {
                "true"
            };
            combination.value_types.insert(key.to_string(), atomic);
            None
        }

        TAtomic::TBool => {
            combination.value_types.remove("false");
            combination.value_types.remove("true");
            combination.value_types.insert("bool".to_string(), atomic);
            None
        }

        // Handle array types
        TAtomic::TArray {
            key_type,
            value_type,
        } => {
            scrape_array_properties(
                combination,
                *key_type,
                *value_type,
                false,
                overwrite_empty_array,
            );
            None
        }

        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => {
            scrape_array_properties(
                combination,
                *key_type,
                *value_type,
                true,
                overwrite_empty_array,
            );
            combination.array_sometimes_filled = true;
            None
        }

        TAtomic::TList { value_type } => {
            scrape_list_properties(combination, *value_type, false, overwrite_empty_array);
            None
        }

        TAtomic::TNonEmptyList { value_type } => {
            scrape_list_properties(combination, *value_type, true, overwrite_empty_array);
            combination.array_sometimes_filled = true;
            None
        }

        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            scrape_keyed_array_properties(
                combination,
                properties,
                is_list,
                sealed,
                fallback_key_type.map(|b| *b),
                fallback_value_type.map(|b| *b),
                overwrite_empty_array,
            );
            None
        }

        // Handle iterable types
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            // Merge with existing array params if present
            if combination.array_type_params.is_some() {
                let (existing_key, existing_value) = combination.array_type_params.take().unwrap();
                combination.builtin_type_params.insert(
                    "iterable".to_string(),
                    vec![
                        combine_union_types(&existing_key, &key_type, overwrite_empty_array),
                        combine_union_types(&existing_value, &value_type, overwrite_empty_array),
                    ],
                );
            } else if let Some(existing_params) =
                combination.builtin_type_params.get_mut("iterable")
            {
                if existing_params.len() >= 2 {
                    existing_params[0] =
                        combine_union_types(&existing_params[0], &key_type, overwrite_empty_array);
                    existing_params[1] = combine_union_types(
                        &existing_params[1],
                        &value_type,
                        overwrite_empty_array,
                    );
                }
            } else {
                combination
                    .builtin_type_params
                    .insert("iterable".to_string(), vec![*key_type, *value_type]);
            }
            None
        }

        // Handle object types
        TAtomic::TObject => {
            combination.has_object_top_type = true;
            combination.named_object_types = None;
            combination.value_types.insert("object".to_string(), atomic);
            None
        }

        TAtomic::TNamedObject {
            ref name,
            ref type_params,
        .. } => {
            // Track static qualifier
            if !combination.object_static.contains_key(name) {
                combination.object_static.insert(*name, false);
            }

            if let Some(type_params) = type_params {
                // Handle Traversable/Generator specially using StrId constants
                if *name == StrId::TRAVERSABLE || *name == StrId::GENERATOR {
                    let type_key = if *name == StrId::TRAVERSABLE {
                        "Traversable".to_string()
                    } else {
                        "Generator".to_string()
                    };
                    if let Some(existing_params) =
                        combination.builtin_type_params.get_mut(&type_key)
                    {
                        for (i, type_param) in type_params.iter().enumerate() {
                            if let Some(existing) = existing_params.get_mut(i) {
                                *existing = combine_union_types(
                                    existing,
                                    type_param,
                                    overwrite_empty_array,
                                );
                            }
                        }
                    } else {
                        combination
                            .builtin_type_params
                            .insert(type_key, type_params.clone());
                    }
                    return None;
                }

                // Generic object - use StrId numeric value for key uniqueness
                let type_key = format!(
                    "{}#{}<{}>",
                    name.0,
                    type_params.len(),
                    type_params
                        .iter()
                        .map(|t| t.get_id(None))
                        .collect::<Vec<_>>()
                        .join(",")
                );

                if let Some((_, existing_params)) =
                    combination.object_type_params.get_mut(&type_key)
                {
                    for (i, type_param) in type_params.iter().enumerate() {
                        if let Some(existing) = existing_params.get_mut(i) {
                            *existing =
                                combine_union_types(existing, type_param, overwrite_empty_array);
                        }
                    }
                } else {
                    combination
                        .object_type_params
                        .insert(type_key, (*name, type_params.clone()));
                }
            } else {
                // Non-generic named object
                if combination.named_object_types.is_none() {
                    // Already have object top type
                    return None;
                }

                // Use StrId numeric value as key
                let key = format!("named#{}", name.0);
                if let Some(ref mut named_types) = combination.named_object_types {
                    named_types.insert(key, atomic);
                }
            }
            None
        }

        // Handle scalar type
        TAtomic::TScalar => {
            combination.strings = None;
            combination.ints = None;
            combination.floats = None;
            combination.value_types.remove("string");
            combination.value_types.remove("int");
            combination.value_types.remove("bool");
            combination.value_types.remove("true");
            combination.value_types.remove("false");
            combination.value_types.remove("float");
            combination.value_types.insert("scalar".to_string(), atomic);
            None
        }

        // Handle array-key type
        TAtomic::TArrayKey => {
            if combination.value_types.contains_key("scalar") {
                return None;
            }
            combination.strings = None;
            combination.ints = None;
            combination.value_types.remove("string");
            combination.value_types.remove("int");
            combination
                .value_types
                .insert("array-key".to_string(), atomic);
            None
        }

        // Handle numeric type
        TAtomic::TNumeric => {
            if combination.value_types.contains_key("scalar") {
                return None;
            }
            combination.ints = None;
            combination.floats = None;
            combination.value_types.remove("int");
            combination.value_types.remove("float");
            combination
                .value_types
                .insert("numeric".to_string(), atomic);
            None
        }

        // Handle string types
        TAtomic::TString => {
            scrape_string_properties(atomic, combination, literal_limit);
            None
        }

        TAtomic::TLiteralString { ref value } => {
            let value_clone = value.clone();
            scrape_literal_string_properties(&value_clone, atomic, combination, literal_limit);
            None
        }

        TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString => {
            scrape_string_properties(atomic, combination, literal_limit);
            None
        }

        TAtomic::TClassString { ref as_type } => {
            if let Some(as_type) = as_type {
                let key = if let TAtomic::TNamedObject { ref name, .. } = **as_type {
                    format!("class-string#{}", name.0)
                } else {
                    "class-string#object".to_string()
                };
                combination
                    .class_string_types
                    .insert(key, (**as_type).clone());
            } else {
                combination
                    .class_string_types
                    .insert("class-string#object".to_string(), TAtomic::TObject);
            }
            None
        }

        TAtomic::TLiteralClassString { ref name } => {
            if let Some(ref mut strings) = combination.strings {
                if strings.len() < literal_limit {
                    strings.insert(format!("literal-class-string#{}", name), atomic);
                } else {
                    combination.strings = None;
                    combination
                        .class_string_types
                        .insert("class-string#object".to_string(), TAtomic::TObject);
                }
            } else {
                combination
                    .class_string_types
                    .insert("class-string#object".to_string(), TAtomic::TObject);
            }
            None
        }

        // Handle int types
        TAtomic::TInt => {
            scrape_int_properties(atomic, combination);
            None
        }

        TAtomic::TLiteralInt { value } => {
            scrape_literal_int_properties(value, atomic, combination, literal_limit);
            None
        }

        TAtomic::TIntRange { min, max } => {
            scrape_int_range_properties(min, max, combination);
            None
        }

        // Handle float types
        TAtomic::TFloat => {
            combination.floats = None;
            combination.value_types.insert("float".to_string(), atomic);
            None
        }

        TAtomic::TLiteralFloat { value } => {
            if combination.value_types.contains_key("float") {
                return None;
            }
            if let Some(ref mut floats) = combination.floats {
                if floats.len() < literal_limit {
                    let key = format!("float({})", value);
                    floats.insert(key, atomic);
                } else {
                    combination.floats = None;
                    combination
                        .value_types
                        .insert("float".to_string(), TAtomic::TFloat);
                }
            }
            None
        }

        // Handle callable
        TAtomic::TCallable { .. } => {
            // Absorb callable-string and callable arrays
            if combination
                .value_types
                .get("string")
                .map_or(false, |t| matches!(t, TAtomic::TClassString { .. }))
            {
                combination.value_types.remove("string");
            }
            combination
                .value_types
                .insert("callable".to_string(), atomic);
            None
        }

        // Handle enum types
        TAtomic::TEnum { ref name } => {
            let key = format!("enum#{}", name.0);
            combination.value_types.insert(key, atomic);
            None
        }

        TAtomic::TEnumCase {
            ref enum_name,
            ref case_name,
        } => {
            // If the full enum is already present, skip the case
            let enum_key = format!("enum#{}", enum_name.0);
            if combination.value_types.contains_key(&enum_key) {
                return None;
            }
            let key = format!("enum-case#{}#{}", enum_name.0, case_name.0);
            combination.value_types.insert(key, atomic);
            None
        }

        // Default: add to value_types
        _ => {
            let key = atomic.get_id(None);
            combination.value_types.insert(key, atomic);
            None
        }
    }
}

fn scrape_array_properties(
    combination: &mut TypeCombination,
    key_type: TUnion,
    value_type: TUnion,
    non_empty: bool,
    overwrite_empty_array: bool,
) {
    let is_empty_array = key_type.is_nothing() && value_type.is_nothing();

    if let Some((ref mut existing_key, ref mut existing_value)) = combination.array_type_params {
        *existing_key = combine_union_types(existing_key, &key_type, overwrite_empty_array);
        *existing_value = combine_union_types(existing_value, &value_type, overwrite_empty_array);
    } else {
        combination.array_type_params = Some((key_type, value_type));
    }

    if !non_empty {
        combination.array_always_filled = false;
    }

    if !is_empty_array {
        combination.all_arrays_lists = false;
    }
    combination.all_arrays_callable = false;
}

fn scrape_list_properties(
    combination: &mut TypeCombination,
    value_type: TUnion,
    non_empty: bool,
    overwrite_empty_array: bool,
) {
    let key_type = TUnion::new(TAtomic::TInt);

    if let Some((ref mut existing_key, ref mut existing_value)) = combination.array_type_params {
        *existing_key = combine_union_types(existing_key, &key_type, overwrite_empty_array);
        *existing_value = combine_union_types(existing_value, &value_type, overwrite_empty_array);
    } else {
        combination.array_type_params = Some((key_type, value_type));
    }

    if !non_empty {
        combination.array_always_filled = false;
    }

    // Keep list status if we haven't seen a non-list
    combination.all_arrays_callable = false;
}

fn scrape_keyed_array_properties(
    combination: &mut TypeCombination,
    properties: FxHashMap<ArrayKey, TUnion>,
    is_list: bool,
    _sealed: bool,
    fallback_key_type: Option<TUnion>,
    fallback_value_type: Option<TUnion>,
    overwrite_empty_array: bool,
) {
    let has_previous_keyed_array = combination
        .array_counts
        .as_ref()
        .is_some_and(|counts| !counts.is_empty());
    let existing_entries = !combination.objectlike_entries.is_empty() || has_previous_keyed_array;
    let mut missing_entries: Vec<ArrayKey> =
        combination.objectlike_entries.keys().cloned().collect();

    combination.objectlike_sealed = combination.objectlike_sealed && fallback_key_type.is_none();

    let mut has_defined_keys = false;

    for (key, value_type) in properties {
        let mut entry_value_type = value_type;

        // If this key only appears in one branch, mark it as possibly undefined.
        if !combination.objectlike_entries.contains_key(&key) && existing_entries {
            if overwrite_empty_array {
                if let Some(existing_fallback_value_type) =
                    combination.objectlike_value_type.as_ref()
                {
                    entry_value_type = combine_union_types(
                        existing_fallback_value_type,
                        &entry_value_type,
                        overwrite_empty_array,
                    );
                }
            } else {
                entry_value_type.possibly_undefined = true;
            }
        }

        if let Some(existing_type) = combination.objectlike_entries.get(&key) {
            let combined =
                combine_union_types(existing_type, &entry_value_type, overwrite_empty_array);
            combination.objectlike_entries.insert(key.clone(), combined);
        } else {
            combination
                .objectlike_entries
                .insert(key.clone(), entry_value_type);
        }

        missing_entries.retain(|k| k != &key);

        let is_possibly_undefined = combination
            .objectlike_entries
            .get(&key)
            .is_some_and(|entry_type| entry_type.possibly_undefined);

        if !is_possibly_undefined {
            has_defined_keys = true;
        }
    }

    // Keys missing in this branch become possibly undefined after merge.
    if !overwrite_empty_array {
        for missing_key in missing_entries {
            if let Some(existing_type) = combination.objectlike_entries.get_mut(&missing_key) {
                existing_type.possibly_undefined = true;
            }
        }
    }

    // Handle fallback types
    if let Some(fallback_key) = fallback_key_type {
        combination.objectlike_key_type = Some(
            if let Some(existing) = combination.objectlike_key_type.take() {
                combine_union_types(&existing, &fallback_key, overwrite_empty_array)
            } else {
                fallback_key
            },
        );
    }

    if let Some(fallback_value) = fallback_value_type {
        combination.objectlike_value_type = Some(
            if let Some(existing) = combination.objectlike_value_type.take() {
                combine_union_types(&existing, &fallback_value, overwrite_empty_array)
            } else {
                fallback_value
            },
        );
    }

    if !has_defined_keys {
        combination.array_always_filled = false;
    }

    // Track array count
    if let Some(ref mut counts) = combination.array_counts {
        counts.insert(combination.objectlike_entries.len());
    }

    if !is_list {
        combination.all_arrays_lists = false;
    }
}

fn scrape_string_properties(
    atomic: TAtomic,
    combination: &mut TypeCombination,
    _literal_limit: usize,
) {
    if combination.value_types.contains_key("array-key")
        || combination.value_types.contains_key("scalar")
    {
        return;
    }

    if !combination.value_types.contains_key("string") {
        if let Some(ref strings) = combination.strings {
            // Check if we need to merge with existing literal strings
            match &atomic {
                TAtomic::TString => {
                    combination.strings = None;
                    combination.value_types.insert("string".to_string(), atomic);
                }
                TAtomic::TNonEmptyString => {
                    // Check if any existing strings are empty
                    let has_empty = strings.values().any(
                        |t| matches!(t, TAtomic::TLiteralString { value } if value.is_empty()),
                    );
                    combination.strings = None;
                    if has_empty {
                        combination
                            .value_types
                            .insert("string".to_string(), TAtomic::TString);
                    } else {
                        combination.value_types.insert("string".to_string(), atomic);
                    }
                }
                TAtomic::TNumericString => {
                    // Check if any existing strings are non-numeric
                    let has_non_numeric = strings.values().any(|t| {
                        if let TAtomic::TLiteralString { value } = t {
                            !php_is_numeric(value)
                        } else {
                            false
                        }
                    });
                    combination.strings = None;
                    if has_non_numeric {
                        combination
                            .value_types
                            .insert("string".to_string(), TAtomic::TString);
                    } else {
                        combination.value_types.insert("string".to_string(), atomic);
                    }
                }
                TAtomic::TTruthyString => {
                    // Check if any strings are falsy (empty or "0")
                    let has_empty = strings.values().any(
                        |t| matches!(t, TAtomic::TLiteralString { value } if value.is_empty()),
                    );
                    let has_zero = strings
                        .values()
                        .any(|t| matches!(t, TAtomic::TLiteralString { value } if value == "0"));
                    let has_falsy = has_empty || has_zero;
                    combination.strings = None;
                    if has_falsy {
                        if has_empty {
                            combination
                                .value_types
                                .insert("string".to_string(), TAtomic::TString);
                        } else {
                            combination
                                .value_types
                                .insert("string".to_string(), TAtomic::TNonEmptyString);
                        }
                    } else {
                        combination.value_types.insert("string".to_string(), atomic);
                    }
                }
                _ => {
                    combination.strings = None;
                    combination.value_types.insert("string".to_string(), atomic);
                }
            }
        } else {
            combination.value_types.insert("string".to_string(), atomic);
        }
    } else {
        // Already have a string type, need to merge
        let existing = combination.value_types.get("string").unwrap().clone();
        let merged = merge_string_types(&existing, &atomic);
        combination.value_types.insert("string".to_string(), merged);
    }

    combination.strings = None;
}

fn scrape_literal_string_properties(
    value: &str,
    atomic: TAtomic,
    combination: &mut TypeCombination,
    literal_limit: usize,
) {
    if combination.value_types.contains_key("array-key")
        || combination.value_types.contains_key("scalar")
    {
        return;
    }

    if let Some(existing) = combination.value_types.get("string") {
        // Check if the literal is contained by the existing string type
        match existing {
            TAtomic::TString => return,
            TAtomic::TNonEmptyString => {
                if value.is_empty() {
                    combination
                        .value_types
                        .insert("string".to_string(), TAtomic::TString);
                }
                return;
            }
            TAtomic::TNumericString => {
                if php_is_numeric(value) {
                    return;
                }
                combination
                    .value_types
                    .insert("string".to_string(), TAtomic::TString);
                return;
            }
            TAtomic::TTruthyString => {
                if !value.is_empty() && value != "0" {
                    return;
                }
                if value.is_empty() {
                    combination
                        .value_types
                        .insert("string".to_string(), TAtomic::TString);
                } else {
                    combination
                        .value_types
                        .insert("string".to_string(), TAtomic::TNonEmptyString);
                }
                return;
            }
            _ => {}
        }
    }

    if let Some(ref mut strings) = combination.strings {
        if strings.len() < literal_limit {
            let key = format!("literal-string#{}", value);
            strings.insert(key, atomic);
        } else {
            // Exceeded limit, collapse to string
            combination.strings = None;
            combination
                .value_types
                .insert("string".to_string(), TAtomic::TString);
        }
    }
}

fn scrape_int_properties(atomic: TAtomic, combination: &mut TypeCombination) {
    if combination.value_types.contains_key("array-key")
        || combination.value_types.contains_key("scalar")
        || combination.value_types.contains_key("numeric")
    {
        return;
    }

    combination.ints = None;
    combination.value_types.insert("int".to_string(), atomic);
}

fn scrape_literal_int_properties(
    value: i64,
    atomic: TAtomic,
    combination: &mut TypeCombination,
    literal_limit: usize,
) {
    if combination.value_types.contains_key("array-key")
        || combination.value_types.contains_key("scalar")
        || combination.value_types.contains_key("numeric")
    {
        return;
    }

    if let Some(existing_int) = combination.value_types.get("int") {
        match existing_int {
            TAtomic::TInt => {
                // Already have full int type, literal is contained
                return;
            }
            TAtomic::TIntRange { min, max } => {
                // Expand range to include the literal value
                let new_min = match min {
                    Some(m) => Some((*m).min(value)),
                    None => None, // Already unbounded below
                };
                let new_max = match max {
                    Some(m) => Some((*m).max(value)),
                    None => None, // Already unbounded above
                };
                combination.value_types.insert(
                    "int".to_string(),
                    TAtomic::TIntRange {
                        min: new_min,
                        max: new_max,
                    },
                );
                return;
            }
            _ => {}
        }
    }

    if let Some(ref mut ints) = combination.ints {
        if ints.len() < literal_limit {
            let key = format!("int({})", value);
            ints.insert(key, atomic);
        } else {
            combination.ints = None;
            combination
                .value_types
                .insert("int".to_string(), TAtomic::TInt);
        }
    }
}

fn scrape_int_range_properties(
    min: Option<i64>,
    max: Option<i64>,
    combination: &mut TypeCombination,
) {
    if combination.value_types.contains_key("array-key")
        || combination.value_types.contains_key("scalar")
        || combination.value_types.contains_key("numeric")
    {
        return;
    }

    // Merge with existing literal ints
    if let Some(ref ints) = combination.ints {
        let mut new_min = min;
        let mut new_max = max;

        for (_, atomic) in ints.iter() {
            if let TAtomic::TLiteralInt { value } = atomic {
                // Expand range to include literal value
                new_min = match new_min {
                    Some(m) => Some(m.min(*value)),
                    None => None, // Already unbounded below
                };
                new_max = match new_max {
                    Some(m) => Some(m.max(*value)),
                    None => None, // Already unbounded above
                };
            }
        }

        combination.ints = None;
        combination
            .value_types
            .insert("int".to_string(), int_range_or_int(new_min, new_max));
        return;
    }

    // Merge with existing int range
    if let Some(TAtomic::TIntRange {
        min: existing_min,
        max: existing_max,
    }) = combination.value_types.get("int")
    {
        // When merging ranges, the result is the union - broader range
        let new_min = match (min, *existing_min) {
            (Some(a), Some(b)) => Some(a.min(b)),
            _ => None, // One is unbounded below
        };
        let new_max = match (max, *existing_max) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None, // One is unbounded above
        };
        combination
            .value_types
            .insert("int".to_string(), int_range_or_int(new_min, new_max));
    } else if combination.value_types.contains_key("int") {
        // Already have TInt, which encompasses all ranges
    } else {
        combination.ints = None;
        combination
            .value_types
            .insert("int".to_string(), int_range_or_int(min, max));
    }
}

/// An int-range atomic, collapsing a fully-open range to plain `int`. A
/// `TIntRange { min: None, max: None }` is degenerate: comparators treat it as a
/// bounded range rather than `int` (e.g. `array-key` then appears unable to
/// contain it), so unioning `positive-int|negative-int|...` would emit a
/// spurious contradiction. Mirrors Psalm collapsing such a range back to `int`.
fn int_range_or_int(min: Option<i64>, max: Option<i64>) -> TAtomic {
    if min.is_none() && max.is_none() {
        TAtomic::TInt
    } else {
        TAtomic::TIntRange { min, max }
    }
}

fn merge_string_types(existing: &TAtomic, new: &TAtomic) -> TAtomic {
    match (existing, new) {
        (TAtomic::TString, _) => TAtomic::TString,
        (_, TAtomic::TString) => TAtomic::TString,

        // non-empty + non-empty-* = non-empty
        (TAtomic::TNonEmptyString, TAtomic::TNonEmptyString)
        | (TAtomic::TNonEmptyString, TAtomic::TTruthyString)
        | (TAtomic::TTruthyString, TAtomic::TNonEmptyString)
        | (TAtomic::TNonEmptyString, TAtomic::TNumericString)
        | (TAtomic::TNumericString, TAtomic::TNonEmptyString)
        | (TAtomic::TNonEmptyString, TAtomic::TNonEmptyLowercaseString)
        | (TAtomic::TNonEmptyLowercaseString, TAtomic::TNonEmptyString) => TAtomic::TNonEmptyString,

        // truthy + truthy = truthy
        (TAtomic::TTruthyString, TAtomic::TTruthyString) => TAtomic::TTruthyString,

        // truthy + numeric = non-empty (numeric includes "0")
        (TAtomic::TTruthyString, TAtomic::TNumericString)
        | (TAtomic::TNumericString, TAtomic::TTruthyString) => TAtomic::TNonEmptyString,

        // lowercase combinations
        (TAtomic::TLowercaseString, TAtomic::TNonEmptyLowercaseString)
        | (TAtomic::TNonEmptyLowercaseString, TAtomic::TLowercaseString) => {
            TAtomic::TLowercaseString
        }

        (TAtomic::TLowercaseString, TAtomic::TLowercaseString) => TAtomic::TLowercaseString,

        (TAtomic::TNonEmptyLowercaseString, TAtomic::TNonEmptyLowercaseString) => {
            TAtomic::TNonEmptyLowercaseString
        }

        // numeric + numeric
        (TAtomic::TNumericString, TAtomic::TNumericString) => TAtomic::TNumericString,

        // Default: fall back to string
        _ => TAtomic::TString,
    }
}

fn handle_keyed_array_entries(
    combination: &mut TypeCombination,
    overwrite_empty_array: bool,
) -> Vec<TAtomic> {
    let mut new_types = Vec::new();

    // If we also have generic array params, keep keyed entries and merge generic
    // key/value information into fallback types. This preserves per-key precision.
    if let Some((generic_key_type, generic_value_type)) = combination.array_type_params.take() {
        if generic_value_type.is_nothing() {
            // Union with an empty generic array means every known key can be absent.
            for entry_type in combination.objectlike_entries.values_mut() {
                entry_type.possibly_undefined = true;
            }
        } else {
            let generic_key_for_entries = generic_key_type.clone();
            let generic_value_for_entries = generic_value_type.clone();

            // Generic arrays do not guarantee presence of specific keys when
            // merged through control-flow unions.
            if !overwrite_empty_array {
                for entry_type in combination.objectlike_entries.values_mut() {
                    entry_type.possibly_undefined = true;
                }
            }

            combination.objectlike_sealed = false;
            combination.objectlike_key_type = Some(
                if let Some(existing) = combination.objectlike_key_type.take() {
                    combine_union_types(&existing, &generic_key_type, overwrite_empty_array)
                } else {
                    generic_key_type
                },
            );
            combination.objectlike_value_type = Some(
                if let Some(existing) = combination.objectlike_value_type.take() {
                    combine_union_types(&existing, &generic_value_type, overwrite_empty_array)
                } else {
                    generic_value_type
                },
            );

            // Generic array branches may already contain any shaped key. Preserve that by
            // widening keyed entries with generic value types where key types overlap.
            for (array_key, entry_type) in &mut combination.objectlike_entries {
                if array_key_may_be_contained_by_union(array_key, &generic_key_for_entries) {
                    *entry_type = combine_union_types(
                        entry_type,
                        &generic_value_for_entries,
                        overwrite_empty_array,
                    );
                }
            }
        }
    }

    // Build keyed array from entries
    if !combination.objectlike_entries.is_empty() {
        let fallback = if combination.objectlike_sealed {
            None
        } else if let (Some(key_type), Some(value_type)) = (
            combination.objectlike_key_type.take(),
            combination.objectlike_value_type.take(),
        ) {
            Some((Box::new(key_type), Box::new(value_type)))
        } else {
            None
        };

        new_types.push(TAtomic::TKeyedArray {
            properties: std::mem::take(&mut combination.objectlike_entries)
                .into_iter()
                .collect(),
            is_list: combination.all_arrays_lists,
            sealed: combination.objectlike_sealed,
            fallback_key_type: fallback.as_ref().map(|(k, _)| k.clone()),
            fallback_value_type: fallback.map(|(_, v)| v),
        });
    }

    new_types
}

fn get_array_type_from_generic_params(
    combination: &TypeCombination,
    key_type: TUnion,
    value_type: TUnion,
    overwrite_empty_array: bool,
) -> TAtomic {
    if combination.array_always_filled
        || (combination.array_sometimes_filled && overwrite_empty_array)
    {
        if combination.all_arrays_lists {
            TAtomic::TNonEmptyList {
                value_type: Box::new(value_type),
            }
        } else {
            TAtomic::TNonEmptyArray {
                key_type: Box::new(key_type),
                value_type: Box::new(value_type),
            }
        }
    } else if combination.all_arrays_lists {
        TAtomic::TList {
            value_type: Box::new(value_type),
        }
    } else {
        TAtomic::TArray {
            key_type: Box::new(key_type),
            value_type: Box::new(value_type),
        }
    }
}

fn array_key_may_be_contained_by_union(key: &ArrayKey, key_union: &TUnion) -> bool {
    key_union.types.iter().any(|atomic| match (key, atomic) {
        (_, TAtomic::TMixed) => true,
        (_, TAtomic::TArrayKey) => true,
        (ArrayKey::Int(_), TAtomic::TInt) => true,
        (ArrayKey::String(_), TAtomic::TString) => true,
        (ArrayKey::Int(value), TAtomic::TLiteralInt { value: literal }) => value == literal,
        (ArrayKey::String(value), TAtomic::TLiteralString { value: literal }) => value == literal,
        (ArrayKey::Int(value), TAtomic::TLiteralString { value: literal }) => literal
            .parse::<i64>()
            .ok()
            .is_some_and(|literal_int| literal_int == *value),
        (ArrayKey::Int(value), TAtomic::TIntRange { min, max }) => {
            let above_min = min.is_none_or(|min| *value >= min);
            let below_max = max.is_none_or(|max| *value <= max);
            above_min && below_max
        }
        _ => false,
    })
}

/// Combine two union types into a new union type.
pub fn combine_union_types(
    type_1: &TUnion,
    type_2: &TUnion,
    overwrite_empty_array: bool,
) -> TUnion {
    if type_1 == type_2 {
        return type_1.clone();
    }

    let mut all_atomic_types = type_1.types.clone();
    all_atomic_types.extend(type_2.types.clone());

    let mut combined_type = TUnion::from_types(combine(all_atomic_types, overwrite_empty_array));
    combined_type.from_docblock = type_1.from_docblock || type_2.from_docblock;
    combined_type.from_calculation = type_1.from_calculation || type_2.from_calculation;
    combined_type.possibly_undefined = type_1.possibly_undefined || type_2.possibly_undefined;
    combined_type.ignore_nullable_issues =
        type_1.ignore_nullable_issues || type_2.ignore_nullable_issues;
    combined_type.ignore_falsable_issues =
        type_1.ignore_falsable_issues || type_2.ignore_falsable_issues;

    let type_1_parent_nodes_empty = type_1.parent_nodes.is_empty();
    let type_2_parent_nodes_empty = type_2.parent_nodes.is_empty();

    if !type_1_parent_nodes_empty || !type_2_parent_nodes_empty {
        if type_1_parent_nodes_empty {
            combined_type.parent_nodes.clone_from(&type_2.parent_nodes);
        } else if type_2_parent_nodes_empty {
            combined_type.parent_nodes.clone_from(&type_1.parent_nodes);
        } else {
            combined_type.parent_nodes.clone_from(&type_1.parent_nodes);
            extend_dataflow_uniquely(&mut combined_type.parent_nodes, type_2.parent_nodes.clone());
        }
    }

    combined_type
}

/// Add a type to an existing union type.
pub fn add_union_type(
    mut base_type: TUnion,
    other_type: &TUnion,
    overwrite_empty_array: bool,
) -> TUnion {
    if &base_type == other_type {
        return base_type;
    }

    let mut all_atomic_types = base_type.types.clone();
    all_atomic_types.extend(other_type.types.clone());

    base_type.types = combine(all_atomic_types, overwrite_empty_array);

    // Update flags
    base_type.is_nullable = base_type.types.iter().any(|t| t.is_nullable());
    base_type.is_falsable = base_type.types.iter().any(|t| t.is_falsable());
    base_type.from_docblock |= other_type.from_docblock;
    base_type.from_calculation |= other_type.from_calculation;
    base_type.possibly_undefined |= other_type.possibly_undefined;
    base_type.ignore_nullable_issues |= other_type.ignore_nullable_issues;
    base_type.ignore_falsable_issues |= other_type.ignore_falsable_issues;

    if !other_type.parent_nodes.is_empty() {
        extend_dataflow_uniquely(&mut base_type.parent_nodes, other_type.parent_nodes.clone());
    }

    base_type
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_int_string() {
        let types = vec![TAtomic::TInt, TAtomic::TString];
        let result = combine(types, false);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_combine_true_false_to_bool() {
        let types = vec![TAtomic::TTrue, TAtomic::TFalse];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], TAtomic::TBool));
    }

    #[test]
    fn test_combine_false_true_to_bool() {
        let types = vec![TAtomic::TFalse, TAtomic::TTrue];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], TAtomic::TBool));
    }

    #[test]
    fn test_combine_mixed_never() {
        let types = vec![TAtomic::TNothing, TAtomic::TMixed];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0], TAtomic::TMixed),
            "Expected TMixed but got {:?}",
            result[0]
        );
    }

    #[test]
    fn test_combine_arrays() {
        let types = vec![
            TAtomic::TArray {
                key_type: Box::new(TUnion::new(TAtomic::TInt)),
                value_type: Box::new(TUnion::new(TAtomic::TString)),
            },
            TAtomic::TArray {
                key_type: Box::new(TUnion::new(TAtomic::TInt)),
                value_type: Box::new(TUnion::new(TAtomic::TInt)),
            },
        ];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        if let TAtomic::TArray { value_type, .. } = &result[0] {
            assert_eq!(value_type.types.len(), 2);
        } else {
            panic!("Expected TArray");
        }
    }

    #[test]
    fn test_combine_positive_int_and_zero() {
        let types = vec![
            TAtomic::TIntRange {
                min: Some(1),
                max: None,
            },
            TAtomic::TLiteralInt { value: 0 },
        ];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        if let TAtomic::TIntRange { min, max } = &result[0] {
            assert_eq!(*min, Some(0));
            assert_eq!(*max, None);
        } else {
            panic!("Expected TIntRange, got {:?}", result[0]);
        }
    }

    #[test]
    fn test_combine_bool_variants() {
        // true + bool = bool
        let types = vec![TAtomic::TTrue, TAtomic::TBool];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], TAtomic::TBool));

        // false + bool = bool
        let types = vec![TAtomic::TFalse, TAtomic::TBool];
        let result = combine(types, false);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], TAtomic::TBool));
    }
}

/// Mimics PHP's `is_numeric()` for a literal string value.
///
/// Unlike Rust's `f64::parse`, PHP's `is_numeric` rejects `inf`/`nan` and hex
/// (`0x..`) forms while allowing surrounding whitespace. Using it keeps
/// numeric-string combination decisions consistent with Psalm (which calls
/// `is_numeric`).
fn php_is_numeric(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let unsigned = trimmed.trim_start_matches(['+', '-']);
    let lower = unsigned.to_ascii_lowercase();

    // PHP rejects the non-decimal words Rust's float parser accepts, plus hex.
    if lower.starts_with("inf") || lower.starts_with("nan") || lower.contains('x') {
        return false;
    }

    trimmed.parse::<f64>().is_ok()
}
