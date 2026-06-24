//! Match expression analyzer.

use mago_allocator::Arena;
use mago_span::HasSpan;
use mago_syntax::cst::cst::control_flow::r#match::{Match, MatchArm};

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze match expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    match_expr: &Match<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Psalm's MatchAnalyzer runs the whole match with inside_call set, so
    // e.g. closures in arms skip MissingClosureReturnType.
    let was_inside_call = context.inside_call;
    context.inside_call = true;

    let subject_pos =
        expression_analyzer::analyze(analyzer, match_expr.expression, analysis_data, context);
    let subject_type = analysis_data
        .expr_types
        .get(&subject_pos)
        .cloned()
        .map(|t| (*t).clone());

    let mut result_types = Vec::new();
    let mut matched_conditions = Vec::new();
    let mut has_default_arm = false;
    let mut all_arms_leave = true;

    // The finite set of literal/enum-case values the subject can take, if known.
    // Empty means the subject isn't a known finite literal set (e.g. plain `int`).
    let subject_values = subject_type
        .as_ref()
        .map(|subject_type| collect_match_subject_values(analyzer, subject_type))
        .unwrap_or_default();
    let subject_is_finite = !subject_values.is_empty();

    // Track which subject values remain unmatched as we walk the arms in order.
    let mut remaining_subject_values = subject_values.clone();
    // Track literal conditions already seen, to detect duplicate (paradoxical) arms.
    let mut seen_conditions: Vec<TAtomic> = Vec::new();

    // `match (true)` desugars to an if/elseif chain over the arm conditions
    // (Psalm's MatchAnalyzer builds virtual ternaries): each arm's condition is
    // evaluated knowing every earlier condition was false, so an arm made
    // impossible by the preceding narrowing is reported.
    let match_on_true = matches!(
        match_expr.expression.unparenthesized(),
        mago_syntax::cst::cst::expression::Expression::Literal(
            mago_syntax::cst::cst::literal::Literal::True(_)
        )
    );
    let mut running_context = context.clone();
    // Vars narrowed by the accumulated negations of earlier arm conditions —
    // each later arm's body sees those narrowings (Psalm's ternary desugar
    // analyzes each branch in the chain's else-context).
    let mut negation_changed_vars: rustc_hash::FxHashSet<pzoom_code_info::VarName> =
        rustc_hash::FxHashSet::default();

    for arm in match_expr.arms.iter() {
        // Branch-local types for this arm's body: the running context's
        // negation narrowing, plus the arm's own truths (the OR of its
        // conditions). Applied to `context` around the body and restored
        // after — only one arm executes, so its narrowing (and any
        // post-coercion adjustment inside the body) must not leak.
        let mut arm_body_overrides: Vec<(pzoom_code_info::VarName, TUnion)> = Vec::new();

        if let MatchArm::Expression(expression_arm) = arm {
            // The body's truths reconcile against the context BEFORE this
            // arm's own negation is folded into the running context.
            let pre_arm_running = running_context.clone();
            let pre_arm_negation_vars = negation_changed_vars.clone();
            let mut arm_clauses: Option<Vec<pzoom_code_info::algebra::Clause>> = None;
            let mut arm_cond_id: Option<(u32, u32)> = None;

            for condition in expression_arm.conditions.iter() {
                let condition_span = condition.span();
                let condition_pos =
                    expression_analyzer::analyze(analyzer, condition, analysis_data, context);
                {
                    let cond_id = (
                        condition_span.start.offset as u32,
                        condition_span.end.offset as u32,
                    );
                    let this_clauses = build_match_condition_clauses(
                        analyzer,
                        match_on_true,
                        match_expr.expression,
                        condition,
                        cond_id,
                        analysis_data,
                    );
                    check_match_arm_condition(
                        analyzer,
                        &this_clauses,
                        cond_id,
                        &mut running_context,
                        analysis_data,
                        &mut negation_changed_vars,
                        match_on_true,
                    );
                    arm_cond_id.get_or_insert(cond_id);
                    arm_clauses = Some(match arm_clauses.take() {
                        None => this_clauses,
                        Some(previous) => pzoom_code_info::algebra::combine_ored_clauses(
                            previous,
                            this_clauses.clone(),
                            cond_id,
                        )
                        .unwrap_or(this_clauses),
                    });
                }
                if let Some(condition_type) = analysis_data.expr_types.get(&condition_pos).cloned()
                {
                    for literal in extract_matchable_literals(&condition_type) {
                        let already_seen = seen_conditions
                            .iter()
                            .any(|seen| match_literals_equal(&literal, seen));

                        if already_seen {
                            // A duplicate condition value can never be reached: the
                            // earlier identical arm already handles it.
                            emit_match_arm_issue(
                                analyzer,
                                analysis_data,
                                IssueKind::ParadoxicalCondition,
                                format!(
                                    "This match condition can never be matched, as {} is handled by an earlier arm",
                                    literal.get_id(Some(analyzer.interner))
                                ),
                                condition_span.start.offset,
                                condition_span.end.offset,
                            );
                        } else if subject_is_finite
                            && !subject_values
                                .iter()
                                .any(|value| match_literals_equal(value, &literal))
                        {
                            // The subject can never equal this literal, so the arm
                            // is impossible.
                            emit_match_arm_issue(
                                analyzer,
                                analysis_data,
                                IssueKind::TypeDoesNotContainType,
                                format!(
                                    "{} is not a possible value of the match subject",
                                    literal.get_id(Some(analyzer.interner))
                                ),
                                condition_span.start.offset,
                                condition_span.end.offset,
                            );
                        }

                        remaining_subject_values
                            .retain(|value| !match_literals_equal(value, &literal));
                        seen_conditions.push(literal.clone());
                        matched_conditions.push(literal);
                    }
                }
            }

            if let (Some(clauses), Some(cond_id)) = (arm_clauses, arm_cond_id) {
                arm_body_overrides = narrow_match_arm_body(
                    analyzer,
                    pre_arm_running,
                    clauses,
                    cond_id,
                    analysis_data,
                    &pre_arm_negation_vars,
                );
            }
        } else {
            // A `default` arm is impossible when every possible subject value has
            // already been handled by the preceding arms.
            if subject_is_finite && remaining_subject_values.is_empty() {
                let default_span = arm.span();
                emit_match_arm_issue(
                    analyzer,
                    analysis_data,
                    IssueKind::TypeDoesNotContainType,
                    "All match conditions have already been met, so the default arm is impossible"
                        .to_string(),
                    default_span.start.offset,
                    default_span.end.offset,
                );
            }
            // The default arm runs when every condition was false: its body
            // sees the accumulated negation narrowing.
            for var_id in &negation_changed_vars {
                if let Some(var_type) = running_context.locals.get(var_id) {
                    arm_body_overrides.push((var_id.clone(), var_type.as_ref().clone()));
                }
            }
            has_default_arm = true;
        }

        // Only one arm executes: a `throw` arm must not mark the enclosing
        // flow as returned (Psalm desugars match into ternaries, where each
        // branch's exit stays branch-local). Track whether EVERY arm leaves —
        // only then can control not continue past the match.
        let was_has_returned = context.has_returned;
        context.has_returned = false;
        let saved_locals: Vec<(pzoom_code_info::VarName, Option<TUnion>)> = arm_body_overrides
            .iter()
            .map(|(var_id, _)| (var_id.clone(), context.locals.get(var_id).map(|__t| (**__t).clone())))
            .collect();
        for (var_id, var_type) in arm_body_overrides {
            context.locals.insert(var_id, var_type);
        }
        let arm_pos =
            expression_analyzer::analyze(analyzer, arm.expression(), analysis_data, context);
        for (var_id, saved) in saved_locals {
            match saved {
                Some(var_type) => {
                    context.locals.insert(var_id, var_type);
                }
                None => {
                    context.locals.remove(&var_id);
                }
            }
        }
        if !context.has_returned {
            all_arms_leave = false;
        }
        context.has_returned = was_has_returned;
        if let Some(arm_type) = analysis_data.expr_types.get(&arm_pos).cloned() {
            result_types.push((*arm_type).clone());
        }
    }

    if all_arms_leave && !match_expr.arms.is_empty() {
        context.has_returned = true;
    }

    if !has_default_arm && subject_is_finite && !remaining_subject_values.is_empty() {
        let remaining_type = TUnion::from_types(remaining_subject_values);
        let subject_span = match_expr.expression.span();
        let (line, col) = analyzer.get_line_column(subject_span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::UnhandledMatchCondition,
            format!(
                "This match expression is not exhaustive - consider values {}",
                remaining_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            subject_span.start.offset,
            subject_span.end.offset,
            line,
            col,
        ));
    } else if !has_default_arm && match_expr.arms.is_empty() {
        // An arm-less match handles nothing at all, finite subject or not.
        let subject_span = match_expr.expression.span();
        let (line, col) = analyzer.get_line_column(subject_span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::UnhandledMatchCondition,
            "This match expression does not handle any value",
            analyzer.file_path,
            subject_span.start.offset,
            subject_span.end.offset,
            line,
            col,
        ));
    }

    let result_type = if result_types.is_empty() {
        TUnion::nothing()
    } else {
        let mut combined = result_types.remove(0);
        for t in result_types {
            combined = combine_union_types(&combined, &t, false);
        }
        combined
    };

    context.inside_call = was_inside_call;
    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}

