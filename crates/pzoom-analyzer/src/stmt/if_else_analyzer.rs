//! If/elseif/else statement analyzer (Psalm `IfElseAnalyzer` equivalent).
//!
//! This is the statement-level orchestrator: it analyzes the `if` condition and
//! branch, then each elseif and the else, tracking CNF formulas through nested
//! conditions for type narrowing and merging the branch contexts. Mirrors
//! Psalm's `IfElseAnalyzer` (which delegates the per-branch bodies to
//! `IfAnalyzer`/`ElseIfAnalyzer`/`ElseAnalyzer`; pzoom keeps the `if` branch
//! inline and uses `elseif_analyzer`/`else_analyzer` for the rest).

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::control_flow::r#if::If;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;
use std::collections::BTreeMap;

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, get_truths_from_formula, negate_formula, simplify_cnf};
use pzoom_code_info::{Assertion, TAtomic, TUnion, combine_union_types};
use pzoom_code_info::{Issue, IssueKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::IfScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_conditional_analyzer;
use crate::stmt::scope_analyzer::{self, ControlAction};
use crate::stmt::{else_analyzer, elseif_analyzer};
use crate::stmt_analyzer::analyze_stmts;
use std::rc::Rc;

/// Analyze an if statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    if_stmt: &If<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition through the conditional scope. The &&/|| analyzers narrow
    // into a shared if_body_context (including right-operand assignments); the
    // returned post_if_context is the fallthrough base used for the else branch.
    let cond_pos = (
        if_stmt.condition.start_offset() as u32,
        if_stmt.condition.end_offset() as u32,
    );

    // Psalm clones the original context for later use when the condition is a
    // binary op (or a negated one) and the if body always leaves
    // ($if_scope->post_leaving_if_context); the conditionally-assigned replay
    // below runs against this pre-condition scope.
    let cond_is_binaryish = match if_stmt.condition.unparenthesized() {
        Expression::Binary(_) => true,
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            matches!(unary.operand.unparenthesized(), Expression::Binary(_))
        }
        _ => false,
    };
    let mut post_leaving_if_context = if cond_is_binaryish {
        let pre_actions = scope_analyzer::get_control_actions(
            if_stmt.body.statements(),
            analysis_data,
            &[],
            true,
        );
        let has_leaving = !pre_actions.is_empty() && !pre_actions.contains(&ControlAction::None);
        has_leaving.then(|| context.clone())
    } else {
        None
    };

    let if_conditional_scope =
        if_conditional_analyzer::analyze(analyzer, if_stmt.condition, analysis_data, context);
    let cond_if_body_context = if_conditional_scope.if_body_context;
    let assigned_in_conditional_var_ids = if_conditional_scope.assigned_in_conditional_var_ids;
    *context = if_conditional_scope.post_if_context;

    // Snapshot the post-condition locals before the if branch's `Context::update`
    // can narrow/insert into them — Psalm clones `else_context` from
    // `post_if_context` up front, so the else branch sees the fallthrough state,
    // not values leaked from the if body (e.g. `$options['b'] = 1`).
    let post_if_locals = context.locals.clone();

    // Get type narrowing assertions from the condition using the assertion finder
    let assertion_result =
        assertion_finder::get_assertions(analyzer, if_stmt.condition, analysis_data);
    if !condition_has_assignments(if_stmt.condition)
        && !condition_contains_unsafe_empty_construct(if_stmt.condition)
    {
        if let Some(condition_is_always_truthy) =
            infer_condition_truthiness_from_clauses(&context.clauses, &assertion_result)
        {
            analysis_data.expr_types.insert(
                cond_pos,
                Rc::new(if condition_is_always_truthy {
                    TUnion::new(TAtomic::TTrue)
                } else {
                    TUnion::new(TAtomic::TFalse)
                }),
            );
        }
    }
    if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        if_stmt.condition,
        cond_pos,
        analysis_data,
        true,
        Some(context),
    );
    if !condition_has_assignments(if_stmt.condition) {
        // Vars an UNCONDITIONAL @psalm-assert inside the condition narrowed
        // apply before the formula in Psalm, so their conditional-assert
        // contradictions never report.
        let reportable_if_true: BTreeMap<VarName, Vec<Vec<Assertion>>> = assertion_result
            .if_true
            .iter()
            .filter(|(var_id, _)| !assertion_result.silently_asserted_vars.contains(*var_id))
            .map(|(var_id, groups)| (var_id.clone(), groups.clone()))
            .collect();
        emit_active_assertion_contradictions(
            analyzer,
            context,
            &reportable_if_true,
            &cond_if_body_context.reconciled_expression_clauses,
            cond_pos,
            analysis_data,
        );
    }

    let if_conditional_id = (
        if_stmt.condition.start_offset() as u32,
        if_stmt.condition.end_offset() as u32,
    );

    // Check whether the condition is redundant with, or contradicts, the clauses
    // already established in this context (Hakana's algebra_analyzer::check_for_paradox).
    if !condition_has_assignments(if_stmt.condition) {
        let if_formula = crate::formula_generator::get_formula(
            if_conditional_id,
            if_conditional_id,
            if_stmt.condition,
            analyzer,
            analysis_data,
            false,
        )
        .unwrap_or_default();
        crate::algebra_analyzer::check_for_paradox(
            analyzer,
            &context.clauses,
            &if_formula,
            analysis_data,
            cond_pos,
        );
    }

    // Analyze the if branch with type narrowing
    let mut if_context = context.child();
    seed_assignment_tracking(context, &mut if_context);
    // Seed the if body from the operator-narrowed conditional scope (matching
    // hakana's if_body_context), before the clause-based reconciliation narrows more.
    if_context.locals = cond_if_body_context.locals.clone();
    for (var_id, count) in &cond_if_body_context.assigned_var_ids {
        if_context
            .assigned_var_ids
            .entry(var_id.clone())
            .or_insert(*count);
    }

    // Combine parent clauses with the new if-true clauses
    let mut if_clauses: Vec<_> = context.clauses.iter().map(|c| (**c).clone()).collect();
    if_clauses.extend(assertion_result.if_true_clauses.clone());
    if_context.clauses = if_clauses.into_iter().map(std::rc::Rc::new).collect();

    // Apply assertions to if branch context using the reconciler. The
    // reconciler's changed ids are Psalm's `if_cond_changed_var_ids`: only
    // vars the condition's *definite truths* narrowed. A var whose body-entry
    // type differs merely because an `&&`/`||` operand context leaked a
    // combined type (e.g. `'SoapFault'` from a `!==` in an `||`) is NOT
    // condition-changed — it must stay possibly-redefined so the post-if
    // merge keeps the leaked member. Vars the conditional seed already
    // narrowed still count for CLAUSE REMOVAL (Psalm narrows them in this
    // entry reconcile, marking them changed).
    let pre_narrowed_vars: FxHashSet<VarName> = if_context
        .locals
        .iter()
        .filter(|(var_id, seeded_type)| context.locals.get(*var_id) != Some(seeded_type))
        .map(|(var_id, _)| var_id.clone())
        .collect();
    // Psalm's IfAnalyzer $omit_keys: vars mentioned in the OUTER context's
    // clauses — minus the outer formula's own unit truths — don't report
    // redundancy at the entry reconcile (an inherited `||` clause means the
    // var's narrowing can't be reasoned about simply).
    let mut omit_report_vars: FxHashSet<VarName> = {
        let outer_clause_refs: Vec<&Clause> = context
            .clauses
            .iter()
            .map(|clause| clause.as_ref())
            .collect();
        let mut outer_referenced = FxHashSet::default();
        let (outer_truths, _) = get_truths_from_formula(
            outer_clause_refs.iter().copied().collect(),
            None,
            &mut outer_referenced,
        );
        context
            .clauses
            .iter()
            .flat_map(|clause| clause.possibilities.keys())
            .filter_map(|key| match key {
                pzoom_code_info::algebra::ClauseKey::Name(name) => Some(name.clone()),
                pzoom_code_info::algebra::ClauseKey::Range(..) => None,
            })
            .filter(|name| !outer_truths.contains_key(name))
            .collect()
    };
    // Vars an unconditional @psalm-assert inside the condition narrowed apply
    // before the formula in Psalm; their entry contradictions stay silent.
    omit_report_vars.extend(assertion_result.silently_asserted_vars.iter().cloned());
    let if_cond_changed_var_ids = apply_clauses_to_context_full(
        analyzer,
        &mut if_context,
        analysis_data,
        Some(if_conditional_id),
        reconciler::EmissionMode::ImpossibleOnly,
        Some(&pre_narrowed_vars),
        Some(&omit_report_vars),
        true,
    );
    promote_guaranteed_true_condition_assignments(analyzer, if_stmt.condition, &mut if_context);

    // The if context as it stands after condition narrowing but before the body —
    // Psalm's `$old_if_context`, the baseline for `Context::update`.
    let old_if_context = if_context.clone();

    // Snapshot assignment tracking so we can isolate what the if body assigns.
    // Psalm clears both sets before the body ($if_context->assigned_var_ids =
    // []) so a var already assigned before the if (a param, an earlier
    // assignment) still registers a body reassignment — a set difference
    // would hide it.
    let pre_if_assigned_var_ids = if_context.assigned_var_ids.clone();
    let pre_if_possibly_assigned_var_ids =
        std::mem::take(&mut if_context.possibly_assigned_var_ids);

    // Analyze the if body
    let if_stmts = if_stmt.body.statements();
    analyze_stmts(analyzer, if_stmts, analysis_data, &mut if_context)?;

    // `class_alias()` registers a GLOBAL runtime alias (Psalm records it
    // codebase-wide at scan): aliases declared inside the branch survive it.
    for (alias_id, target_id) in &if_context.class_aliases {
        context.class_aliases.entry(*alias_id).or_insert(*target_id);
    }

    // A var whose clauses were evicted inside the branch (it was assigned/
    // unset there) invalidates the outer context's clauses about it too —
    // Psalm's `$if_context->parent_remove_vars` loop (IfAnalyzer 168-170).
    for var_name in if_context.parent_remove_vars.clone() {
        context.remove_var_name_from_conflicting_clauses(&var_name);
    }

    // Determine the if branch's control actions.
    let if_actions = scope_analyzer::get_control_actions(if_stmts, analysis_data, &[], true);
    let if_exits = !if_actions.contains(&ControlAction::None);
    let has_ending_if = if_actions.len() == 1 && if_actions.contains(&ControlAction::End);

    // The if condition's CNF formula (formula-generator based, so disjunctions like
    // `$a || $b` are represented faithfully — unlike the assertion-finder clauses
    // pzoom uses to narrow the if body).
    let if_clauses = crate::formula_generator::get_formula(
        if_conditional_id,
        if_conditional_id,
        if_stmt.condition,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();

    // reasonable_clauses = the clauses true on the if path (parent ∧ condition).
    // Built from the formula so a continuing-if/leaving-else like
    // `if ($a || $b) {} else { return; }` carries `$a || $b` to the outer context.
    let reasonable_clauses: Vec<std::rc::Rc<Clause>> = {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_clauses.iter().cloned());
        simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect()
    };

    // ----- Build the IfScope (Psalm `IfElseAnalyzer` setup) -----
    let mut if_scope = IfScope {
        if_actions: if_actions.clone(),
        final_actions: if_actions.clone(),
        if_cond_changed_var_ids,
        reasonable_clauses,
        ..IfScope::default()
    };

    // negated_clauses = negateFormula(if_clauses). When that is too complex
    // (Psalm's ComplicatedExpressionException), Psalm retries by generating the
    // formula of `!cond` directly (FormulaGenerator::getFormula on a
    // VirtualBooleanNot, IfElseAnalyzer.php), which De Morgans the condition
    // instead of expanding the positive CNF; only then an empty formula.
    if_scope.negated_clauses = negate_formula(if_clauses.clone()).unwrap_or_else(|_| {
        crate::formula_generator::get_negated_formula(
            if_conditional_id,
            if_stmt.condition,
            analyzer,
            analysis_data,
        )
        .unwrap_or_default()
    });

    // negated_types = truths of simplifyCNF(context.clauses + negated_clauses).
    {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        let simplified = simplify_cnf(combined.iter().collect());
        let mut referenced = FxHashSet::default();
        if_scope.negated_types =
            get_truths_from_formula(simplified.iter().collect(), None, &mut referenced).0;
    }

    // Psalm reconciles negated_types into a *temporary* clone of the
    // post-condition context to compute the vars a hypothetical else would
    // redefine (`$pre_assignment_else_redefined_vars`). When the if branch
    // always leaves and there is no elseif, these are the continuation's
    // guard-proven types, used below to retract MixedAssignment issues.
    let pre_assignment_else_redefined_vars: FxHashMap<VarName, TUnion> =
        if !if_scope.negated_types.is_empty() {
            let mut temp_else_context = context.clone();
            temp_else_context.locals = post_if_locals.clone();
            let mut changed_var_ids = FxHashSet::default();
            let inside_loop = context.inside_loop;
            reconciler::reconcile_keyed_types(
                &if_scope.negated_types,
                &mut temp_else_context,
                &mut changed_var_ids,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                reconciler::EmissionMode::Silent,
                None,
            );
            temp_else_context
                .get_redefined_vars(&context.locals, true)
                .into_iter()
                .filter(|(var_id, _)| changed_var_ids.contains(var_id))
                .collect()
        } else {
            FxHashMap::default()
        };

    // Psalm's negated-types reconcile (into a temporary clone) reports
    // RedundantPropertyInitializationCheck when `isset(self::$prop)` negates
    // over a non-nullable typed static property. pzoom checks those keys
    // directly rather than re-running an emitting reconcile.
    let if_side_truths = {
        let mut referenced = FxHashSet::default();
        get_truths_from_formula(if_clauses.iter().collect(), None, &mut referenced).0
    };
    for (var_name, assertion_groups) in if_scope.negated_types.iter().chain(if_side_truths.iter()) {
        let is_static_property = var_name.contains("::$");
        // `$this->prop` accessing a *real declared* non-nullable property: an
        // undeclared/magic property (`$xml->child` on SimpleXMLElement) carries
        // no initialization guarantee, and inside the constructor the property
        // is still being initialized (Psalm keeps it possibly_undefined there).
        let is_instance_property = var_name.strip_prefix("$this->").is_some_and(|prop_path| {
            !prop_path.contains("->")
                && !prop_path.contains("::")
                && !prop_path.contains('[')
                && analyzer.function_info.map(|info| info.name) != Some(pzoom_str::StrId::CONSTRUCT)
                && context.self_class.is_some_and(|self_class| {
                    let prop_id = analyzer.interner.intern(prop_path);
                    analyzer
                        .codebase
                        .get_class(self_class)
                        .is_some_and(|class_info| class_info.properties.contains_key(&prop_id))
                })
        });
        if (is_static_property || is_instance_property)
            && !var_name.contains('[')
            && assertion_groups
                .iter()
                .flatten()
                .any(|assertion| matches!(assertion, pzoom_code_info::Assertion::IsNotIsset))
            && let Some(declared) = reconciler::resolve_key_type(var_name, context, analyzer)
            && !declared.possibly_undefined_from_try
            && !declared.is_nullable()
            && !declared
                .types
                .iter()
                .any(|atomic| matches!(atomic, pzoom_code_info::TAtomic::TNull))
            && !declared.is_mixed()
        {
            let start = analysis_data.current_stmt_start.unwrap_or(0);
            let end = analysis_data.current_stmt_end.unwrap_or(start);
            let (line, col) = analyzer.get_line_column(start);
            // Psalm's SimpleNegatedAssertionReconciler distinguishes static
            // (`from_static_property`) and instance (`from_property`) properties.
            let message = if is_static_property {
                format!(
                    "Static property {} with type {} has unexpected isset check — should it be nullable?",
                    var_name,
                    declared.get_id(Some(analyzer.interner))
                )
            } else {
                format!(
                    "Property {} with type {} should already be set in the constructor",
                    var_name,
                    declared.get_id(Some(analyzer.interner))
                )
            };
            analysis_data.add_issue(pzoom_code_info::Issue::new(
                pzoom_code_info::IssueKind::RedundantPropertyInitializationCheck,
                message,
                analyzer.file_path,
                start,
                end,
                line,
                col,
            ));
        }
    }

    // negatable_if_types: only the variables whose negation is a *definite* per-var
    // fact (single-possibility clause) are safe to push back through
    // `BlockContext::update`. A disjunctive negation such as `$a || $b` (from
    // `if (!$a && !$b)`) yields no truths here, so neither var is negated per-var —
    // that fact is carried as a clause instead. Restrict to the condition's own
    // variables so unrelated context truths aren't substituted, and exclude
    // variables the if branch *evicted* (a mutating call invalidated the
    // narrowing): their negated fact no longer survives the branch merge.
    {
        // Psalm's IfAnalyzer: vars_to_update = the negated-types keys whose
        // NEGATED reconcile actually redefined the var
        // (pre_assignment_else_redefined_vars ∩ negated_types). A var the
        // negation leaves unchanged (`!== $other` on an object union) must
        // not be substituted away by Context::update when the if exits.
        let mut negatable = FxHashSet::default();
        for var_name in if_scope.negated_types.keys() {
            if pre_assignment_else_redefined_vars.contains_key(var_name)
                && !(old_if_context.locals.contains_key(var_name)
                    && !if_context.locals.contains_key(var_name))
            {
                negatable.insert(var_name.clone());
            }
        }
        if_scope.negatable_if_types = negatable;
    }

    // Fold the if branch into the if_scope when it can fall through
    // (Psalm `IfAnalyzer::updateIfScope`). Isolate what the if body assigned.
    let new_if_assigned_var_ids: FxHashMap<VarName, usize> = if_context
        .assigned_var_ids
        .iter()
        .filter(|(var_id, count)| {
            pre_if_assigned_var_ids
                .get(*var_id)
                .map_or(true, |pre| *count > pre)
        })
        .map(|(var_id, count)| (var_id.clone(), *count))
        .collect();
    let new_if_possibly_assigned_var_ids: FxHashSet<VarName> =
        if_context.possibly_assigned_var_ids.clone();
    if_context
        .possibly_assigned_var_ids
        .extend(pre_if_possibly_assigned_var_ids.iter().cloned());

    if !if_exits {
        let if_cond_changed = if_scope.if_cond_changed_var_ids.clone();
        update_if_scope(
            analyzer,
            &mut if_scope,
            &if_context,
            context,
            &old_if_context.locals,
            &new_if_assigned_var_ids,
            &new_if_possibly_assigned_var_ids,
            &if_cond_changed,
            true,
        );
    } else if !(if_actions.len() == 1 && if_actions.contains(&ControlAction::Break))
        && !assigned_in_conditional_var_ids.is_empty()
        && let Some(replay_context) = post_leaving_if_context.as_mut()
    {
        // Psalm IfAnalyzer::addConditionallyAssignedVarsToContext: the if body
        // always leaves, so the fallthrough is the condition-false path — but
        // vars the condition assigned must still reach it. Replay the negated
        // condition against the pre-condition clone and copy the assigned
        // vars' final types into the post-if context.
        let pre_replay_types: FxHashMap<VarName, Option<TUnion>> = assigned_in_conditional_var_ids
            .iter()
            .map(|var_id| (var_id.clone(), replay_context.locals.get(var_id).cloned()))
            .collect();
        add_conditionally_assigned_vars_to_if_fallthrough(
            analyzer,
            if_stmt.condition,
            replay_context,
            &assigned_in_conditional_var_ids,
            analysis_data,
        );
        for var_id in &assigned_in_conditional_var_ids {
            // The recorded-assignment replay only covers `$x = ...` syntax; a
            // var assigned through a by-ref call keeps its pre-condition type
            // here, so fall back to the post-condition value (Psalm's replay
            // re-analyzes the real AST, re-running the call). Detect "replay
            // didn't touch it" by comparing against the pre-replay snapshot.
            if pre_replay_types.get(var_id).is_some_and(|pre| {
                match (pre, replay_context.locals.get(var_id)) {
                    (Some(pre), Some(post)) => crate::context::unions_structurally_equal(pre, post),
                    (None, None) => true,
                    _ => false,
                }
            }) && let Some(post_type) = post_if_locals
                .get(var_id)
                .or_else(|| cond_if_body_context.locals.get(var_id))
            {
                replay_context
                    .locals
                    .insert(var_id.clone(), post_type.clone());
            }
            if let Some(final_type) = replay_context.locals.get(var_id) {
                let mut final_type = final_type.clone();
                // Keep the condition's assignment dataflow nodes (Psalm's replay
                // re-analyzes the real AST, so its uses naturally reach them):
                // a read in the fallthrough/else must still mark the condition's
                // assignments used.
                for source_type in [
                    post_if_locals.get(var_id),
                    cond_if_body_context.locals.get(var_id),
                ]
                .into_iter()
                .flatten()
                {
                    for node in &source_type.parent_nodes {
                        if !final_type.parent_nodes.contains(node) {
                            final_type.parent_nodes.push(node.clone());
                        }
                    }
                }
                replay_context
                    .locals
                    .insert(var_id.clone(), final_type.clone());
                context.locals.insert(var_id.clone(), final_type);
                context.vars_possibly_in_scope.insert(var_id.clone());
            }
        }
    }

    // Psalm IfAnalyzer: unless the if body *ends* (return/throw), vars it
    // possibly assigned become possibly-in-scope — inside a loop they reach
    // the loop scope (a break/continue path's assignments are visible after
    // the loop as possibly-defined), otherwise the if scope when the body
    // falls through.
    if !has_ending_if {
        let vars_possibly_in_scope: FxHashSet<_> = if_context
            .vars_possibly_in_scope
            .difference(&context.vars_possibly_in_scope)
            .cloned()
            .collect();

        let has_break_statement =
            if_actions.len() == 1 && if_actions.contains(&ControlAction::Break);
        let has_continue_statement =
            if_actions.len() == 1 && if_actions.contains(&ControlAction::Continue);

        if context.inside_loop
            && let Some(loop_scope) = analysis_data.loop_scopes.last_mut()
        {
            if !has_break_statement && !has_continue_statement {
                if_scope
                    .new_vars_possibly_in_scope
                    .extend(vars_possibly_in_scope.iter().cloned());
                if_scope
                    .possibly_assigned_var_ids
                    .extend(new_if_possibly_assigned_var_ids.iter().cloned());
            }
            loop_scope
                .vars_possibly_in_scope
                .extend(vars_possibly_in_scope);
        } else if !if_exits {
            if_scope
                .new_vars_possibly_in_scope
                .extend(vars_possibly_in_scope);
            if_scope
                .possibly_assigned_var_ids
                .extend(new_if_possibly_assigned_var_ids.iter().cloned());
        }
    }

    // Propagate the if-condition's narrowing into the outer context. When the if
    // branch leaves, this strips the now-impossible (condition-true) possibility
    // from the outer types (`if (!$a) return;` ⇒ `$a` truthy afterwards); Psalm's
    // `$outer_context->update(...)` gated on negated_types.
    if !if_scope.negated_types.is_empty() && !if_scope.negatable_if_types.is_empty() {
        let negatable = if_scope.negatable_if_types.clone();
        let mut updated_vars = std::mem::take(&mut if_scope.updated_vars);
        context.update(
            &old_if_context,
            &if_context,
            if_exits,
            &negatable,
            &mut updated_vars,
        );
        if_scope.updated_vars = updated_vars;
    }

    context.update_references_possibly_from_confusing_scope(&if_context);

    // Carry the if branch's by-ref constraints outward, reporting conflicts
    // (Psalm's IfAnalyzer byref_constraints merge). The pre-carry set is kept
    // for the else/elseif contexts: Psalm clones those from the outer context
    // BEFORE branch analysis, so a constraint established inside the if
    // branch never applies to its sibling branches.
    let pre_if_reference_constraints = context.reference_constraints.clone();
    {
        let if_span = if_stmt.span();
        let branch_constraints = if_context.clone();
        carry_reference_constraints_to_outer(
            analyzer,
            analysis_data,
            context,
            &branch_constraints,
            (if_span.start.offset, if_span.end.offset),
        );
    }

    // ----- Build the fallthrough context and drive elseif/else branches -----
    let mut else_context = context.child();
    else_context.reference_constraints = pre_if_reference_constraints;
    // Reset to the pre-if-body fallthrough locals so the else/elseif branches don't
    // inherit values the if body (via `update`) wrote into the outer context.
    // Psalm clones the else context from post_leaving_if_context when the if
    // body always leaves ($scope_to_clone = post_leaving ?? post_if), so the
    // conditionally-assigned replay's types reach the elseif/else branches.
    else_context.locals = match post_leaving_if_context {
        Some(replayed) => replayed.locals,
        None => post_if_locals.clone(),
    };
    seed_assignment_tracking(context, &mut else_context);
    {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        else_context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
    }
    // Psalm reconciles negated_types only into a *temporary* clone (to compute
    // pre-assignment else-redefined vars); the real else_context reaches
    // ElseAnalyzer un-reconciled so its redefinition baseline still shows the
    // pre-narrowing types — that is what lets a negation-narrowed var count as
    // redefined and merge against the if branch's assignment instead of the
    // outer type. pzoom's else_analyzer re-derives the same narrowing from the
    // combined clauses (its else_types reconcile).

    let mut all_elseifs_leave = true;
    let mut has_elseifs = false;
    for (elseif_cond, elseif_stmts) in if_stmt.body.else_if_clauses() {
        has_elseifs = true;
        let elseif_actions =
            scope_analyzer::get_control_actions(elseif_stmts, analysis_data, &[], true);
        all_elseifs_leave = all_elseifs_leave && !elseif_actions.contains(&ControlAction::None);
        elseif_analyzer::analyze(
            analyzer,
            elseif_cond,
            elseif_stmts,
            &mut if_scope,
            &mut else_context,
            context,
            analysis_data,
        )?;
    }

    // The else's per-variable `update()` is only sound for a simple if/else: with
    // intervening elseif branches the fallthrough can still reach the continuation
    // with the variable at its else-path value, so narrowing it away (when the else
    // leaves) would be unsound. The branch merge handles those cases instead.
    if has_elseifs {
        if_scope.negatable_if_types.clear();
    }

    else_analyzer::analyze(
        analyzer,
        if_stmt.body.else_statements(),
        &mut if_scope,
        &mut else_context,
        context,
        analysis_data,
    )?;

    // Psalm IfElseAnalyzer: when the if branch always leaves (throw/return/
    // continue/break) and there are no elseifs, the negated condition is a
    // proven fact for the continuation — any variable it redefines from a
    // mixed-bearing type to a non-mixed one was effectively guarded, so the
    // MixedAssignment reported at the variable's first assignment is
    // retracted (`IssueBuffer::remove`).
    if !if_actions.is_empty() && !if_actions.contains(&ControlAction::None) && !has_elseifs {
        for (var_id, reconciled_type) in &pre_assignment_else_redefined_vars {
            if post_if_locals
                .get(var_id)
                .is_some_and(|post_if_type| post_if_type.is_mixed())
                && !reconciled_type.is_mixed()
                && let Some(first_appearance) =
                    analysis_data.first_var_appearances.get(var_id).copied()
            {
                analysis_data.remove_issue(IssueKind::MixedAssignment, first_appearance);
            }
        }
    }

    // When there is no explicit `else` and every condition branch leaves (returns,
    // throws, …), reaching the code after the construct implies every branch
    // condition was false. Carry the accumulated negated clauses into the outer
    // context so a disjunctive fact like `$a || $b` (from `if (!$a && !$b) return;`)
    // survives to narrow later statements. (Psalm threads this via the
    // post-leaving-if continuation context.)
    // With an explicit else: the if leaving means control reaches the
    // continuation only through the else, so the negated condition clauses
    // still hold — minus clauses about vars the branches (possibly) assigned
    // (their narrowing is stale). Psalm reaches the same end state through its
    // post-leaving-if continuation context.
    if if_stmt.body.has_else_clause() && if_exits && all_elseifs_leave {
        let mut stale_vars: rustc_hash::FxHashSet<pzoom_code_info::VarName> =
            if_scope.possibly_assigned_var_ids.iter().cloned().collect();
        if let Some(assigned) = &if_scope.assigned_var_ids {
            stale_vars.extend(assigned.keys().cloned());
        }

        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(
            if_scope
                .negated_clauses
                .iter()
                .filter(|clause| {
                    !clause.possibilities.keys().any(|key| match key {
                        pzoom_code_info::algebra::ClauseKey::Name(name) => {
                            stale_vars.contains(name)
                        }
                        pzoom_code_info::algebra::ClauseKey::Range(..) => false,
                    })
                })
                .cloned(),
        );
        context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
    }

    if !if_stmt.body.has_else_clause() && if_exits && all_elseifs_leave {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
        apply_clauses_to_context(
            analyzer,
            context,
            analysis_data,
            None,
            reconciler::EmissionMode::Silent,
            // This reconciles directly into the OUTER context (Psalm threads
            // the leaving-if continuation through the else fork instead), so
            // the dependent-key sweep must not run here.
            false,
        );

        // The assertion finder narrows some negations the formula→reconcile path
        // doesn't (e.g. `!== ""` ⇒ non-empty-string, `get_class($a) !== B` ⇒
        // instanceof B). When there are no elseifs the if's `if_false` assertions
        // are exactly the continuation's facts, so apply them directly too.
        if !has_elseifs && !assertion_result.if_false.is_empty() {
            let inside_loop = context.inside_loop;
            let mut changed = FxHashSet::default();
            reconciler::reconcile_keyed_types(
                &assertion_result.if_false,
                context,
                &mut changed,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                crate::reconciler::EmissionMode::Silent,
                None,
            );
        }
    }

    // ----- Apply the IfScope back into the outer context (Psalm tail) -----
    // Psalm: vars_possibly_in_scope = new_vars_possibly_in_scope + existing.
    context
        .vars_possibly_in_scope
        .extend(if_scope.new_vars_possibly_in_scope.iter().cloned());
    context
        .possibly_assigned_var_ids
        .extend(if_scope.new_vars_possibly_in_scope.iter().cloned());
    context
        .possibly_assigned_var_ids
        .extend(if_scope.possibly_assigned_var_ids.iter().cloned());
    if let Some(assigned) = &if_scope.assigned_var_ids {
        for (var_id, count) in assigned {
            context.assigned_var_ids.insert(var_id.clone(), *count);
        }
    }

    if let Some(new_vars) = if_scope.new_vars.clone() {
        for (var_id, var_type) in new_vars {
            context.locals.insert(var_id.clone(), var_type);

            // A variable defined in *every* branch is definitely assigned —
            // clear any possibly-assigned demotion (both key spellings).
            context.possibly_assigned_var_ids.remove(&var_id);
            let alternate = if let Some(stripped) = var_id.strip_prefix('$') {
                VarName::new(stripped)
            } else {
                VarName::from(format!("${}", var_id))
            };
            context.assigned_var_ids.entry(var_id).or_insert(1);
            context.possibly_assigned_var_ids.remove(&alternate);
            context.assigned_var_ids.entry(alternate).or_insert(1);
        }
    }

    if let Some(redefined_vars) = if_scope.redefined_vars.clone() {
        for (var_id, var_type) in redefined_vars {
            let structurally_changed = context.locals.get(&var_id).map_or(true, |existing| {
                !crate::context::unions_structurally_equal(&var_type, existing)
            });
            context.locals.insert(var_id.clone(), var_type);
            if_scope.updated_vars.insert(var_id.clone());

            // Psalm's Context::filterClauses: a redefined var's prior clauses can
            // no longer be trusted, so drop the ones that mention it. (Variables
            // only narrowed by a condition fall back to their outer type during the
            // branch merge and so are not present here.) Parent-node-only
            // changes keep the clauses.
            if structurally_changed && !if_scope.reasonable_clauses.is_empty() {
                let mut changed = FxHashSet::default();
                changed.insert(var_id.clone());
                if_scope.reasonable_clauses = BlockContext::remove_reconciled_clause_refs(
                    &if_scope.reasonable_clauses,
                    &changed,
                )
                .0;
            }
        }
    }

    if !if_scope.reasonable_clauses.is_empty()
        && (if_scope.reasonable_clauses.len() > 1 || !if_scope.reasonable_clauses[0].wedge)
    {
        let mut combined: Vec<Clause> = if_scope
            .reasonable_clauses
            .iter()
            .map(|c| (**c).clone())
            .collect();
        combined.extend(context.clauses.iter().map(|c| (**c).clone()));
        context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
    }

    context
        .locals
        .retain(|var_id, _| !if_scope.removed_var_ids.contains(var_id));

    for (var_id, var_type) in if_scope.possibly_redefined_vars.clone() {
        if context.locals.contains_key(&var_id) && !if_scope.updated_vars.contains(&var_id) {
            let existing = context.locals.get(&var_id).cloned().unwrap();
            let combined = combine_union_types(&existing, &var_type, false);
            // Structural changes invalidate dependents; parent-node-only
            // changes (dataflow merging) must not.
            if !crate::context::unions_structurally_equal(&combined, &existing) {
                context.invalidate_dependent_types(&var_id);
            }
            context.locals.insert(var_id, combined);
        } else if let Some(existing) = context.locals.get_mut(&var_id) {
            // The type was already settled through Context::update, but the
            // branch assignment nodes must still reach the post-if dataflow
            // (an elseif's reassignment is no less a definition for it).
            for parent_node in var_type.parent_nodes {
                if !existing
                    .parent_nodes
                    .iter()
                    .any(|node| node.id == parent_node.id)
                {
                    existing.parent_nodes.push(parent_node);
                }
            }
        }
    }

    // If no branch can fall through, control never reaches the code after the if.
    if !if_scope.final_actions.contains(&ControlAction::None) {
        context.has_returned = true;
    }

    // Hakana's ifelse_analyzer: the enclosing loop learns the if's control
    // actions — in particular the `None` of a fall-through/missing-else branch,
    // without which a lone `if (...) { break; }` reads as "the loop always
    // breaks" and the loop merge drops the continue path.
    if let Some(loop_scope) = analysis_data.loop_scopes.last_mut() {
        loop_scope
            .final_actions
            .extend(if_scope.final_actions.iter().copied());
    }

    Ok(())
}

