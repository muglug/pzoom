//! Switch-case analysis helpers: per-case exit flow, assertion-map merging/narrowing,
//! case-uniqueness and gettype/class-string case handling. Mirrors Psalm `SwitchCaseAnalyzer`.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::{Access, ClassConstantAccess};
use mago_syntax::ast::ast::binary::{Binary, BinaryOperator};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::control_flow::switch::SwitchCase;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, combine_ored_clauses, negate_formula, simplify_cnf};
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::algebra_analyzer::check_for_paradox;
use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::scope::SwitchScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_else_analyzer::apply_clauses_to_context;
use crate::stmt::scope_analyzer::{self, BreakContext, ControlAction};
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use rustc_hash::FxHashSet;

use super::switch_analyzer::*;

pub(crate) fn get_case_exit_flow(
    cases: &[SwitchCase<'_>],
    analysis_data: &FunctionAnalysisData,
) -> Vec<(FxHashSet<ControlAction>, CaseExitType)> {
    let mut case_flow_rev: Vec<(FxHashSet<ControlAction>, CaseExitType)> =
        Vec::with_capacity(cases.len());
    let mut last_case_exit_type = CaseExitType::Break;

    for case in cases.iter().rev() {
        let case_actions = scope_analyzer::get_control_actions(
            case.statements(),
            analysis_data,
            &[BreakContext::Switch],
            true,
        );

        if !case_actions.contains(&ControlAction::None) {
            let has_end = case_actions.contains(&ControlAction::End)
                || case_actions.contains(&ControlAction::Return);
            let has_switch_break = case_actions.contains(&ControlAction::LeaveSwitch)
                || case_actions.contains(&ControlAction::Break)
                || case_actions.contains(&ControlAction::BreakImmediateLoop);
            let has_continue = case_actions.contains(&ControlAction::Continue);

            if has_end && has_switch_break {
                last_case_exit_type = CaseExitType::Hybrid;
            } else if has_end {
                // Continue can come from nested loops and does not prevent the case from ending.
                last_case_exit_type = CaseExitType::ReturnThrow;
            } else if has_switch_break {
                last_case_exit_type = CaseExitType::Break;
            } else if has_continue {
                last_case_exit_type = CaseExitType::Continue;
            }
        } else if case_actions.len() != 1 {
            last_case_exit_type = CaseExitType::Hybrid;
        }

        case_flow_rev.push((case_actions, last_case_exit_type));
    }

    case_flow_rev.reverse();
    case_flow_rev
}

/// Analyze a single switch case against the switch subject. Mirrors Psalm
/// `SwitchCaseAnalyzer::analyze`: it narrows the case context from the case
/// condition (literal equality, `gettype()`, `Foo::class` and switch(true)
/// assertion chains), tracks the remaining (not-yet-matched) switch type to
/// flag impossible cases, analyzes the case body, and threads fall-through and
/// continuing contexts back through the shared `SwitchScope`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    case: &SwitchCase<'_>,
    switch_condition: &Expression<'_>,
    case_actions: &FxHashSet<ControlAction>,
    case_exit_type: CaseExitType,
    switch_expr_type: &TUnion,
    original_switch_type: &TUnion,
    switch_is_true: bool,
    gettype_origin: Option<(VarName, bool)>,
    class_string_origin: Option<VarName>,
    can_track_remaining: bool,
    inside_loop: bool,
    is_last: bool,
    original_context: &BlockContext,
    scope: &mut SwitchScope,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    if case_exit_type != CaseExitType::ReturnThrow {
        scope.all_options_returned = false;
    }

    let mut case_context = original_context.clone();
    let case_type: Option<TUnion>;
    let mut case_span = case.span();
    let mut case_is_default = false;

    match case {
        SwitchCase::Expression(expr_case) => {
            case_span = expr_case.expression.span();
            let mut case_condition_context = original_context.clone();
            // Mirrors Psalm: the case condition is analyzed as a conditional
            // expression, then `inside_conditional` is restored afterwards.
            let was_inside_conditional = case_condition_context.inside_conditional;
            case_condition_context.inside_conditional = true;
            let case_expr_pos = expression_analyzer::analyze(
                analyzer,
                expr_case.expression,
                analysis_data,
                &mut case_condition_context,
            );
            case_condition_context.inside_conditional = was_inside_conditional;
            case_context = case_condition_context;
            case_type = analysis_data
                .expr_types
                .get(&case_expr_pos)
                .cloned()
                .map(|t| (*t).clone());
            let mut effective_case_type = case_type.clone();

            if !expr_case.statements.is_empty() {
                if let Some(base_case_type) = effective_case_type.take() {
                    let combined_case_type = scope.pending_fallthrough_case_types.iter().fold(
                        base_case_type,
                        |acc, pending_case_type| {
                            combine_union_types(&acc, pending_case_type, false)
                        },
                    );
                    effective_case_type = Some(combined_case_type);
                }
            }

            if switch_is_true {
                apply_assertion_map(
                    analyzer,
                    &scope.accumulated_false_assertions,
                    &mut case_context,
                    analysis_data,
                );

                let assertions =
                    assertion_finder::get_assertions(analyzer, expr_case.expression, analysis_data);
                apply_assertion_map(
                    analyzer,
                    &assertions.if_true,
                    &mut case_context,
                    analysis_data,
                );
                merge_assertion_maps(
                    &mut scope.accumulated_false_assertions,
                    &assertions.if_false,
                );
            } else if let Some((origin_var_id, is_debug_type)) = gettype_origin {
                let case_type = if is_debug_type {
                    // get_debug_type() labels: scalar spellings or class
                    // names (Psalm's TDependentGetDebugType narrowing).
                    get_case_string_literal(expr_case.expression)
                        .and_then(|label| get_debug_type_case_type(analyzer, &label))
                        .or_else(|| {
                            get_case_class_id(analyzer, expr_case.expression).map(|class_id| {
                                TUnion::new(TAtomic::TNamedObject {
                                    name: class_id,
                                    type_params: None,
                                    is_static: false,
                                    remapped_params: false,
                                })
                            })
                        })
                } else if let Some(case_label) = get_case_string_literal(expr_case.expression) {
                    let asserted = get_gettype_case_type(&case_label);
                    if asserted.is_none() {
                        emit_issue(
                            analyzer,
                            analysis_data,
                            case_span.start.offset as u32,
                            case_span.end.offset as u32,
                            IssueKind::UnevaluatedCode,
                            format!("Invalid gettype() case label {}", case_label),
                        );
                    }
                    asserted
                } else {
                    None
                };
                if let Some(asserted_type) = case_type {
                    narrow_var_to_type(analyzer, &mut case_context, origin_var_id, &asserted_type);
                }
            } else if let Some(origin_var_id) = class_string_origin {
                if let Some(case_class_id) = get_case_class_id(analyzer, expr_case.expression) {
                    if analyzer.codebase.get_class(case_class_id).is_none() {
                        emit_issue(
                            analyzer,
                            analysis_data,
                            case_span.start.offset as u32,
                            case_span.end.offset as u32,
                            IssueKind::UndefinedClass,
                            format!(
                                "Class {} is not defined",
                                analyzer.interner.lookup(case_class_id)
                            ),
                        );
                    } else {
                        let asserted_atomic = TAtomic::TNamedObject {
                            name: case_class_id,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        };
                        if let Some(existing_type) =
                            case_context.locals.get(&origin_var_id).cloned()
                        {
                            let narrowed = assertion_reconciler::intersect_union_with_atomic(
                                &existing_type,
                                &asserted_atomic,
                                analyzer,
                            )
                            .unwrap_or_else(|| TUnion::new(asserted_atomic.clone()));
                            case_context.locals.insert(origin_var_id.clone(), narrowed);
                        } else {
                            case_context
                                .locals
                                .insert(origin_var_id.clone(), TUnion::new(asserted_atomic));
                        }
                    }
                } else if get_case_string_literal(expr_case.expression).is_some()
                    && union_is_class_string(switch_expr_type)
                {
                    emit_issue(
                        analyzer,
                        analysis_data,
                        case_span.start.offset as u32,
                        case_span.end.offset as u32,
                        IssueKind::TypeDoesNotContainType,
                        "Switch condition type class-string cannot contain plain string literal",
                    );
                }
            } else {
                // General switch: build the synthetic `switch_cond === case_cond`
                // equality expression and run it through the formula/clause
                // pipeline (Psalm/Hakana's "case equality expression"). This
                // narrows the switch subject inside the case, flags cases made
                // impossible by earlier ones, and accumulates the negation so
                // later cases (and the default) know it did not match.
                apply_case_equality_clauses(
                    analyzer,
                    switch_condition,
                    expr_case.expression,
                    expr_case.statements.is_empty(),
                    is_last,
                    original_context,
                    &mut case_context,
                    scope,
                    analysis_data,
                );
            }

            if can_track_remaining {
                if let Some(case_type) = effective_case_type.as_ref() {
                    let matches_original = assertion_reconciler::intersect_union_with_union(
                        original_switch_type,
                        case_type,
                    );
                    if matches_original.is_none() {
                        emit_issue(
                            analyzer,
                            analysis_data,
                            case_span.start.offset as u32,
                            case_span.end.offset as u32,
                            IssueKind::TypeDoesNotContainType,
                            format!(
                                "Case type {} is not contained in switch type {}",
                                case_type.get_id(Some(analyzer.interner)),
                                original_switch_type.get_id(Some(analyzer.interner))
                            ),
                        );
                    } else {
                        // Track the not-yet-matched portion of the switch subject
                        // so an exhaustive default can be flagged as impossible.
                        // The impossible-case paradox itself is now reported by
                        // `apply_case_equality_clauses` via `check_for_paradox`.
                        scope.remaining_switch_type =
                            subtract_union_types(analyzer, &scope.remaining_switch_type, case_type);
                    }
                }
            }

            // Mirrors Psalm `$case_context->break_types[] = 'switch'`: a `break`
            // in the case body leaves the switch, not an enclosing loop. Popped
            // afterwards so it does not leak into the post-switch merge context.
            // A preceding case that can fall through arrives here too. Vars the
            // fallthrough introduced (absent on the direct entry path) are
            // possibly undefined in this case — Psalm gets this by replaying
            // the previous case's statements under the previous case's
            // condition, so `isset($x)` after a fallthrough assignment keeps
            // its isset semantics.
            if let Some(fallthrough_context) = scope.fallthrough_entry.take() {
                let vars_before_fallthrough: rustc_hash::FxHashSet<VarName> =
                    case_context.locals.keys().cloned().collect();
                case_context.merge(&fallthrough_context);
                for (var_id, var_type) in case_context.locals.iter_mut() {
                    if !vars_before_fallthrough.contains(var_id) {
                        var_type.possibly_undefined = true;
                    }
                }
            }

            case_context.break_types.push(BreakContext::Switch);
            stmt_analyzer::analyze_stmts(
                analyzer,
                expr_case.statements.as_slice(),
                analysis_data,
                &mut case_context,
            )?;
            case_context.break_types.pop();

            if expr_case.statements.is_empty() {
                if let Some(case_type) = case_type {
                    scope.pending_fallthrough_case_types.push(case_type);
                }
            } else {
                scope.pending_fallthrough_case_types.clear();
            }
        }
        SwitchCase::Default(default_case) => {
            scope.has_default = true;
            case_is_default = true;
            scope.pending_fallthrough_case_types.clear();
            apply_assertion_map(
                analyzer,
                &scope.accumulated_false_assertions,
                &mut case_context,
                analysis_data,
            );

            // A preceding case that can fall through arrives here too.
            if let Some(fallthrough_context) = scope.fallthrough_entry.take() {
                case_context.merge(&fallthrough_context);
            }

            case_context.break_types.push(BreakContext::Switch);
            stmt_analyzer::analyze_stmts(
                analyzer,
                default_case.statements.as_slice(),
                analysis_data,
                &mut case_context,
            )?;
            case_context.break_types.pop();
        }
    }

    let case_has_statements = match case {
        SwitchCase::Expression(expr_case) => !expr_case.statements.is_empty(),
        SwitchCase::Default(default_case) => !default_case.statements.is_empty(),
    };
    handle_non_returning_case(
        analyzer,
        analysis_data,
        case_span.start.offset as u32,
        case_span.end.offset as u32,
        case_actions,
        case_is_default,
        case_exit_type,
        can_track_remaining,
        inside_loop,
        is_last,
        case_has_statements,
        case_context,
        scope,
    );

    Ok(())
}

