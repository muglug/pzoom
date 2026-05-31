//! String concatenation (.) operator analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{
    Issue, IssueKind, TAtomic, TUnion, t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a string concatenation expression (.).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, context);
    let right_pos = expression_analyzer::analyze(analyzer, right, analysis_data, context);

    let left_type = analysis_data.get_expr_type(left_pos);
    let right_type = analysis_data.get_expr_type(right_pos);

    analysis_data.set_expr_type(
        pos,
        infer_concat_type(analyzer, left_type.as_deref(), right_type.as_deref()),
    );
}

#[derive(Clone, Copy)]
struct ConcatAtomicInfo {
    lowercase: bool,
    non_empty: bool,
    truthy: bool,
    numericish: bool,
    non_negative_int: bool,
}

#[derive(Clone, Copy)]
struct ConcatUnionInfo {
    all_castable: bool,
    all_lowercase: bool,
    all_non_empty: bool,
    any_truthy: bool,
    all_numericish: bool,
    all_non_negative_int: bool,
}

pub(crate) fn infer_concat_type(
    analyzer: &StatementsAnalyzer<'_>,
    left: Option<&TUnion>,
    right: Option<&TUnion>,
) -> TUnion {
    let (Some(left), Some(right)) = (left, right) else {
        return TUnion::string();
    };

    if let (Some(left_literal), Some(right_literal)) = (
        get_single_literal_concat_string_value(left),
        get_single_literal_concat_string_value(right),
    ) {
        return TUnion::new(TAtomic::TLiteralString {
            value: format!("{}{}", left_literal, right_literal),
        });
    }

    let left_info = get_concat_union_info(analyzer, left);
    let right_info = get_concat_union_info(analyzer, right);

    if left_info.all_numericish && right_info.all_non_negative_int {
        return TUnion::new(TAtomic::TNumericString);
    }

    let all_lowercase = left_info.all_lowercase && right_info.all_lowercase;
    let has_non_empty = left_info.all_non_empty || right_info.all_non_empty;

    if all_lowercase && has_non_empty {
        return TUnion::new(TAtomic::TNonEmptyLowercaseString);
    }

    if all_lowercase {
        return TUnion::new(TAtomic::TLowercaseString);
    }

    if left_info.any_truthy || right_info.any_truthy {
        return TUnion::new(TAtomic::TTruthyString);
    }

    if has_non_empty {
        return TUnion::new(TAtomic::TNonEmptyString);
    }

    TUnion::string()
}

pub(crate) fn emit_concat_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    union: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(union) = union else {
        return;
    };
    if union.is_mixed() {
        return;
    }

    if union_has_explicit_null(union) && !union.ignore_nullable_issues {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullOperand,
            format!(
                "Cannot concatenate with possibly null type {}",
                union.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let mut has_valid = false;
    let mut has_invalid = false;

    for atomic in &union.types {
        if matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing) {
            continue;
        }

        if get_concat_atomic_info(analyzer, atomic).is_some() {
            has_valid = true;
        } else {
            has_invalid = true;
        }
    }

    if !has_invalid {
        return;
    }

    let kind = if has_valid {
        IssueKind::PossiblyInvalidOperand
    } else {
        IssueKind::InvalidOperand
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        format!(
            "Cannot concatenate with type {}",
            union.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn union_has_explicit_null(t: &TUnion) -> bool {
    t.types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNull))
}

fn get_single_literal_concat_string_value(union: &TUnion) -> Option<String> {
    let atomic = union.get_single()?;
    match atomic {
        TAtomic::TLiteralString { value } => {
            if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                None
            } else {
                Some(value.clone())
            }
        }
        TAtomic::TLiteralInt { value } => Some(value.to_string()),
        TAtomic::TLiteralFloat { value } => Some(value.to_string()),
        TAtomic::TTrue => Some("1".to_string()),
        TAtomic::TFalse | TAtomic::TNull => Some(String::new()),
        _ => None,
    }
}

