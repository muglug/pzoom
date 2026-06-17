//! Port of Psalm's `ArrayFunctionArgumentsAnalyzer`: the by-ref out-type
//! special cases for the builtin array functions — array_pop/array_shift
//! (`handleByRefArrayAdjustment`), array_push/array_unshift
//! (`handleAddition`) and array_splice (`handleSplice`).

use mago_syntax::ast::ast::argument::Argument;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion, VarName, combine_union_types};

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Psalm's `handleSplice` replacement path: `array_splice($arr, $o, $l, $rep)`
/// leaves `$arr` as the combination of its own (list-ified) type and the
/// replacement's value type — int-keyed inputs stay lists instead of demoting
/// to the stub's array<array-key, mixed>.
pub(crate) fn handle_splice_by_ref(
    context: &mut BlockContext,
    analysis_data: &FunctionAnalysisData,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    var_name: &str,
) -> bool {
    if args.len() < 4 {
        // Psalm's no-replacement paths default to a plain array, which the
        // generic @param-out handling already produces.
        return false;
    }

    let var_id = VarName::new(var_name);
    let Some(existing_type) = context.get_var_type(&var_id).cloned() else {
        return false;
    };
    let Some(array_info) =
        function_call_analyzer::extract_array_like_info_from_union(&existing_type)
    else {
        return false;
    };

    let input_is_listy = existing_type.types.iter().all(|atomic| match atomic {
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => true,
        TAtomic::TKeyedArray { is_list, .. } => *is_list,
        TAtomic::TArray { key_type, .. } | TAtomic::TNonEmptyArray { key_type, .. } => {
            key_type.types.iter().all(|key_atomic| {
                matches!(
                    key_atomic,
                    TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
                )
            })
        }
        _ => false,
    });

    let Some(replacement_pos) = arg_positions.get(3).copied() else {
        return false;
    };
    let Some(replacement_type) = analysis_data.expr_types.get(&replacement_pos).cloned() else {
        return false;
    };
    let replacement_value_type = if let Some(replacement_info) =
        function_call_analyzer::extract_array_like_info_from_union(&replacement_type)
    {
        replacement_info.value_type
    } else if replacement_type.is_single() && replacement_type.has_string() {
        // Psalm wraps a single string replacement in array<int, string>.
        (*replacement_type).clone()
    } else {
        return false;
    };

    let combined_value = pzoom_code_info::combine_union_types(
        &array_info.value_type,
        &replacement_value_type,
        false,
    );
    let by_ref_type = if input_is_listy {
        TUnion::new(TAtomic::TList {
            value_type: Box::new(combined_value),
        })
    } else {
        TUnion::new(TAtomic::TArray {
            key_type: Box::new(pzoom_code_info::combine_union_types(
                &array_info.key_type,
                &TUnion::int(),
                false,
            )),
            value_type: Box::new(combined_value),
        })
    };

    context.set_var_type(var_id, by_ref_type);
    true
}

