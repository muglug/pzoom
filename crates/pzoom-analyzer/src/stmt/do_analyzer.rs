//! Do-while statement analyzer (Psalm `DoAnalyzer` / Hakana `do_analyzer`).
//!
//! A `do { ... } while (cond)` body always executes at least once and evaluates its
//! condition *after* the body. This delegates to the shared [`loop_analyzer`]
//! fixpoint with `is_do = true`, mirroring Hakana's do-loop handling.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::do_while::DoWhile;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};
use crate::stmt::loop_analyzer;
use crate::stmt::while_analyzer::get_and_expressions;

/// Analyze a do-while statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    do_while: &DoWhile<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let condition = do_while.condition;

    let mut loop_context = context.clone();
    loop_context.inside_loop = true;
    loop_context.inside_foreach = false;
    loop_context.break_types.push(BreakContext::Loop);

    let loop_scope = LoopScope::new(context.locals.clone());

    // The body of a do-while always runs at least once.
    let body_stmts = std::slice::from_ref(do_while.statement);
    let pre_conditions = get_and_expressions(condition);

    let (loop_scope, mut inner_loop_context) = loop_analyzer::analyze(
        analyzer,
        body_stmts,
        pre_conditions,
        vec![],
        loop_scope,
        &mut loop_context,
        context,
        analysis_data,
        true,
        true,
        // Psalm's DoAnalyzer copies loop vars unconditionally (the body
        // always runs once), so the can-leave gate does not apply.
        false,
    )?;

    let while_true = matches!(
        condition.unparenthesized(),
        Expression::Literal(Literal::True(_))
    );
    let can_leave_loop = !while_true || loop_scope.final_actions.contains(&ControlAction::Break);

    if !can_leave_loop {
        context.control_actions.insert(ControlAction::End);
        context.has_returned = true;
    }

    // Hakana's do_analyzer tail: the inner do context (the body's final state,
    // with the break-path merges applied by the loop analyzer) becomes the
    // post-loop variable state, after reconciling the negated while condition.
    if can_leave_loop {
        let condition_id = (
            condition.span().start.offset,
            condition.span().end.offset,
        );
        let while_clauses = crate::formula_generator::get_formula(
            condition_id,
            condition_id,
            condition,
            analyzer,
            analysis_data,
            false,
        )
        .unwrap_or_default();

        let mut clauses_to_simplify: Vec<pzoom_code_info::algebra::Clause> = context
            .clauses
            .iter()
            .map(|clause| (**clause).clone())
            .collect();
        clauses_to_simplify
            .extend(pzoom_code_info::algebra::negate_formula(while_clauses).unwrap_or_default());

        let (negated_while_types, _) = pzoom_code_info::algebra::get_truths_from_formula(
            pzoom_code_info::algebra::simplify_cnf(clauses_to_simplify.iter().collect())
                .iter()
                .collect(),
            None,
            &mut rustc_hash::FxHashSet::default(),
        );

        if !negated_while_types.is_empty() {
            let mut changed_var_ids = rustc_hash::FxHashSet::default();
            crate::reconciler::reconcile_keyed_types(
                &negated_while_types,
                &mut inner_loop_context,
                &mut changed_var_ids,
                analyzer,
                analysis_data,
                true,
                false,
                crate::reconciler::EmissionMode::Silent,
                None,
            );
        }

        // Psalm's LoopAnalyzer::setLoopVars: with break/continue in the body
        // the end-of-body state may not hold — only variables captured at a
        // break (possibly_defined_loop_parent_vars) carry over, combined with
        // that capture; otherwise the body's final state is the post-loop
        // state.
        let does_break_or_continue = loop_scope.final_actions.contains(&ControlAction::Break)
            || loop_scope
                .final_actions
                .contains(&ControlAction::BreakImmediateLoop)
            || loop_scope.final_actions.contains(&ControlAction::Continue);
        for (var_id, var_type) in inner_loop_context.locals {
            if does_break_or_continue {
                if let Some(possibly_defined) =
                    loop_scope.possibly_defined_loop_parent_vars.get(&var_id)
                {
                    context.locals.insert(
                        var_id,
                        pzoom_code_info::combine_union_types(&var_type, possibly_defined, false),
                    );
                }
            } else {
                context.locals.insert(var_id, var_type);
            }
        }
    }

    Ok(())
}
