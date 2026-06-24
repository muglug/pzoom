//! `"filter_var"` / `"filter_input"` return-type providers.
//!
//! A small port of Psalm's `FilterUtils`-based providers: the filter id and
//! options/flags resolve from literal argument types (named arguments
//! included), giving the validated success type, the failure default, and
//! FILTER_FORCE_ARRAY wrapping.

use mago_syntax::cst::cst::argument::Argument;

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};

const FILTER_DEFAULT: i64 = 516;
const FILTER_VALIDATE_INT: i64 = 257;
const FILTER_VALIDATE_BOOLEAN: i64 = 258;
const FILTER_VALIDATE_FLOAT: i64 = 259;
const FILTER_VALIDATE_REGEXP: i64 = 272;
const FILTER_VALIDATE_DOMAIN: i64 = 277;
const FILTER_NULL_ON_FAILURE: i64 = 134_217_728;
const FILTER_FORCE_ARRAY: i64 = 67_108_864;

pub(super) struct FilterVarReturnTypeProvider;

impl FunctionReturnTypeProvider for FilterVarReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["filter_var", "filter_input"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let is_input = event.function_id == "filter_input";
        // filter_var(value, filter, options) / filter_input(type, var_name,
        // filter, options): named arguments resolve by parameter name.
        let (filter_name, filter_position) = if is_input {
            ("filter", 2)
        } else {
            ("filter", 1)
        };
        let options_position = filter_position + 1;

        let filter_pos = named_arg_pos(event, filter_name, filter_position);
        let options_pos = named_arg_pos(event, "options", options_position);
        let unknown_fallback = || {
            is_input.then(|| {
                TUnion::from_types(vec![TAtomic::TString, TAtomic::TFalse, TAtomic::TNull])
            })
        };

        let filter_id = match filter_pos {
            Some(pos) => match analysis_data
                .expr_types
                .get(&pos)
                .cloned()
                .as_deref()
                .and_then(TUnion::get_single)
            {
                Some(TAtomic::TLiteralInt { value }) => *value,
                _ => return unknown_fallback(),
            },
            None => FILTER_DEFAULT,
        };

        // Decompose the options argument: an int is flags; an array carries
        // 'flags' and 'options' => ['default', 'min_range', 'max_range'].
        let mut flags = 0i64;
        let mut default_type: Option<TUnion> = None;
        let mut min_range: Option<i64> = None;
        let mut max_range: Option<i64> = None;
        if let Some(pos) = options_pos {
            let Some(options_type) = analysis_data.expr_types.get(&pos).cloned() else {
                return unknown_fallback();
            };
            match options_type.get_single() {
                Some(TAtomic::TLiteralInt { value }) => flags = *value,
                // An options *shape* (former TKeyedArray): a generic array
                // (non-empty known_values, or the empty `[]`). A generic
                // `array<...>` is not a shape and defers to the fallback.
                Some(TAtomic::TArray {
                    known_values,
                    params,
                    ..
                }) if !known_values.is_empty() || params.is_none() => {
                    if let Some((_, flags_type)) =
                        known_values.get(&pzoom_code_info::ArrayKey::String("flags".to_string()))
                    {
                        match flags_type.get_single() {
                            Some(TAtomic::TLiteralInt { value }) => flags = *value,
                            _ => return None,
                        }
                    }
                    if let Some((_, options_value)) =
                        known_values.get(&pzoom_code_info::ArrayKey::String("options".to_string()))
                        && let Some(TAtomic::TArray {
                            known_values: option_known_values,
                            ..
                        }) = options_value.get_single()
                    {
                        default_type = option_known_values
                            .get(&pzoom_code_info::ArrayKey::String("default".to_string()))
                            .map(|(_, value)| value.clone());
                        min_range = literal_int_property(option_known_values, "min_range");
                        max_range = literal_int_property(option_known_values, "max_range");
                    }
                }
                _ => return unknown_fallback(),
            }
        }

        let null_on_failure = flags & FILTER_NULL_ON_FAILURE != 0;
        let force_array = flags & FILTER_FORCE_ARRAY != 0;

        // Psalm's FilterUtils: FILTER_NULL_ON_FAILURE is redundant when a
        // `default` option is set — the default already replaces the failure
        // value, so the flag does nothing.
        if null_on_failure
            && default_type.is_some()
            && let Some(issue_pos) = options_pos
        {
            let (line, col) = event.analyzer.get_line_column(issue_pos.0);
            analysis_data.add_issue(pzoom_code_info::Issue::new(
                pzoom_code_info::IssueKind::RedundantFlag,
                "Redundant flag FILTER_NULL_ON_FAILURE when using the \"default\" option"
                    .to_string(),
                event.analyzer.file_path,
                issue_pos.0,
                issue_pos.1,
                line,
                col,
            ));
        }

        let success_atomic = match filter_id {
            FILTER_DEFAULT => TAtomic::TString,
            FILTER_VALIDATE_INT => {
                if min_range.is_some() || max_range.is_some() {
                    TAtomic::TIntRange {
                        min: min_range,
                        max: max_range,
                    }
                } else {
                    TAtomic::TInt
                }
            }
            FILTER_VALIDATE_FLOAT => TAtomic::TFloat,
            FILTER_VALIDATE_BOOLEAN => {
                // bool on success; failure folds into bool unless
                // FILTER_NULL_ON_FAILURE/default applies.
                let mut types = vec![TAtomic::TBool];
                if null_on_failure {
                    types.push(TAtomic::TNull);
                } else if let Some(default_type) = &default_type {
                    types.extend(default_type.types.iter().cloned());
                }
                if is_input && !null_on_failure {
                    types.push(TAtomic::TNull);
                }
                return Some(TUnion::from_types(types));
            }
            FILTER_VALIDATE_REGEXP..=FILTER_VALIDATE_DOMAIN => TAtomic::TString,
            _ => return unknown_fallback(),
        };

        let success_atomic = if force_array {
            TAtomic::array(TUnion::array_key(), TUnion::new(success_atomic))
        } else {
            success_atomic
        };

        let mut types = vec![success_atomic];
        if let Some(default_type) = default_type {
            types.extend(default_type.types.iter().cloned());
        } else if null_on_failure {
            types.push(TAtomic::TNull);
        } else {
            types.push(TAtomic::TFalse);
        }
        if is_input && !types.iter().any(|t| matches!(t, TAtomic::TNull)) {
            // The input variable may be absent entirely.
            types.push(TAtomic::TNull);
        }
        Some(TUnion::from_types(types))
    }
}

/// The expression position of a parameter given by name or position,
/// resolving named arguments.
fn named_arg_pos(
    event: &FunctionReturnTypeProviderEvent<'_, '_>,
    param_name: &str,
    param_position: usize,
) -> Option<Pos> {
    let mut positional_index = 0usize;
    for (index, argument) in event.args.iter().enumerate() {
        match argument {
            Argument::Named(named) => {
                if pzoom_syntax::bytes_to_str(named.name.value).eq_ignore_ascii_case(param_name) {
                    return event.arg_positions.get(index).copied();
                }
            }
            Argument::Positional(_) => {
                if positional_index == param_position {
                    return event.arg_positions.get(index).copied();
                }
                positional_index += 1;
            }
        }
    }
    None
}

fn literal_int_property(
    known_values: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, (bool, TUnion)>,
    name: &str,
) -> Option<i64> {
    match known_values
        .get(&pzoom_code_info::ArrayKey::String(name.to_string()))?
        .1
        .get_single()
    {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}
