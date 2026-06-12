//! Analyzer for array access expressions ($arr[key]).

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::data_flow::path::ArrayDataKind;
use pzoom_code_info::VarName;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{
    ArrayKey, Assertion, ClauseKey, DataFlowNode, GraphKind, PathKind, TAtomic, TUnion,
    WholeProgramKind, combine_union_types,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template::inferred_type_replacer;
use crate::type_comparator::object_type_comparator;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

/// Compute line number from byte offset in source.
fn get_line_number(source: &str, offset: u32) -> u32 {
    let offset = offset as usize;
    source[..offset.min(source.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1
}

/// Creates a path between a variable `$foo` and a fetched value `$foo["a"]`
/// (Hakana `array_fetch_analyzer::add_array_fetch_dataflow`).
pub(crate) fn add_array_fetch_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    array_expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    keyed_array_var_id: Option<String>,
    value_type: &mut TUnion,
    key_type: &mut TUnion,
) {
    // Hakana additionally skips this work in whole-program taint mode when the value
    // type is not taintable; pzoom does not track `has_taintable_value` yet.
    let _ = matches!(
        analysis_data.data_flow_graph.kind,
        GraphKind::WholeProgram(WholeProgramKind::Taint)
    );

    let Some(stmt_var_type) = analysis_data.expr_types.get(&array_expr_pos).cloned() else {
        return;
    };

    if stmt_var_type.parent_nodes.is_empty() {
        return;
    }

    let node_name = if let Some(keyed_array_var_id) = &keyed_array_var_id {
        keyed_array_var_id.clone()
    } else {
        "arrayvalue-fetch".to_string()
    };
    let new_parent_node = DataFlowNode::get_for_local_string(
        node_name,
        make_data_flow_node_position(analyzer, array_expr_pos),
    );
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    let key_type_single = if key_type.types.len() == 1 {
        key_type.types.first()
    } else {
        None
    };

    let dim_value = if let Some(key_type_single) = key_type_single {
        match key_type_single {
            TAtomic::TLiteralString { value } => Some(value.clone()),
            TAtomic::TLiteralInt { value } => Some(value.to_string()),
            _ => None,
        }
    } else {
        None
    };

    let mut array_key_node = None;

    if keyed_array_var_id.is_none() && dim_value.is_none() {
        let fetch_node = DataFlowNode::get_for_local_string(
            "arraykey-fetch".to_string(),
            make_data_flow_node_position(analyzer, array_expr_pos),
        );
        analysis_data.data_flow_graph.add_node(fetch_node.clone());
        array_key_node = Some(fetch_node);
        analysis_data
            .data_flow_graph
            .add_node(new_parent_node.clone());
    }

    for parent_node in stmt_var_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &new_parent_node.id,
            if let Some(dim_value) = dim_value.clone() {
                PathKind::ArrayFetch(ArrayDataKind::ArrayValue, dim_value)
            } else {
                PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)
            },
            vec![],
            vec![],
        );

        if let Some(array_key_node) = array_key_node.clone() {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &array_key_node.id,
                PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey),
                vec![],
                vec![],
            );
        }
    }

    // The offset value is consumed by the fetch: its dataflow parents feed
    // the fetch node, so non-fetch offset expressions (e.g. `$keys[++$key]`)
    // count as used (Psalm reaches the same verdict).
    let key_parent_nodes = key_type.parent_nodes.clone();
    for parent_node in key_parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &new_parent_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    value_type.parent_nodes.push(new_parent_node);

    if let Some(array_key_node) = array_key_node {
        key_type.parent_nodes.push(array_key_node);
    }
}