/// Build the synthetic `switch_cond === case_cond` equality expression and run
/// it through the formula/clause pipeline, mirroring the core of Psalm/Hakana
/// `SwitchCaseAnalyzer::analyze` (the *case equality expression*):
///
/// 1. generate the CNF formula of the equality (`get_formula`);
/// 2. enter with the original clauses conjoined with every earlier case's
///    negation (`negated_clauses`), simplified to CNF;
/// 3. flag a case made impossible/redundant by the earlier ones
///    (`check_for_paradox`);
/// 4. narrow the switch subject (and any vars the condition references) from the
///    resulting truths (`apply_clauses_to_context`);
/// 5. negate this case's clauses into `negated_clauses` for the later cases.
#[allow(clippy::too_many_arguments)]
fn apply_case_equality_clauses(
    analyzer: &StatementsAnalyzer<'_>,
    switch_condition: &Expression<'_>,
    case_cond: &Expression<'_>,
    is_empty_body: bool,
    is_last: bool,
    original_context: &BlockContext,
    case_context: &mut BlockContext,
    scope: &mut SwitchScope,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(arena) = analyzer.arena else {
        return;
    };

    // `switch_cond === case_cond` (Psalm `VirtualIdentical`, Hakana `Eqeqeq`).
    let case_equality_expr: &Expression = arena.alloc(Expression::Binary(Binary {
        lhs: switch_condition,
        operator: BinaryOperator::Identical(case_cond.span()),
        rhs: case_cond,
    }));

    let id = (
        case_cond.start_offset() as u32,
        case_cond.end_offset() as u32,
    );

    let this_case_clauses =
        formula_generator::get_formula(id, id, case_equality_expr, analyzer, analysis_data, false)
            .unwrap_or_default();

    // OR-combine with any deferred fall-through cases (`case "a": case "b":`),
    // so the group's body is narrowed to `$x === "a" || $x === "b" || …`.
    let case_clauses = if let Some(leftover) = scope.leftover_case_equality_clauses.take() {
        combine_ored_clauses(leftover, this_case_clauses.clone(), id).unwrap_or(this_case_clauses)
    } else {
        this_case_clauses
    };

    // entry_clauses = original-context clauses ∧ negations of all earlier cases.
    let entry_clauses: Vec<Rc<Clause>> =
        if !scope.negated_clauses.is_empty() && scope.negated_clauses.len() < 50 {
            let mut refs: Vec<&Clause> = original_context
                .clauses
                .iter()
                .map(|c| c.as_ref())
                .collect();
            refs.extend(scope.negated_clauses.iter());
            simplify_cnf(refs).into_iter().map(Rc::new).collect()
        } else {
            original_context.clauses.clone()
        };

    // An empty case that is not the last one falls through: defer its equality
    // (OR-combined above) and process the whole group when the next case with a
    // body — or the final case — is reached.
    if is_empty_body && !is_last {
        scope.leftover_case_equality_clauses = Some(case_clauses);
        case_context.clauses = entry_clauses;
        return;
    }

    if !case_clauses.is_empty() {
        check_for_paradox(analyzer, &entry_clauses, &case_clauses, analysis_data, id);

        let mut combined: Vec<Clause> = entry_clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(case_clauses.iter().cloned());
        case_context.clauses = if combined.len() < 50 {
            simplify_cnf(combined.iter().collect())
                .into_iter()
                .map(Rc::new)
                .collect()
        } else {
            combined.into_iter().map(Rc::new).collect()
        };
    } else {
        case_context.clauses = entry_clauses;
    }

    // Narrow from the case clauses. RedundantCondition reporting is suppressed
    // here, matching Psalm's switch-case reconciliation.
    apply_clauses_to_context(
        analyzer,
        case_context,
        analysis_data,
        Some(id),
        crate::reconciler::EmissionMode::Silent,
        true,
    );

    if !case_clauses.is_empty() {
        let negated = negate_formula(case_clauses.clone()).unwrap_or_else(|_| {
            let not_expr: &Expression = arena.alloc(Expression::UnaryPrefix(UnaryPrefix {
                operator: UnaryPrefixOperator::Not(case_equality_expr.span()),
                operand: case_equality_expr,
            }));
            formula_generator::get_formula(id, id, not_expr, analyzer, analysis_data, false)
                .unwrap_or_default()
        });
        scope.negated_clauses.extend(negated);
    }
}

