//! `continue` statement analyzer.
//!
//! Mirrors Hakana's `continue_analyzer`: records, against the innermost loop scope,
//! the variables that were redefined relative to the loop's entry context (so the
//! fixpoint widens them) and marks the current path as terminated.

use pzoom_code_info::combine_union_types;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::scope_analyzer::ControlAction;

pub fn analyze(
    _analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if let Some(loop_scope) = analysis_data.loop_scopes.last_mut() {
        loop_scope.final_actions.insert(ControlAction::Continue);
        context.control_actions.insert(ControlAction::Continue);

        let mut removed_var_ids = FxHashSet::default();
        let redefined_vars =
            context.get_redefined_locals(&loop_scope.parent_context_vars, false, &mut removed_var_ids);

        for (var_id, var_type) in redefined_vars {
            let combined = match loop_scope.possibly_redefined_loop_vars.get(&var_id) {
                Some(existing) => combine_union_types(&var_type, existing, false),
                None => var_type,
            };
            loop_scope
                .possibly_redefined_loop_vars
                .insert(var_id, combined);
        }
    }

    context.has_returned = true;
}
