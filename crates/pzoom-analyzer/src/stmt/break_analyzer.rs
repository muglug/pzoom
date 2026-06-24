//! `break` statement analyzer.
//!
//! Mirrors Hakana's `break_analyzer`: records the break against the innermost loop
//! scope (so the loop analyzer knows the loop can exit early and which variables it
//! may have redefined by that point) and marks the current path as terminated.

use pzoom_code_info::combine_union_types;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};

pub fn analyze(
    _analyzer: &StatementsAnalyzer<'_>,
    break_stmt: &mago_syntax::cst::cst::r#loop::Break<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Psalm: a break leaves the switch only when its level is < 2
    // (`break 2` inside a switch exits the enclosing loop instead).
    let leaving_switch = matches!(context.break_types.last(), Some(BreakContext::Switch))
        && crate::stmt::continue_analyzer::break_level(break_stmt.level) < 2;

    // A break that leaves a switch carries its context to the post-switch
    // merge (Hakana's case_scope.break_vars).
    if leaving_switch && let Some(frame) = analysis_data.switch_break_contexts.last_mut() {
        frame.push(context.clone());
    }

    let scope_index = crate::stmt::continue_analyzer::loop_scope_index_for_level(
        &context.break_types,
        analysis_data.loop_scopes.len(),
        break_stmt.level,
    );
    if let Some(loop_scope) = scope_index.and_then(|index| analysis_data.loop_scopes.get_mut(index))
    {
        if leaving_switch {
            loop_scope.final_actions.insert(ControlAction::LeaveSwitch);
            context.control_actions.insert(ControlAction::LeaveSwitch);
        } else {
            loop_scope.final_actions.insert(ControlAction::Break);
            context.control_actions.insert(ControlAction::Break);
        }

        // Psalm's BreakAnalyzer records `getRedefinedVars(loop_parent
        // vars_in_scope)`: only variables the body actually redefined by
        // break time — present in the loop-parent scope with a *different*
        // type. Unchanged variables stay definitely-assigned after the loop
        // (recording everything demoted pre-loop vars to possibly-assigned).
        for (var_id, var_type) in &context.locals {
            let Some(parent_type) = loop_scope.parent_context_vars.get(var_id) else {
                continue;
            };

            // Psalm's `Union::equals` (parent dataflow nodes included): a
            // reassignment to the same display type is still a redefinition —
            // its new parent nodes must reach the post-loop merge.
            if parent_type == var_type {
                continue;
            }

            let combined = match loop_scope.possibly_redefined_loop_parent_vars.get(var_id) {
                Some(existing) => combine_union_types(var_type, existing, false),
                None => var_type.as_ref().clone(),
            };
            loop_scope
                .possibly_redefined_loop_parent_vars
                .insert(var_id.clone(), combined);
        }

        if loop_scope.iteration_count == 0 {
            for (var_id, var_type) in &context.locals {
                if !loop_scope.parent_context_vars.contains_key(var_id) {
                    let combined = match loop_scope.possibly_defined_loop_parent_vars.get(var_id) {
                        Some(existing) => combine_union_types(var_type, existing, false),
                        None => var_type.as_ref().clone(),
                    };
                    loop_scope
                        .possibly_defined_loop_parent_vars
                        .insert(var_id.clone(), combined);
                }
            }
        }
    } else {
        context.control_actions.insert(ControlAction::Break);
    }

    context.has_returned = true;
}