/// Port of Psalm's `IfAnalyzer::updateIfScope`: folds one branch's redefinitions
/// into the shared [`IfScope`]. New variables are intersected across branches and
/// their types unioned; redefined variables are unioned; "possibly redefined"
/// tracks variables changed in only some branches.
#[allow(clippy::too_many_arguments)]
/// Psalm's IfAnalyzer byref-constraints merge: carry a branch's by-ref
/// constraints into the outer context, reporting ConflictingReferenceConstraint
/// when the branch's constraint is incompatible with one already present.
pub(crate) fn carry_reference_constraints_to_outer(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    outer_context: &mut BlockContext,
    branch_context: &BlockContext,
    stmt_pos: (u32, u32),
) {
    for (var_id, constraints) in &branch_context.reference_constraints {
        let entry = outer_context
            .reference_constraints
            .entry(var_id.clone())
            .or_default();
        for constraint in constraints {
            if entry.contains(constraint) {
                continue;
            }

            let conflicting_existing = entry.iter().find(|existing| {
                let mut comparison = crate::type_comparator::TypeComparisonResult::new();
                !crate::type_comparator::union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    constraint,
                    existing,
                    false,
                    false,
                    &mut comparison,
                )
            });

            if let Some(existing) = conflicting_existing {
                let (line, col) = analyzer.get_line_column(stmt_pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ConflictingReferenceConstraint,
                    format!(
                        "There is more than one pass-by-reference constraint on {} between {} and {}",
                        var_id,
                        constraint.get_id(Some(analyzer.interner)),
                        existing.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    stmt_pos.0,
                    stmt_pos.1,
                    line,
                    col,
                ));
            } else {
                entry.push(constraint.clone());
            }
        }
    }
}