/// Mirrors Psalm `SwitchCaseAnalyzer::handleNonReturningCase`: for a case that
/// does not return/throw, this flags an impossible `default:` (all cases
/// already matched) and a `continue` used outside a loop, then contributes the
/// case's post-analysis context to the set of branches merged after the switch.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn handle_non_returning_case(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    case_start: u32,
    case_end: u32,
    case_actions: &FxHashSet<ControlAction>,
    case_is_default: bool,
    case_exit_type: CaseExitType,
    can_track_remaining: bool,
    inside_loop: bool,
    is_last: bool,
    case_has_statements: bool,
    case_context: BlockContext,
    scope: &mut SwitchScope,
) {
    if case_is_default
        && can_track_remaining
        && scope.remaining_switch_type.is_nothing()
        && !case_actions.contains(&ControlAction::End)
        && !case_actions.contains(&ControlAction::Return)
        && !crate::issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            case_start,
            "ParadoxicalCondition",
        )
    {
        emit_issue(
            analyzer,
            analysis_data,
            case_start,
            case_end,
            IssueKind::ParadoxicalCondition,
            "All possible case statements have been met, default is impossible here",
        );
    }

    // If we're leaving this block via `continue`, it must be inside a loop;
    // otherwise the branch's context joins the post-switch merge.
    if matches!(case_exit_type, CaseExitType::Continue) {
        if !inside_loop {
            emit_issue(
                analyzer,
                analysis_data,
                case_start,
                case_end,
                IssueKind::ContinueOutsideLoop,
                "Continue called when not in loop",
            );
        }
    } else if !is_last && case_has_statements && case_actions.contains(&ControlAction::None) {
        // The body can fall through into the next case: its end context is
        // that case's (additional) entry, not a post-switch merge source
        // (break-exit paths inside the body were already captured into the
        // per-switch break frame).
        scope.fallthrough_entry = Some(case_context);
    } else if !is_last && !case_has_statements {
        // An empty case (`case "a":` falling into `case "b":`) is part of the
        // next case's group: Psalm merges its statements/condition into that
        // case, so it never contributes its own post-switch context.
    } else if matches!(case_exit_type, CaseExitType::Break | CaseExitType::Hybrid) {
        scope.continuing_contexts.push(case_context);
    }
}

