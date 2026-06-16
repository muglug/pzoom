//! Null coalesce (??) operator analyzer.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
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
        Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) => {
            !is_superglobal(direct.name) && !context.locals.contains_key(&VarName::new(direct.name))
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
            .filter(|entry| entry.possibly_undefined && !entry.is_mixed())
            .map(|entry| {
                let mut present = entry.clone();
                present.possibly_undefined = false;
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
    let left_type = analysis_data.expr_types.get(&left_pos).cloned();

    // A direct VARIABLE left operand still reports redundancy when its type
    // can never be null/undefined (Psalm: "Type string for $s is never
    // null"); array/property accesses legitimately probe existence.
    let left_is_plain_variable = matches!(
        left.unparenthesized(),
        Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(_))
    ) && !left_var_undefined;
    if let Some(left_type) = left_type.as_ref()
        && (!use_isset_context || left_is_plain_variable)
        // Psalm's isset-based coalesce handling never reports redundancy for
        // an assignment left operand (`($a =& $var) ?? ...`).
        && !matches!(left.unparenthesized(), Expression::Assignment(_))
        && !left_type.is_nullable()
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
                    .insert(var_id.clone(), outer_type.clone());
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
                    // Combine non-null left types with right types. Keep the
                    // left's dataflow parents — from_types builds a fresh union
                    // and would otherwise sever the flow through the coalesce.
                    let mut left_non_null = TUnion::from_types(left_without_null);
                    left_non_null.parent_nodes = lt.parent_nodes.clone();
                    let mut combined = combine_union_types(&left_non_null, &rt, false);
                    // An internal-function falsable-leniency flag survives the
                    // coalesce (parse_url(...) ?? '' stays assignable to string).
                    combined.ignore_falsable_issues =
                        lt.ignore_falsable_issues || rt.ignore_falsable_issues;
                    combined
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

    let var_id = VarName::new(&root_var_name);
    let alt_var_id = if let Some(stripped) = root_var_name.strip_prefix('$') {
        VarName::new(stripped)
    } else {
        VarName::from(format!("${}", root_var_name))
    };

    if context.locals.contains_key(&var_id)
        || context.assigned_var_ids.contains_key(&var_id)
        || context.possibly_assigned_var_ids.contains(&var_id)
        || context.locals.contains_key(&alt_var_id)
        || context.assigned_var_ids.contains_key(&alt_var_id)
        || context.possibly_assigned_var_ids.contains(&alt_var_id)
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
    // Split at the *earliest* access/offset delimiter so the root is the base
    // variable (e.g. `$this->data[$x]` -> `$this`, not `$this->data`); a property
    // or static access root is not an undefined-variable candidate.
    let split_at = ["[", "->", "::"]
        .iter()
        .filter_map(|delim| var_key.find(delim))
        .min();

    match split_at {
        Some(offset) if offset > 0 => {
            let root = &var_key[..offset];
            // Only `$variable` roots are undefined-variable candidates; a
            // class-name root (`self::$prop`, `Foo::$prop`) is not.
            root.starts_with('$').then(|| root.to_string())
        }
        _ => var_key.starts_with('$').then(|| var_key.to_string()),
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

    isset_context.locals.get(&left_var_name).cloned()
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