/// Apply array-fetch dataflow to a result type and record it as the expression type.
#[allow(clippy::too_many_arguments)]
fn set_fetch_type_with_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
    array_expr_pos: Pos,
    keyed_array_var_id: Option<String>,
    mut expr_type: TUnion,
    index_type: Option<&TUnion>,
) {
    let mut key_type = index_type.cloned().unwrap_or_else(TUnion::array_key);
    add_array_fetch_dataflow(
        analyzer,
        array_expr_pos,
        analysis_data,
        keyed_array_var_id,
        &mut expr_type,
        &mut key_type,
    );
    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

/// Analyze an array access expression like $arr[0] or $arr['key'].
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let cached_base_key = expression_identifier::get_expression_var_key(access.array);
    let cached_index_key = get_array_index_key(access.index);

    if let (Some(base_key), Some(index_key)) = (cached_base_key.as_ref(), cached_index_key.as_ref())
    {
        // Taint (whole-program) mode needs the full fetch dataflow — the
        // memo shortcut only registers a use sink.
        if can_reuse_cached_dim_path(base_key, index_key)
            && !matches!(analysis_data.data_flow_graph.kind, GraphKind::WholeProgram(_))
        {
            let full_key = format!("{}[{}]", base_key, index_key);
            let full_key_id = VarName::new(&full_key);
            // The narrowed dim-path type is captured BEFORE the base
            // re-analysis below can re-derive (and widen) it.
            if let Some(existing_type) = context.locals.get(&full_key_id).cloned() {
                let has_asserted_dim = context_asserts_isset_state(context, &full_key) == Some(true);
                let base_has_nullable_or_falsable_access = context
                    .locals
                    .get(base_key.as_str())
                    .is_some_and(|base_type| base_type.is_nullable() || base_type.is_falsable());

                let reusable = has_asserted_dim
                    || !existing_type.possibly_undefined
                    || (context_asserts_isset_state(context, &full_key) == Some(true)
                        && !base_has_nullable_or_falsable_access);

                if reusable {
                    // Psalm analyzes the base expression regardless of the
                    // reuse, so the read counts as a use of the base variable
                    // in the variable-use graph. Full re-analysis here would
                    // re-derive (and disturb) the narrowed path types, so
                    // only the use sink is registered.
                    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
                        && let Some(base_type) = context.locals.get(base_key.as_str())
                        && !base_type.parent_nodes.is_empty()
                    {
                        let base_span = access.array.span();
                        let sink_node = DataFlowNode::get_for_variable_sink(
                            pzoom_code_info::VarId(analyzer.interner.intern(base_key)),
                            make_data_flow_node_position(
                                analyzer,
                                (base_span.start.offset, base_span.end.offset),
                            ),
                        );
                        for parent_node in &base_type.parent_nodes {
                            analysis_data.data_flow_graph.add_path(
                                &parent_node.id,
                                &sink_node.id,
                                PathKind::Default,
                                vec![],
                                vec![],
                            );
                        }
                        analysis_data.data_flow_graph.add_node(sink_node);
                    }

                    analysis_data.expr_types.insert(pos, Rc::new(existing_type));
                    return;
                }
            }
        }
    }

    // Analyze the array expression (general use — Hakana's
    // array_fetch_analyzer marks the whole fetch as consuming).
    let was_inside_general_use_for_base = context.inside_general_use;
    context.inside_general_use = true;
    let array_pos = expression_analyzer::analyze(analyzer, access.array, analysis_data, context);
    context.inside_general_use = was_inside_general_use_for_base;

    // Psalm/Hakana do not suppress undefined-variable checks for dim expressions
    // inside isset()/unset() guards.
    let was_inside_isset = context.inside_isset;
    let was_inside_unset = context.inside_unset;
    if context.inside_isset || context.inside_unset {
        context.inside_isset = false;
        context.inside_unset = false;
    }

    // Analyze the index expression
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let index_pos = expression_analyzer::analyze(analyzer, access.index, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    context.inside_isset = was_inside_isset;
    context.inside_unset = was_inside_unset;

    let array_type = analysis_data
        .expr_types.get(&array_pos).cloned()
        .map(|rc| (*rc).clone())
        // Reading through a type variable resolves it via its accumulated
        // lower bounds (Hakana's instance-call receiver pattern applied to
        // array reads — a concrete element shape is required here).
        .map(|array_type| {
            crate::template::resolve_type_variables_in_union(
                &array_type,
                &analysis_data.type_variable_bounds,
            )
        });
    let index_type = analysis_data
        .expr_types.get(&index_pos).cloned()
        .map(|rc| (*rc).clone());

    let base_has_nullable_or_falsable_access = array_type
        .as_ref()
        .is_some_and(|t| t.is_nullable() || t.is_falsable());

    let mut cached_possibly_undefined_offset = false;

    let keyed_array_var_id = match (cached_base_key.as_ref(), cached_index_key.as_ref()) {
        (Some(base_key), Some(index_key)) => Some(format!("{}[{}]", base_key, index_key)),
        _ => None,
    };

    if let (Some(base_key), Some(index_key)) = (cached_base_key.as_ref(), cached_index_key.as_ref())
    {
        if can_reuse_cached_dim_path(base_key, index_key) {
            let full_key = format!("{}[{}]", base_key, index_key);
            let full_key_id = VarName::new(&full_key);
            if let Some(existing_type) = context.locals.get(&full_key_id) {
                let has_asserted_dim = context_asserts_isset_state(context, &full_key) == Some(true);
                if has_asserted_dim {
                    set_fetch_type_with_dataflow(
                        analyzer,
                        analysis_data,
                        pos,
                        array_pos,
                        keyed_array_var_id.clone(),
                        existing_type.clone(),
                        index_type.as_ref(),
                    );
                    return;
                }
                if existing_type.possibly_undefined {
                    if context_asserts_isset_state(context, &full_key) == Some(true) {
                        if !base_has_nullable_or_falsable_access {
                            set_fetch_type_with_dataflow(
                                analyzer,
                                analysis_data,
                                pos,
                                array_pos,
                                keyed_array_var_id.clone(),
                                existing_type.clone(),
                                index_type.as_ref(),
                            );
                            return;
                        }
                    }
                    cached_possibly_undefined_offset = true;
                } else {
                    set_fetch_type_with_dataflow(
                        analyzer,
                        analysis_data,
                        pos,
                        array_pos,
                        keyed_array_var_id.clone(),
                        existing_type.clone(),
                        index_type.as_ref(),
                    );
                    return;
                }
            }
        }
    }

    // If we don't know the array type, return mixed
    let Some(array_type) = array_type else {
        analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
        return;
    };
    // Psalm's fetched-element provenance is the nested value union's own
    // from_docblock (Union::setFromDocblock marks all levels), so a
    // signature-backed outer union can lose the flag while its elements stay
    // docblock-defined. The combiner below rebuilds the union from atomics,
    // so the value unions' flags are OR-accumulated here.
    let mut result_from_docblock = array_type.from_docblock;
    // Psalm's ArrayFetchAnalyzer recurses into a template parameter's bound
    // for array access (`DATA[$k]` where DATA as array<string, V> fetches V) —
    // except when the offset is itself a template (`DATA[K]` with K as
    // key-of<DATA> stays a deferred indexed access).
    let index_is_templated = index_type.as_ref().is_some_and(|index_type| {
        index_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
    });
    let array_type = if index_is_templated {
        array_type
    } else {
        expand_template_union_bounds(&array_type)
    };

    // Check each type in the union
    let mut result_types: Vec<TAtomic> = Vec::new();
    let mut result_ignore_nullable = false;
    let mut result_ignore_falsable = false;
    let mut has_valid_access = false;
    let mut has_invalid_access = false;
    let mut has_null = false;
    let mut has_mixed_access = false;
    let mut invalid_type_name = String::new();
    let literal_index_keys = index_type
        .as_ref()
        .and_then(|index_type| get_literal_array_keys_from_union(index_type))
        .or_else(|| get_literal_array_key_from_expr(access.index).map(|key| vec![key]))
        .unwrap_or_default();
    let mut has_possibly_undefined_offset = cached_possibly_undefined_offset;
    let mut has_literal_index_hit = false;
    let mut has_literal_index_miss = false;
    let mut expected_offset_type: Option<TUnion> = None;
    let mut atomic_expected_offset_types: Vec<TUnion> = Vec::new();

    for atomic in &array_type.types {
        match atomic {
            // Null access
            TAtomic::TNull => {
                has_null = true;
            }

            // Array types - valid access
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                has_valid_access = true;
                let is_list_atomic =
                    matches!(atomic, TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. });
                if !literal_index_keys.is_empty() {
                    // List keys are non-negative (Psalm types them
                    // int<0, max>): a provably-negative literal offset is a
                    // miss, not a hit.
                    let all_negative = is_list_atomic
                        && literal_index_keys.iter().all(|key| match key {
                            ArrayKey::Int(value) => *value < 0,
                            ArrayKey::String(_) => false,
                        });
                    if all_negative {
                        has_literal_index_miss = true;
                    } else {
                        has_literal_index_hit = true;
                    }
                }
                let key_type = match atomic {
                    TAtomic::TArray { key_type, .. } | TAtomic::TNonEmptyArray { key_type, .. } => {
                        (**key_type).clone()
                    }
                    _ => TUnion::int(),
                };
                merge_expected_offset_type(&mut expected_offset_type, key_type.clone());
                atomic_expected_offset_types.push(key_type);
                result_from_docblock |= value_type.from_docblock;
                for t in &value_type.types {
                    if !result_types.contains(t) {
                        result_types.push(t.clone());
                    }
                }
            }

            // Keyed array - check specific key or use fallback
            TAtomic::TKeyedArray {
                properties,
                is_list,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                has_valid_access = true;
                let keyed_key_type = extract_keyed_array_key_type(
                    properties,
                    *is_list,
                    fallback_key_type.as_deref(),
                );
                merge_expected_offset_type(&mut expected_offset_type, keyed_key_type.clone());
                atomic_expected_offset_types.push(keyed_key_type);
                if !literal_index_keys.is_empty() {
                    for index_key in &literal_index_keys {
                        if let Some(value_type) = properties.get(index_key) {
                            has_literal_index_hit = true;
                            result_ignore_nullable |= value_type.ignore_nullable_issues;
                            result_ignore_falsable |= value_type.ignore_falsable_issues;
                            if value_type.possibly_undefined {
                                if fallback_value_type.is_none() {
                                    has_possibly_undefined_offset = true;
                                }
                                if let Some(fallback) = fallback_value_type {
                                    for t in &fallback.types {
                                        if !result_types.contains(t) {
                                            result_types.push(t.clone());
                                        }
                                    }
                                }
                            }
                            for t in &value_type.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        } else if let Some(fallback) = fallback_value_type {
                            has_literal_index_hit = true;
                            result_ignore_nullable |= fallback.ignore_nullable_issues;
                            result_ignore_falsable |= fallback.ignore_falsable_issues;
                            for t in &fallback.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        } else {
                            has_literal_index_miss = true;
                            has_possibly_undefined_offset = true;
                        }
                    }
                } else {
                    // Non-literal keyed access is handled via key-type checks; Psalm does not
                    // generally emit possibly-undefined-offset here.
                    // A plain-mixed contributor swallows the others (Psalm's
                    // combineUnionTypes collapses X|mixed to mixed), so a
                    // shape's known keys don't leak past a mixed fallback.
                    let has_plain_mixed = properties
                        .values()
                        .chain(fallback_value_type.as_deref())
                        .any(|value| {
                            value.types.iter().any(|t| matches!(t, TAtomic::TMixed))
                        });
                    if has_plain_mixed {
                        if !result_types.contains(&TAtomic::TMixed) {
                            result_types.push(TAtomic::TMixed);
                        }
                    } else {
                        for value in properties.values() {
                            for t in &value.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        }
                        if let Some(fallback) = fallback_value_type {
                            for t in &fallback.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        }
                    }
                }
            }

            // class-string-map: the value type is a function of the class-string
            // key. Mirrors Psalm's handleArrayAccessOnClassStringMap — substitute
            // the placeholder template with each offset's class-string target.
            TAtomic::TClassStringMap {
                param_name,
                as_type,
                value_param,
            } => {
                has_valid_access = true;
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }

                let expected_key = TUnion::new(TAtomic::TClassString {
                    as_type: as_type.clone(),
                });
                merge_expected_offset_type(&mut expected_offset_type, expected_key.clone());
                atomic_expected_offset_types.push(expected_key);

                if let Some(index_type) = index_type.as_ref() {
                    for offset_atomic in &index_type.types {
                        let Some(replacement) =
                            class_string_map_offset_replacement(analyzer, offset_atomic)
                        else {
                            continue;
                        };

                        let mut template_result = pzoom_code_info::TemplateResult::default();
                        crate::template::lower_bounds_insert(
                            &mut template_result,
                            *param_name,
                            pzoom_code_info::GenericParent::TypeDefinition(StrId::CLASS_STRING_MAP),
                            TUnion::new(replacement),
                        );
                        let substituted =
                            inferred_type_replacer::replace(value_param, &template_result);

                        for t in &substituted.types {
                            if !result_types.contains(t) {
                                result_types.push(t.clone());
                            }
                        }
                    }
                }
            }

            // String access - returns string
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString => {
                has_valid_access = true;
                // A literal string bounds its valid offsets (Psalm's
                // handleArrayAccessOnString): '' accepts none, a short
                // literal accepts -len..len-1, anything else any int.
                let mut literal_len: Option<i64> = match atomic {
                    TAtomic::TLiteralString { value } => Some(value.len() as i64),
                    _ => None,
                };
                // A string-offset read yields a single character (Psalm's
                // TSingleLetter): `$s[0][1]` only accepts offset 0.
                if literal_len.is_none()
                    && let mago_syntax::ast::ast::expression::Expression::ArrayAccess(
                        inner_access,
                    ) = access.array.unparenthesized()
                {
                    let inner_span = mago_span::HasSpan::span(inner_access.array);
                    let inner_base_is_string = analysis_data
                        .expr_types.get(&(inner_span.start.offset, inner_span.end.offset)).cloned()
                        .is_some_and(|inner_base| {
                            !inner_base.types.is_empty()
                                && inner_base.types.iter().all(|inner_atomic| {
                                    matches!(
                                        inner_atomic,
                                        TAtomic::TString
                                            | TAtomic::TNonEmptyString
                                            | TAtomic::TLiteralString { .. }
                                            | TAtomic::TNumericString
                                            | TAtomic::TTruthyString
                                            | TAtomic::TLowercaseString
                                            | TAtomic::TNonEmptyLowercaseString
                                    )
                                })
                        });
                    if inner_base_is_string {
                        literal_len = Some(1);
                    }
                }
                let valid_offset_type = match literal_len {
                    // '' has no valid offsets (Psalm: never). The expectation
                    // machinery skips empty unions (empty arrays rely on
                    // that), so report directly.
                    Some(0) => {
                        if !context.inside_isset && !context.inside_unset {
                            let span = access.index.span();
                            let start_line =
                                get_line_number(analyzer.source, span.start.offset);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidArrayOffset,
                                "Cannot access value on an empty string",
                                analyzer.file_path,
                                span.start.offset,
                                span.end.offset,
                                start_line,
                                0,
                            ));
                        }
                        TUnion::int()
                    }
                    Some(len) if len < 10 => TUnion::from_types(
                        (-len..len)
                            .map(|value| TAtomic::TLiteralInt { value })
                            .collect(),
                    ),
                    _ => TUnion::int(),
                };
                // An in-range int literal offset is valid on a string, so a
                // shape miss elsewhere in the union is only *possibly* invalid.
                if !literal_index_keys.is_empty()
                    && literal_index_keys.iter().all(|key| match key {
                        ArrayKey::Int(value) => match literal_len {
                            Some(len) => *value >= -len && *value < len,
                            None => true,
                        },
                        ArrayKey::String(_) => false,
                    })
                {
                    has_literal_index_hit = true;
                }
                merge_expected_offset_type(&mut expected_offset_type, valid_offset_type.clone());
                atomic_expected_offset_types.push(valid_offset_type);
                if !result_types.contains(&TAtomic::TString) {
                    result_types.push(TAtomic::TString);
                }
            }

            // Mixed - could be anything
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                has_valid_access = true;
                has_mixed_access = true;
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }
                result_types.clear();
                result_types.push(TAtomic::TMixed);
            }

            // Invalid array access types. `iterable` may be a Traversable,
            // which has no dim access (Psalm: InvalidArrayAccess).
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TObject
            | TAtomic::TResource
            | TAtomic::TIterable { .. }
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TVoid => {
                has_invalid_access = true;
                invalid_type_name = atomic.get_id(Some(analyzer.interner));
            }
            TAtomic::TNothing => {}

            // An intersection (`Traversable&ArrayAccess<int, string>&...`,
            // e.g. arraylike-object) fetches through its ArrayAccess member.
            TAtomic::TObjectIntersection { types } => {
                let access_member = types.iter().find(|member| {
                    matches!(member, TAtomic::TNamedObject { name, .. }
                        if class_supports_array_access(analyzer, *name))
                });
                if let Some(member @ TAtomic::TNamedObject { name, .. }) = access_member {
                    has_valid_access = true;
                    if !literal_index_keys.is_empty() {
                        has_literal_index_hit = true;
                    }
                    if let Some((key_type, value_type)) =
                        resolve_array_access_method_types(analyzer, member, *name)
                    {
                        merge_expected_offset_type(&mut expected_offset_type, key_type.clone());
                        atomic_expected_offset_types.push(key_type);
                        result_ignore_nullable |= value_type.ignore_nullable_issues;
                        result_ignore_falsable |= value_type.ignore_falsable_issues;
                        for t in &value_type.types {
                            if !result_types.contains(t) {
                                result_types.push(t.clone());
                            }
                        }
                    } else {
                        merge_expected_offset_type(&mut expected_offset_type, TUnion::mixed());
                        atomic_expected_offset_types.push(TUnion::mixed());
                        if !result_types.contains(&TAtomic::TMixed) {
                            result_types.push(TAtomic::TMixed);
                        }
                    }
                } else {
                    has_invalid_access = true;
                    invalid_type_name = atomic.get_id(Some(analyzer.interner));
                }
            }

            TAtomic::TNamedObject { name, .. } => {
                // Psalm hard-codes SimpleXMLElement dim access to
                // SimpleXMLElement|null (handleArrayAccessOnNamedObject).
                if *name == StrId::SIMPLE_XML_ELEMENT
                    || analyzer.codebase.get_class(*name).is_some_and(|class_info| {
                        class_info.all_parent_classes.contains(&StrId::SIMPLE_XML_ELEMENT)
                    })
                {
                    has_valid_access = true;
                    if !literal_index_keys.is_empty() {
                        has_literal_index_hit = true;
                    }
                    merge_expected_offset_type(&mut expected_offset_type, TUnion::array_key());
                    atomic_expected_offset_types.push(TUnion::array_key());
                    for simplexml_member in [
                        TAtomic::TNull,
                        TAtomic::TNamedObject {
                            name: StrId::SIMPLE_XML_ELEMENT,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        },
                    ] {
                        if !result_types.contains(&simplexml_member) {
                            result_types.push(simplexml_member);
                        }
                    }
                } else if class_supports_array_access(analyzer, *name) {
                    has_valid_access = true;
                    if !literal_index_keys.is_empty() {
                        has_literal_index_hit = true;
                    }
                    // Psalm's handleArrayAccessOnObject types the access via
                    // the class's offsetGet, localizing the declaring class's
                    // templates through the receiver's type params
                    // (SplObjectStorage<Node, Union>[$node] is Union).
                    if let Some((key_type, value_type)) =
                        resolve_array_access_method_types(analyzer, atomic, *name)
                    {
                        merge_expected_offset_type(&mut expected_offset_type, key_type.clone());
                        atomic_expected_offset_types.push(key_type);
                        // ArrayAccess::offsetGet is `TValue|null` with
                        // @psalm-ignore-nullable-return: the fetched union
                        // keeps the ignore flags (Psalm's combineUnionTypes
                        // preserves them).
                        result_ignore_nullable |= value_type.ignore_nullable_issues;
                        result_ignore_falsable |= value_type.ignore_falsable_issues;
                        for t in &value_type.types {
                            if !result_types.contains(t) {
                                result_types.push(t.clone());
                            }
                        }
                    } else {
                        merge_expected_offset_type(&mut expected_offset_type, TUnion::mixed());
                        atomic_expected_offset_types.push(TUnion::mixed());
                        if !result_types.contains(&TAtomic::TMixed) {
                            result_types.push(TAtomic::TMixed);
                        }
                    }
                } else {
                    has_invalid_access = true;
                    invalid_type_name = atomic.get_id(Some(analyzer.interner));
                }
            }

            // Other types - treat as potentially valid for now
            _ => {
                has_valid_access = true;
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }
                result_types.push(TAtomic::TMixed);
            }
        }
    }

    // Hakana `handle_array_access_on_mixed`: record mixed-source data and connect the
    // array's parents to a "mixed-var-array-access" node.
    if has_mixed_access && !context.inside_isset {
        for origin in &array_type.parent_nodes {
            analysis_data
                .data_flow_graph
                .add_mixed_data(origin, format!("{}-{}", pos.0, pos.1));
        }

        if !array_type.parent_nodes.is_empty() {
            let new_parent_node = DataFlowNode::get_for_local_string(
                "mixed-var-array-access".to_string(),
                make_data_flow_node_position(analyzer, pos),
            );
            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            for parent_node in array_type.parent_nodes.iter() {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &new_parent_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }
    }

    // Report issues based on what we found
    let span = access.array.span();
    let start_line = get_line_number(analyzer.source, span.start.offset);

    // Pure null access
    if has_null
        && !has_valid_access
        && !has_invalid_access
        && !context.inside_isset
        && !context.inside_conditional
    {
        analysis_data.add_issue(Issue::new(
            IssueKind::NullArrayAccess,
            "Cannot access array offset on null".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        set_fetch_type_with_dataflow(
            analyzer,
            analysis_data,
            pos,
            array_pos,
            keyed_array_var_id,
            TUnion::mixed(),
            index_type.as_ref(),
        );
        return;
    }

    // Possibly null access
    if has_null
        && (has_valid_access || has_invalid_access)
        && !context.inside_isset
        && !context.inside_conditional
    {
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullArrayAccess,
            "Cannot access array offset on possibly null value".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    // Pure invalid access (non-array type)
    if has_invalid_access && !has_valid_access && !context.inside_isset {
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArrayAccess,
            format!("Cannot access array offset on {}", invalid_type_name),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        set_fetch_type_with_dataflow(
            analyzer,
            analysis_data,
            pos,
            array_pos,
            keyed_array_var_id,
            TUnion::mixed(),
            index_type.as_ref(),
        );
        return;
    }

    // Possibly invalid access (union with non-array type). Psalm skips the
    // false-member complaint for internal-function returns marked
    // ignore_falsable_issues (ignoreInternalFunctionFalseReturn).
    let invalid_only_ignored_false =
        invalid_type_name == "false" && array_type.ignore_falsable_issues;
    if has_invalid_access && has_valid_access && !context.inside_isset && !invalid_only_ignored_false {
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyInvalidArrayAccess,
            format!(
                "Cannot access array offset on value that may be {}",
                invalid_type_name
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    if has_mixed_access && !context.inside_isset && !context.inside_unset {
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArrayAccess,
            "Cannot access array value on mixed type".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    let should_emit_invalid_literal_offset = !literal_index_keys.is_empty()
        && has_literal_index_miss
        && !has_literal_index_hit
        && !context.inside_unset;

    let mut emitted_offset_issue = false;
    if !literal_index_keys.is_empty()
        && has_literal_index_miss
        && has_literal_index_hit
        && !has_mixed_access
        && !context.inside_unset
        && !context.inside_isset
    {
        // Some union members accept the offset (e.g. a string half taking int
        // offsets) while an array shape misses it — Psalm's
        // PossiblyInvalidArrayOffset.
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        // Psalm spells literal offsets as `using offset value of '0|1'`
        // (raw values joined with |, in single quotes) and other offsets as
        // `using a {type} offset`.
        let literal_values: Option<Vec<String>> = index_type.as_ref().and_then(|union| {
            union
                .types
                .iter()
                .map(|atomic| match atomic {
                    TAtomic::TLiteralInt { value } => Some(value.to_string()),
                    TAtomic::TLiteralString { value } => Some(value.clone()),
                    _ => None,
                })
                .collect()
        });
        let used_offset = match literal_values {
            Some(values) if !values.is_empty() => {
                format!("using offset value of '{}'", values.join("|"))
            }
            _ => format!(
                "using a {} offset",
                index_type
                    .as_ref()
                    .map(|t| t.get_id(Some(analyzer.interner)))
                    .unwrap_or_else(|| "array-key".to_string())
            ),
        };
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyInvalidArrayOffset,
            format!("Cannot access value {}", used_offset),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        emitted_offset_issue = true;
    }
    if should_emit_invalid_literal_offset {
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        let index_type_id = index_type
            .as_ref()
            .map(|t| t.get_id(Some(analyzer.interner)))
            .unwrap_or_else(|| "array-key".to_string());

        // A definitely-null / possibly-null index gets Psalm's dedicated
        // NullArrayOffset / PossiblyNullArrayOffset kinds.
        let (kind, message) = match index_type.as_ref() {
            Some(union) if union.is_null() => (
                IssueKind::NullArrayOffset,
                "Cannot access value using null offset".to_string(),
            ),
            Some(union) if union.types.iter().any(|atomic| matches!(atomic, TAtomic::TNull)) => (
                IssueKind::PossiblyNullArrayOffset,
                format!("Cannot access value using possibly null offset {index_type_id}"),
            ),
            _ => (
                IssueKind::InvalidArrayOffset,
                format!("Invalid array offset type: {index_type_id}"),
            ),
        };
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        emitted_offset_issue = true;
    }

    // Psalm reports the possibly-undefined offset alongside a
    // possibly-invalid (e.g. array|false) access — pure invalid accesses
    // returned earlier.
    if has_possibly_undefined_offset
        && !should_emit_invalid_literal_offset
        && !context.inside_isset
        && !context.inside_unset
        && !context.inside_conditional
        // Psalm's IssueBuffer::accepts gates the |null widening on the issue
        // actually reporting: a suppressed PossiblyUndefinedArrayOffset
        // leaves the fetched type alone.
        && !crate::issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            span.start.offset,
            "PossiblyUndefinedArrayOffset",
        )
    {
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyUndefinedArrayOffset,
            "Possibly undefined array offset".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        // Psalm widens the reported fetch with |null (the runtime yields
        // null plus a warning when the key is absent).
        if !result_types.is_empty() && !result_types.contains(&TAtomic::TNull) {
            result_types.push(TAtomic::TNull);
        }
    }

    // Check for invalid array offset type
    if let Some(index_type) = index_type.clone() {
        if emitted_offset_issue {
            let result_union = if result_types.is_empty() {
                TUnion::nothing()
            } else {
                let combined = type_combiner::combine(result_types, false);
                let mut result_union = TUnion::from_types(combined);
                result_union.from_docblock |= result_from_docblock;
                result_union
            };
            set_fetch_type_with_dataflow(
                analyzer,
                analysis_data,
                pos,
                array_pos,
                keyed_array_var_id,
                result_union,
                Some(&index_type),
            );
            return;
        }

        if context.inside_unset {
            // Psalm/Hakana do not report array-offset-type issues inside unset guards.
            let result_union = if result_types.is_empty() {
                TUnion::mixed()
            } else {
                let combined = type_combiner::combine(result_types, false);
                let mut result_union = TUnion::from_types(combined);
                result_union.from_docblock |= result_from_docblock;
                result_union
            };
            set_fetch_type_with_dataflow(
                analyzer,
                analysis_data,
                pos,
                array_pos,
                keyed_array_var_id,
                result_union,
                Some(&index_type),
            );
            return;
        }

        // Psalm's getArrayAccessTypeGivenOffset: a mixed offset reports
        // MixedArrayOffset before any expected-offset comparison.
        if index_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        {
            let span = access.index.span();
            let start_line = get_line_number(analyzer.source, span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedArrayOffset,
                "Cannot access value using mixed offset",
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
        } else if atomic_expected_offset_types.is_empty() {
            emitted_offset_issue = check_array_offset(
                &index_type,
                expected_offset_type.as_ref(),
                analyzer,
                access,
                analysis_data,
                context.inside_isset,
            );
        } else {
            emitted_offset_issue = check_array_offset_against_expected_branches(
                &index_type,
                &atomic_expected_offset_types,
                analyzer,
                access,
                analysis_data,
                context.inside_isset,
            );
        }
    }

    // Set the result type using the type combiner for proper simplification
    let result_union = if result_types.is_empty() {
        if emitted_offset_issue {
            TUnion::nothing()
        } else {
            TUnion::mixed()
        }
    } else {
        let combined = type_combiner::combine(result_types, false);
        let mut result_union = TUnion::from_types(combined);
        result_union.from_docblock |= result_from_docblock;
        result_union.ignore_nullable_issues |= result_ignore_nullable;
        result_union.ignore_falsable_issues |= result_ignore_falsable;
        result_union
    };
    // Psalm's ArrayFetchAnalyzer tail: a fetch from a provably-empty array
    // reports EmptyArrayAccess and degrades to mixed rather than flowing a
    // `never` into later expressions.
    let result_union = if result_union.is_nothing()
        && !emitted_offset_issue
        && !array_type.is_mixed()
        && !context.inside_isset
        && !context.inside_by_ref_argument
    {
        analysis_data.add_issue(Issue::new(
            IssueKind::EmptyArrayAccess,
            format!(
                "Cannot access value on empty array variable {}",
                cached_base_key.as_deref().unwrap_or("")
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        TUnion::mixed()
    } else {
        result_union
    };
    set_fetch_type_with_dataflow(
        analyzer,
        analysis_data,
        pos,
        array_pos,
        keyed_array_var_id,
        result_union,
        index_type.as_ref(),
    );

    if let (Some(base_key), Some(index_key)) = (
        expression_identifier::get_expression_var_key(access.array),
        get_array_index_key(access.index),
    ) {
        // Psalm only stores fetch results outside isset() — an isset-guarded
        // fetch is issue-suppressed (e.g. `b?:` loses possibly_undefined), so
        // caching it would leak a definite type into the surrounding scope.
        if !context.inside_isset
            && can_reuse_cached_dim_path(&base_key, &index_key)
            && let Some(expr_type) = analysis_data.expr_types.get(&pos).cloned().map(|t| (*t).clone())
        {
            let is_cacheable = !expr_type.is_mixed()
                && !expr_type.is_nullable()
                && !expr_type.is_falsable()
                && !expr_type.possibly_undefined
                && !expr_type.is_nothing();

            if is_cacheable {
                let full_key = format!("{}[{}]", base_key, index_key);
                let full_key_id = VarName::new(&full_key);
                context.locals.insert(full_key_id, expr_type);
            }
        }
    }
}

/// The class-string target of an array offset, used to substitute a
/// `class-string-map`'s placeholder template (Psalm's
/// `handleArrayAccessOnClassStringMap`): a templated `class-string<T2>` stays
/// deferred as the template param `T2`, `class-string<Foo>` / `Foo::class`
/// resolve to the named object, and a bare `class-string` yields `object`.
/// Non-class-string offsets are skipped (Psalm ignores them too).
fn class_string_map_offset_replacement(
    analyzer: &StatementsAnalyzer<'_>,
    offset_atomic: &TAtomic,
) -> Option<TAtomic> {
    match offset_atomic {
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => Some(TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(TUnion::new((**as_type).clone())),
        }),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some((**as_type).clone()),
        TAtomic::TClassString { as_type: None } => Some(TAtomic::TObject),
        TAtomic::TLiteralClassString { name } => Some(TAtomic::named_object(
            analyzer.interner.intern(name.trim_start_matches('\\')),
        )),
        _ => None,
    }
}

fn get_array_index_key(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => {
            int_lit.value.map(|value| value.to_string())
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                int_value.to_string()
            } else {
                let escaped = value.replace('\'', "\\'");
                format!("'{}'", escaped)
            }
        }),
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name.to_string()),
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            let class_name = match class_const_access.class.unparenthesized() {
                Expression::Identifier(identifier) => identifier.value().to_string(),
                Expression::Self_(_) => "self".to_string(),
                Expression::Static(_) => "static".to_string(),
                Expression::Parent(_) => "parent".to_string(),
                _ => return None,
            };

            let constant_name = match &class_const_access.constant {
                ClassLikeConstantSelector::Identifier(identifier) => identifier.value,
                _ => return None,
            };

            Some(format!("{}::{}", class_name, constant_name))
        }
        // Property paths / memoized method calls as dims key the same way the
        // assertion finder keys them (expression_identifier), so a narrowed
        // `$arr[$obj->prop]` entry is found again by the body's refetch.
        Expression::ArrayAccess(_)
        | Expression::Access(Access::Property(_))
        | Expression::Access(Access::NullSafeProperty(_))
        | Expression::Access(Access::StaticProperty(_))
        | Expression::Call(mago_syntax::ast::ast::call::Call::Method(_))
        | Expression::Call(mago_syntax::ast::ast::call::Call::NullSafeMethod(_)) => {
            expression_identifier::get_expression_var_key(expr).map(|key| key.to_string())
        }
        _ => None,
    }
}

fn can_reuse_cached_dim_path(base_key: &str, index_key: &str) -> bool {
    let _ = (base_key, index_key);
    true
}

fn context_asserts_isset_state(context: &BlockContext, var_name: &str) -> Option<bool> {
    let descendant_prefix_array = format!("{var_name}[");
    let descendant_prefix_property = format!("{var_name}->");

    for clause in &context.clauses {
        for (name, assertions_by_offset) in clause.possibilities.iter() {
            let ClauseKey::Name(name) = name else {
                continue;
            };

            if name != var_name
                && !name.starts_with(&descendant_prefix_array)
                && !name.starts_with(&descendant_prefix_property)
            {
                continue;
            }

            for assertion in assertions_by_offset.values() {
                match assertion {
                    Assertion::IsIsset | Assertion::IsEqualIsset => return Some(true),
                    Assertion::ArrayKeyExists
                    | Assertion::HasArrayKey(_)
                    | Assertion::HasNonnullEntryForKey(_) => return Some(true),
                    Assertion::IsNotIsset => {
                        if name == var_name {
                            return Some(false);
                        }
                    }
                    Assertion::ArrayKeyDoesNotExist
                    | Assertion::DoesNotHaveArrayKey(_)
                    | Assertion::DoesNotHaveNonnullEntryForKey(_) => {
                        if name == var_name {
                            return Some(false);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

fn get_literal_array_key_from_expr(expr: &Expression<'_>) -> Option<ArrayKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(ArrayKey::Int),
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                ArrayKey::Int(int_value)
            } else {
                ArrayKey::String(value.to_string())
            }
        }),
        _ => None,
    }
}

fn get_literal_array_keys_from_union(index_type: &TUnion) -> Option<Vec<ArrayKey>> {
    if index_type.types.is_empty() {
        return None;
    }

    let mut keys = Vec::with_capacity(index_type.types.len());

    for atomic in &index_type.types {
        let key = match atomic {
            TAtomic::TLiteralInt { value } => ArrayKey::Int(*value),
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .map(ArrayKey::Int)
                .unwrap_or_else(|_| ArrayKey::String(value.clone())),
            _ => return None,
        };

        if !keys.contains(&key) {
            keys.push(key);
        }
    }

    Some(keys)
}

/// The (key, value) types of an ArrayAccess-like receiver, from its
/// offsetGet signature with the declaring class's templates localized
/// through the receiver's type params (Psalm's handleArrayAccessOnObject).
pub(crate) fn resolve_array_access_method_types(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    class_id: StrId,
) -> Option<(TUnion, TUnion)> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let type_params = if let TAtomic::TNamedObject { type_params, .. } = atomic {
        type_params.as_deref()
    } else {
        None
    };

    // Psalm rewrites `$nodeList[$i]` on DOMNodeList to `$nodeList->item($i)`
    // (handleArrayAccessOnNamedObject's domnodelist special case).
    let accessor = if analyzer
        .interner
        .lookup(class_id)
        .eq_ignore_ascii_case("DOMNodeList")
    {
        "item"
    } else {
        "offsetGet"
    };
    let offset_get_id = analyzer.interner.intern(accessor);
    let method_info = class_info.methods.get(&offset_get_id).map(|m| &**m).or_else(|| {
        class_info
            .all_parent_classes
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
            .find_map(|ancestor| {
                analyzer
                    .codebase
                    .get_class(*ancestor)?
                    .methods
                    .get(&offset_get_id)
                    .map(|m| &**m)
            })
    })?;

    let key_type = method_info
        .params
        .first()
        .and_then(|param| param.get_type().cloned())
        .unwrap_or_else(TUnion::mixed);
    let value_type = method_info.get_return_type().cloned().unwrap_or_else(TUnion::mixed);

    if value_type.is_mixed() && key_type.is_mixed() {
        return None;
    }

    let mut localized_key = crate::expr::call::method_call_return_type_fetcher::localize_class_union_type(
        class_info, type_params, &key_type,
    );
    let mut localized_value = crate::expr::call::method_call_return_type_fetcher::localize_class_union_type(
        class_info, type_params, &value_type,
    );
    // `static`/`self` in the offsetGet signature resolve to the receiver
    // (SimpleXMLElement::offsetGet(): static|null reads as the element type).
    for union in [&mut localized_key, &mut localized_value] {
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            union,
            &crate::type_expander::TypeExpansionOptions {
                self_class: Some(class_id),
                static_class_type: crate::type_expander::StaticClassType::Name(class_id),
                ..Default::default()
            },
        );
    }
    Some((localized_key, localized_value))
}

fn class_supports_array_access(analyzer: &StatementsAnalyzer<'_>, class_name: StrId) -> bool {
    if class_name == StrId::SIMPLE_XML_ELEMENT {
        return true;
    }

    // Psalm special-cases DOMNodeList dim access as an ->item() call.
    if analyzer
        .interner
        .lookup(class_name)
        .eq_ignore_ascii_case("DOMNodeList")
    {
        return true;
    }

    if class_name == StrId::ARRAY_ACCESS {
        return true;
    }

    let Some(class_info) = analyzer.codebase.get_class(class_name) else {
        // An unknown class can't be disproven to implement ArrayAccess;
        // UndefinedClass/UndefinedDocblockClass is reported elsewhere
        // (Psalm doesn't add InvalidArrayAccess on top).
        return true;
    };

    class_info.interfaces.contains(&StrId::ARRAY_ACCESS)
        || class_info.all_parent_interfaces.contains(&StrId::ARRAY_ACCESS)
}

/// Check if the array offset type is valid.
fn check_array_offset(
    index_type: &TUnion,
    expected_offset_type: Option<&TUnion>,
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    analysis_data: &mut FunctionAnalysisData,
    suppress_possible_issue: bool,
) -> bool {
    if let Some(expected_offset_type) = expected_offset_type {
        let coerce_class_strings = should_coerce_class_string_offsets(expected_offset_type);
        let normalized_index_type = normalize_array_offset_comparison_union(
            index_type,
            analyzer,
            coerce_class_strings,
        );

        if expected_offset_type
            .types
            .iter()
            .any(|atomic| is_unresolved_expected_offset_atomic(atomic, analyzer))
        {
            let span = access.array.span();
            let start_line = get_line_number(analyzer.source, span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedArrayAccess,
                "Cannot access array value with unresolved key type".to_string(),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }

        if expected_offset_type.is_mixed() || expected_offset_type.is_nothing() {
            return false;
        }

        let mut comparison_result = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected_offset_type,
            false,
            false,
            &mut comparison_result,
        ) {
            return false;
        }

        // Psalm's handleArrayAccessOnKeyedArray accepts an offset whose
        // mismatch is a coercion from the wider scalar (a plain string key on
        // a literal-keyed shape) or from mixed — no offset issue is emitted.
        if comparison_result.type_coerced_from_scalar.unwrap_or(false)
            || comparison_result.type_coerced_from_mixed.unwrap_or(false)
        {
            return false;
        }

        if suppress_possible_issue {
            let mut reverse_comparison_result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                expected_offset_type,
                &normalized_index_type,
                false,
                false,
                &mut reverse_comparison_result,
            ) {
                return false;
            }

            if expected_offsets_fit_class_string_bound(
                analyzer,
                expected_offset_type,
                &normalized_index_type,
            ) {
                return false;
            }

            if union_contains_class_string_like(&normalized_index_type)
                || union_contains_class_string_like(index_type)
            {
                return false;
            }
        }

        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);

        if union_type_comparator::can_be_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected_offset_type,
        ) {
            if suppress_possible_issue {
                return false;
            }
            // A null possibility among otherwise-fitting offsets is Psalm's
            // PossiblyNullArrayOffset, not the generic possibly-invalid kind.
            if normalized_index_type
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TNull))
            {
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyNullArrayOffset,
                    format!(
                        "Cannot access value using possibly null offset {}",
                        normalized_index_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    start_line,
                    0,
                ));
                return true;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidArrayOffset,
                format!(
                    "Array offset may be invalid type: {}",
                    normalized_index_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        } else {
            if suppress_possible_issue {
                return false;
            }
            // A definitely-null offset is Psalm's NullArrayOffset.
            if normalized_index_type.is_null() {
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullArrayOffset,
                    "Cannot access value using null offset".to_string(),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    start_line,
                    0,
                ));
                return true;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!(
                    "Invalid array offset type: {}",
                    normalized_index_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }
    }

    let mut has_valid_offset = false;
    let mut has_invalid_offset = false;
    let mut has_null_offset = false;
    let mut invalid_offset_type = String::new();

    for atomic in &index_type.types {
        match atomic {
            // Valid offset types
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TArrayKey
            | TAtomic::TMixed => {
                has_valid_offset = true;
            }

            // Invalid offset types
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TObject
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TResource
            | TAtomic::TVoid => {
                has_invalid_offset = true;
                invalid_offset_type = atomic.get_id(Some(analyzer.interner));
            }
            // Null gets Psalm's dedicated NullArrayOffset /
            // PossiblyNullArrayOffset kinds (folded into the generic invalid
            // report only when other invalid offset parts are present too).
            TAtomic::TNull => {
                has_null_offset = true;
            }
            TAtomic::TNamedObject { name, .. } => {
                // Docblocks may carry unresolved class-constant pseudo-types such as `self::FOO`.
                let type_name = analyzer.interner.lookup(*name);
                if type_name.contains("::") {
                    has_valid_offset = true;
                } else {
                    has_invalid_offset = true;
                    invalid_offset_type = atomic.get_id(Some(analyzer.interner));
                }
            }

            // Bool can be used as offset (converts to 0/1)
            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => {
                has_valid_offset = true;
            }

            _ => {
                has_valid_offset = true;
            }
        }
    }

    if has_null_offset && !has_invalid_offset {
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        if suppress_possible_issue {
            return false;
        }
        if has_valid_offset {
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyNullArrayOffset,
                format!(
                    "Cannot access value using possibly null offset {}",
                    index_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
        } else {
            analysis_data.add_issue(Issue::new(
                IssueKind::NullArrayOffset,
                "Cannot access value using null offset".to_string(),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
        }
        return true;
    }

    if has_invalid_offset {
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        if has_null_offset {
            has_valid_offset = true;
        }

        if has_valid_offset {
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidArrayOffset,
                format!("Array offset may be invalid type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        } else {
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!("Invalid array offset type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }
    }

    false
}

fn check_array_offset_against_expected_branches(
    index_type: &TUnion,
    expected_branches: &[TUnion],
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    analysis_data: &mut FunctionAnalysisData,
    suppress_possible_issue: bool,
) -> bool {
    let coerce_class_strings = expected_branches
        .iter()
        .all(should_coerce_class_string_offsets);
    let normalized_index_type = normalize_array_offset_comparison_union(
        index_type,
        analyzer,
        coerce_class_strings,
    );

    if expected_branches.iter().any(|expected| {
        expected
            .types
            .iter()
            .any(|atomic| is_unresolved_expected_offset_atomic(atomic, analyzer))
    }) {
        let span = access.array.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArrayAccess,
            "Cannot access array value with unresolved key type".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        return true;
    }

    let mut has_valid_branch = false;
    let mut has_invalid_branch = false;

    for expected in expected_branches {
        if expected.is_mixed() || expected.is_nothing() {
            has_valid_branch = true;
            continue;
        }

        let mut comparison_result = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected,
            false,
            false,
            &mut comparison_result,
        ) {
            has_valid_branch = true;
            continue;
        }

        // Psalm's handleArrayAccessOnKeyedArray accepts an offset whose
        // mismatch is a coercion from the wider scalar (a plain string key on
        // a literal-keyed shape) or from mixed — no offset issue is emitted.
        if comparison_result.type_coerced_from_scalar.unwrap_or(false)
            || comparison_result.type_coerced_from_mixed.unwrap_or(false)
        {
            has_valid_branch = true;
            continue;
        }

        if suppress_possible_issue {
            let mut reverse_comparison_result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                expected,
                &normalized_index_type,
                false,
                false,
                &mut reverse_comparison_result,
            ) {
                has_valid_branch = true;
                continue;
            }

            if expected_offsets_fit_class_string_bound(analyzer, expected, &normalized_index_type) {
                has_valid_branch = true;
                continue;
            }
        }

        if union_type_comparator::can_be_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected,
        ) {
            has_valid_branch = true;
            has_invalid_branch = true;
        } else {
            has_invalid_branch = true;
        }
    }

    if !has_invalid_branch {
        return false;
    }

    if suppress_possible_issue
        && (union_contains_class_string_like(&normalized_index_type)
            || union_contains_class_string_like(index_type))
    {
        return false;
    }

    if has_valid_branch && suppress_possible_issue {
        return false;
    }

    if suppress_possible_issue {
        return false;
    }

    let span = access.index.span();
    let start_line = get_line_number(analyzer.source, span.start.offset);
    let index_has_null = normalized_index_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNull));
    let (kind, message) = if normalized_index_type.is_null() {
        (
            IssueKind::NullArrayOffset,
            "Cannot access value using null offset".to_string(),
        )
    } else if has_valid_branch && index_has_null {
        (
            IssueKind::PossiblyNullArrayOffset,
            format!(
                "Cannot access value using possibly null offset {}",
                normalized_index_type.get_id(Some(analyzer.interner))
            ),
        )
    } else if has_valid_branch {
        (
            IssueKind::PossiblyInvalidArrayOffset,
            format!(
                "Array offset may be invalid type: {}",
                normalized_index_type.get_id(Some(analyzer.interner))
            ),
        )
    } else {
        (
            IssueKind::InvalidArrayOffset,
            format!(
                "Invalid array offset type: {}",
                normalized_index_type.get_id(Some(analyzer.interner))
            ),
        )
    };

    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        start_line,
        0,
    ));

    true
}

