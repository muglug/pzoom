//! Null coalesce (??) operator analyzer.

use std::collections::BTreeMap;
use std::rc::Rc;

use indexmap::IndexMap;
use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use rustc_hash::FxHashSet;

use pzoom_code_info::Assertion;
use pzoom_code_info::algebra::{Clause, ClauseKey, get_truths_from_formula, simplify_cnf};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a null coalesce expression (??).
///
/// The ?? operator returns the left operand if it exists and is not null,
/// otherwise returns the right operand.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the left side with isset context only for direct existence checks.
    // For expressions like function calls, Psalm still reports undefined arguments.
    let use_isset_context = matches!(
        left.unparenthesized(),
        Expression::Variable(_) | Expression::ArrayAccess(_) | Expression::Access(_)
    );
    let was_inside_isset = context.inside_isset;
    if use_isset_context {
        if matches!(
            left.unparenthesized(),
            Expression::ArrayAccess(_) | Expression::Access(_)
        ) {
            maybe_emit_undefined_root_variable_for_coalesce_left(
                analyzer,
                left,
                analysis_data,
                context,
            );
        }
        context.inside_isset = true;
    }

    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, context);
    let left_type = analysis_data.get_expr_type(left_pos);

    if let Some(left_type) = left_type.as_ref()
        && !use_isset_context
        && !left_type.is_nullable
        && !left_type.possibly_undefined
        && !left_type.is_mixed()
        && !context.inside_loop
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            if left_type.from_docblock {
                IssueKind::RedundantConditionGivenDocblockType
            } else {
                IssueKind::RedundantCondition
            },
            format!(
                "Left operand of null coalesce is never null ({})",
                left_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if use_isset_context {
        context.inside_isset = was_inside_isset;
    }

    // Analyze the right side under the assumption that the left side is null/undefined.
    // This mirrors Psalm/Hakana's conditional flow for null coalescing.
    let mut right_context = context.clone();
    right_context.inside_conditional = true;
    apply_left_null_assumption(analyzer, left, analysis_data, &mut right_context);
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);
    let right_type = analysis_data.get_expr_type(right_pos);
    context.merge(&right_context);

    // Combine the types: left type (minus null) + right type
    let result_type = match (left_type, right_type) {
        (Some(lt), Some(rt)) => {
            // Remove null from left type
            let left_without_null: Vec<_> = lt
                .types
                .iter()
                .filter(|t| !matches!(t, TAtomic::TNull))
                .cloned()
                .collect();

            if left_without_null.is_empty() {
                // Left was only null, result is just right type
                (*rt).clone()
            } else {
                // Combine non-null left types with right types
                let left_non_null = TUnion::from_types(left_without_null);
                combine_union_types(&left_non_null, &rt, false)
            }
        }
        (Some(lt), None) => {
            // Remove null from left type
            let left_without_null: Vec<_> = lt
                .types
                .iter()
                .filter(|t| !matches!(t, TAtomic::TNull))
                .cloned()
                .collect();

            if left_without_null.is_empty() {
                TUnion::mixed()
            } else {
                TUnion::from_types(left_without_null)
            }
        }
        (None, Some(rt)) => (*rt).clone(),
        (None, None) => TUnion::mixed(),
    };

    analysis_data.set_expr_type(pos, result_type);
}

fn maybe_emit_undefined_root_variable_for_coalesce_left(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !context.check_variables {
        return;
    }

    let Some(root_var_name) = extract_root_var_name_for_coalesce(left) else {
        return;
    };

    let normalized_var = root_var_name.trim_start_matches('$');
    if normalized_var.eq_ignore_ascii_case("this") || is_superglobal(normalized_var) {
        return;
    }

    let var_id = analyzer.interner.intern(&root_var_name);
    let alt_var_id = if let Some(stripped) = root_var_name.strip_prefix('$') {
        analyzer.interner.find(stripped)
    } else {
        analyzer.interner.find(&format!("${}", root_var_name))
    };

    if context.locals.contains_key(&var_id)
        || context.assigned_var_ids.contains_key(&var_id)
        || context.possibly_assigned_var_ids.contains(&var_id)
        || alt_var_id.is_some_and(|alt| {
            context.locals.contains_key(&alt)
                || context.assigned_var_ids.contains_key(&alt)
                || context.possibly_assigned_var_ids.contains(&alt)
        })
    {
        return;
    }

    let pos = (left.start_offset() as u32, left.end_offset() as u32);
    let (line, col) = analyzer.get_line_column(pos.0);
    let issue_kind = if analyzer.function_info.is_none() {
        IssueKind::UndefinedGlobalVariable
    } else {
        IssueKind::UndefinedVariable
    };
    let message = if analyzer.function_info.is_none() {
        format!("Undefined global variable ${}", normalized_var)
    } else {
        format!("Undefined variable ${}", normalized_var)
    };

    analysis_data.add_issue(Issue::new(
        issue_kind,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn extract_root_var_name_for_coalesce(expr: &Expression<'_>) -> Option<String> {
    let var_key = expression_identifier::get_expression_var_key(expr)?;
    let split_at = var_key
        .find('[')
        .or_else(|| var_key.find("->"))
        .or_else(|| var_key.find("::"));

    match split_at {
        Some(offset) if offset > 0 => Some(var_key[..offset].to_string()),
        _ => Some(var_key),
    }
}

fn is_superglobal(var_name: &str) -> bool {
    matches!(
        var_name,
        "GLOBALS"
            | "_SERVER"
            | "_GET"
            | "_POST"
            | "_FILES"
            | "_COOKIE"
            | "_SESSION"
            | "_REQUEST"
            | "_ENV"
            | "argc"
            | "argv"
    )
}

fn apply_left_null_assumption(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(left_var_name) = expression_identifier::get_expression_var_key(left) else {
        return;
    };

    let cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = IndexMap::new();
    let null_assertion = Assertion::IsType(TAtomic::TNull);
    var_possibilities.insert(null_assertion.to_hash(), null_assertion);
    possibilities.insert(ClauseKey::Name(left_var_name), var_possibilities);

    context.clauses.push(Rc::new(Clause::new(
        possibilities,
        cond_id,
        cond_id,
        None,
        None,
        None,
    )));

    let clause_refs: Vec<&Clause> = context
        .clauses
        .iter()
        .map(|clause| clause.as_ref())
        .collect();
    let simplified = simplify_cnf(clause_refs);
    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, _) = get_truths_from_formula(
        simplified.iter().collect(),
        None,
        &mut cond_referenced_var_ids,
    );

    let mut flattened_assertions: BTreeMap<String, Vec<Assertion>> = BTreeMap::new();
    for (var_name, assertion_lists) in truths {
        let entry = flattened_assertions.entry(var_name).or_default();
        for assertion_list in assertion_lists {
            entry.extend(assertion_list);
        }
    }

    if flattened_assertions.is_empty() {
        return;
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        &flattened_assertions,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        false,
        None,
    );
}
