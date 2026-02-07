//! Or (||) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Assertion, TUnion};
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;

/// Analyze a logical OR expression (||, 'or').
///
/// The OR operator short-circuits: if the left side is truthy, the right side
/// is not evaluated. This analyzer handles type narrowing through negation.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the left side
    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, context);
    if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        left,
        left_pos,
        analysis_data,
        false,
        None,
    );

    // The right side executes only when the left side is falsy.
    let mut right_context = context.clone();
    right_context.inside_conditional = context.inside_conditional;
    let assertions = assertion_finder::get_assertions(analyzer, left, analysis_data);
    if !assertions.if_false.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = right_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &assertions.if_false,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            true,
            false,
            None,
        );
        promote_asserted_vars_to_assigned(analyzer, &assertions.if_false, &mut right_context);
    }
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);
    if context.inside_conditional || !matches!(right.unparenthesized(), Expression::Assignment(_))
    {
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            right,
            right_pos,
            analysis_data,
            false,
            None,
        );
    }

    // If left is truthy, right is skipped but truthy assertions on left still apply.
    let mut skipped_right_context = context.clone();
    skipped_right_context.inside_conditional = context.inside_conditional;
    if !assertions.if_true.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = skipped_right_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &assertions.if_true,
            &mut skipped_right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            false,
            None,
        );
        promote_asserted_vars_to_assigned(
            analyzer,
            &assertions.if_true,
            &mut skipped_right_context,
        );
    }

    skipped_right_context.merge(&right_context);
    *context = skipped_right_context;

    // The result type is always bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}

fn promote_asserted_vars_to_assigned(
    analyzer: &StatementsAnalyzer<'_>,
    assertions: &std::collections::BTreeMap<String, Vec<Assertion>>,
    context: &mut BlockContext,
) {
    for var_name in assertions.keys() {
        if var_name.contains('[')
            || var_name.contains("->")
            || var_name.contains("::")
            || var_name.contains('(')
        {
            continue;
        }

        let mut candidates = vec![var_name.as_str()];
        if let Some(stripped) = var_name.strip_prefix('$') {
            candidates.push(stripped);
        } else {
            // Keep both `$x` and `x` spellings in sync.
            let with_dollar = format!("${var_name}");
            if let Some(var_id) = analyzer.interner.find(&with_dollar) {
                if context.locals.contains_key(&var_id) {
                    *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
                    context.possibly_assigned_var_ids.remove(&var_id);
                }
            }
        }

        for candidate in candidates {
            if let Some(var_id) = analyzer.interner.find(candidate) {
                if context.locals.contains_key(&var_id) {
                    *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
                    context.possibly_assigned_var_ids.remove(&var_id);
                }
            }
        }
    }
}