fn merge_expected_offset_type(target: &mut Option<TUnion>, incoming: TUnion) {
    match target {
        Some(existing) => {
            *existing = combine_union_types(existing, &incoming, false);
        }
        None => {
            *target = Some(incoming);
        }
    }
}

fn expand_template_union_bounds(union: &TUnion) -> TUnion {
    let has_template = union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
        )
    });
    if !has_template {
        // Nothing to expand — keep the union (and its metadata: parent_nodes,
        // ignore_falsable_issues, …) untouched.
        return union.clone();
    }

    let mut expanded = Vec::new();

    for atomic in &union.types {
        match atomic {
            TAtomic::TTemplateParam { as_type, .. } => {
                for nested in &as_type.types {
                    if !expanded.contains(nested) {
                        expanded.push(nested.clone());
                    }
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !expanded.contains(as_type) {
                    expanded.push((**as_type).clone());
                }
            }
            _ => {
                if !expanded.contains(atomic) {
                    expanded.push(atomic.clone());
                }
            }
        }
    }

    if expanded.is_empty() {
        union.clone()
    } else {
        let mut expanded_union = TUnion::from_types(expanded);
        expanded_union.parent_nodes = union.parent_nodes.clone();
        expanded_union.ignore_falsable_issues = union.ignore_falsable_issues;
        expanded_union.ignore_nullable_issues = union.ignore_nullable_issues;
        expanded_union.possibly_undefined = union.possibly_undefined;
        expanded_union
    }
}

