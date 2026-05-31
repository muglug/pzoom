//! Match expression analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::control_flow::r#match::{Match, MatchArm};

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze match expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    match_expr: &Match<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let subject_pos =
        expression_analyzer::analyze(analyzer, match_expr.expression, analysis_data, context);
    let subject_type = analysis_data.get_expr_type(subject_pos).map(|t| (*t).clone());

    let mut result_types = Vec::new();
    let mut matched_conditions = Vec::new();
    let mut has_default_arm = false;

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

    for arm in match_expr.arms.iter() {
        if let MatchArm::Expression(expression_arm) = arm {
            for condition in expression_arm.conditions.iter() {
                let condition_span = condition.span();
                let condition_pos =
                    expression_analyzer::analyze(analyzer, condition, analysis_data, context);
                if let Some(condition_type) = analysis_data.get_expr_type(condition_pos) {
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
            has_default_arm = true;
        }

        let arm_pos =
            expression_analyzer::analyze(analyzer, arm.expression(), analysis_data, context);
        if let Some(arm_type) = analysis_data.get_expr_type(arm_pos) {
            result_types.push((*arm_type).clone());
        }
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

    analysis_data.set_expr_type(pos, result_type);
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

fn collect_match_subject_values(analyzer: &StatementsAnalyzer<'_>, subject_type: &TUnion) -> Vec<TAtomic> {
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
