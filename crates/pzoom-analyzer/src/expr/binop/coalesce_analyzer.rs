//! Null coalesce (??) operator analyzer.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::cst::cst::access::Access;
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::variable::Variable;
use rustc_hash::FxHashSet;

use pzoom_code_info::Assertion;
use pzoom_code_info::VarName;
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
    // A direct variable that has no type in scope: Psalm's coalesce gives the
    // expression the right operand's type outright (the left contributes
    // nothing), rather than the mixed placeholder our variable fetch returns.
    let left_var_undefined = match left.unparenthesized() {
        Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(direct)) => {
            !is_superglobal(pzoom_syntax::bytes_to_str(direct.name)) && !context.locals.contains_key(&VarName::new(pzoom_syntax::bytes_to_str(direct.name)))
        }
        _ => false,
    };

    // Resolve the implicit isset against the PRE-analysis clauses: the left
    // fetch below seeds the key and invalidates clauses that mention it.
    let clause_resolved_left_type = if left_var_undefined {
        None
    } else {
        // A tracked entry for the key (e.g. `string` possibly-undefined from
        // a `!isset(...) || is_string(...)` guard) is exactly the
        // present-side type the implicit isset selects.
        expression_identifier::get_expression_var_key(left)
            .and_then(|key| context.locals.get(&key))
            .filter(|entry| entry.possibly_undefined_from_try && !entry.is_mixed())
            .map(|entry| {
                let mut present = (**entry).clone();
                present.possibly_undefined_from_try = false;
                present
            })
            .or_else(|| resolve_left_isset_type(analyzer, left, analysis_data, context))
    };

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
            // Psalm's CoalesceAnalyzer desugars `$base[...] ?? $r` into
            // `isset($base[...]) ? $base[...] : $r` and analyzes the true-branch
            // value outside isset(): the base variable's (possibly) undefined
            // report is emitted by VariableFetchAnalyzer there, not by a
            // coalesce-specific check. Analyze the walked root variable the same
            // way to reproduce that report through the normal variable-fetch
            // path, leaving the offset's existence to the isset probe below.
            let mut root = left.unparenthesized();
            loop {
                match root {
                    Expression::ArrayAccess(access) => root = access.array.unparenthesized(),
                    Expression::Access(Access::Property(property)) => {
                        root = property.object.unparenthesized();
                    }
                    _ => break,
                }
            }
            if matches!(root, Expression::Variable(Variable::Direct(_))) {
                expression_analyzer::analyze(analyzer, root, analysis_data, context);
            }
        }
        context.inside_isset = true;
    }

    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, context);
    let left_type = analysis_data.expr_types.get(&left_pos).cloned();

    // A direct VARIABLE left operand still reports redundancy when its type
    // can never be null/undefined (Psalm: "Type string for $s is never
    // null"); array/property accesses legitimately probe existence.
    let left_is_plain_variable = matches!(
        left.unparenthesized(),
        Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(_))
    ) && !left_var_undefined;
    if let Some(left_type) = left_type.as_ref()
        && (!use_isset_context || left_is_plain_variable)
        // Psalm's isset-based coalesce handling never reports redundancy for
        // an assignment left operand (`($a =& $var) ?? ...`).
        && !matches!(left.unparenthesized(), Expression::Assignment(_))
        && !left_type.is_nullable()
        && !left_type.possibly_undefined_from_try
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
    let null_assumed_vars =
        apply_left_null_assumption(analyzer, left, analysis_data, &mut right_context);
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);
    let right_type = analysis_data.expr_types.get(&right_pos).cloned();
    // The left-is-null assumption is local to the right operand: restore any
    // vars it narrowed/seeded (unless the right operand itself reassigned
    // them) so the merge can't leak `$a['k'] = null` into the outer scope.
    for var_id in null_assumed_vars {
        let right_count = right_context
            .assigned_var_ids
            .get(&var_id)
            .copied()
            .unwrap_or(0);
        let pre_count = context.assigned_var_ids.get(&var_id).copied().unwrap_or(0);
        if right_count > pre_count {
            continue;
        }
        match context.locals.get(&var_id) {
            Some(outer_type) => {
                right_context
                    .locals
                    .insert(var_id.clone(), outer_type.as_ref().clone());
            }
            None => {
                right_context.locals.remove(&var_id);
            }
        }
    }
    context.merge(&right_context);

    // The implicit isset over the left operand can resolve context clauses
    // (a guard like `!isset(\$o['k']) || is_string(\$o['k'])`) to a narrower
    // present-side type than the raw fetch.
    let left_type = match (left_type, clause_resolved_left_type) {
        (Some(raw_left), Some(mut resolved)) if !resolved.is_mixed() && !resolved.is_nothing() => {
            resolved.parent_nodes = raw_left.parent_nodes.clone();
            Some(std::rc::Rc::new(resolved))
        }
        (other, _) => other,
    };

    // Combine the types: left type (minus null) + right type
    let result_type = if left_var_undefined {
        right_type
            .map(|rt| (*rt).clone())
            .unwrap_or_else(TUnion::mixed)
    } else {
        match (left_type, right_type) {
            (Some(lt), Some(rt)) => combine_coalesce_value_types(&lt, &rt),
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
                    let mut left_non_null = TUnion::from_types(left_without_null);
                    left_non_null.parent_nodes = lt.parent_nodes.clone();
                    left_non_null.ignore_falsable_issues = lt.ignore_falsable_issues;
                    left_non_null
                }
            }
            (None, Some(rt)) => (*rt).clone(),
            (None, None) => TUnion::mixed(),
        }
    };

    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}

