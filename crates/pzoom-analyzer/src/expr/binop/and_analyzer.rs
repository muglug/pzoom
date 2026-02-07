//! And (&&) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Assertion, TUnion};
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;

/// Analyze a logical AND expression (&&, 'and').
///
/// The AND operator short-circuits: if the left side is falsy, the right side
/// is not evaluated. This analyzer handles type narrowing through the left side.
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
        Some(context),
    );

    // The right side executes only when the left side is truthy.
    let mut right_context = context.clone();
    right_context.inside_conditional = context.inside_conditional;
    let assertions = assertion_finder::get_assertions(analyzer, left, analysis_data);
    if !assertions.if_true.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let should_emit_redundant_issues =
            assertions_touch_reference_cluster(analyzer, &right_context, &assertions.if_true);
        let active_assertion_offsets = if should_emit_redundant_issues {
            Some(build_active_assertion_offsets(&assertions.if_true))
        } else {
            None
        };
        let inside_loop = right_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &assertions.if_true,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            should_emit_redundant_issues,
            active_assertion_offsets.as_ref(),
        );
        promote_asserted_vars_to_assigned(analyzer, &assertions.if_true, &mut right_context);
    }
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);

    let right_assertions = assertion_finder::get_assertions(analyzer, right, analysis_data);
    emit_active_contradictions_for_shared_assertions(
        analyzer,
        &right_context,
        &assertions.if_true,
        &right_assertions.if_true,
        analysis_data,
    );

    if context.inside_conditional || !matches!(right.unparenthesized(), Expression::Assignment(_))
    {
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            right,
            right_pos,
            analysis_data,
            false,
            Some(&right_context),
        );
    }

    // If left is falsy, right is skipped but falsy assertions on left still apply.
    let mut skipped_right_context = context.clone();
    skipped_right_context.inside_conditional = context.inside_conditional;
    if !assertions.if_false.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = skipped_right_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &assertions.if_false,
            &mut skipped_right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            true,
            false,
            None,
        );
        promote_asserted_vars_to_assigned(
            analyzer,
            &assertions.if_false,
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

fn build_active_assertion_offsets(
    assertions: &BTreeMap<String, Vec<Assertion>>,
) -> BTreeMap<String, FxHashSet<usize>> {
    let mut active_offsets = BTreeMap::new();

    for (var_name, var_assertions) in assertions {
        if var_assertions.is_empty() {
            continue;
        }

        let mut offsets = FxHashSet::default();
        for offset in 0..var_assertions.len() {
            offsets.insert(offset);
        }

        active_offsets.insert(var_name.clone(), offsets);
    }

    active_offsets
}

fn assertions_touch_reference_cluster(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    assertions: &BTreeMap<String, Vec<Assertion>>,
) -> bool {
    for var_name in assertions.keys() {
        if var_name.contains('[') || var_name.contains("->") || var_name.contains("::") {
            continue;
        }

        let mut candidate_ids = Vec::new();
        if let Some(id) = analyzer.interner.find(var_name) {
            candidate_ids.push(id);
        }

        if let Some(stripped) = var_name.strip_prefix('$') {
            if let Some(id) = analyzer.interner.find(stripped) {
                candidate_ids.push(id);
            }
        } else {
            let with_dollar = format!("${var_name}");
            if let Some(id) = analyzer.interner.find(&with_dollar) {
                candidate_ids.push(id);
            }
        }

        for candidate_id in candidate_ids {
            if context.references_in_scope.contains_key(&candidate_id)
                || context
                    .references_in_scope
                    .values()
                    .any(|target| *target == candidate_id)
            {
                return true;
            }
        }
    }

    false
}

fn emit_active_contradictions_for_shared_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    left_assertions: &BTreeMap<String, Vec<Assertion>>,
    right_assertions: &BTreeMap<String, Vec<Assertion>>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if left_assertions.is_empty() || right_assertions.is_empty() {
        return;
    }

    let mut shared_assertions: BTreeMap<String, Vec<Assertion>> = BTreeMap::new();
    for (var_name, assertions) in right_assertions {
        if !left_assertions.contains_key(var_name) || assertions.is_empty() {
            continue;
        }

        let scoped_assertions = assertions
            .iter()
            .filter(|assertion| {
                matches!(
                    assertion,
                    Assertion::IsType(pzoom_code_info::TAtomic::TNamedObject { .. })
                        | Assertion::ArrayKeyExists
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        if !scoped_assertions.is_empty() {
            shared_assertions.insert(var_name.clone(), scoped_assertions);
        }
    }

    if shared_assertions.is_empty() {
        return;
    }

    let active_assertion_offsets = build_active_assertion_offsets(&shared_assertions);
    if active_assertion_offsets.is_empty() {
        return;
    }

    let mut contradiction_context = context.clone();
    let mut changed_var_ids = FxHashSet::default();
    let inside_loop = contradiction_context.inside_loop;
    reconciler::reconcile_keyed_types(
        &shared_assertions,
        &mut contradiction_context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        inside_loop,
        false,
        true,
        Some(&active_assertion_offsets),
    );
}