fn emit_match_arm_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    kind: IssueKind,
    message: String,
    start_offset: u32,
    end_offset: u32,
) {
    let (line, col) = analyzer.get_line_column(start_offset);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        start_offset,
        end_offset,
        line,
        col,
    ));
}

fn extract_matchable_literals(t_union: &TUnion) -> Vec<TAtomic> {
    t_union
        .types
        .iter()
        .filter(|atomic| {
            matches!(
                atomic,
                TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TEnumCase { .. }
            )
        })
        .cloned()
        .collect()
}

fn collect_match_subject_values(
    analyzer: &StatementsAnalyzer<'_>,
    subject_type: &TUnion,
) -> Vec<TAtomic> {
    let mut values = Vec::new();

    for atomic in &subject_type.types {
        match atomic {
            TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TEnumCase { .. } => values.push(atomic.clone()),
            TAtomic::TNamedObject { name, .. } | TAtomic::TEnum { name } => {
                let Some(enum_info) = analyzer.codebase.get_class(*name) else {
                    return Vec::new();
                };

                if enum_info.kind != pzoom_code_info::class_like_info::ClassLikeKind::Enum {
                    return Vec::new();
                }

                for constant in enum_info.constants.values() {
                    if let Some(enum_case_atomic @ TAtomic::TEnumCase { .. }) =
                        constant.constant_type.get_single()
                        && !values.contains(enum_case_atomic)
                    {
                        values.push(enum_case_atomic.clone());
                    }
                }
            }
            _ => return Vec::new(),
        }
    }

    values
}

