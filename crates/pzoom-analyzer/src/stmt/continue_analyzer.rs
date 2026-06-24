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

/// The literal level of a `break N`/`continue N` (default 1).
pub(crate) fn break_level(
    level: Option<&mago_syntax::cst::cst::expression::Expression<'_>>,
) -> usize {
    level
        .and_then(|level_expr| {
            if let mago_syntax::cst::cst::expression::Expression::Literal(
                mago_syntax::cst::cst::literal::Literal::Integer(int_lit),
            ) = level_expr
            {
                int_lit.value.map(|v| v as usize)
            } else {
                None
            }
        })
        .unwrap_or(1)
        .max(1)
}

/// `continue N`/`break N` target the loop scope reached after unwinding N
/// break frames — switch frames count toward N but contribute no loop scope
/// (Psalm's Continue/BreakAnalyzer walk `loop_parent_context->loop_scope`).
pub(crate) fn loop_scope_index_for_level(
    break_types: &[crate::stmt::scope_analyzer::BreakContext],
    loop_scopes_len: usize,
    level: Option<&mago_syntax::cst::cst::expression::Expression<'_>>,
) -> Option<usize> {
    let count = break_level(level);
    let loops_among = if count <= break_types.len() {
        break_types[break_types.len() - count..]
            .iter()
            .filter(|frame| matches!(frame, crate::stmt::scope_analyzer::BreakContext::Loop))
            .count()
            .max(1)
    } else {
        // More levels than tracked frames (analysis entered mid-construct):
        // fall back to the innermost loop.
        1
    };
    loop_scopes_len.checked_sub(loops_among)
}

pub fn analyze(
    _analyzer: &StatementsAnalyzer<'_>,
    continue_stmt: &mago_syntax::cst::cst::r#loop::Continue<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let scope_index = loop_scope_index_for_level(
        &context.break_types,
        analysis_data.loop_scopes.len(),
        continue_stmt.level,
    );
    if let Some(loop_scope) = scope_index.and_then(|index| analysis_data.loop_scopes.get_mut(index))
    {
        loop_scope.final_actions.insert(ControlAction::Continue);
        context.control_actions.insert(ControlAction::Continue);

        let mut removed_var_ids = FxHashSet::default();
        let redefined_vars = context.get_redefined_locals(
            &loop_scope.parent_context_vars,
            false,
            &mut removed_var_ids,
        );

        for (var_id, var_type) in redefined_vars {
            let combined = match loop_scope.possibly_redefined_loop_vars.get(&var_id) {
                Some(existing) => combine_union_types(&var_type, existing, false),
                None => var_type,
            };
            loop_scope
                .possibly_redefined_loop_vars
                .insert(var_id, combined);
        }
    } else if context.break_types.is_empty() {
        // Psalm's ContinueAnalyzer: no enclosing loop (and not inside a
        // switch, whose `continue` acts like `break`).
        let span = mago_span::HasSpan::span(continue_stmt);
        let (line, col) = _analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(pzoom_code_info::Issue::new(
            pzoom_code_info::IssueKind::ContinueOutsideLoop,
            "Continue call outside loop context",
            _analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    context.has_returned = true;
}