pub(crate) fn update_if_scope(
    _analyzer: &StatementsAnalyzer<'_>,
    if_scope: &mut IfScope,
    branch_context: &BlockContext,
    outer_context: &BlockContext,
    // The branch context right after condition narrowing, before the body —
    // the baseline for "the body removed this var". Vars the CONDITION's
    // reconciliation dropped (the dependent-key sweep on a narrowed root,
    // e.g. `$storage->return_type` under `if ($storage instanceof X)`) are
    // already absent here, and the outer narrowing stays valid after the if
    // (Psalm never removes outer entries for branch-missing vars at all).
    post_condition_locals: &FxHashMap<VarName, TUnion>,
    assigned_var_ids: &FxHashMap<VarName, usize>,
    possibly_assigned_var_ids: &FxHashSet<VarName>,
    newly_reconciled_var_ids: &FxHashSet<VarName>,
    update_new_vars: bool,
) {
    let mut removed_vars = FxHashSet::default();
    let mut redefined_vars =
        branch_context.get_redefined_locals(&outer_context.locals, false, &mut removed_vars);
    // A var present in the scope entering the body but gone from the branch
    // (e.g. a property narrowing invalidated by a mutating method call) is no
    // longer known after the if — Hakana records these on the IfScope and the
    // merge drops them from the outer context.
    removed_vars.retain(|var_id| post_condition_locals.contains_key(var_id));
    if_scope
        .removed_var_ids
        .extend(removed_vars.iter().cloned());

    // The `||`/`&&` analyzers can re-emit a variable's type with fresh data-flow
    // nodes but the same structure; treating that as a redefinition would wrongly
    // invalidate clauses about the variable (e.g. drop `$a || $b` after an empty
    // `if ($a || $b)` body). Keep only genuine structural changes.
    redefined_vars.retain(|var_id, branch_type| {
        outer_context.locals.get(var_id).map_or(true, |outer_type| {
            // Parent-only differences stay (their dataflow must merge —
            // a branch assignment with the same structural type still has
            // its own assignment node); the structural rule below keeps
            // clause invalidation honest.
            !crate::context::unions_structurally_equal(branch_type, outer_type)
                || branch_type.parent_nodes != outer_type.parent_nodes
        })
    });

    match &mut if_scope.new_vars {
        None => {
            if update_new_vars {
                let new_vars: BTreeMap<VarName, TUnion> = branch_context
                    .locals
                    .iter()
                    .filter(|(var_id, _)| !outer_context.locals.contains_key(*var_id))
                    .map(|(var_id, ty)| (var_id.clone(), ty.clone()))
                    .collect();
                if_scope.new_vars = Some(new_vars);
            }
        }
        Some(new_vars) => {
            new_vars.retain(|var_id, _| branch_context.has_variable(var_id));
            for (var_id, ty) in new_vars.iter_mut() {
                if let Some(branch_ty) = branch_context.locals.get(var_id) {
                    *ty = combine_union_types(ty, branch_ty, false);
                }
            }
        }
    }

    // possibly_redefined = redefined, minus vars only narrowed but never assigned.
    let mut possibly_redefined_vars = redefined_vars.clone();
    possibly_redefined_vars.retain(|var_id, _| {
        !(!possibly_assigned_var_ids.contains(var_id) && newly_reconciled_var_ids.contains(var_id))
    });

    match &mut if_scope.assigned_var_ids {
        None => if_scope.assigned_var_ids = Some(assigned_var_ids.clone()),
        Some(existing) => {
            let intersected: FxHashMap<VarName, usize> = assigned_var_ids
                .iter()
                .filter(|(var_id, _)| existing.contains_key(*var_id))
                .map(|(var_id, count)| (var_id.clone(), *count))
                .collect();
            *existing = intersected;
        }
    }

    for var_id in possibly_assigned_var_ids {
        if_scope.possibly_assigned_var_ids.insert(var_id.clone());
    }

    match &mut if_scope.redefined_vars {
        None => {
            if_scope.redefined_vars = Some(redefined_vars);
            if_scope.possibly_redefined_vars = possibly_redefined_vars;
        }
        Some(existing) => {
            existing.retain(|var_id, _| redefined_vars.contains_key(var_id));
            for (var_id, ty) in existing.iter_mut() {
                *ty = combine_union_types(&redefined_vars[var_id], ty, false);
            }
            // Once the per-branch types are combined, a variable whose merged type
            // matches the outer type again was not really redefined (e.g. narrowed
            // one way in the `if`, the other in the `elseif`, recombining to the
            // original) — drop it so its clauses survive. Parent-node-only
            // differences must stay merged, though: a branch reassignment with
            // the same structural type still owns assignment nodes that the
            // post-if dataflow has to reach (same rule as the branch retain
            // above).
            existing.retain(|var_id, ty| match outer_context.locals.get(var_id) {
                Some(outer_ty) => {
                    !crate::context::unions_structurally_equal(ty, outer_ty)
                        || ty.parent_nodes != outer_ty.parent_nodes
                }
                None => true,
            });
            for (var_id, ty) in possibly_redefined_vars {
                use std::collections::hash_map::Entry;
                match if_scope.possibly_redefined_vars.entry(var_id) {
                    Entry::Occupied(mut occupied) => {
                        let combined = combine_union_types(&ty, occupied.get(), false);
                        occupied.insert(combined);
                    }
                    Entry::Vacant(vacant) => {
                        vacant.insert(ty);
                    }
                }
            }
        }
    }
}

