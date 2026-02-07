//! Switch statement analyzer.

use std::collections::BTreeMap;

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::{Access, ClassConstantAccess};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::control_flow::switch::{Switch, SwitchBody, SwitchCase};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::control_analyzer::{self, BreakContext, ControlAction};
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use rustc_hash::FxHashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaseExitType {
    ReturnThrow,
    Continue,
    Break,
    Hybrid,
}

/// Analyze a switch statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    switch: &Switch<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let switch_expr_pos =
        expression_analyzer::analyze(analyzer, switch.expression, analysis_data, context);
    let switch_expr_type = analysis_data
        .get_expr_type(switch_expr_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let class_string_origin = get_switch_class_string_origin(analyzer, switch.expression, context);
    let gettype_origin = get_switch_gettype_origin(analyzer, switch.expression);
    let switch_is_get_class = is_get_class_call(switch.expression);
    let switch_var_id = expression_identifier::get_expression_var_key(switch.expression)
        .and_then(|key| analyzer.interner.find(&key));
    let switch_is_true = is_true_literal(switch.expression);

    let cases = match &switch.body {
        SwitchBody::BraceDelimited(body) => body.cases.as_slice(),
        SwitchBody::ColonDelimited(body) => body.cases.as_slice(),
    };

    let original_context = context.clone();
    let mut has_default = false;
    let mut all_options_returned = true;
    let mut continuing_contexts: Vec<BlockContext> = Vec::new();

    let mut can_track_remaining = union_all_literals(&switch_expr_type);
    if switch_is_true || switch_is_get_class || gettype_origin.is_some() {
        can_track_remaining = false;
    }
    let original_switch_type = switch_expr_type.clone();
    let mut remaining_switch_type = switch_expr_type.clone();

    let case_flow = get_case_exit_flow(cases, analysis_data);

    let mut accumulated_false_assertions: BTreeMap<String, Vec<Assertion>> = BTreeMap::new();
    let mut seen_case_keys: FxHashSet<String> = FxHashSet::default();
    let mut pending_fallthrough_case_types: Vec<TUnion> = Vec::new();

    for (case_index, case) in cases.iter().enumerate() {
        let (case_actions, case_exit_type) = &case_flow[case_index];

        if *case_exit_type != CaseExitType::ReturnThrow {
            all_options_returned = false;
        }

        let mut case_context = original_context.clone();
        let case_type: Option<TUnion>;
        let mut case_span = case.span();
        let mut case_is_default = false;

        match case {
            SwitchCase::Expression(expr_case) => {
                case_span = expr_case.expression.span();
                let mut case_condition_context = original_context.clone();
                let case_expr_pos = expression_analyzer::analyze(
                    analyzer,
                    expr_case.expression,
                    analysis_data,
                    &mut case_condition_context,
                );
                case_context = case_condition_context;
                case_type = analysis_data
                    .get_expr_type(case_expr_pos)
                    .map(|t| (*t).clone());
                let mut effective_case_type = case_type.clone();

                if !expr_case.statements.is_empty() {
                    if let Some(base_case_type) = effective_case_type.take() {
                        let combined_case_type = pending_fallthrough_case_types.iter().fold(
                            base_case_type,
                            |acc, pending_case_type| {
                                combine_union_types(&acc, pending_case_type, false)
                            },
                        );
                        effective_case_type = Some(combined_case_type);
                    }
                }

                if let Some(case_key) = get_case_uniqueness_key(analyzer, expr_case.expression) {
                    if !seen_case_keys.insert(case_key) {
                        emit_issue(
                            analyzer,
                            analysis_data,
                            case_span.start.offset as u32,
                            case_span.end.offset as u32,
                            IssueKind::ParadoxicalCondition,
                            "This switch case is impossible due to previous case matches",
                        );
                    }
                }

                if switch_is_true {
                    apply_assertion_map(
                        analyzer,
                        &accumulated_false_assertions,
                        &mut case_context,
                        analysis_data,
                    );

                    let assertions = assertion_finder::get_assertions(
                        analyzer,
                        expr_case.expression,
                        analysis_data,
                    );
                    apply_assertion_map(
                        analyzer,
                        &assertions.if_true,
                        &mut case_context,
                        analysis_data,
                    );
                    merge_assertion_maps(&mut accumulated_false_assertions, &assertions.if_false);
                } else if let Some(origin_var_id) = gettype_origin {
                    if let Some(case_label) = get_case_string_literal(expr_case.expression) {
                        if let Some(asserted_type) = get_gettype_case_type(&case_label) {
                            narrow_var_to_type(
                                analyzer,
                                &mut case_context,
                                origin_var_id,
                                &asserted_type,
                            );
                        } else {
                            emit_issue(
                                analyzer,
                                analysis_data,
                                case_span.start.offset as u32,
                                case_span.end.offset as u32,
                                IssueKind::UnevaluatedCode,
                                format!("Invalid gettype() case label {}", case_label),
                            );
                        }
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
                                case_context.locals.insert(origin_var_id, narrowed);
                            } else {
                                case_context
                                    .locals
                                    .insert(origin_var_id, TUnion::new(asserted_atomic));
                            }
                        }
                    } else if get_case_string_literal(expr_case.expression).is_some()
                        && union_is_class_string(&switch_expr_type)
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
                } else if let Some(var_id) = switch_var_id {
                    if let (Some(existing_type), Some(case_type)) = (
                        case_context.locals.get(&var_id).cloned(),
                        effective_case_type.as_ref(),
                    ) {
                        if let Some(narrowed) = assertion_reconciler::intersect_union_with_union(
                            &existing_type,
                            case_type,
                        ) {
                            case_context.locals.insert(var_id, narrowed);
                        }
                    }
                }

                if can_track_remaining {
                    if let Some(case_type) = effective_case_type.as_ref() {
                        let matches_original = assertion_reconciler::intersect_union_with_union(
                            &original_switch_type,
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
                        } else if assertion_reconciler::intersect_union_with_union(
                            &remaining_switch_type,
                            case_type,
                        )
                        .is_none()
                        {
                            emit_issue(
                                analyzer,
                                analysis_data,
                                case_span.start.offset as u32,
                                case_span.end.offset as u32,
                                IssueKind::ParadoxicalCondition,
                                "This switch case is impossible due to previous case matches",
                            );
                        } else {
                            remaining_switch_type =
                                subtract_union_types(analyzer, &remaining_switch_type, case_type);
                        }
                    }
                }

                stmt_analyzer::analyze_stmts(
                    analyzer,
                    expr_case.statements.as_slice(),
                    analysis_data,
                    &mut case_context,
                )?;

                if expr_case.statements.is_empty() {
                    if let Some(case_type) = case_type {
                        pending_fallthrough_case_types.push(case_type);
                    }
                } else {
                    pending_fallthrough_case_types.clear();
                }
            }
            SwitchCase::Default(default_case) => {
                has_default = true;
                case_is_default = true;
                pending_fallthrough_case_types.clear();
                apply_assertion_map(
                    analyzer,
                    &accumulated_false_assertions,
                    &mut case_context,
                    analysis_data,
                );

                stmt_analyzer::analyze_stmts(
                    analyzer,
                    default_case.statements.as_slice(),
                    analysis_data,
                    &mut case_context,
                )?;
            }
        }

        if matches!(case_exit_type, CaseExitType::Continue) && !context.inside_loop {
            emit_issue(
                analyzer,
                analysis_data,
                case_span.start.offset as u32,
                case_span.end.offset as u32,
                IssueKind::ContinueOutsideLoop,
                "Continue called when not in loop",
            );
        }

        if case_is_default
            && can_track_remaining
            && remaining_switch_type.is_nothing()
            && !case_actions.contains(&ControlAction::End)
            && !case_actions.contains(&ControlAction::Return)
        {
            emit_issue(
                analyzer,
                analysis_data,
                case_span.start.offset as u32,
                case_span.end.offset as u32,
                IssueKind::ParadoxicalCondition,
                "All possible case statements have been met, default is impossible here",
            );
        }

        if matches!(case_exit_type, CaseExitType::Break | CaseExitType::Hybrid) {
            continuing_contexts.push(case_context);
        }
    }

    let all_options_matched =
        has_default || (can_track_remaining && remaining_switch_type.is_nothing());
    let mut merge_sources = continuing_contexts;
    if !all_options_matched {
        merge_sources.push(original_context);
    }

    if !merge_sources.is_empty() {
        let mut merged_context = merge_sources.remove(0);
        for branch_context in &merge_sources {
            merged_context.merge(branch_context);
        }
        *context = merged_context;
    }

    context.has_returned = all_options_returned && all_options_matched;

    Ok(())
}

