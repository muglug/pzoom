//! Foreach statement analyzer.

use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::r#loop::foreach::{Foreach, ForeachTarget};

use pzoom_code_info::{combine_union_types, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::analyze_stmts;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze a foreach statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    foreach: &Foreach<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the iterable expression
    let iterable_pos = expr_analyzer::analyze(analyzer, foreach.expression, analysis_data, context);
    let iterable_type = analysis_data.get_expr_type(iterable_pos);

    // Create loop context
    let mut loop_context = context.child();
    loop_context.inside_loop = true;

    // Determine the value type from the iterable
    let value_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_value_type(iter_type)
    } else {
        TUnion::mixed()
    };

    // Determine the key type from the iterable
    let key_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_key_type(iter_type)
    } else {
        TUnion::array_key()
    };

    // Set the iterator variable types in loop context
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            set_expression_var_type(value_target.value, &value_type, analyzer, &mut loop_context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_expression_var_type(kv_target.key, &key_type, analyzer, &mut loop_context);
            set_expression_var_type(kv_target.value, &value_type, analyzer, &mut loop_context);
        }
    }

    // Analyze the loop body using helper method
    let body_stmts = foreach.body.statements();
    analyze_stmts(analyzer, body_stmts, analysis_data, &mut loop_context)?;

    // Variables assigned in the loop body are "possibly assigned" in the parent
    // Also propagate their types back (variables modified in loop have their new types)
    for var_id in loop_context.assigned_var_ids.keys() {
        context.possibly_assigned_var_ids.insert(*var_id);

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

    Ok(())
}

/// Extract the value type from an iterable type.
fn extract_iterable_value_type(iter_type: &TUnion) -> TUnion {
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
            TAtomic::TNamedObject { .. } => {
                // Could be Traversable - return mixed for now
                // TODO: Check if implements Traversable and get value type
                value_types.push(TUnion::mixed());
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
fn extract_iterable_key_type(iter_type: &TUnion) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TNonEmptyArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => {
                key_types.push(TUnion::int())
            }
            TAtomic::TKeyedArray { properties, fallback_key_type, .. } => {
                // Union of all property key types
                for key in properties.keys() {
                    match key {
                        pzoom_code_info::t_atomic::ArrayKey::Int(_) => {
                            key_types.push(TUnion::int());
                        }
                        pzoom_code_info::t_atomic::ArrayKey::String(_) => {
                            key_types.push(TUnion::string());
                        }
                    }
                }
                if let Some(fallback) = fallback_key_type {
                    key_types.push((**fallback).clone());
                }
            }
            TAtomic::TIterable { key_type, .. } => key_types.push((**key_type).clone()),
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
    if let Expression::Variable(var) = expr {
        if let mago_syntax::ast::ast::variable::Variable::Direct(direct) = var {
            let var_name = direct.name;
            // Intern the variable name
            let var_id = analyzer.interner.intern(var_name);
            context.set_var_type(var_id, var_type.clone());
        }
    }
}