#[derive(Default, Clone)]
struct GuaranteedAssignmentSets {
    when_true: FxHashSet<VarName>,
    when_false: FxHashSet<VarName>,
}

fn promote_guaranteed_true_condition_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    condition: &Expression<'_>,
    context: &mut BlockContext,
) {
    let guaranteed = collect_guaranteed_assignments(analyzer, condition);
    for var_id in guaranteed.when_true {
        if context.locals.contains_key(&var_id) {
            *context.assigned_var_ids.entry(var_id.clone()).or_insert(0) += 1;
            context.possibly_assigned_var_ids.remove(&var_id);
        }
    }
}

fn seed_assignment_tracking(from: &BlockContext, to: &mut BlockContext) {
    to.assigned_var_ids = from.assigned_var_ids.clone();
    to.possibly_assigned_var_ids = from.possibly_assigned_var_ids.clone();
}

pub(crate) fn condition_has_assignments(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Assignment(_) | Expression::UnaryPostfix(_) => true,
        Expression::Binary(binary) => {
            condition_has_assignments(binary.lhs) || condition_has_assignments(binary.rhs)
        }
        Expression::UnaryPrefix(unary) => condition_has_assignments(unary.operand),
        Expression::Parenthesized(parenthesized) => {
            condition_has_assignments(parenthesized.expression)
        }
        _ => false,
    }
}