pub(crate) fn emit_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start: u32,
    end: u32,
    kind: IssueKind,
    message: impl Into<String>,
) {
    let (line, col) = analyzer.get_line_column(start);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        start,
        end.max(start + 1),
        line,
        col,
    ));
}

pub(crate) fn merge_assertion_maps(
    target: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    source: &BTreeMap<VarName, Vec<Vec<Assertion>>>,
) {
    for (var_name, groups) in source {
        target
            .entry(var_name.clone())
            .or_default()
            .extend(groups.iter().cloned());
    }
}

fn apply_assertion_map(
    analyzer: &StatementsAnalyzer<'_>,
    assertions: &BTreeMap<VarName, Vec<Vec<Assertion>>>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    if assertions.is_empty() {
        return;
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        assertions,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        crate::reconciler::EmissionMode::Silent,
        None,
    );
}

fn narrow_var_to_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_id: VarName,
    asserted_type: &TUnion,
) {
    let Some(asserted_atomic) = asserted_type.get_single().cloned() else {
        if let Some(existing_type) = context.locals.get(var_id.as_str()).cloned() {
            if let Some(intersection) =
                assertion_reconciler::intersect_union_with_union(&existing_type, asserted_type)
            {
                context.locals.insert(var_id.clone(), intersection);
            }
        }
        return;
    };

    if let Some(existing_type) = context.locals.get(var_id.as_str()).cloned() {
        let narrowed = assertion_reconciler::intersect_union_with_atomic(
            &existing_type,
            &asserted_atomic,
            analyzer,
        )
        .unwrap_or_else(|| TUnion::new(asserted_atomic.clone()));
        context.locals.insert(var_id.clone(), narrowed);
    } else {
        context
            .locals
            .insert(var_id.clone(), TUnion::new(asserted_atomic));
    }
}