/// Combine a coalesce's left value (minus null) with its right value — the
/// `non_null($l) | $r` that Psalm's `CoalesceAnalyzer` produces by desugaring
/// `$l ?? $r` into `isset($l) ? $l : $r` and combining the ternary's branches.
/// The surviving left value keeps its dataflow parents and its `reference_free`
/// flag: Psalm's then-branch is the reconciled `$l` with its union flags intact,
/// and `Type::combineUnionTypes` then ANDs `reference_free` against `$r`.
/// `allow_mutations` is deliberately left to the combiner — Psalm never carries
/// it across a combine. Shared with the `??=` assignment path so that
/// `$a ??= $b` matches `$a = $a ?? $b` exactly, the way Psalm's
/// `AssignmentAnalyzer` reuses `CoalesceAnalyzer`.
pub(crate) fn combine_coalesce_value_types(left_type: &TUnion, right_type: &TUnion) -> TUnion {
    // Remove null from the left value.
    let left_without_null: Vec<_> = left_type
        .types
        .iter()
        .filter(|t| !matches!(t, TAtomic::TNull))
        .cloned()
        .collect();

    if left_without_null.is_empty() {
        // Left was only null, so the result is exactly the right value.
        return right_type.clone();
    }

    // Combine the non-null left value with the right value. Keep the left's
    // dataflow parents and `reference_free` flag — `from_types` builds a fresh
    // union and would otherwise sever the flow through the coalesce and launder
    // the surviving value's freshness.
    let mut left_non_null = TUnion::from_types(left_without_null);
    left_non_null.parent_nodes = left_type.parent_nodes.clone();
    left_non_null.reference_free = left_type.reference_free;
    let mut combined = combine_union_types(&left_non_null, right_type, false);
    // An internal-function falsable-leniency flag survives the coalesce
    // (parse_url(...) ?? '' stays assignable to string).
    combined.ignore_falsable_issues =
        left_type.ignore_falsable_issues || right_type.ignore_falsable_issues;
    combined
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

/// Resolve the left operand's type under the coalesce's implicit
/// `isset($left)` (Psalm treats `$x ?? $y` as `isset($x) ? $x : $y`): the
/// isset assertion simplifies against the context's clauses, so a surviving
/// disjunction like `!isset($o['k']) || is_string($o['k'])` narrows the
/// present-side value to string.
fn resolve_left_isset_type(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let left_var_name = expression_identifier::get_expression_var_key(left)?;
    if std::env::var("PZDBG").is_ok() {
        eprintln!(
            "DBG resolver key={} clauses={:?}",
            left_var_name,
            context
                .clauses
                .iter()
                .map(|c| c
                    .possibilities
                    .keys()
                    .map(|k| format!("{:?}", k))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }
    if context.clauses.is_empty() {
        return None;
    }

    let cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let mut isset_context = context.clone();

    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = pzoom_code_info::AssertionSet::default();
    let isset_assertion = Assertion::IsIsset;
    var_possibilities.insert(isset_assertion.to_hash(), isset_assertion);
    possibilities.insert(ClauseKey::Name(left_var_name.clone()), var_possibilities);
    isset_context.clauses.push(Rc::new(Clause::new(
        possibilities,
        cond_id,
        cond_id,
        None,
        None,
        None,
    )));

    let clause_refs: Vec<&Clause> = isset_context
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

    let mut flattened_assertions: BTreeMap<VarName, Vec<Vec<Assertion>>> = BTreeMap::new();
    for (var_name, assertion_lists) in truths {
        let entry = flattened_assertions.entry(var_name).or_default();
        for assertion_list in assertion_lists {
            entry.extend(assertion_list.into_iter().map(|assertion| vec![assertion]));
        }
    }
    if flattened_assertions.is_empty() {
        return None;
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        &flattened_assertions,
        &mut isset_context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        crate::reconciler::EmissionMode::Silent,
        None,
    );

    isset_context.locals.get(&left_var_name).map(|__t| (**__t).clone())
}

fn apply_left_null_assumption(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> FxHashSet<VarName> {
    let Some(left_var_name) = expression_identifier::get_expression_var_key(left) else {
        return FxHashSet::default();
    };

    let cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = pzoom_code_info::AssertionSet::default();
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

    // Flatten the formula truths into singleton AND groups (each assertion its
    // own group, preserving the pre-grouping reconcile order).
    let mut flattened_assertions: BTreeMap<VarName, Vec<Vec<Assertion>>> = BTreeMap::new();
    for (var_name, assertion_lists) in truths {
        let entry = flattened_assertions.entry(var_name).or_default();
        for assertion_list in assertion_lists {
            entry.extend(assertion_list.into_iter().map(|assertion| vec![assertion]));
        }
    }

    if flattened_assertions.is_empty() {
        return FxHashSet::default();
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
        crate::reconciler::EmissionMode::Silent,
        None,
    );
    // Every asserted key may have been seeded into locals even when the
    // reconciler reports no change (get_value_for_key resolution).
    changed_var_ids.extend(flattened_assertions.keys().cloned());
    changed_var_ids
}
