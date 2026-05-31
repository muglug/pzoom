//! Yield expression analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::r#yield::Yield;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a yield expression.
///
/// yield produces a value from a generator function.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    yield_expr: &Yield<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match yield_expr {
        Yield::Value(yield_value) => {
            if let Some(value) = yield_value.value {
                let value_pos =
                    expression_analyzer::analyze(analyzer, value, analysis_data, context);
                let value_type = analysis_data
                    .get_expr_type(value_pos)
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::mixed);
                analysis_data.add_yield_type(None, value_type);
            } else {
                analysis_data.add_yield_type(None, TUnion::mixed());
            }
        }
        Yield::Pair(yield_pair) => {
            let key_pos =
                expression_analyzer::analyze(analyzer, yield_pair.key, analysis_data, context);
            let value_pos =
                expression_analyzer::analyze(analyzer, yield_pair.value, analysis_data, context);

            let key_type = analysis_data
                .get_expr_type(key_pos)
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::array_key);
            let value_type = analysis_data
                .get_expr_type(value_pos)
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed);

            analysis_data.add_yield_type(Some(key_type), value_type);
        }
        Yield::From(yield_from) => {
            let iterator_pos =
                expression_analyzer::analyze(analyzer, yield_from.iterator, analysis_data, context);
            if let Some(iterator_type) = analysis_data.get_expr_type(iterator_pos) {
                let iterator_type = (*iterator_type).clone();
                emit_yield_from_iterator_issue_if_needed(
                    analyzer,
                    &iterator_type,
                    yield_from.iterator.span().start.offset,
                    yield_from.iterator.span().end.offset,
                    analysis_data,
                );

                if let Some((key_type, value_type)) = extract_iterable_key_value(&iterator_type) {
                    analysis_data.add_yield_type(Some(key_type), value_type);
                } else {
                    analysis_data.add_yield_type(None, TUnion::mixed());
                }

                // The type of the `yield from` expression itself is whatever
                // the delegated generator returns (its 4th template param), or
                // `null` for arrays — *not* the enclosing function's send type.
                // Mirrors Psalm's YieldFromAnalyzer.
                analysis_data.set_expr_type(pos, yield_from_expression_type(analyzer, &iterator_type));
            } else {
                analysis_data.set_expr_type(pos, TUnion::mixed());
            }
            return;
        }
    }

    let mut inferred_send_type: Option<TUnion> = None;

    if let Some(function_info) = analyzer.function_info
        && let Some(return_type) = function_info.get_return_type()
    {
        for atomic in &return_type.types {
            if let TAtomic::TNamedObject { name, type_params , .. } = atomic {
                let class_name = analyzer.interner.lookup(*name);
                if class_name.eq_ignore_ascii_case("Generator") {
                    let send_type = if let Some(type_params) = type_params {
                        if type_params.len() >= 3 {
                            type_params[2].clone()
                        } else {
                            TUnion::mixed()
                        }
                    } else {
                        TUnion::mixed()
                    };

                    inferred_send_type = Some(if let Some(existing) = inferred_send_type {
                        combine_union_types(&existing, &send_type, false)
                    } else {
                        send_type
                    });
                }
            }
        }
    }

    analysis_data.set_expr_type(pos, inferred_send_type.unwrap_or_else(TUnion::mixed));
}

/// Compute the type of a `yield from <iter>` expression, i.e. the value the
/// delegated iterator returns. Mirrors Psalm's `YieldFromAnalyzer`: the first
/// atomic resolves to the `Generator`'s 4th template param (its return type),
/// or `null` for any array; once more than one atomic contributes the result
/// degrades to `mixed`, and an unrecognised single atomic also falls back to
/// `mixed`.
fn yield_from_expression_type(analyzer: &StatementsAnalyzer<'_>, iter_type: &TUnion) -> TUnion {
    let mut yield_from_type: Option<TUnion> = None;

    for atomic in &iter_type.types {
        if yield_from_type.is_none() {
            match atomic {
                TAtomic::TNamedObject {
                    name,
                    type_params: Some(type_params),
                    ..
                } if type_params.len() >= 4
                    && analyzer
                        .interner
                        .lookup(*name)
                        .eq_ignore_ascii_case("Generator") =>
                {
                    yield_from_type = Some(type_params[3].clone());
                }
                TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TKeyedArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. } => {
                    yield_from_type = Some(TUnion::null());
                }
                _ => {}
            }
        } else {
            yield_from_type = Some(TUnion::mixed());
        }
    }

    yield_from_type.unwrap_or_else(TUnion::mixed)
}