fn match_literals_equal(subject: &TAtomic, condition: &TAtomic) -> bool {
    match (subject, condition) {
        (
            TAtomic::TLiteralInt {
                value: subject_value,
            },
            TAtomic::TLiteralInt {
                value: condition_value,
            },
        ) => subject_value == condition_value,
        (
            TAtomic::TLiteralString {
                value: subject_value,
            },
            TAtomic::TLiteralString {
                value: condition_value,
            },
        ) => subject_value == condition_value,
        (
            TAtomic::TLiteralFloat {
                value: subject_value,
            },
            TAtomic::TLiteralFloat {
                value: condition_value,
            },
        ) => (subject_value - condition_value).abs() < f64::EPSILON,
        (
            TAtomic::TEnumCase {
                enum_name: subject_enum,
                case_name: subject_case,
            },
            TAtomic::TEnumCase {
                enum_name: condition_enum,
                case_name: condition_case,
            },
        ) => subject_enum == condition_enum && subject_case == condition_case,
        _ => false,
    }
}

/// The CNF clauses a match arm condition contributes: the condition itself
/// for `match (true)`, otherwise the synthetic `subject === condition`
/// equality (Psalm's MatchAnalyzer builds `VirtualIdentical` ternaries; the
/// switch analyzer's case-equality expression is the same pattern).
fn build_match_condition_clauses(
    analyzer: &StatementsAnalyzer<'_>,
    match_on_true: bool,
    subject: &mago_syntax::cst::cst::expression::Expression<'_>,
    condition: &mago_syntax::cst::cst::expression::Expression<'_>,
    cond_id: (u32, u32),
    analysis_data: &mut FunctionAnalysisData,
) -> Vec<pzoom_code_info::algebra::Clause> {
    if match_on_true {
        return crate::formula_generator::get_formula(
            cond_id,
            cond_id,
            condition,
            analyzer,
            analysis_data,
            false,
        )
        .unwrap_or_default();
    }

    let Some(arena) = analyzer.arena else {
        return Vec::new();
    };

    let equality_expr: &mago_syntax::cst::cst::expression::Expression =
        arena.alloc(mago_syntax::cst::cst::expression::Expression::Binary(
            mago_syntax::cst::cst::binary::Binary {
                lhs: subject,
                operator: mago_syntax::cst::cst::binary::BinaryOperator::Identical(
                    condition.span(),
                ),
                rhs: condition,
            },
        ));

    crate::formula_generator::get_formula(
        cond_id,
        cond_id,
        equality_expr,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default()
}

/// Reconciles an arm's truths into a throwaway clone of `base` (whose types
/// already carry the earlier arms' negations) and returns the narrowed vars
/// for the arm's body: the truths' changes plus the inherited negation
/// narrowing.
fn narrow_match_arm_body(
    analyzer: &StatementsAnalyzer<'_>,
    mut base: BlockContext,
    arm_clauses: Vec<pzoom_code_info::algebra::Clause>,
    cond_id: (u32, u32),
    analysis_data: &mut FunctionAnalysisData,
    inherited_negation_vars: &rustc_hash::FxHashSet<pzoom_code_info::VarName>,
) -> Vec<(pzoom_code_info::VarName, TUnion)> {
    use pzoom_code_info::algebra::get_truths_from_formula;

    let mut changed = rustc_hash::FxHashSet::default();

    if !arm_clauses.is_empty() {
        let mut referenced = rustc_hash::FxHashSet::default();
        let (truths, active_truths) =
            get_truths_from_formula(arm_clauses.iter().collect(), Some(cond_id), &mut referenced);
        if !truths.is_empty() {
            let inside_loop = base.inside_loop;
            crate::reconciler::reconcile_keyed_types(
                &truths,
                &mut base,
                &mut changed,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                crate::reconciler::EmissionMode::Silent,
                Some(&active_truths),
            );
        }
    }

    let mut narrowed_vars = Vec::new();
    for var_id in changed.iter().chain(inherited_negation_vars.iter()) {
        if let Some(var_type) = base.locals.get(var_id)
            && !narrowed_vars.iter().any(|(existing, _)| existing == var_id)
        {
            narrowed_vars.push((var_id.clone(), (**var_type).clone()));
        }
    }
    narrowed_vars
}

/// One step of the match if-chain: reconcile the condition's truths into a
/// throwaway clone of the running context (reporting impossibilities against
/// the narrowing accumulated from earlier arms — `match (true)` only, where
/// the condition is real user-written boolean logic), then fold the
/// condition's negation into the running context for the next arm, extending
/// `negation_changed_vars` with the vars the negation narrowed.
fn check_match_arm_condition(
    analyzer: &StatementsAnalyzer<'_>,
    cond_clauses: &[pzoom_code_info::algebra::Clause],
    cond_id: (u32, u32),
    running_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    negation_changed_vars: &mut rustc_hash::FxHashSet<pzoom_code_info::VarName>,
    emit_truth_issues: bool,
) {
    use pzoom_code_info::algebra::{get_truths_from_formula, negate_formula, simplify_cnf};

    if cond_clauses.is_empty() {
        return;
    }

    // Truth side: reconcile the arm's own truths into a throwaway clone of the
    // running context (whose types already carry the earlier arms' negations)
    // — impossibilities report.
    if emit_truth_issues {
        let mut referenced = rustc_hash::FxHashSet::default();
        let (truths, active_truths) = get_truths_from_formula(
            cond_clauses.iter().collect(),
            Some(cond_id),
            &mut referenced,
        );
        if !truths.is_empty() {
            let mut check_context = running_context.clone();
            let inside_loop = check_context.inside_loop;
            let mut changed = rustc_hash::FxHashSet::default();
            let issues_before = analysis_data.issues.len();
            crate::reconciler::reconcile_keyed_types(
                &truths,
                &mut check_context,
                &mut changed,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                crate::reconciler::EmissionMode::All,
                Some(&active_truths),
            );
            // Psalm reports impossibilities for match arms but not
            // redundancies (a structurally-true arm is normal control flow).
            let arm_issues = analysis_data.issues.split_off(issues_before);
            analysis_data
                .issues
                .extend(arm_issues.into_iter().filter(|issue| {
                    !matches!(
                        issue.kind,
                        IssueKind::RedundantCondition
                            | IssueKind::RedundantConditionGivenDocblockType
                    )
                }));
        }
    }

    // Negation side: the next arm knows this condition was false.
    if let Ok(negated) = negate_formula(cond_clauses.to_vec()) {
        let mut combined: Vec<pzoom_code_info::Clause> = running_context
            .clauses
            .iter()
            .map(|clause| (**clause).clone())
            .collect();
        combined.extend(negated);
        running_context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();

        let mut referenced = rustc_hash::FxHashSet::default();
        let (negated_truths, _) = get_truths_from_formula(
            running_context.clauses.iter().map(|c| c.as_ref()).collect(),
            None,
            &mut referenced,
        );
        if !negated_truths.is_empty() {
            let before = running_context.locals.clone();
            let inside_loop = running_context.inside_loop;
            let mut changed = rustc_hash::FxHashSet::default();
            crate::reconciler::reconcile_keyed_types(
                &negated_truths,
                running_context,
                &mut changed,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                crate::reconciler::EmissionMode::Silent,
                None,
            );
            // A negation that annihilates a var (e.g. `!instanceof stdClass`
            // on stdClass) failed to reconcile — keep the previous type so the
            // next arm's check still reports against something meaningful
            // (Psalm's failed_reconciliation keeps the var usable).
            for (var_id, var_type) in running_context.locals.iter_mut() {
                if var_type.is_nothing()
                    && let Some(previous) = before.get(var_id)
                    && !previous.is_nothing()
                {
                    *var_type = previous.clone();
                }
            }
            negation_changed_vars.extend(changed);
        }
    }
}