pub(crate) fn condition_contains_unsafe_empty_construct(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Construct(Construct::Empty(empty_construct)) => {
            matches!(
                empty_construct.value.unparenthesized(),
                Expression::ArrayAccess(_)
                    | Expression::Access(Access::Property(_))
                    | Expression::Access(Access::NullSafeProperty(_))
            )
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            condition_contains_unsafe_empty_construct(unary.operand)
        }
        Expression::Parenthesized(paren) => {
            condition_contains_unsafe_empty_construct(paren.expression)
        }
        Expression::Binary(binary) => {
            condition_contains_unsafe_empty_construct(binary.lhs)
                || condition_contains_unsafe_empty_construct(binary.rhs)
        }
        _ => false,
    }
}

fn collect_guaranteed_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> GuaranteedAssignmentSets {
    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => {
            collect_guaranteed_assignments(analyzer, parenthesized.expression)
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = collect_guaranteed_assignments(analyzer, unary.operand);
            GuaranteedAssignmentSets {
                when_true: inner.when_false,
                when_false: inner.when_true,
            }
        }
        Expression::Assignment(assignment) => {
            let mut rhs = collect_guaranteed_assignments(analyzer, assignment.rhs);
            if let Expression::Variable(Variable::Direct(direct)) = assignment.lhs.unparenthesized()
            {
                let var_id = VarName::new(direct.name);
                rhs.when_true.insert(var_id.clone());
                rhs.when_false.insert(var_id);
            }
            rhs
        }
        Expression::Call(call) => {
            let by_ref_assignments =
                crate::expr::call::arguments_analyzer::collect_call_by_ref_assignments(
                    analyzer, call,
                );
            GuaranteedAssignmentSets {
                when_true: by_ref_assignments.clone(),
                when_false: by_ref_assignments.clone(),
            }
        }
        Expression::Binary(binary) => {
            let left = collect_guaranteed_assignments(analyzer, binary.lhs);
            let right = collect_guaranteed_assignments(analyzer, binary.rhs);

            match &binary.operator {
                BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
                    // true path: lhs true OR (lhs false and rhs true)
                    let path_true_rhs = union_set(&left.when_false, &right.when_true);
                    // false path: lhs false AND rhs false
                    let false_set = union_set(&left.when_false, &right.when_false);

                    GuaranteedAssignmentSets {
                        when_true: intersect_set(&left.when_true, &path_true_rhs),
                        when_false: false_set,
                    }
                }
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
                    // true path: lhs true AND rhs true
                    let true_set = union_set(&left.when_true, &right.when_true);
                    // false path: lhs false OR (lhs true and rhs false)
                    let path_false_rhs = union_set(&left.when_true, &right.when_false);

                    GuaranteedAssignmentSets {
                        when_true: true_set,
                        when_false: intersect_set(&left.when_false, &path_false_rhs),
                    }
                }
                _ => {
                    // Non short-circuit expression: both operands are evaluated.
                    let eval_set = union_set(
                        &guaranteed_when_evaluated(&left),
                        &guaranteed_when_evaluated(&right),
                    );
                    GuaranteedAssignmentSets {
                        when_true: eval_set.clone(),
                        when_false: eval_set,
                    }
                }
            }
        }
        _ => GuaranteedAssignmentSets::default(),
    }
}