fn emit_yield_from_iterator_issue_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    iter_type: &TUnion,
    start: u32,
    end: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut has_valid_iterable = false;
    let mut has_raw_object = false;

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TIterable { .. } => {
                has_valid_iterable = true;
            }
            TAtomic::TNamedObject { name, .. } => {
                if named_object_is_iterable(analyzer, *name) {
                    has_valid_iterable = true;
                } else {
                    has_raw_object = true;
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return,
            _ => {}
        }
    }

    let issue_message = if has_raw_object && has_valid_iterable {
        Some("PossibleRawObjectIteration - Cannot iterate over a possibly non-Traversable object")
    } else if has_raw_object {
        Some("RawObjectIteration - Cannot iterate over a non-Traversable object")
    } else if !has_valid_iterable {
        Some("InvalidIterator - Value yielded from must be iterable")
    } else {
        None
    };

    let Some(message) = issue_message else {
        return;
    };

    let (line, col) = analyzer.get_line_column(start);
    analysis_data.add_issue(Issue::new(
        IssueKind::PossiblyInvalidIterator,
        message,
        analyzer.file_path,
        start,
        end,
        line,
        col,
    ));
}

fn named_object_is_iterable(analyzer: &StatementsAnalyzer<'_>, class_name: StrId) -> bool {
    if matches!(
        class_name,
        StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
    ) {
        return true;
    }

    let mut to_visit = vec![class_name];
    let mut visited = FxHashSet::default();

    while let Some(current) = to_visit.pop() {
        if !visited.insert(current) {
            continue;
        }

        if matches!(
            current,
            StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
        ) {
            return true;
        }

        let Some(class_info) = analyzer.codebase.get_class(current) else {
            continue;
        };

        if let Some(parent) = class_info.parent_class {
            to_visit.push(parent);
        }

        to_visit.extend(class_info.interfaces.iter().copied());
        to_visit.extend(class_info.all_parent_interfaces.iter().copied());
    }

    false
}

fn extract_iterable_key_value(iter_type: &TUnion) -> Option<(TUnion, TUnion)> {
    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for atomic in &iter_type.types {
        let (atomic_key, atomic_value) = match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            }
            | TAtomic::TIterable {
                key_type,
                value_type,
            } => ((**key_type).clone(), (**value_type).clone()),
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                (TUnion::int(), (**value_type).clone())
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                let mut combined_key = fallback_key_type
                    .as_ref()
                    .map(|k| (**k).clone())
                    .unwrap_or_else(TUnion::nothing);
                let mut combined_value = fallback_value_type
                    .as_ref()
                    .map(|v| (**v).clone())
                    .unwrap_or_else(TUnion::nothing);

                for (key, value) in properties {
                    let literal_key = match key {
                        pzoom_code_info::ArrayKey::Int(value) => {
                            TUnion::new(TAtomic::TLiteralInt { value: *value })
                        }
                        pzoom_code_info::ArrayKey::String(value) => {
                            TUnion::new(TAtomic::TLiteralString {
                                value: value.clone(),
                            })
                        }
                    };
                    combined_key = if combined_key.is_nothing() {
                        literal_key
                    } else {
                        combine_union_types(&combined_key, &literal_key, false)
                    };
                    combined_value = if combined_value.is_nothing() {
                        value.clone()
                    } else {
                        combine_union_types(&combined_value, value, false)
                    };
                }

                if combined_key.is_nothing() {
                    combined_key = TUnion::array_key();
                }
                if combined_value.is_nothing() {
                    combined_value = TUnion::mixed();
                }

                (combined_key, combined_value)
            }
            _ => continue,
        };

        key_type = Some(if let Some(existing) = key_type {
            combine_union_types(&existing, &atomic_key, false)
        } else {
            atomic_key
        });
        value_type = Some(if let Some(existing) = value_type {
            combine_union_types(&existing, &atomic_value, false)
        } else {
            atomic_value
        });
    }

    Some((key_type?, value_type?))
}