fn normalize_array_offset_comparison_union(
    union: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    coerce_class_strings: bool,
) -> TUnion {
    let mut normalized = Vec::new();

    for atomic in &union.types {
        let next = match atomic {
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .ok()
                .map(|int_value| TAtomic::TLiteralInt { value: int_value })
                .unwrap_or_else(|| atomic.clone()),
            TAtomic::TLiteralClassString { name } if coerce_class_strings => {
                TAtomic::TLiteralString {
                    value: name.trim_start_matches('\\').to_string(),
                }
            }
            TAtomic::TClassString {
                as_type: Some(as_type),
            } if coerce_class_strings => {
                if let TAtomic::TNamedObject { name, .. } = as_type.as_ref() {
                    TAtomic::TLiteralString {
                        value: analyzer
                            .interner
                            .lookup(*name)
                            .trim_start_matches('\\')
                            .to_string(),
                    }
                } else {
                    TAtomic::TString
                }
            }
            TAtomic::TClassString { .. } if coerce_class_strings => TAtomic::TString,
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => TAtomic::TInt,
            _ => atomic.clone(),
        };

        if !normalized.contains(&next) {
            normalized.push(next);
        }
    }

    if normalized.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(normalized)
    }
}

fn should_coerce_class_string_offsets(expected_offset_type: &TUnion) -> bool {
    let mut has_literal_string = false;

    for atomic in &expected_offset_type.types {
        match atomic {
            TAtomic::TLiteralString { .. } => {
                has_literal_string = true;
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TArrayKey
            | TAtomic::TMixed
            | TAtomic::TNonEmptyMixed => {
                return false;
            }
            _ => {}
        }
    }

    has_literal_string
}

fn expected_offsets_fit_class_string_bound(
    analyzer: &StatementsAnalyzer<'_>,
    expected_offset_type: &TUnion,
    index_type: &TUnion,
) -> bool {
    let mut bounds: Vec<Option<StrId>> = Vec::new();

    for atomic in &index_type.types {
        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if let TAtomic::TNamedObject { name, .. } = as_type.as_ref() {
                    bounds.push(Some(*name));
                }
            }
            TAtomic::TClassString { as_type: None } => {
                bounds.push(None);
            }
            TAtomic::TLiteralClassString { name } => {
                if let Some(class_id) = resolve_class_id_from_literal_string(analyzer, name) {
                    bounds.push(Some(class_id));
                }
            }
            _ => {}
        }
    }

    if bounds.is_empty() {
        return false;
    }

    let mut expected_class_ids = Vec::new();

    for atomic in &expected_offset_type.types {
        let class_id = match atomic {
            TAtomic::TLiteralClassString { name } => {
                resolve_class_id_from_literal_string(analyzer, name)
            }
            TAtomic::TLiteralString { value } => resolve_class_id_from_literal_string(analyzer, value),
            _ => None,
        };

        let Some(class_id) = class_id else {
            return false;
        };

        expected_class_ids.push(class_id);
    }

    if expected_class_ids.is_empty() {
        return false;
    }

    expected_class_ids.into_iter().all(|expected_class_id| {
        bounds.iter().any(|bound| match bound {
            None => true,
            Some(bound_class_id) => object_type_comparator::is_class_subtype_of(
                expected_class_id,
                *bound_class_id,
                analyzer.codebase,
            ),
        })
    })
}