fn get_case_exit_flow(
    cases: &[SwitchCase<'_>],
    analysis_data: &FunctionAnalysisData,
) -> Vec<(FxHashSet<ControlAction>, CaseExitType)> {
    let mut case_flow_rev: Vec<(FxHashSet<ControlAction>, CaseExitType)> =
        Vec::with_capacity(cases.len());
    let mut last_case_exit_type = CaseExitType::Break;

    for case in cases.iter().rev() {
        let case_actions = control_analyzer::get_control_actions(
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

fn emit_issue(
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

fn merge_assertion_maps(
    target: &mut BTreeMap<String, Vec<Assertion>>,
    source: &BTreeMap<String, Vec<Assertion>>,
) {
    for (var_name, assertions) in source {
        target
            .entry(var_name.clone())
            .or_default()
            .extend(assertions.iter().cloned());
    }
}

fn apply_assertion_map(
    analyzer: &StatementsAnalyzer<'_>,
    assertions: &BTreeMap<String, Vec<Assertion>>,
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
        false,
        None,
    );
}

fn narrow_var_to_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_id: StrId,
    asserted_type: &TUnion,
) {
    let Some(asserted_atomic) = asserted_type.get_single().cloned() else {
        if let Some(existing_type) = context.locals.get(&var_id).cloned() {
            if let Some(intersection) =
                assertion_reconciler::intersect_union_with_union(&existing_type, asserted_type)
            {
                context.locals.insert(var_id, intersection);
            }
        }
        return;
    };

    if let Some(existing_type) = context.locals.get(&var_id).cloned() {
        let narrowed = assertion_reconciler::intersect_union_with_atomic(
            &existing_type,
            &asserted_atomic,
            analyzer,
        )
        .unwrap_or_else(|| TUnion::new(asserted_atomic.clone()));
        context.locals.insert(var_id, narrowed);
    } else {
        context.locals.insert(var_id, TUnion::new(asserted_atomic));
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

fn is_true_literal(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Literal(Literal::True(_))
    )
}

fn is_get_class_call(expr: &Expression<'_>) -> bool {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return false;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return false;
    };

    function_name.value().eq_ignore_ascii_case("get_class")
}

fn union_all_literals(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(TAtomic::is_literal)
}

fn get_case_uniqueness_key(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<String> {
    if let Some(string_value) = get_case_string_literal(expr) {
        return Some(format!("string:{string_value}"));
    }

    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => {
            int_lit.value.map(|value| format!("int:{value}"))
        }
        Expression::Literal(Literal::Float(float_lit)) => {
            Some(format!("float:{}", float_lit.value))
        }
        Expression::Literal(Literal::True(_)) => Some("bool:true".to_string()),
        Expression::Literal(Literal::False(_)) => Some("bool:false".to_string()),
        Expression::Literal(Literal::Null(_)) => Some("null".to_string()),
        Expression::Access(Access::ClassConstant(ClassConstantAccess {
            class,
            constant: ClassLikeConstantSelector::Identifier(constant),
            ..
        })) if constant.value.eq_ignore_ascii_case("class") => {
            let class_id = resolve_class_expression(analyzer, class)?;
            Some(format!("class:{}", class_id.0))
        }
        _ => None,
    }
}

fn get_case_string_literal(expr: &Expression<'_>) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    string_lit.value.map(|value| value.to_string())
}

fn get_gettype_case_type(case_label: &str) -> Option<TUnion> {
    match case_label.to_ascii_lowercase().as_str() {
        "boolean" => Some(TUnion::new(TAtomic::TBool)),
        "integer" => Some(TUnion::new(TAtomic::TInt)),
        "double" => Some(TUnion::new(TAtomic::TFloat)),
        "string" => Some(TUnion::new(TAtomic::TString)),
        "array" => Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        })),
        "object" => Some(TUnion::new(TAtomic::TObject)),
        "null" => Some(TUnion::new(TAtomic::TNull)),
        "resource" => Some(TUnion::new(TAtomic::TResource)),
        "unknown type" => Some(TUnion::mixed()),
        _ => None,
    }
}

fn union_is_class_string(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
            )
        })
}

fn get_switch_class_string_origin(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            let class_var_id = analyzer.interner.intern(direct.name);
            context.class_string_origins.get(&class_var_id).copied()
        }
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
            Some(analyzer.interner.intern(direct.name))
        }
        _ => None,
    }
}

fn get_switch_gettype_origin(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    if !function_name.value().eq_ignore_ascii_case("gettype") {
        return None;
    }

    let arg = function_call.argument_list.arguments.first()?;
    let Expression::Variable(Variable::Direct(direct)) = arg.value().unparenthesized() else {
        return None;
    };

    Some(analyzer.interner.intern(direct.name))
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

fn resolve_class_expression(
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