fn get_concat_union_info(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> ConcatUnionInfo {
    let mut all_castable = !union.types.is_empty();
    let mut all_lowercase = !union.types.is_empty();
    let mut all_non_empty = !union.types.is_empty();
    let mut any_truthy = false;
    let mut all_numericish = !union.types.is_empty();
    let mut all_non_negative_int = !union.types.is_empty();

    for atomic in &union.types {
        let Some(info) = get_concat_atomic_info(analyzer, atomic) else {
            all_castable = false;
            all_lowercase = false;
            all_non_empty = false;
            all_numericish = false;
            all_non_negative_int = false;
            continue;
        };

        all_lowercase &= info.lowercase;
        all_non_empty &= info.non_empty;
        any_truthy |= info.truthy;
        all_numericish &= info.numericish;
        all_non_negative_int &= info.non_negative_int;
    }

    ConcatUnionInfo {
        all_castable,
        all_lowercase,
        all_non_empty,
        any_truthy,
        all_numericish,
        all_non_negative_int,
    }
}

fn get_concat_atomic_info(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<ConcatAtomicInfo> {
    match atomic {
        TAtomic::TLiteralString { value } => Some(ConcatAtomicInfo {
            lowercase: value != NON_SPECIFIC_LITERAL_STRING_VALUE
                && value.eq(&value.to_ascii_lowercase()),
            non_empty: value != NON_SPECIFIC_LITERAL_STRING_VALUE && !value.is_empty(),
            truthy: value != NON_SPECIFIC_LITERAL_STRING_VALUE && !value.is_empty() && value != "0",
            numericish: value != NON_SPECIFIC_LITERAL_STRING_VALUE && value.parse::<f64>().is_ok(),
            non_negative_int: value != NON_SPECIFIC_LITERAL_STRING_VALUE
                && value.parse::<i64>().is_ok_and(|v| v >= 0),
        }),
        TAtomic::TLiteralClassString { .. } => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: true,
            truthy: true,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TString => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: false,
            truthy: false,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TNonEmptyString => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: true,
            truthy: false,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TLowercaseString => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: false,
            truthy: false,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TNonEmptyLowercaseString => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: false,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TTruthyString => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: true,
            truthy: true,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TNumericString => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: false,
            numericish: true,
            non_negative_int: false,
        }),
        TAtomic::TNonEmptyNumericString => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: false,
            numericish: true,
            non_negative_int: false,
        }),
        TAtomic::TInt => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: false,
            numericish: true,
            non_negative_int: false,
        }),
        TAtomic::TIntRange { min, max } => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            // A range that excludes 0 (e.g. `positive-int`/`negative-int`) is truthy.
            truthy: min.is_some_and(|v| v > 0) || max.is_some_and(|v| v < 0),
            numericish: true,
            non_negative_int: min.is_some_and(|v| v >= 0),
        }),
        TAtomic::TLiteralInt { value } => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: *value != 0,
            numericish: true,
            non_negative_int: *value >= 0,
        }),
        TAtomic::TFloat | TAtomic::TNumeric => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: false,
            numericish: true,
            non_negative_int: false,
        }),
        TAtomic::TLiteralFloat { value } => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: *value != 0.0,
            numericish: true,
            non_negative_int: false,
        }),
        TAtomic::TTrue => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: true,
            truthy: true,
            numericish: true,
            non_negative_int: true,
        }),
        TAtomic::TFalse | TAtomic::TNull => Some(ConcatAtomicInfo {
            lowercase: true,
            non_empty: false,
            truthy: false,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TClassString { .. } => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: true,
            truthy: true,
            numericish: false,
            non_negative_int: false,
        }),
        TAtomic::TTemplateParam { as_type, .. } => {
            // A template parameter with a mixed bound concatenates like `mixed`
            // (Psalm treats this as a mixed operand, not an InvalidOperand).
            if as_type.is_mixed() {
                return Some(ConcatAtomicInfo {
                    lowercase: false,
                    non_empty: false,
                    truthy: false,
                    numericish: false,
                    non_negative_int: false,
                });
            }

            let nested_info = get_concat_union_info(analyzer, as_type);
            if !nested_info.all_castable {
                return None;
            }

            Some(ConcatAtomicInfo {
                lowercase: nested_info.all_lowercase,
                non_empty: nested_info.all_non_empty,
                truthy: nested_info.any_truthy,
                numericish: nested_info.all_numericish,
                non_negative_int: nested_info.all_non_negative_int,
            })
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_concat_atomic_info(analyzer, as_type),
        TAtomic::TNamedObject { name, .. } => {
            let class_info = analyzer.codebase.get_class(*name)?;
            let to_string = class_info.methods.get(&StrId::TO_STRING)?;
            let return_type = to_string.get_return_type()?;
            let nested_info = get_concat_union_info(analyzer, return_type);

            if !nested_info.all_castable {
                return None;
            }

            Some(ConcatAtomicInfo {
                lowercase: nested_info.all_lowercase,
                non_empty: nested_info.all_non_empty,
                truthy: nested_info.any_truthy,
                numericish: nested_info.all_numericish,
                non_negative_int: nested_info.all_non_negative_int,
            })
        }
        _ => None,
    }
}
