//! Foreach statement analyzer.

use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::foreach::{Foreach, ForeachTarget};
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer::analyze_stmts;

/// Analyze a foreach statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    foreach: &Foreach<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the iterable expression
    let iterable_pos =
        expression_analyzer::analyze(analyzer, foreach.expression, analysis_data, context);
    let iterable_type = analysis_data.get_expr_type(iterable_pos);
    let pre_loop_assigned_var_ids = context.assigned_var_ids.clone();
    let pre_loop_possibly_assigned_var_ids = context.possibly_assigned_var_ids.clone();

    // Create loop context
    let mut loop_context = context.child();
    loop_context.inside_loop = true;
    loop_context.inside_foreach = true;
    loop_context.assigned_var_ids = pre_loop_assigned_var_ids.clone();
    loop_context.possibly_assigned_var_ids = pre_loop_possibly_assigned_var_ids;

    // Determine the value type from the iterable
    let value_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_value_type(iter_type, analyzer)
    } else {
        TUnion::mixed()
    };

    // Determine the key type from the iterable
    let key_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_key_type(iter_type, analyzer)
    } else {
        TUnion::array_key()
    };

    // Set the iterator variable types in loop context
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            mark_foreach_reference_target(value_target.value, analyzer, &mut loop_context);
            set_expression_var_type(value_target.value, &value_type, analyzer, &mut loop_context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_expression_var_type(kv_target.key, &key_type, analyzer, &mut loop_context);
            mark_foreach_reference_target(kv_target.value, analyzer, &mut loop_context);
            set_expression_var_type(kv_target.value, &value_type, analyzer, &mut loop_context);
        }
    }

    // Analyze the loop body using helper method
    let body_stmts = foreach.body.statements();
    analyze_stmts(analyzer, body_stmts, analysis_data, &mut loop_context)?;

    // Variables assigned in the loop body are "possibly assigned" in the parent
    // Also propagate their types back (variables modified in loop have their new types)
    for (var_id, loop_assigned_count) in &loop_context.assigned_var_ids {
        let pre_loop_count = pre_loop_assigned_var_ids.get(var_id).copied().unwrap_or(0);
        if *loop_assigned_count <= pre_loop_count {
            continue;
        }

        let parent_had_local = context.locals.contains_key(var_id);
        if !parent_had_local {
            context.possibly_assigned_var_ids.insert(*var_id);
        }

        // Propagate the modified type back to parent context
        if let Some(loop_type) = loop_context.locals.get(var_id) {
            if let Some(parent_type) = context.locals.get(var_id) {
                // Combine the parent type with the loop-modified type
                let combined = combine_union_types(parent_type, loop_type, false);
                context.locals.insert(*var_id, combined);
            } else {
                // Variable was created in the loop
                context.locals.insert(*var_id, loop_type.clone());
            }
        }
    }

    // Iterator variables are now visible in the parent scope (PHP quirk)
    // They have the loop's type after the loop finishes
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            set_expression_var_type(value_target.value, &value_type, analyzer, context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_expression_var_type(kv_target.key, &key_type, analyzer, context);
            set_expression_var_type(kv_target.value, &value_type, analyzer, context);
        }
    }

    context.update_references_possibly_from_confusing_scope(&loop_context);

    Ok(())
}

/// Extract the value type from an iterable type.
fn extract_iterable_value_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut value_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNonEmptyArray { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TList { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNonEmptyList { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                // Union of all property types
                for prop_type in properties.values() {
                    value_types.push(prop_type.clone());
                }
                if let Some(fallback) = fallback_value_type {
                    value_types.push((**fallback).clone());
                }
            }
            TAtomic::TIterable { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNamedObject { type_params, .. } => {
                if let Some(type_params) = type_params {
                    if type_params.len() >= 2 {
                        value_types.push(type_params[1].clone());
                    } else if let Some(first) = type_params.first() {
                        value_types.push(first.clone());
                    } else {
                        value_types.push(TUnion::mixed());
                    }
                } else {
                    value_types.push(TUnion::mixed());
                }
            }
            _ => {}
        }
    }

    if value_types.is_empty() {
        TUnion::mixed()
    } else {
        // Combine all value types using the type combiner
        let mut result = value_types.remove(0);
        for t in value_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Extract the key type from an iterable type.
fn extract_iterable_key_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TNonEmptyArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => key_types.push(TUnion::int()),
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                ..
            } => {
                // Union of all property key types
                for key in properties.keys() {
                    match key {
                        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                            key_types.push(TUnion::new(TAtomic::TLiteralInt {
                                value: *value,
                            }));
                        }
                        pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                            if let Ok(int_value) = value.parse::<i64>() {
                                key_types.push(TUnion::new(TAtomic::TLiteralInt {
                                    value: int_value,
                                }));
                            } else {
                                key_types.push(TUnion::new(TAtomic::TLiteralString {
                                    value: value.clone(),
                                }));
                            }
                        }
                    }
                }
                if let Some(fallback) = fallback_key_type {
                    key_types.push((**fallback).clone());
                }
            }
            TAtomic::TIterable { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TNamedObject { type_params, .. } => {
                if let Some(type_params) = type_params {
                    if type_params.len() >= 2 {
                        key_types.push(type_params[0].clone());
                    } else {
                        key_types.push(TUnion::array_key());
                    }
                } else {
                    key_types.push(TUnion::array_key());
                }
            }
            _ => {}
        }
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        // Combine all key types using the type combiner
        let mut result = key_types.remove(0);
        for t in key_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Set a variable's type in the context from an expression.
