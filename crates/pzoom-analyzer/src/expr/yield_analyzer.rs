//! Yield expression analyzer.

use mago_span::HasSpan;
use mago_syntax::cst::cst::r#yield::Yield;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

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
    let mut yielded_value_type: Option<TUnion> = None;

    match yield_expr {
        Yield::Value(yield_value) => {
            if let Some(value) = yield_value.value {
                let value_pos =
                    expression_analyzer::analyze(analyzer, value, analysis_data, context);
                let value_type = analysis_data
                    .expr_types
                    .get(&value_pos)
                    .cloned()
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::mixed);
                add_yield_value_dataflow(analyzer, analysis_data, &value_type, pos);
                analysis_data
                    .inferred_yield_types
                    .push((None, value_type.clone()));
                yielded_value_type = Some(value_type);
            }
            // A value-less `yield;` contributes nothing to the inferred yield
            // types (Psalm's ReturnTypeCollector skips it); the function still
            // counts as a generator via the syntactic has-yield flag.
        }
        Yield::Pair(yield_pair) => {
            let key_pos =
                expression_analyzer::analyze(analyzer, yield_pair.key, analysis_data, context);
            let value_pos =
                expression_analyzer::analyze(analyzer, yield_pair.value, analysis_data, context);

            let key_type = analysis_data
                .expr_types
                .get(&key_pos)
                .cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::array_key);
            let value_type = analysis_data
                .expr_types
                .get(&value_pos)
                .cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed);

            add_yield_value_dataflow(analyzer, analysis_data, &value_type, pos);
            analysis_data
                .inferred_yield_types
                .push((Some(key_type), value_type.clone()));
            yielded_value_type = Some(value_type);
        }
        Yield::From(yield_from) => {
            let iterator_pos =
                expression_analyzer::analyze(analyzer, yield_from.iterator, analysis_data, context);
            if let Some(iterator_type) = analysis_data.expr_types.get(&iterator_pos).cloned() {
                let iterator_type = (*iterator_type).clone();
                emit_yield_from_iterator_issue_if_needed(
                    analyzer,
                    &iterator_type,
                    yield_from.iterator.span().start.offset,
                    yield_from.iterator.span().end.offset,
                    analysis_data,
                );

                if let Some((key_type, value_type)) =
                    extract_iterable_key_value(analyzer.codebase, &iterator_type)
                {
                    analysis_data
                        .inferred_yield_types
                        .push((Some(key_type), value_type));
                } else {
                    analysis_data
                        .inferred_yield_types
                        .push((None, TUnion::mixed()));
                }

                // The type of the `yield from` expression itself is whatever
                // the delegated generator returns (its 4th template param), or
                // `null` for arrays — *not* the enclosing function's send type.
                // Mirrors Psalm's YieldFromAnalyzer.
                analysis_data.expr_types.insert(
                    pos,
                    Rc::new(yield_from_expression_type(analyzer, &iterator_type)),
                );
            } else {
                analysis_data
                    .expr_types
                    .insert(pos, Rc::new(TUnion::mixed()));
            }
            return;
        }
    }

    // `@psalm-yield`: yielding an instance of a class with a promised yield
    // type produces that type (resolved through the instance's template
    // params) instead of the generator's send type — Psalm's YieldAnalyzer.
    if let Some(value_type) = &yielded_value_type
        && let Some(promised_type) = promised_yield_type(analyzer, value_type)
    {
        analysis_data.expr_types.insert(pos, Rc::new(promised_type));
        return;
    }

    // Psalm's YieldAnalyzer: the yield expression's type starts as the yielded
    // value's type (never for a bare `yield;`) and is only replaced by the
    // declared `Generator` return type's send param when one exists — a
    // non-void generic send type wins outright, while a bare `Generator`
    // return widens the value type with mixed.
    let mut expression_type = yielded_value_type.unwrap_or_else(TUnion::nothing);

    if let Some(function_info) = analyzer.function_info
        && let Some(return_type) = function_info.get_return_type()
    {
        for atomic in &return_type.types {
            if let TAtomic::TNamedObject {
                name, type_params, ..
            } = atomic
            {
                let class_name = analyzer.interner.lookup(*name);
                if class_name.eq_ignore_ascii_case("Generator") {
                    match type_params {
                        Some(type_params) if type_params.len() >= 3 => {
                            if !type_params[2].is_void() {
                                expression_type = type_params[2].clone();
                            }
                        }
                        _ => {
                            expression_type =
                                combine_union_types(&TUnion::mixed(), &expression_type, false);
                        }
                    }
                }
            }
        }
    }

    analysis_data
        .expr_types
        .insert(pos, Rc::new(expression_type));
}