pub(crate) fn handle_by_ref_array_adjustment(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
    is_shift: bool,
) {
    let var_id = VarName::new(var_name);
    context.remove_var_from_conflicting_clauses(var_id.clone());

    let Some(existing_type) = context.locals.get(&var_id).cloned() else {
        return;
    };

    let inside_loop = context.inside_loop;
    let mut new_atomics: Vec<TAtomic> = Vec::new();

    for atomic in &existing_type.types {
        let mut atomic = atomic.clone();

        if let TAtomic::TKeyedArray {
            properties,
            is_list,
            fallback_key_type,
            fallback_value_type,
            ..
        } = &atomic
        {
            if *is_list && !inside_loop && (is_shift || fallback_value_type.is_none()) {
                // Drop the first (shift, reindexing) / last (pop) element of
                // the fixed shape.
                let mut int_entries: Vec<(i64, TUnion)> = properties
                    .iter()
                    .filter_map(|(key, value)| match key {
                        ArrayKey::Int(index) => Some((*index, value.clone())),
                        ArrayKey::String(_) | ArrayKey::ClassString(_) => None,
                    })
                    .collect();
                int_entries.sort_by_key(|(index, _)| *index);

                if is_shift {
                    if !int_entries.is_empty() {
                        int_entries.remove(0);
                    }
                } else {
                    int_entries.pop();
                }

                if int_entries.is_empty() {
                    new_atomics.push(match fallback_value_type {
                        Some(fallback_value) => TAtomic::TList {
                            value_type: fallback_value.clone(),
                        },
                        None => TAtomic::TArray {
                            key_type: Box::new(TUnion::new(TAtomic::TNothing)),
                            value_type: Box::new(TUnion::new(TAtomic::TNothing)),
                        },
                    });
                } else {
                    let reindexed: rustc_hash::FxHashMap<ArrayKey, TUnion> = int_entries
                        .into_iter()
                        .enumerate()
                        .map(|(new_index, (_, value))| (ArrayKey::Int(new_index as i64), value))
                        .collect();
                    new_atomics.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(reindexed),
                        is_list: true,
                        sealed: fallback_value_type.is_none(),
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                }
                continue;
            }

            // Degrade other shapes to their generic form before the
            // emptiness adjustment below.
            let value_type = properties
                .values()
                .fold(None::<TUnion>, |acc, value| {
                    Some(match acc {
                        None => value.clone(),
                        Some(existing) => combine_union_types(&existing, value, false),
                    })
                })
                .map(|value| match fallback_value_type {
                    Some(fallback) => combine_union_types(&value, fallback, false),
                    None => value,
                })
                .unwrap_or_else(TUnion::mixed);
            atomic = if *is_list {
                TAtomic::TList {
                    value_type: Box::new(value_type),
                }
            } else {
                TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(value_type),
                }
            };
        }

        match atomic {
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => {
                new_atomics.push(TAtomic::TArray {
                    key_type,
                    value_type,
                });
            }
            TAtomic::TNonEmptyList { value_type } => {
                new_atomics.push(TAtomic::TList { value_type });
            }
            other => new_atomics.push(other),
        }
    }

    if new_atomics.is_empty() {
        return;
    }

    let mut new_type = TUnion::from_types(pzoom_code_info::ttype::type_combiner::combine(
        new_atomics,
        false,
    ));
    new_type.parent_nodes = existing_type.parent_nodes.clone();
    let _ = analyzer;
    context.set_var_type(var_id, new_type);
}

