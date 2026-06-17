//! `"count"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
pub(super) struct CountReturnTypeProvider;

impl FunctionReturnTypeProvider for CountReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["count"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        maybe_emit_impure_countable_call(event, analysis_data);
        infer_count_return_type(event.arg_positions, analysis_data)
    }
}

/// Psalm: count() delegates to a Countable's count() method; calling it on
/// an object whose count() is not mutation-free from a pure context is an
/// ImpureFunctionCall.
fn maybe_emit_impure_countable_call(
    event: &FunctionReturnTypeProviderEvent<'_, '_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let analyzer = event.analyzer;
    if !analyzer.function_info.is_some_and(|info| info.is_pure) {
        return;
    }
    let Some(value_pos) = event.arg_positions.first().copied() else {
        return;
    };
    let Some(value_type) = analysis_data.expr_types.get(&value_pos).cloned() else {
        return;
    };

    let count_method_id = pzoom_str::StrId::COUNT;
    let calls_impure_count = value_type.types.iter().any(|atomic| {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            return false;
        };
        analyzer
            .codebase
            .get_class(*name)
            .and_then(|class_info| class_info.methods.get(&count_method_id))
            .is_some_and(|count_info| !count_info.is_mutation_free && !count_info.is_pure)
    });

    if calls_impure_count {
        let (line, col) = analyzer.get_line_column(value_pos.0);
        analysis_data.add_issue(pzoom_code_info::Issue::new(
            pzoom_code_info::IssueKind::ImpureFunctionCall,
            "Cannot call an impure function count from a mutation-free context",
            analyzer.file_path,
            value_pos.0,
            value_pos.1,
            line,
            col,
        ));
    }
}

fn infer_count_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let value_pos = arg_positions.first().copied()?;
    let value_type = analysis_data.expr_types.get(&value_pos).cloned()?;

    let mut saw_array_like = false;
    let mut saw_non_array_like = false;
    let mut saw_non_empty = false;
    let mut exact_count: Option<i64> = None;

    for atomic in &value_type.types {
        match atomic {
            // A generic array/list (former TArray/TNonEmptyArray/TList/
            // TNonEmptyList): empty known_values with a typed fallback.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                is_nonempty,
                ..
            } if known_values.is_empty() => {
                saw_array_like = true;
                if params.1.is_nothing() {
                    // count(array<never, never>) is provably 0 (Psalm's
                    // CountReturnTypeProvider TArray empty arm).
                    exact_count = match exact_count {
                        None => Some(0),
                        Some(0) => Some(0),
                        Some(_) => None,
                    };
                } else {
                    if *is_nonempty {
                        saw_non_empty = true;
                    }
                    exact_count = None;
                }
            }
            // A keyed-array shape (former TKeyedArray), including the empty
            // array `[]` (empty known_values, no typed fallback — counts as 0).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                saw_array_like = true;

                if params.is_some() {
                    exact_count = None;
                } else {
                    let mut fixed_count = 0i64;
                    let mut has_optional = false;

                    for (possibly_undefined, _value) in known_values.values() {
                        if *possibly_undefined {
                            has_optional = true;
                        } else {
                            fixed_count += 1;
                        }
                    }

                    if fixed_count > 0 {
                        saw_non_empty = true;
                    }

                    if has_optional {
                        exact_count = None;
                    } else {
                        exact_count = match exact_count {
                            None => Some(fixed_count),
                            Some(existing) if existing == fixed_count => Some(existing),
                            Some(_) => None,
                        };
                    }
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(info) = fca::extract_array_like_info_from_union(as_type) {
                    saw_array_like = true;
                    if info.is_non_empty {
                        saw_non_empty = true;
                    }
                    exact_count = None;
                }
            }
            _ => {
                // A non-array member (mixed, Countable object) makes the
                // count inexact.
                saw_non_array_like = true;
            }
        }
    }

    if !saw_array_like {
        return None;
    }

    if saw_non_array_like {
        exact_count = None;
    }

    if let Some(count) = exact_count {
        return Some(TUnion::new(TAtomic::TLiteralInt { value: count }));
    }

    if saw_non_empty {
        return Some(TUnion::new(TAtomic::TIntRange {
            min: Some(1),
            max: None,
        }));
    }

    Some(TUnion::new(TAtomic::TIntRange {
        min: Some(0),
        max: None,
    }))
}