/// Resolve the promised yield type for a yielded value (Psalm's
/// `@psalm-yield` handling in YieldAnalyzer): for each yielded named-object
/// atomic whose class declares (or inherits) a yield type, localize that type
/// through the instance's extended template params and union the results.
fn promised_yield_type(analyzer: &StatementsAnalyzer<'_>, value_type: &TUnion) -> Option<TUnion> {
    let mut promised: Option<TUnion> = None;

    for atomic in &value_type.types {
        let TAtomic::TNamedObject {
            name, type_params, ..
        } = atomic
        else {
            continue;
        };

        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };
        let Some(yield_type) = &class_info.yield_type else {
            continue;
        };

        // Bind the declaring class's templates: explicit type params on the
        // instance first, then the `@extends` substitutions recorded on the
        // class (b extends a<"test"> binds a's T to "test").
        let mut template_result =
            crate::expr::call::function_call_analyzer::infer_class_template_replacements_from_type_params(
                class_info,
                type_params.as_deref(),
            );
        crate::expr::call::function_call_analyzer::infer_class_template_replacements_from_extended_params(
            &mut template_result,
            class_info,
        );

        let localized = crate::expr::call::function_call_analyzer::replace_templates_in_union(
            yield_type,
            &template_result,
        );

        promised = Some(match promised {
            Some(existing) => combine_union_types(&existing, &localized, false),
            None => localized,
        });
    }

    promised
}

/// Port of Hakana `yield_analyzer`'s function-body dataflow: the yielded
/// value's dataflow terminates in an unlabelled variable-use sink at the
/// `yield` expression (the value is consumed by the generator's caller).
/// (Hakana leaves whole-program taint flows through `yield` as a todo;
/// `yield from` has no Hack equivalent and gets no dataflow here either.)
fn add_yield_value_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    value_type: &TUnion,
    pos: Pos,
) {
    if analysis_data.data_flow_graph.kind != pzoom_code_info::GraphKind::FunctionBody {
        return;
    }

    let return_node = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
        crate::data_flow::make_data_flow_node_position(analyzer, pos),
    );

    for parent_node in &value_type.parent_nodes {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &return_node.id,
            pzoom_code_info::PathKind::Default,
            vec![],
            vec![],
        );
    }
    analysis_data.data_flow_graph.add_node(return_node);
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
                TAtomic::TArray { .. } => {
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
            TAtomic::TArray { .. } | TAtomic::TIterable { .. } => {
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

fn extract_iterable_key_value(
    codebase: &pzoom_code_info::CodebaseInfo,
    iter_type: &TUnion,
) -> Option<(TUnion, TUnion)> {
    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for atomic in &iter_type.types {
        let (atomic_key, atomic_value) = match atomic {
            // A delegated generator/iterator: its key/value are the first two
            // Traversable slots, remapped through @template-extends when the
            // delegate is a Traversable subtype (Psalm's YieldFromAnalyzer).
            TAtomic::TNamedObject {
                name,
                type_params: Some(type_params),
                ..
            } => {
                let mapped =
                    crate::type_comparator::object_type_comparator::get_mapped_generic_type_params(
                        codebase,
                        *name,
                        type_params,
                        pzoom_str::StrId::TRAVERSABLE,
                    )
                    .unwrap_or_else(|| type_params.clone());
                (
                    mapped.first().cloned().unwrap_or_else(TUnion::mixed),
                    mapped.get(1).cloned().unwrap_or_else(TUnion::mixed),
                )
            }
            TAtomic::TIterable {
                key_type,
                value_type,
            } => ((**key_type).clone(), (**value_type).clone()),
            // A generic array/list (no known entries) yields its fallback params
            // directly (old `TArray`/`TList`); `[]` (no params) yields nothing —
            // matching the old generic-array behaviour, with no array-key/mixed
            // defaulting.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } if known_values.is_empty() => match params.as_deref() {
                Some((key_type, value_type)) => (key_type.clone(), value_type.clone()),
                None => (TUnion::nothing(), TUnion::nothing()),
            },
            // A shape (known entries): combine the entries' literal keys/values
            // with the typed fallback `params`, defaulting empties to
            // array-key/mixed (old `TKeyedArray`).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                let mut combined_key = params
                    .as_deref()
                    .map(|(k, _)| k.clone())
                    .unwrap_or_else(TUnion::nothing);
                let mut combined_value = params
                    .as_deref()
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(TUnion::nothing);

                for (key, (_possibly_undefined, value)) in known_values.iter() {
                    let literal_key = match key {
                        pzoom_code_info::ArrayKey::Int(value) => {
                            TUnion::new(TAtomic::TLiteralInt { value: *value })
                        }
                        pzoom_code_info::ArrayKey::String(value)
                        | pzoom_code_info::ArrayKey::ClassString(value) => {
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
