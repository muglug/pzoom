//! Function call return type fetcher.
//!
//! Mirrors Psalm/Hakana's dedicated function return-type fetcher flow:
//! special-case builtins first, then function storage return type.

use mago_syntax::ast::ast::argument::Argument;
use rustc_hash::FxHashMap;

use pzoom_code_info::{ArrayKey, FunctionLikeInfo, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::function_call_analyzer;

pub(crate) fn fetch(
    analyzer: &StatementsAnalyzer<'_>,
    normalized_name: &str,
    function_info: Option<&FunctionLikeInfo>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    template_defaults: Option<&FxHashMap<StrId, TUnion>>,
    template_replacements: Option<&FxHashMap<StrId, TUnion>>,
) -> Option<TUnion> {
    let normalized_name = normalized_name
        .strip_prefix('\\')
        .unwrap_or(normalized_name);

    if normalized_name.eq_ignore_ascii_case("microtime") {
        return fetch_microtime_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("preg_split") {
        return fetch_preg_split_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("hrtime") {
        return fetch_hrtime_return_type(args, arg_positions, analysis_data);
    }

    if let Some(builtin_return_type) = function_call_analyzer::infer_builtin_return_type(
        analyzer,
        normalized_name,
        args,
        arg_positions,
        analysis_data,
        context,
    ) {
        return Some(builtin_return_type);
    }

    let Some(function_info) = function_info else {
        return None;
    };

    if function_info.return_type.is_none() {
        return None;
    }

    let empty_template_defaults = FxHashMap::default();
    let empty_template_replacements = FxHashMap::default();
    let template_defaults = template_defaults.unwrap_or(&empty_template_defaults);
    let template_replacements = template_replacements.unwrap_or(&empty_template_replacements);

    function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        function_info,
        template_defaults,
        template_replacements,
        args.len(),
    )
    .or_else(|| Some(TUnion::mixed()))
}

fn fetch_microtime_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let arg_pos = *arg_positions.first()?;
    let arg_type = analysis_data.get_expr_type(arg_pos)?;

    if arg_type.is_always_truthy() {
        Some(TUnion::float())
    } else if arg_type.is_always_falsy() {
        Some(TUnion::string())
    } else {
        None
    }
}

fn fetch_preg_split_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let pattern_pos = *arg_positions.first()?;
    let subject_pos = *arg_positions.get(1)?;
    let pattern_type = analysis_data.get_expr_type(pattern_pos)?;
    let subject_type = analysis_data.get_expr_type(subject_pos)?;

    if !union_is_string_like(&pattern_type) || !union_is_string_like(&subject_type) {
        return None;
    }

    let list_atomic = if let Some(flags_pos) = arg_positions.get(3).copied() {
        let flags_type = analysis_data.get_expr_type(flags_pos)?;
        match get_single_literal_int(&flags_type) {
            Some(0 | 2) => TAtomic::TNonEmptyList {
                value_type: Box::new(TUnion::string()),
            },
            Some(1 | 3) => TAtomic::TList {
                value_type: Box::new(TUnion::string()),
            },
            Some(_) => TAtomic::TList {
                value_type: Box::new(TUnion::new(make_offset_capture_shape())),
            },
            None => TAtomic::TNonEmptyList {
                value_type: Box::new(TUnion::string()),
            },
        }
    } else {
        TAtomic::TNonEmptyList {
            value_type: Box::new(TUnion::string()),
        }
    };

    let mut result = TUnion::from_types(vec![list_atomic, TAtomic::TFalse]);
    result.ignore_falsable_issues = true;
    Some(result)
}

fn fetch_hrtime_return_type(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let tuple_type = TAtomic::TNonEmptyList {
        value_type: Box::new(TUnion::int()),
    };

    if args.is_empty() {
        return Some(TUnion::new(tuple_type));
    }

    let first_arg_pos = *arg_positions.first()?;
    let first_arg_type = analysis_data.get_expr_type(first_arg_pos)?;

    match get_single_literal_bool(&first_arg_type) {
        Some(true) => Some(TUnion::int()),
        Some(false) => Some(TUnion::new(tuple_type)),
        None => Some(TUnion::from_types(vec![TAtomic::TInt, tuple_type])),
    }
}

fn union_is_string_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TClassString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
        )
    })
}

fn get_single_literal_int(union: &TUnion) -> Option<i64> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}

fn get_single_literal_bool(union: &TUnion) -> Option<bool> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn make_offset_capture_shape() -> TAtomic {
    let mut properties = FxHashMap::default();
    properties.insert(ArrayKey::Int(0), TUnion::string());
    properties.insert(ArrayKey::Int(1), TUnion::int());

    TAtomic::TKeyedArray {
        properties,
        is_list: true,
        sealed: true,
        fallback_key_type: None,
        fallback_value_type: None,
    }
}