/// Psalm's `ArrayFunctionArgumentsAnalyzer::handleAddition` for
/// array_push/array_unshift: the written-back type is derived from the
/// argument's current array type plus the added values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_array_addition(
    analyzer: &crate::statements_analyzer::StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    var_name: &str,
    is_unshift: bool,
) -> bool {
    let var_id = VarName::new(var_name);
    let Some(existing_type) = context.locals.get(&var_id).cloned() else {
        return false;
    };

    // Only act when the variable's type is a single array atomic the handler
    // models precisely: a fixed keyed list shape or the empty array. Psalm
    // also covers generic lists via its element-count tracking, which pzoom
    // does not have — those keep the generic @param-out flow.
    let array_atomic = match existing_type.get_single() {
        Some(atomic) => atomic.clone(),
        None => {
            // Psalm's Union holds at most one `array` atomic, so its
            // adjustment always sees one; fold pzoom's loop-merged
            // `array{}|list<T>`-style unions into the combined atomic so
            // push/unshift still adjusts precisely instead of falling back
            // to the stub's generic param-out.
            let all_arrays = existing_type.types.iter().all(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TArray { .. }
                        | TAtomic::TNonEmptyArray { .. }
                        | TAtomic::TList { .. }
                        | TAtomic::TNonEmptyList { .. }
                        | TAtomic::TKeyedArray { .. }
                )
            });
            if !all_arrays {
                return false;
            }
            let mut combined: Option<TUnion> = None;
            for atomic in &existing_type.types {
                let next = TUnion::new(atomic.clone());
                combined = Some(match combined {
                    None => next,
                    Some(existing) => combine_union_types(&existing, &next, false),
                });
            }
            match combined.and_then(|combined| combined.get_single().cloned()) {
                Some(atomic) => atomic,
                None => return false,
            }
        }
    };
    let is_empty_array = matches!(
        &array_atomic,
        TAtomic::TArray { key_type, value_type }
            if key_type.is_nothing() && value_type.is_nothing()
    );
    let is_definite_list_shape = matches!(
        &array_atomic,
        TAtomic::TKeyedArray { properties, is_list: true, .. }
            if properties.values().all(|value| !value.possibly_undefined)
    );
    // Generic (non-shape) arrays and lists need no element-count tracking:
    // Psalm's handleAddition always runs for them and the result is non-empty
    // (push/unshift onto any array yields at least one element).
    let is_generic_array_or_list = matches!(
        &array_atomic,
        TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
    );
    // An optional-entry list shape (a loop-merge of empty and filled states)
    // degrades to its generic list form before the addition — Psalm's
    // element-count tracking keeps these precise; folding to list<V> keeps
    // listness and the value union instead of bailing to the stub's
    // array<array-key, mixed> param-out.
    let array_atomic = match &array_atomic {
        TAtomic::TKeyedArray {
            properties,
            is_list: true,
            fallback_value_type,
            ..
        } if !is_definite_list_shape => {
            let combined_value = properties
                .values()
                .fold(None::<TUnion>, |acc, value| {
                    let mut value = value.clone();
                    value.possibly_undefined = false;
                    Some(match acc {
                        None => value,
                        Some(existing) => combine_union_types(&existing, &value, false),
                    })
                })
                .map(|value| match fallback_value_type {
                    Some(fallback) => combine_union_types(&value, fallback, false),
                    None => value,
                });
            match combined_value {
                Some(value_type) => TAtomic::TList {
                    value_type: Box::new(value_type),
                },
                None => array_atomic,
            }
        }
        _ => array_atomic,
    };
    let is_generic_array_or_list =
        is_generic_array_or_list || matches!(&array_atomic, TAtomic::TList { .. });

    if !is_empty_array && !is_definite_list_shape && !is_generic_array_or_list {
        // Non-list optional-entry shapes keep the generic flow.
        return false;
    }

    let value_arg_count = args.len().saturating_sub(1);
    let mut by_ref_type = TUnion::new(array_atomic.clone());

    for (argument_offset, arg) in args.iter().enumerate() {
        if argument_offset == 0 {
            continue;
        }

        let arg_value_type = arg_positions
            .get(argument_offset)
            .and_then(|pos| {
                crate::expr::call::arguments_analyzer::get_argument_value_type(
                    analysis_data,
                    arg,
                    *pos,
                )
            })
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed);

        // `array_unshift($a, $single)` writes offset 0; everything else just
        // contributes ints (Psalm's new_offset_type).
        let new_offset_type = if is_unshift && value_arg_count == 1 && !arg.is_unpacked() {
            TUnion::new(TAtomic::TLiteralInt { value: 0 })
        } else {
            TUnion::int()
        };

        if arg_value_type.is_mixed() {
            by_ref_type = combine_union_types(
                &by_ref_type,
                &TUnion::new(TAtomic::TArray {
                    key_type: Box::new(new_offset_type),
                    value_type: Box::new(TUnion::mixed()),
                }),
                false,
            );
        } else if arg.is_unpacked() {
            // Degrade unpacked shapes to their generic list/array form.
            let mut degraded = arg_value_type.clone();
            for atomic in degraded.types.iter_mut() {
                if let TAtomic::TKeyedArray {
                    properties,
                    is_list,
                    fallback_value_type,
                    ..
                } = atomic
                {
                    let value_type = properties
                        .values()
                        .fold(None::<TUnion>, |acc, value| {
                            Some(match acc {
                                None => value.clone(),
                                Some(existing) => combine_union_types(&existing, value, false),
                            })
                        })
                        .map(|value| match fallback_value_type {
                            Some(fallback) => combine_union_types(&value, fallback, false),
                            None => value,
                        })
                        .unwrap_or_else(TUnion::mixed);
                    *atomic = if *is_list {
                        TAtomic::TNonEmptyList {
                            value_type: Box::new(value_type),
                        }
                    } else {
                        TAtomic::TNonEmptyArray {
                            key_type: Box::new(TUnion::array_key()),
                            value_type: Box::new(value_type),
                        }
                    };
                }
            }
            by_ref_type = combine_union_types(&by_ref_type, &degraded, false);
        } else if let TAtomic::TKeyedArray {
            properties,
            is_list: true,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } = &array_atomic
        {
            // A fixed list shape gains the value at the front (unshift,
            // reindexing) or the end (push).
            let mut int_entries: Vec<TUnion> = {
                let mut entries: Vec<(i64, TUnion)> = properties
                    .iter()
                    .filter_map(|(key, value)| match key {
                        ArrayKey::Int(index) => Some((*index, value.clone())),
                        ArrayKey::String(_) | ArrayKey::ClassString(_) => None,
                    })
                    .collect();
                entries.sort_by_key(|(index, _)| *index);
                entries.into_iter().map(|(_, value)| value).collect()
            };
            if is_unshift {
                int_entries.insert(0, arg_value_type);
            } else {
                int_entries.push(arg_value_type);
            }
            let reindexed: rustc_hash::FxHashMap<ArrayKey, TUnion> = int_entries
                .into_iter()
                .enumerate()
                .map(|(index, value)| (ArrayKey::Int(index as i64), value))
                .collect();
            by_ref_type = TUnion::new(TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(reindexed),
                is_list: true,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.clone(),
                fallback_value_type: fallback_value_type.clone(),
            });
        } else if let TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } =
            &array_atomic
        {
            // Keep list-ness (Psalm's list representation is a keyed shape, so
            // its shape arm covers this): unshift puts the definite value at
            // offset 0 with the old values as the fallback; push keeps a
            // non-empty list of the combined value type.
            by_ref_type = if is_unshift {
                let mut properties = rustc_hash::FxHashMap::default();
                properties.insert(ArrayKey::Int(0), arg_value_type);
                TUnion::new(TAtomic::TKeyedArray {
                    properties: std::sync::Arc::new(properties),
                    is_list: true,
                    sealed: false,
                    fallback_key_type: Some(Box::new(TUnion::int())),
                    fallback_value_type: Some(value_type.clone()),
                })
            } else {
                TUnion::new(TAtomic::TNonEmptyList {
                    value_type: Box::new(combine_union_types(value_type, &arg_value_type, false)),
                })
            };
        } else if matches!(
            &array_atomic,
            TAtomic::TArray { key_type, value_type }
                if key_type.is_nothing() && value_type.is_nothing()
        ) {
            // Adding to an empty array yields the one-element list shape.
            let mut properties = rustc_hash::FxHashMap::default();
            properties.insert(ArrayKey::Int(0), arg_value_type);
            by_ref_type = TUnion::new(TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(properties),
                is_list: true,
                sealed: true,
                fallback_key_type: None,
                fallback_value_type: None,
            });
        } else {
            // overwrite_empty_array=true, like Psalm's handleAddition generic
            // arm: one non-empty member makes the combined array non-empty
            // (push/unshift onto any array yields at least one element).
            by_ref_type = combine_union_types(
                &by_ref_type,
                &TUnion::new(TAtomic::TNonEmptyArray {
                    key_type: Box::new(new_offset_type),
                    value_type: Box::new(arg_value_type),
                }),
                true,
            );
        }
    }

    // Pushing onto `$this->prop` is a property write: check the new array
    // against the declared property type (Psalm's handleAddition runs the
    // virtual `$arr[] = $v` through InstancePropertyAssignmentAnalyzer).
    if let Some(prop_name) = var_name.strip_prefix("$this->")
        && !prop_name.contains("->")
        && !prop_name.contains('[')
        && let Some(declaring_class) = analyzer.get_declaring_class()
        && let Some(class_info) = analyzer.codebase.get_class(declaring_class)
    {
        let prop_id = analyzer.interner.intern(prop_name);
        if let Some(prop_type) = class_info
            .properties
            .get(&prop_id)
            .and_then(|prop_info| prop_info.get_type())
        {
            let mut comparison_result =
                crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
            if !crate::type_comparator::union_type_comparator::is_contained_by(
                analyzer.codebase,
                &by_ref_type,
                prop_type,
                false,
                false,
                &mut comparison_result,
            ) && !comparison_result.type_coerced.unwrap_or(false)
            {
                let issue_pos = arg_positions.first().copied().unwrap_or((0, 0));
                let (line, col) = analyzer.get_line_column(issue_pos.0);
                analysis_data.add_issue(pzoom_code_info::Issue::new(
                    pzoom_code_info::IssueKind::InvalidPropertyAssignmentValue,
                    format!(
                        "Property {}::${} expects {}, got {}",
                        analyzer.interner.lookup(declaring_class),
                        prop_name,
                        prop_type.get_id(Some(analyzer.interner)),
                        by_ref_type.get_id(Some(analyzer.interner)),
                    ),
                    analyzer.file_path,
                    issue_pos.0,
                    issue_pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    by_ref_type.parent_nodes = existing_type.parent_nodes.clone();
    context.remove_var_from_conflicting_clauses(var_id.clone());
    context.set_var_type(var_id, by_ref_type);
    true
}