fn set_expression_var_type(
    expr: &Expression<'_>,
    var_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let target = unwrap_reference_target(expr);

    match target.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            let var_id = analyzer.interner.intern(direct.name);
            context.set_var_type(var_id, var_type.clone());
        }
        Expression::List(list) => {
            for (offset, element) in list.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        Expression::Array(array) => {
            for (offset, element) in array.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
enum DestructuringLookupKey {
    Int(i64),
    String(String),
    Unknown,
}

fn set_destructuring_element_var_type(
    element: &ArrayElement<'_>,
    offset: usize,
    source_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let (target_expr, lookup_key) = match element {
        ArrayElement::Missing(_) | ArrayElement::Variadic(_) => return,
        ArrayElement::Value(value_element) => (
            value_element.value,
            DestructuringLookupKey::Int(offset as i64),
        ),
        ArrayElement::KeyValue(key_value) => (
            key_value.value,
            extract_destructuring_key(key_value.key).unwrap_or(DestructuringLookupKey::Unknown),
        ),
    };

    let target_type = infer_destructured_value_type(source_type, &lookup_key);
    set_expression_var_type(target_expr, &target_type, analyzer, context);
}

fn extract_destructuring_key(expr: &Expression<'_>) -> Option<DestructuringLookupKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .map(|value| DestructuringLookupKey::Int(value as i64)),
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| DestructuringLookupKey::String(value.to_string())),
        _ => None,
    }
}

fn infer_destructured_value_type(
    source_type: &TUnion,
    lookup_key: &DestructuringLookupKey,
) -> TUnion {
    let mut inferred_type: Option<TUnion> = None;
    let mut saw_destructurable_type = false;

    for atomic in &source_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                saw_destructurable_type = true;
                add_inferred_union(&mut inferred_type, value_type);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                saw_destructurable_type = true;
                if let Some(array_key) = lookup_key_to_array_key(lookup_key) {
                    if let Some(property_type) = properties.get(&array_key) {
                        add_inferred_union(&mut inferred_type, property_type);
                    } else if let Some(fallback_value_type) = fallback_value_type {
                        add_inferred_union(&mut inferred_type, fallback_value_type);
                    }
                } else if let Some(fallback_value_type) = fallback_value_type {
                    add_inferred_union(&mut inferred_type, fallback_value_type);
                } else {
                    for property_type in properties.values() {
                        add_inferred_union(&mut inferred_type, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return TUnion::mixed(),
            _ => {}
        }
    }

    if let Some(inferred_type) = inferred_type {
        inferred_type
    } else if saw_destructurable_type {
        TUnion::mixed()
    } else {
        source_type.clone()
    }
}

fn lookup_key_to_array_key(key: &DestructuringLookupKey) -> Option<ArrayKey> {
    match key {
        DestructuringLookupKey::Int(value) => Some(ArrayKey::Int(*value)),
        DestructuringLookupKey::String(value) => Some(ArrayKey::String(value.clone())),
        DestructuringLookupKey::Unknown => None,
    }
}

fn add_inferred_union(target: &mut Option<TUnion>, next: &TUnion) {
    if let Some(existing) = target {
        *existing = combine_union_types(existing, next, false);
    } else {
        *target = Some(next.clone());
    }
}

fn unwrap_reference_target<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    if let Expression::UnaryPrefix(unary) = expr.unparenthesized()
        && matches!(unary.operator, UnaryPrefixOperator::Reference(_))
    {
        return unary.operand;
    }

    expr
}

fn mark_foreach_reference_target(
    expr: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return;
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Reference(_)) {
        return;
    }

    let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized() else {
        return;
    };

    let var_id = analyzer.interner.intern(direct.name);
    context.clear_confusing_reference(var_id);
    context.mark_external_reference(var_id);
}