fn resolve_class_id_from_literal_string(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
) -> Option<StrId> {
    let mut normalized = class_name.trim().trim_start_matches('\\').to_string();

    if normalized
        .to_ascii_lowercase()
        .ends_with("::class")
        && let Some((class_part, _)) = normalized.rsplit_once("::")
    {
        normalized = class_part.trim_start_matches('\\').to_string();
    }

    analyzer
        .interner
        .find(&normalized)
        .or_else(|| analyzer.interner.find(&format!("\\{}", normalized)))
}

fn union_contains_class_string_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TTemplateParamClass { .. }
        )
    })
}

fn extract_keyed_array_key_type(
    properties: &rustc_hash::FxHashMap<ArrayKey, TUnion>,
    is_list: bool,
    fallback_key_type: Option<&TUnion>,
) -> TUnion {
    let mut key_type = fallback_key_type.cloned().unwrap_or_else(TUnion::nothing);

    for key in properties.keys() {
        let literal_key_type = match key {
            ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
            ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                value: value.clone(),
            }),
        };

        key_type = if key_type.is_nothing() {
            literal_key_type
        } else {
            combine_union_types(&key_type, &literal_key_type, false)
        };
    }

    if is_list {
        key_type = if key_type.is_nothing() {
            TUnion::int()
        } else {
            combine_union_types(&key_type, &TUnion::int(), false)
        };
    }

    if key_type.is_nothing() {
        TUnion::array_key()
    } else {
        key_type
    }
}

fn is_unresolved_expected_offset_atomic(
    atomic: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. }
        | TAtomic::TObject => true,
        TAtomic::TNamedObject { name, .. } => {
            // A known class as the expected offset is a legitimate object key
            // (e.g. WeakMap<Throwable, int>); only unresolvable names count
            // as unresolved.
            let type_name = analyzer.interner.lookup(*name);
            !type_name.contains("::") && analyzer.codebase.get_class(*name).is_none()
        }
        _ => false,
    }
}
