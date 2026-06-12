//! `"array_column"` return-type provider.
//!
//! Ports Psalm's `ArrayColumnReturnTypeProvider` (general path). The column/key
//! selectors are resolved from literal arguments, the row "shape" is taken from
//! the input array's value type (objects are expanded to their public-property
//! shape, mirroring `get_object_vars`), and the result is keyed by the index
//! column when one is given or a list otherwise.

use rustc_hash::FxHashMap;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{ArrayKey, TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) struct ArrayColumnReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayColumnReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_column"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_column_return_type(event.analyzer, event.arg_positions, analysis_data)
    }
}

fn infer_array_column_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    if arg_positions.len() < 2 {
        return None;
    }

    // Value column (2nd arg): a literal key, or null.
    let value_arg = analysis_data.expr_types.get(&arg_positions[1]).cloned()?;
    let value_column = single_literal_array_key(&value_arg);
    let value_column_is_null = value_arg.is_null();

    // Key column (3rd arg, optional): a literal key, or null.
    let third_present = arg_positions.len() >= 3;
    let (key_column, key_column_is_null) = if third_present {
        let key_arg = analysis_data.expr_types.get(&arg_positions[2]).cloned()?;
        (single_literal_array_key(&key_arg), key_arg.is_null())
    } else {
        (None, false)
    };

    // Row type = the value type of the input array.
    let input_type = analysis_data.expr_types.get(&arg_positions[0]).cloned()?;
    let (row_type, input_not_empty) = input_array_value_type(&input_type);

    let row_shape = row_type
        .as_ref()
        .and_then(|row| row_shape_properties(analyzer, row));

    let mut result_key_type = TUnion::array_key();
    let mut result_element_type: Option<TUnion> = if value_column_is_null {
        row_type.clone()
    } else {
        None
    };
    let mut have_at_least_one_res = false;

    if let Some(properties) = &row_shape {
        if let Some(value_column) = &value_column {
            if let Some(element) = properties.get(value_column) {
                let possibly_undefined = element.possibly_undefined;
                let mut element = element.clone();
                // array_column skips undefined elements, so the result element is
                // always defined.
                element.possibly_undefined = false;
                result_element_type = Some(element);
                if input_not_empty && !possibly_undefined {
                    have_at_least_one_res = true;
                }
            } else if !value_column_is_null {
                result_element_type = Some(TUnion::mixed());
            }
        } else if !value_column_is_null {
            result_element_type = Some(TUnion::mixed());
        }

        if let Some(key_column) = &key_column {
            if let Some(key) = properties.get(key_column) {
                result_key_type = key.clone();
            }
        }
    }

    let element = result_element_type.unwrap_or_else(TUnion::mixed);

    let atomic = if third_present && !key_column_is_null {
        if have_at_least_one_res {
            TAtomic::TNonEmptyArray {
                key_type: Box::new(result_key_type),
                value_type: Box::new(element),
            }
        } else {
            TAtomic::TArray {
                key_type: Box::new(result_key_type),
                value_type: Box::new(element),
            }
        }
    } else if have_at_least_one_res {
        TAtomic::TNonEmptyList {
            value_type: Box::new(element),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(element),
        }
    };

    Some(TUnion::new(atomic))
}

/// The single literal array key a union represents, if any.
fn single_literal_array_key(union: &TUnion) -> Option<ArrayKey> {
    match union.get_single()? {
        TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => Some(ArrayKey::String(value.clone())),
        _ => None,
    }
}

/// The value (row) type of an input array argument, plus whether the array is
/// known to be non-empty.
fn input_array_value_type(input: &TUnion) -> (Option<TUnion>, bool) {
    let Some(atomic) = input.get_single() else {
        return (None, false);
    };

    match atomic {
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut combined: Option<TUnion> = None;
            let mut has_definite = false;
            for value in properties.values() {
                if !value.possibly_undefined {
                    has_definite = true;
                }
                combined = Some(match combined {
                    Some(existing) => combine_union_types(&existing, value, false),
                    None => value.clone(),
                });
            }
            if let Some(fallback) = fallback_value_type {
                combined = Some(match combined {
                    Some(existing) => combine_union_types(&existing, fallback, false),
                    None => (**fallback).clone(),
                });
            }
            (combined, has_definite)
        }
        TAtomic::TArray { value_type, .. } => (Some((**value_type).clone()), false),
        TAtomic::TNonEmptyArray { value_type, .. } => (Some((**value_type).clone()), true),
        TAtomic::TList { value_type } => (Some((**value_type).clone()), false),
        TAtomic::TNonEmptyList { value_type } => (Some((**value_type).clone()), true),
        _ => (None, false),
    }
}

/// The "row shape" — a map of known keys to value types — for a row type.
/// Arrays contribute their keyed-array properties; objects are expanded to their
/// public-property shape (mirroring Psalm's `getRowShape` → `get_object_vars`).
fn row_shape_properties(
    analyzer: &StatementsAnalyzer<'_>,
    row: &TUnion,
) -> Option<FxHashMap<ArrayKey, TUnion>> {
    match row.get_single()? {
        TAtomic::TKeyedArray { properties, .. } => Some((**properties).clone()),
        TAtomic::TObjectWithProperties { properties, .. } => Some(properties.clone()),
        TAtomic::TNamedObject { name, .. } => {
            let class_info = analyzer.codebase.get_class(*name)?;
            let calling_class = analyzer.get_declaring_class();
            let mut properties: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
            for (prop_id, prop_info) in &class_info.properties {
                // Mirror Psalm's get_object_vars: include every property visible
                // from the current context, not just public ones.
                let declaring = class_info
                    .declaring_property_ids
                    .get(prop_id)
                    .copied()
                    .unwrap_or(*name);
                let accessible = match prop_info.visibility {
                    Visibility::Public => true,
                    Visibility::Private => calling_class == Some(declaring),
                    Visibility::Protected => calling_class
                        .is_some_and(|caller| classes_are_related(analyzer, caller, declaring)),
                };
                if !accessible {
                    continue;
                }
                let prop_name = analyzer.interner.lookup(*prop_id);
                let key = ArrayKey::String(prop_name.trim_start_matches('$').to_string());
                properties.insert(
                    key,
                    prop_info.get_type().cloned().unwrap_or_else(TUnion::mixed),
                );
            }
            if properties.is_empty() {
                None
            } else {
                Some(properties)
            }
        }
        _ => None,
    }
}

/// Whether two classes are in the same hierarchy (so a `protected` member of one
/// is visible from the other).
fn classes_are_related(
    analyzer: &StatementsAnalyzer<'_>,
    a: pzoom_str::StrId,
    b: pzoom_str::StrId,
) -> bool {
    if a == b {
        return true;
    }
    let a_extends_b = analyzer
        .codebase
        .get_class(a)
        .is_some_and(|info| info.all_parent_classes.contains(&b));
    let b_extends_a = analyzer
        .codebase
        .get_class(b)
        .is_some_and(|info| info.all_parent_classes.contains(&a));
    a_extends_b || b_extends_a
}