fn guaranteed_when_evaluated(sets: &GuaranteedAssignmentSets) -> FxHashSet<VarName> {
    intersect_set(&sets.when_true, &sets.when_false)
}

fn union_set(left: &FxHashSet<VarName>, right: &FxHashSet<VarName>) -> FxHashSet<VarName> {
    left.union(right).cloned().collect()
}

fn intersect_set(left: &FxHashSet<VarName>, right: &FxHashSet<VarName>) -> FxHashSet<VarName> {
    left.intersection(right).cloned().collect()
}

/// Apply clauses to a context by simplifying the CNF formula and extracting
/// truths. Returns the reconciler's changed var ids (Psalm's
/// `$cond_changed_var_ids`), which feed `if_cond_changed_var_ids`.
/// `sweep_dependent_keys`: Psalm only runs the post-reconcile dependent-key
/// sweep on branch-entry (forked) contexts — IfAnalyzer/ElseIfAnalyzer/
/// ElseAnalyzer. When this helper reconciles facts directly into an OUTER
/// context (pzoom's leaving-if shortcut, where Psalm threads the result
/// through the else fork's new/redefined vars instead), the sweep must not
/// run: Psalm's outer context keeps sibling dim entries like `$arr['a']`.
pub(crate) fn apply_clauses_to_context(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    creating_conditional_id: Option<(u32, u32)>,
    emission_mode: reconciler::EmissionMode,
    sweep_dependent_keys: bool,
) -> FxHashSet<VarName> {
    apply_clauses_to_context_full(
        analyzer,
        context,
        analysis_data,
        creating_conditional_id,
        emission_mode,
        None,
        None,
        sweep_dependent_keys,
    )
}

