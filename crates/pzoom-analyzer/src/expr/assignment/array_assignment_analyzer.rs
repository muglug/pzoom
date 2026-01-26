//! Array assignment analyzer.

use mago_syntax::ast::ast::array::ArrayAccess;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{combine_union_types, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an array assignment ($arr[key] = value or $arr[] = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the array expression
    let array_pos = expr_analyzer::analyze(analyzer, access.array, analysis_data, context);
    let array_type = analysis_data.get_expr_type(array_pos);

    // Analyze the key expression
    let key_pos = expr_analyzer::analyze(analyzer, access.index, analysis_data, context);
    let key_type = analysis_data.get_expr_type(key_pos).map(|t| (*t).clone());

    // Analyze the value expression
    let value_pos = expr_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Update the array type
    if let Some(array_type) = array_type {
        let new_array_type = update_array_type(&array_type, key_type, value_type.clone());

        // Update the variable in context if it's a simple variable
        if let Expression::Variable(var) = access.array {
            if let mago_syntax::ast::ast::variable::Variable::Direct(direct) = var {
                let var_id = analyzer.interner.intern(direct.name);
                // Use set_var_type to properly track assignment
                context.set_var_type(var_id, new_array_type.clone());
            }
        }
    }

    // The assignment expression returns the assigned value
    analysis_data.set_expr_type(pos, value_type);
}

/// Update an array type with a new key-value pair.
fn update_array_type(array_type: &TUnion, key_type: Option<TUnion>, value_type: TUnion) -> TUnion {
    let mut result_types = Vec::new();

    for atomic in &array_type.types {
        match atomic {
            TAtomic::TArray {
                key_type: existing_key,
                value_type: existing_value,
            } => {
                // Merge the new key and value types using the type combiner
                let new_key = if let Some(ref kt) = key_type {
                    combine_union_types(existing_key, kt, false)
                } else {
                    (**existing_key).clone()
                };

                let new_value = combine_union_types(existing_value, &value_type, false);

                result_types.push(TAtomic::TNonEmptyArray {
                    key_type: Box::new(new_key),
                    value_type: Box::new(new_value),
                });
            }
            TAtomic::TNonEmptyArray {
                key_type: existing_key,
                value_type: existing_value,
            } => {
                let new_key = if let Some(ref kt) = key_type {
                    combine_union_types(existing_key, kt, false)
                } else {
                    (**existing_key).clone()
                };

                let new_value = combine_union_types(existing_value, &value_type, false);

                result_types.push(TAtomic::TNonEmptyArray {
                    key_type: Box::new(new_key),
                    value_type: Box::new(new_value),
                });
            }
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                let mut new_properties = properties.clone();

                // If we have a literal key, update that specific property
                if let Some(ref kt) = key_type {
                    if let Some(literal_key) = get_literal_key(kt) {
                        new_properties.insert(literal_key, value_type.clone());

                        result_types.push(TAtomic::TKeyedArray {
                            properties: new_properties,
                            is_list: *is_list && matches!(key_type, Some(ref k) if is_sequential_int_key(k, properties.len())),
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    } else {
                        // Non-literal key - we can't track the exact property
                        // Convert to a general array type
                        let existing_value_types: Vec<_> = properties.values().cloned().collect();
                        let mut combined_value = if existing_value_types.is_empty() {
                            TUnion::mixed()
                        } else {
                            let mut result = existing_value_types[0].clone();
                            for t in &existing_value_types[1..] {
                                result = combine_union_types(&result, t, false);
                            }
                            result
                        };

                        combined_value = combine_union_types(&combined_value, &value_type, false);

                        result_types.push(TAtomic::TNonEmptyArray {
                            key_type: Box::new(kt.clone()),
                            value_type: Box::new(combined_value),
                        });
                    }
                } else {
                    // Append operation - add to end
                    let next_index = properties.len() as i64;
                    new_properties.insert(ArrayKey::Int(next_index), value_type.clone());

                    result_types.push(TAtomic::TKeyedArray {
                        properties: new_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                }
            }
            _ => {
                result_types.push(atomic.clone());
            }
        }
    }

    if result_types.is_empty() {
        // Create a new array type
        TUnion::new(TAtomic::TNonEmptyArray {
            key_type: Box::new(key_type.unwrap_or_else(TUnion::int)),
            value_type: Box::new(value_type),
        })
    } else {
        TUnion::from_types(result_types)
    }
}

/// Try to extract a literal key from a type union.
fn get_literal_key(key_type: &TUnion) -> Option<ArrayKey> {
    if key_type.types.len() != 1 {
        return None;
    }

    match &key_type.types[0] {
        TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => Some(ArrayKey::String(value.clone())),
        _ => None,
    }
}

/// Check if a key type represents a sequential integer key for a list.
fn is_sequential_int_key(key_type: &TUnion, current_len: usize) -> bool {
    if key_type.types.len() != 1 {
        return false;
    }

    matches!(&key_type.types[0], TAtomic::TLiteralInt { value } if *value == current_len as i64)
}
