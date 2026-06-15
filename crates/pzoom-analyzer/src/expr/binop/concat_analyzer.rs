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
use std::rc::Rc;

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

    let left_type = analysis_data.expr_types.get(&left_pos).cloned();
    let right_type = analysis_data.expr_types.get(&right_pos).cloned();

    analysis_data.expr_types.insert(pos, Rc::new(infer_concat_type(analyzer, left_type.as_deref(), right_type.as_deref())));
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
    /// Whether the whole operand is non-falsy (every atomic truthy) — Psalm's
    /// `isContainedBy($operand, non-falsy-string)`. A union like `''|'a'` is
    /// NOT all_non_falsy even though one member is truthy.
    all_non_falsy: bool,
    all_numericish: bool,
    all_non_negative_int: bool,
    all_literal: bool,
}

pub(crate) fn infer_concat_type(
    analyzer: &StatementsAnalyzer<'_>,
    left: Option<&TUnion>,
    right: Option<&TUnion>,
) -> TUnion {
    let (Some(left), Some(right)) = (left, right) else {
        return TUnion::string();
    };

    // Psalm enumerates the concatenations when both sides are unions of
    // specific literals and the combination count stays small (< 64).
    let literal_concat_values = |union: &TUnion| -> Option<Vec<String>> {
        union
            .types
            .iter()
            .map(get_literal_concat_string_value)
            .collect()
    };
    if let (Some(left_values), Some(right_values)) =
        (literal_concat_values(left), literal_concat_values(right))
        && !left_values.is_empty()
        && !right_values.is_empty()
        && left_values.len() * right_values.len() < 64
    {
        let mut combined_values = Vec::new();
        let mut all_within_limit = true;
        for left_value in &left_values {
            for right_value in &right_values {
                let combined = format!("{}{}", left_value, right_value);
                // A literal at or over the limit is graded like any
                // non-literal concat instead (Psalm `ConcatAnalyzer`'s
                // `$literal_concat = false`).
                if combined.len() >= analyzer.config.max_string_length {
                    all_within_limit = false;
                    break;
                }
                if !combined_values.contains(&combined) {
                    combined_values.push(combined);
                }
            }
            if !all_within_limit {
                break;
            }
        }
        if all_within_limit {
            return TUnion::from_types(
                combined_values
                    .into_iter()
                    .map(|value| TAtomic::TLiteralString { value })
                    .collect(),
            );
        }
    }

    let left_info = get_concat_union_info(analyzer, left);
    let right_info = get_concat_union_info(analyzer, right);

    if left_info.all_numericish && right_info.all_non_negative_int {
        return TUnion::new(TAtomic::TNumericString);
    }

    let all_lowercase = left_info.all_lowercase && right_info.all_lowercase;
    let has_non_empty = left_info.all_non_empty || right_info.all_non_empty;
    // Psalm's ConcatAnalyzer: the result is non-falsy when both operands are
    // non-empty (the concatenation is >= 2 chars, so never `''` or `'0'`) or
    // when either operand is itself non-falsy. A single non-empty operand only
    // guarantees a non-empty (possibly `'0'`) result.
    let non_falsy = (left_info.all_non_empty && right_info.all_non_empty)
        || left_info.all_non_falsy
        || right_info.all_non_falsy;

    // Psalm's all-literals concat keeps literal-string-ness
    // (TNonspecificLiteralString / its non-empty flavor).
    if left_info.all_literal && right_info.all_literal {
        return TUnion::new(TAtomic::TLiteralString {
            value: NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
        });
    }

    if all_lowercase && has_non_empty {
        return TUnion::new(TAtomic::TNonEmptyLowercaseString);
    }

    if all_lowercase {
        return TUnion::new(TAtomic::TLowercaseString);
    }

    if has_non_empty {
        if non_falsy {
            return TUnion::new(TAtomic::TTruthyString);
        }
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

    // Psalm's ConcatAnalyzer::analyzeOperand ordering: a definitely-null /
    // definitely-false operand reports NullOperand / FalseOperand and stops;
    // null or false inside a wider union reports the Possibly* variants.
    let (line, col) = analyzer.get_line_column(pos.0);
    let mut emit = |kind: IssueKind, message: String| {
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    };
    let union_id = union.get_id(Some(analyzer.interner));
    if union.is_null() {
        emit(
            IssueKind::NullOperand,
            format!("Cannot concatenate with a {union_id}"),
        );
        return;
    }
    if union.is_false() {
        emit(
            IssueKind::FalseOperand,
            format!("Cannot concatenate with a {union_id}"),
        );
        return;
    }
    if union_has_explicit_null(union) && !union.ignore_nullable_issues {
        emit(
            IssueKind::PossiblyNullOperand,
            format!("Cannot concatenate with possibly null type {union_id}"),
        );
    }
    if union.is_falsable() && !union.ignore_falsable_issues {
        emit(
            IssueKind::PossiblyFalseOperand,
            format!("Cannot concatenate with a possibly false {union_id}"),
        );
    }

    let mut has_valid = false;
    let mut has_invalid = false;

    for atomic in &union.types {
        if matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing) {
            continue;
        }

        // Hakana's `can_be_coerced_to_string`: a type variable concatenates,
        // constrained from above to `array-key`.
        if let TAtomic::TTypeVariable { name } = atomic {
            let bound_pos = crate::template::bound_location(analyzer, pos);
            analysis_data
                .type_variable_bounds
                .entry(name.clone())
                .and_modify(|bounds| {
                    bounds.upper_bounds.push(pzoom_code_info::TemplateBound {
                        bound_type: TUnion::array_key(),
                        appearance_depth: 0,
                        arg_offset: None,
                        equality_bound_classlike: None,
                        pos: Some(bound_pos),
                    })
                });
            has_valid = true;
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

fn get_literal_concat_string_value(atomic: &TAtomic) -> Option<String> {
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
    let mut all_non_falsy = !union.types.is_empty();
    let mut all_numericish = !union.types.is_empty();
    let mut all_non_negative_int = !union.types.is_empty();
    // Psalm's Union::allLiterals: literal strings (including the nonspecific
    // `literal-string`), literal ints/floats, and bools.
    let mut all_literal = !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TLiteralString { .. }
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TNonspecificLiteralInt
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TTrue
                    | TAtomic::TFalse
            )
        });

    for atomic in &union.types {
        let Some(info) = get_concat_atomic_info(analyzer, atomic) else {
            all_castable = false;
            all_lowercase = false;
            all_non_empty = false;
            all_non_falsy = false;
            all_numericish = false;
            all_non_negative_int = false;
            all_literal = false;
            continue;
        };

        all_lowercase &= info.lowercase;
        all_non_empty &= info.non_empty;
        all_non_falsy &= info.truthy;
        all_numericish &= info.numericish;
        all_non_negative_int &= info.non_negative_int;
    }

    ConcatUnionInfo {
        all_castable,
        all_lowercase,
        all_non_empty,
        all_non_falsy,
        all_numericish,
        all_non_negative_int,
        all_literal,
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
        TAtomic::TLiteralClassString { .. }
        | TAtomic::TClassString { .. }
        | TAtomic::TDependentGetClass { .. }
        | TAtomic::TDependentGetType { .. } => Some(ConcatAtomicInfo {
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
        TAtomic::TNonspecificLiteralInt => Some(ConcatAtomicInfo {
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
        // array-key is int|string — both concatenate (Psalm allows it).
        TAtomic::TArrayKey => Some(ConcatAtomicInfo {
            lowercase: false,
            non_empty: false,
            truthy: false,
            numericish: false,
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
                truthy: nested_info.all_non_falsy,
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
                truthy: nested_info.all_non_falsy,
                numericish: nested_info.all_numericish,
                non_negative_int: nested_info.all_non_negative_int,
            })
        }
        _ => None,
    }
}