fn subtract_union_types(
    analyzer: &StatementsAnalyzer<'_>,
    left: &TUnion,
    right: &TUnion,
) -> TUnion {
    let mut remaining = Vec::new();

    for atomic in &left.types {
        let single_atomic_union = TUnion::new(atomic.clone());
        let mut comparison_result = TypeComparisonResult::new();
        let contained = union_type_comparator::is_contained_by(
            analyzer.codebase,
            &single_atomic_union,
            right,
            false,
            false,
            &mut comparison_result,
        );

        if !contained {
            remaining.push(atomic.clone());
        }
    }

    if remaining.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(remaining)
    }
}

pub(crate) fn is_true_literal(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Literal(Literal::True(_))
    )
}

pub(crate) fn is_get_class_call(expr: &Expression<'_>) -> bool {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return false;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return false;
    };

    function_name.value().eq_ignore_ascii_case("get_class")
}

pub(crate) fn union_all_literals(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(TAtomic::is_literal)
}

fn get_case_string_literal(expr: &Expression<'_>) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    string_lit.value.map(|value| value.to_string())
}

/// get_debug_type() label resolution: PHP type spellings, or a class name.
fn get_debug_type_case_type(analyzer: &StatementsAnalyzer<'_>, case_label: &str) -> Option<TUnion> {
    match case_label {
        "string" => Some(TUnion::new(TAtomic::TString)),
        "int" => Some(TUnion::new(TAtomic::TInt)),
        "float" => Some(TUnion::new(TAtomic::TFloat)),
        "bool" => Some(TUnion::new(TAtomic::TBool)),
        "null" => Some(TUnion::null()),
        "array" => Some(TUnion::new(TAtomic::array(
            TUnion::array_key(),
            TUnion::mixed(),
        ))),
        other => {
            let class_id = analyzer.interner.intern(other.trim_start_matches('\\'));
            analyzer.codebase.get_class(class_id).map(|_| {
                TUnion::new(TAtomic::TNamedObject {
                    name: class_id,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                })
            })
        }
    }
}

