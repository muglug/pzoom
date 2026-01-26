//! Array expression analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::array::{Array, ArrayElement, List};

use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{combine_union_types, TAtomic, TUnion};
use rustc_hash::FxHashMap;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an array creation expression.
pub fn analyze_array(
    analyzer: &StatementsAnalyzer<'_>,
    array: &Array<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if array.elements.is_empty() {
        // Empty array
        analysis_data.set_expr_type(
            pos,
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            }),
        );
        return;
    }

    let mut known_items: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
    let mut key_types: Vec<TAtomic> = Vec::new();
    let mut value_types: Vec<TUnion> = Vec::new();
    let mut next_int_key: i64 = 0;
    let mut is_list = true;
    let mut all_keys_known = true;

    for element in array.elements.iter() {
        match element {
            ArrayElement::KeyValue(kv) => {
                // Analyze key and value
                let key_pos = expr_analyzer::analyze(analyzer, kv.key, analysis_data, context);
                let value_pos = expr_analyzer::analyze(analyzer, kv.value, analysis_data, context);

                let key_type = analysis_data.get_expr_type(key_pos);
                let value_type = analysis_data
                    .get_expr_type(value_pos)
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::mixed);

                // Check if key is a literal
                if let Some(kt) = key_type {
                    match kt.types.first() {
                        Some(TAtomic::TLiteralInt { value }) => {
                            known_items.insert(ArrayKey::Int(*value), value_type.clone());
                            key_types.push(TAtomic::TInt);
                            if *value != next_int_key {
                                is_list = false;
                            }
                            next_int_key = value + 1;
                        }
                        Some(TAtomic::TLiteralString { value }) => {
                            known_items.insert(ArrayKey::String(value.clone()), value_type.clone());
                            key_types.push(TAtomic::TString);
                            is_list = false;
                        }
                        _ => {
                            all_keys_known = false;
                            if let Some(first) = kt.types.first() {
                                key_types.push(first.clone());
                            }
                            is_list = false;
                        }
                    }
                } else {
                    all_keys_known = false;
                    is_list = false;
                }

                value_types.push(value_type);
            }
            ArrayElement::Value(val) => {
                // Implicit integer key
                let value_pos = expr_analyzer::analyze(analyzer, val.value, analysis_data, context);

                let value_type = analysis_data
                    .get_expr_type(value_pos)
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::mixed);

                known_items.insert(ArrayKey::Int(next_int_key), value_type.clone());
                key_types.push(TAtomic::TInt);
                value_types.push(value_type);
                next_int_key += 1;
            }
            ArrayElement::Variadic(variadic) => {
                // Spreading another array - we lose key information
                let _spread_pos =
                    expr_analyzer::analyze(analyzer, variadic.value, analysis_data, context);
                all_keys_known = false;
                is_list = false;
            }
            ArrayElement::Missing(_) => {
                // Missing element (syntax error recovery)
            }
        }
    }

    // Determine the array type
    let expr_type = if all_keys_known && !known_items.is_empty() {
        // We have a keyed array with known keys
        TUnion::new(TAtomic::TKeyedArray {
            properties: known_items,
            is_list,
            sealed: true,
            fallback_key_type: None,
            fallback_value_type: None,
        })
    } else if is_list && !value_types.is_empty() {
        // It's a list
        let value_union = combine_types(value_types);
        TUnion::new(TAtomic::TNonEmptyList {
            value_type: Box::new(value_union),
        })
    } else {
        // General array
        let key_union = if key_types.is_empty() {
            TUnion::array_key()
        } else {
            let combined = type_combiner::combine(key_types, false);
            TUnion::from_types(combined)
        };

        let value_union = combine_types(value_types);

        TUnion::new(TAtomic::TNonEmptyArray {
            key_type: Box::new(key_union),
            value_type: Box::new(value_union),
        })
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze a list() expression (used as LHS of assignment).
pub fn analyze_list(
    analyzer: &StatementsAnalyzer<'_>,
    list: &List<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // list() is typically used on the LHS of assignment for destructuring
    // When analyzed as an expression, it represents the pattern being matched

    for element in list.elements.iter() {
        match element {
            ArrayElement::Value(val) => {
                // This is a variable or nested list that will receive a value
                let elem_span = val.value.span();
                let elem_pos: Pos = (elem_span.start.offset, elem_span.end.offset);
                let _inner_pos = expr_analyzer::analyze(analyzer, val.value, analysis_data, context);
                analysis_data.set_expr_type(elem_pos, TUnion::mixed());
            }
            ArrayElement::KeyValue(kv) => {
                // Keyed destructuring: list('key' => $var)
                let _key_pos = expr_analyzer::analyze(analyzer, kv.key, analysis_data, context);
                let elem_span = kv.value.span();
                let elem_pos: Pos = (elem_span.start.offset, elem_span.end.offset);
                let _inner_pos = expr_analyzer::analyze(analyzer, kv.value, analysis_data, context);
                analysis_data.set_expr_type(elem_pos, TUnion::mixed());
            }
            ArrayElement::Missing(_) => {
                // Skipped element
            }
            ArrayElement::Variadic(_) => {
                // Variadic in list() - this is a syntax error in PHP
            }
        }
    }

    // The list expression itself has an array type
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Combine multiple types into a union.
fn combine_types(types: Vec<TUnion>) -> TUnion {
    if types.is_empty() {
        return TUnion::mixed();
    }

    let mut result = types[0].clone();
    for t in &types[1..] {
        result = combine_union_types(&result, t, false);
    }
    result
}
