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

    for arm in match_expr.arms.iter() {
        if let MatchArm::Expression(expression_arm) = arm {
            for condition in expression_arm.conditions.iter() {
                let condition_pos =
                    expression_analyzer::analyze(analyzer, condition, analysis_data, context);
                if let Some(condition_type) = analysis_data.get_expr_type(condition_pos) {
                    matched_conditions.extend(extract_matchable_literals(&condition_type));
                }
            }
        } else {
            has_default_arm = true;
        }

        let arm_pos =
            expression_analyzer::analyze(analyzer, arm.expression(), analysis_data, context);
        if let Some(arm_type) = analysis_data.get_expr_type(arm_pos) {
            result_types.push((*arm_type).clone());
        }
    }

    if !has_default_arm && let Some(subject_type) = subject_type {
        let mut remaining_subject_values = collect_match_subject_values(analyzer, &subject_type);

        if !remaining_subject_values.is_empty() {
            remaining_subject_values.retain(|subject_atomic| {
                !matched_conditions
                    .iter()
                    .any(|condition_atomic| match_literals_equal(subject_atomic, condition_atomic))
            });

            if !remaining_subject_values.is_empty() {
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
        }
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