fn get_gettype_case_type(case_label: &str) -> Option<TUnion> {
    match case_label.to_ascii_lowercase().as_str() {
        "boolean" => Some(TUnion::new(TAtomic::TBool)),
        "integer" => Some(TUnion::new(TAtomic::TInt)),
        "double" => Some(TUnion::new(TAtomic::TFloat)),
        "string" => Some(TUnion::new(TAtomic::TString)),
        "array" => Some(TUnion::new(TAtomic::array(
            TUnion::array_key(),
            TUnion::mixed(),
        ))),
        "object" => Some(TUnion::new(TAtomic::TObject)),
        "null" => Some(TUnion::new(TAtomic::TNull)),
        "resource" => Some(TUnion::new(TAtomic::TResource)),
        "resource (closed)" => Some(TUnion::new(TAtomic::TClosedResource)),
        "unknown type" => Some(TUnion::mixed()),
        _ => None,
    }
}

fn union_is_class_string(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TClassString { .. }
                    | TAtomic::TLiteralClassString { .. }
                    | TAtomic::TDependentGetClass { .. }
            )
        })
}

pub(crate) fn get_switch_class_string_origin(expr: &Expression<'_>) -> Option<VarName> {
    match expr.unparenthesized() {
        Expression::Call(Call::Function(function_call)) => {
            let Expression::Identifier(function_name) = function_call.function.unparenthesized()
            else {
                return None;
            };
            if !function_name.value().eq_ignore_ascii_case("get_class") {
                return None;
            }
            let arg = function_call.argument_list.arguments.first()?;
            let Expression::Variable(Variable::Direct(direct)) = arg.value().unparenthesized()
            else {
                return None;
            };
            Some(VarName::new(direct.name))
        }
        _ => None,
    }
}