/// `omit_report_vars`: Psalm's IfAnalyzer `$omit_keys` — vars mentioned in the
/// OUTER context's clauses (minus the outer formula's own unit truths) are
/// removed from `$cond_referenced_var_ids`, so the entry reconcile applies
/// their assertions without reporting redundancy/impossibility ("if the if
/// has an || in the conditional, we cannot easily reason about it").
fn apply_clauses_to_context_full(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    creating_conditional_id: Option<(u32, u32)>,
    emission_mode: reconciler::EmissionMode,
    pre_narrowed_vars: Option<&FxHashSet<VarName>>,
    omit_report_vars: Option<&FxHashSet<VarName>>,
    sweep_dependent_keys: bool,
) -> FxHashSet<VarName> {
    if context.clauses.is_empty() {
        return FxHashSet::default();
    }

    // Simplify the CNF formula
    let clause_refs: Vec<&Clause> = context.clauses.iter().map(|c| c.as_ref()).collect();
    let simplified = simplify_cnf(clause_refs);

    // Extract truths from the simplified formula
    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, mut active_truths) = get_truths_from_formula(
        simplified.iter().collect(),
        creating_conditional_id,
        &mut cond_referenced_var_ids,
    );
    if let Some(omit_report_vars) = omit_report_vars {
        active_truths.retain(|var_id, _| !omit_report_vars.contains(var_id));
    }

    // `truths` carries OR groups (each inner list is OR-ed, multiple lists are
    // applied conjunctively). reconcile_keyed_types handles both natively, and
    // active_truths is already clause-indexed.
    let mut changed_var_ids = FxHashSet::default();
    let previous_reconcile_pos = analysis_data.current_reconcile_pos;
    if creating_conditional_id.is_some() {
        analysis_data.current_reconcile_pos = creating_conditional_id;
    }
    reconciler::reconcile_keyed_types(
        &truths,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        emission_mode,
        if emission_mode == reconciler::EmissionMode::Silent {
            None
        } else {
            Some(&active_truths)
        },
    );
    analysis_data.current_reconcile_pos = previous_reconcile_pos;

    // Mirror Psalm: every `Reconciler::reconcileKeyedTypes` is followed by
    // `Context::removeReconciledClauses($clauses, $changed_var_ids)`. Once a
    // clause has been reconciled into a variable's type, retaining it is unsound
    // (the type now carries the fact) and lets it bleed into later constructs —
    // e.g. a disjunctive `$s === "a" || $s === "b"` surviving into a following
    // `switch ($s)`, where simplifying it against a case's negation re-derives the
    // case and wrongly reports `RedundantCondition`.
    let mut clause_removal_ids = changed_var_ids.clone();
    if let Some(pre_narrowed_vars) = pre_narrowed_vars {
        for var_id in truths.keys() {
            if pre_narrowed_vars.contains(var_id) {
                clause_removal_ids.insert(var_id.clone());
            }
        }
    }
    if !clause_removal_ids.is_empty() {
        context.clauses =
            BlockContext::remove_reconciled_clause_refs(&context.clauses, &clause_removal_ids).0;
    }
    if !clause_removal_ids.is_empty() && sweep_dependent_keys {
        // Psalm's IfAnalyzer/ElseIfAnalyzer/ElseAnalyzer follow the reconcile
        // with a dependent-key sweep: a changed root (including `$this`, which
        // the reconciler itself spares) invalidates memoized paths containing
        // it (`$key->...`, `$key[...]`, `...[$key]`), unless the path was
        // itself reconciled or referenced by the condition.
        let dependent_keys: Vec<pzoom_code_info::VarName> = context
            .locals
            .keys()
            .filter(|var_id| {
                !changed_var_ids.contains(var_id.as_str())
                    && !cond_referenced_var_ids.contains(var_id.as_str())
                    // Any asserted key was referenced by the condition (Psalm's
                    // $cond_referenced_var_ids exemption).
                    && !truths.contains_key(var_id.as_str())
                    && changed_var_ids.iter().any(|changed| {
                        var_id
                            .as_str()
                            .match_indices(changed.as_str())
                            .any(|(idx, _)| {
                                matches!(
                                    var_id.as_str().as_bytes().get(idx + changed.len()),
                                    Some(b'[') | Some(b']') | Some(b'-')
                                )
                            })
                    })
            })
            .cloned()
            .collect();
        for dependent_key in dependent_keys {
            context.locals.remove(&dependent_key);
        }
    }

    changed_var_ids
}

