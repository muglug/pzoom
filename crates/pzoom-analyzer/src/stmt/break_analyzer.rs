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
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let leaving_switch = matches!(context.break_types.last(), Some(BreakContext::Switch));

    if let Some(loop_scope) = analysis_data.loop_scopes.last_mut() {
        if leaving_switch {
            loop_scope.final_actions.insert(ControlAction::LeaveSwitch);
            context.control_actions.insert(ControlAction::LeaveSwitch);
        } else {
            loop_scope.final_actions.insert(ControlAction::Break);
            context.control_actions.insert(ControlAction::Break);
        }

        // Every variable in scope at the break may be its redefined value when the
        // loop exits via this break.
        for (var_id, var_type) in &context.locals {
            let combined = match loop_scope.possibly_redefined_loop_parent_vars.get(var_id) {
                Some(existing) => combine_union_types(var_type, existing, false),
                None => var_type.clone(),
            };
            loop_scope
                .possibly_redefined_loop_parent_vars
                .insert(*var_id, combined);
        }

        if loop_scope.iteration_count == 0 {
            for (var_id, var_type) in &context.locals {
                if !loop_scope.parent_context_vars.contains_key(var_id) {
                    let combined = match loop_scope.possibly_defined_loop_parent_vars.get(var_id) {
                        Some(existing) => combine_union_types(var_type, existing, false),
                        None => var_type.clone(),
                    };
                    loop_scope
                        .possibly_defined_loop_parent_vars
                        .insert(*var_id, combined);
                }
            }
        }
    } else {
        context.control_actions.insert(ControlAction::Break);
    }

    context.has_returned = true;
}