/// The dependent variable of a switch subject typed as `get_class($x)`
/// (`TDependentGetClass`), used to drive class-string case narrowing on `$x`.
pub(crate) fn switch_dependent_class_var(switch_expr_type: &TUnion) -> Option<VarName> {
    match switch_expr_type.get_single() {
        Some(TAtomic::TDependentGetClass { var_id, .. }) => Some(var_id.clone()),
        _ => None,
    }
}

/// The dependent variable of a switch subject typed as `gettype($x)`
/// (`TDependentGetType`), used to drive gettype case narrowing on `$x`.
pub(crate) fn switch_dependent_type_var(switch_expr_type: &TUnion) -> Option<VarName> {
    match switch_expr_type.get_single() {
        Some(TAtomic::TDependentGetType { var_id }) => Some(var_id.clone()),
        _ => None,
    }
}

pub(crate) fn get_switch_gettype_origin(expr: &Expression<'_>) -> Option<(VarName, bool)> {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    let is_debug_type = function_name.value().eq_ignore_ascii_case("get_debug_type");
    if !is_debug_type && !function_name.value().eq_ignore_ascii_case("gettype") {
        return None;
    }

    let arg = function_call.argument_list.arguments.first()?;
    let Expression::Variable(Variable::Direct(direct)) = arg.value().unparenthesized() else {
        return None;
    };

    Some((VarName::new(direct.name), is_debug_type))
}

fn get_case_class_id(analyzer: &StatementsAnalyzer<'_>, expr: &Expression<'_>) -> Option<StrId> {
    let Expression::Access(Access::ClassConstant(ClassConstantAccess {
        class,
        constant: ClassLikeConstantSelector::Identifier(constant),
        ..
    })) = expr.unparenthesized()
    else {
        return None;
    };

    if !constant.value.eq_ignore_ascii_case("class") {
        return None;
    }

    resolve_class_expression(analyzer, class)
}

pub(crate) fn resolve_class_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Identifier(id) => analyzer
            .get_resolved_name(id.start_offset() as u32)
            .or_else(|| Some(analyzer.interner.intern(id.value()))),
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }
}