fn emit_active_assertion_contradictions(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    assertions: &BTreeMap<VarName, Vec<Vec<Assertion>>>,
    reconciled_expression_clauses: &[std::rc::Rc<Clause>],
    conditional_pos: (u32, u32),
    analysis_data: &mut FunctionAnalysisData,
) {
    if assertions.is_empty() {
        return;
    }

    // Mirror Hakana's reconciled_expression_clauses: an assertion an `&&`/`||`/ternary
    // already reconciled during condition analysis still applies (for narrowing
    // sequence) but is no longer *reported* — only the not-yet-reconciled assertions
    // are active. This lets `is_int($c) && is_numeric($c)` apply is_int (silently) and
    // still flag is_numeric as redundant, without re-reporting is_int itself.
    let reconciled_assertions: FxHashSet<(&str, u64)> = reconciled_expression_clauses
        .iter()
        .flat_map(|clause| {
            clause.possibilities.iter().filter_map(|(key, assertions)| {
                if let pzoom_code_info::algebra::ClauseKey::Name(name) = key {
                    Some(assertions.keys().map(move |hash| (name.as_str(), *hash)))
                } else {
                    None
                }
            })
        })
        .flatten()
        .collect();

    // Mirror Psalm's Reconciler::reconcileKeyedTypes contradiction reporting: an
    // assertion is eligible to report when it is referenced and active. Psalm gates
    // on `referenced_var_ids[$key]` and `active_new_types[$key][$offset]` — there is
    // no per-assertion-type allowlist. Offsets index the AND groups (Psalm's outer
    // `$if_types` list); a group is inactive only when every OR alternative in it
    // was already reconciled during condition analysis.
    // Psalm's IfAnalyzer $omit_keys: a var mentioned in the outer context's
    // clauses — unless it is one of the outer formula's own truths — is
    // dropped from $cond_referenced_var_ids, so its assertions reconcile
    // without reporting ("if the if has an || in the conditional, we cannot
    // easily reason about it").
    let omit_report_vars: FxHashSet<VarName> = {
        let outer_clause_refs: Vec<&Clause> = context
            .clauses
            .iter()
            .map(|clause| clause.as_ref())
            .collect();
        let mut outer_referenced = FxHashSet::default();
        let (outer_truths, _) = get_truths_from_formula(
            outer_clause_refs.iter().copied().collect(),
            None,
            &mut outer_referenced,
        );
        context
            .clauses
            .iter()
            .flat_map(|clause| clause.possibilities.keys())
            .filter_map(|key| match key {
                pzoom_code_info::algebra::ClauseKey::Name(name) => Some(name.clone()),
                pzoom_code_info::algebra::ClauseKey::Range(..) => None,
            })
            .filter(|name| !outer_truths.contains_key(name))
            .collect()
    };

    let mut active_assertion_offsets: BTreeMap<VarName, FxHashSet<usize>> = BTreeMap::new();
    for (var_name, var_assertion_groups) in assertions {
        if omit_report_vars.contains(var_name) {
            continue;
        }
        let offsets: FxHashSet<usize> = var_assertion_groups
            .iter()
            .enumerate()
            .filter(|(_, group)| {
                !group.iter().all(|assertion| {
                    reconciled_assertions.contains(&(var_name.as_str(), assertion.to_hash()))
                })
            })
            .map(|(offset, _)| offset)
            .collect();
        if !offsets.is_empty() {
            active_assertion_offsets.insert(var_name.clone(), offsets);
        }
    }

    if active_assertion_offsets.is_empty() {
        return;
    }

    let mut contradiction_context = context.clone();
    let mut changed_var_ids = FxHashSet::default();
    let inside_loop = contradiction_context.inside_loop;
    // Psalm's reconciler issues point at the condition expression.
    let previous_reconcile_pos = analysis_data.current_reconcile_pos;
    analysis_data.current_reconcile_pos = Some(conditional_pos);
    reconciler::reconcile_keyed_types(
        assertions,
        &mut contradiction_context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        inside_loop,
        false,
        crate::reconciler::EmissionMode::All,
        Some(&active_assertion_offsets),
    );
    analysis_data.current_reconcile_pos = previous_reconcile_pos;
}

/// Merge variable assignments from branches back to parent context.
pub(crate) fn infer_condition_truthiness_from_clauses(
    entry_clauses: &[std::rc::Rc<Clause>],
    assertion_result: &assertion_finder::AssertionResult,
) -> Option<bool> {
    let true_branch_impossible =
        formula_contradicts_entry_clauses(entry_clauses, &assertion_result.if_true_clauses);
    let false_branch_impossible =
        formula_contradicts_entry_clauses(entry_clauses, &assertion_result.if_false_clauses);

    match (true_branch_impossible, false_branch_impossible) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => None,
    }
}

fn formula_contradicts_entry_clauses(
    entry_clauses: &[std::rc::Rc<Clause>],
    formula: &[Clause],
) -> bool {
    if entry_clauses.is_empty() || formula.is_empty() {
        return false;
    }

    let Ok(negated_formula) = negate_formula(formula.to_vec()) else {
        return false;
    };

    for negated_clause in negated_formula {
        if !negated_clause.reconcilable || negated_clause.wedge {
            continue;
        }

        for entry_clause in entry_clauses {
            if !entry_clause.reconcilable || entry_clause.wedge {
                continue;
            }

            let mut negated_contains_entry = true;
            for (key, entry_possibilities) in entry_clause.possibilities.iter() {
                let Some(negated_possibilities) = negated_clause.possibilities.get(key) else {
                    negated_contains_entry = false;
                    break;
                };

                if negated_possibilities != entry_possibilities {
                    negated_contains_entry = false;
                    break;
                }

                if entry_possibilities.values().any(|assertion| {
                    matches!(assertion, Assertion::InArray(_) | Assertion::NotInArray(_))
                }) {
                    negated_contains_entry = false;
                    break;
                }
            }

            if negated_contains_entry {
                return true;
            }
        }
    }

    false
}

/// Psalm `IfAnalyzer::addConditionallyAssignedVarsToContext`: replay the
/// negated condition against the pre-condition context when the if body always
/// leaves, so conditionally-assigned vars reach the fallthrough/else. Psalm
/// re-analyzes `assert(!expr)` per definitely-evaluated ored expression — an
/// `&&` becomes `!left || !right`, so right-operand assignments run only on
/// the or's conditional path. The pzoom replay applies recorded assignment
/// types instead: definitely for the or-left, combined-with-previous for the
/// conditional or-right, then silently reconciles each expr's if-false
/// assertions (the effect of asserting its negation).
fn add_conditionally_assigned_vars_to_if_fallthrough(
    analyzer: &StatementsAnalyzer<'_>,
    cond: &Expression<'_>,
    replay_context: &mut BlockContext,
    assigned_var_ids: &FxHashSet<VarName>,
    analysis_data: &mut FunctionAnalysisData,
) {
    use crate::expr::binop::or_analyzer::{apply_recorded_assignments, flatten_ored_expressions};

    let mut ored_exprs = Vec::new();
    flatten_ored_expressions(cond, &mut ored_exprs);

    for expr in ored_exprs {
        if let Expression::Binary(binary) = expr.unparenthesized()
            && matches!(
                binary.operator,
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_)
            )
        {
            // assert(!left || !right): !left is definitely evaluated; the
            // right operand's assignments run only when !left is falsy, so
            // their effect combines with the pre-existing type.
            apply_recorded_assignments(binary.lhs, analysis_data, replay_context);
            let pre_right_types: Vec<(VarName, Option<TUnion>)> = assigned_var_ids
                .iter()
                .map(|var_id| (var_id.clone(), replay_context.locals.get(var_id).cloned()))
                .collect();
            apply_recorded_assignments(binary.rhs, analysis_data, replay_context);
            for (var_id, pre_type) in pre_right_types {
                if let Some(pre) = pre_type
                    && let Some(post) = replay_context.locals.get(&var_id)
                    && !crate::context::unions_structurally_equal(&pre, post)
                {
                    let combined = combine_union_types(&pre, post, false);
                    replay_context.locals.insert(var_id, combined);
                }
            }
        } else {
            apply_recorded_assignments(expr, analysis_data, replay_context);
        }

        let assertions = assertion_finder::get_assertions(analyzer, expr, analysis_data);
        if !assertions.if_false.is_empty() {
            let mut changed_var_ids = FxHashSet::default();
            let inside_loop = replay_context.inside_loop;
            reconciler::reconcile_keyed_types(
                &assertions.if_false,
                replay_context,
                &mut changed_var_ids,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                reconciler::EmissionMode::Silent,
                None,
            );
        }
    }
}
