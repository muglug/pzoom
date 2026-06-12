//! Array expression analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::array::{Array, ArrayElement, List};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::data_flow::path::ArrayDataKind;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{DataFlowNode, PathKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expr::call::function_call_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

#[derive(Default)]
struct ArrayCreationInfo {
    property_types: FxHashMap<ArrayKey, TUnion>,
    seen_keys: FxHashSet<ArrayKey>,
    item_key_atomic_types: Vec<TAtomic>,
    item_value_types: Vec<TUnion>,
    can_create_objectlike: bool,
    can_be_empty: bool,
    all_list: bool,
    int_offset: i64,
    parent_nodes: Vec<DataFlowNode>,
}

impl ArrayCreationInfo {
    fn new() -> Self {
        Self {
            property_types: FxHashMap::default(),
            seen_keys: FxHashSet::default(),
            item_key_atomic_types: Vec::new(),
            item_value_types: Vec::new(),
            can_create_objectlike: true,
            can_be_empty: true,
            all_list: true,
            int_offset: -1,
            parent_nodes: Vec::new(),
        }
    }
}

/// Hakana `collection_analyzer::add_array_value_dataflow`: connect an element
/// value's parents to a per-item node, labelled with the literal key if known.
fn add_array_value_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    key_item_type: &TUnion,
    value_pos: Pos,
    info: &mut ArrayCreationInfo,
) {
    if value_type.parent_nodes.is_empty() {
        return;
    }

    let mut key_name = "".to_string();

    let key_item_single = if key_item_type.types.len() == 1 {
        key_item_type.types.first()
    } else {
        None
    };

    if let Some(key_item_single) = key_item_single {
        if let TAtomic::TLiteralString { value } = key_item_single {
            key_name.clone_from(value);
        } else if let TAtomic::TLiteralInt { value } = key_item_single {
            key_name = value.to_string();
        }
    }

    let new_parent_node = DataFlowNode::get_for_array_item(
        key_name,
        make_data_flow_node_position(analyzer, value_pos),
    );
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    for parent_node in value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &new_parent_node.id,
            match key_item_single {
                Some(TAtomic::TLiteralInt { value }) => {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, value.to_string())
                }
                Some(TAtomic::TLiteralString { value }) => {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, value.clone())
                }
                _ => PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
            },
            vec![],
            vec![],
        );
    }

    info.parent_nodes.push(new_parent_node);
}

/// Hakana `collection_analyzer::add_array_key_dataflow`.
fn add_array_key_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    key_item_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    item_key_pos: Pos,
    info: &mut ArrayCreationInfo,
) {
    if key_item_type.parent_nodes.is_empty() {
        return;
    }

    let new_parent_node = DataFlowNode::get_for_array_item(
        "array".to_string(),
        make_data_flow_node_position(analyzer, item_key_pos),
    );
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    let key_item_single = if key_item_type.types.len() == 1 {
        key_item_type.types.first()
    } else {
        None
    };

    for parent_node in key_item_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &new_parent_node.id,
            match key_item_single {
                Some(TAtomic::TLiteralInt { value }) => {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayKey, value.to_string())
                }
                Some(TAtomic::TLiteralString { value }) => {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayKey, value.clone())
                }
                _ => PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayKey),
            },
            vec![],
            vec![],
        );
    }

    info.parent_nodes.push(new_parent_node);
}

/// Analyze an array creation expression.
pub fn analyze_array(
    analyzer: &StatementsAnalyzer<'_>,
    array: &Array<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if array.elements.is_empty() {
        // Empty array
        analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            })));
        return;
    }

    let mut info = ArrayCreationInfo::new();

    for element in array.elements.iter() {
        analyze_array_element(analyzer, analysis_data, context, &mut info, element);
    }

    let item_key_type = if info.item_key_atomic_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(type_combiner::combine(
            info.item_key_atomic_types,
            false,
        )))
    };

    let item_value_type = if info.item_value_types.is_empty() {
        None
    } else {
        Some(combine_types(info.item_value_types))
    };

    let parent_nodes = std::mem::take(&mut info.parent_nodes);

    let mut expr_type = if !info.property_types.is_empty() {
        let fallback = if info.can_create_objectlike {
            (None, None, true)
        } else {
            (
                Some(Box::new(
                    item_key_type.clone().unwrap_or_else(TUnion::array_key),
                )),
                Some(Box::new(
                    item_value_type.clone().unwrap_or_else(TUnion::mixed),
                )),
                false,
            )
        };

        TUnion::new(TAtomic::TKeyedArray {
            properties: std::sync::Arc::new(info.property_types),
            is_list: info.all_list,
            sealed: fallback.2,
            fallback_key_type: fallback.0,
            fallback_value_type: fallback.1,
        })
    } else if info.all_list {
        let value_type = item_value_type.unwrap_or_else(TUnion::mixed);
        if info.can_be_empty {
            TUnion::new(TAtomic::TList {
                value_type: Box::new(value_type),
            })
        } else {
            TUnion::new(TAtomic::TNonEmptyList {
                value_type: Box::new(value_type),
            })
        }
    } else {
        let key_type = item_key_type.unwrap_or_else(TUnion::array_key);
        let value_type = item_value_type.unwrap_or_else(TUnion::mixed);

        if info.can_be_empty {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(key_type),
                value_type: Box::new(value_type),
            })
        } else {
            TUnion::new(TAtomic::TNonEmptyArray {
                key_type: Box::new(key_type),
                value_type: Box::new(value_type),
            })
        }
    };

    // Hakana funnels every item node into a composition node for the literal.
    if !parent_nodes.is_empty() {
        let array_node =
            DataFlowNode::get_for_composition(make_data_flow_node_position(analyzer, pos));

        for child_node in parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &child_node.id,
                &array_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }

        analysis_data.data_flow_graph.add_node(array_node.clone());

        expr_type.parent_nodes = vec![array_node];
    }

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

fn analyze_array_element(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    info: &mut ArrayCreationInfo,
    element: &ArrayElement<'_>,
) {
    match element {
        ArrayElement::Missing(_) => {}
        ArrayElement::Variadic(variadic) => {
            let spread_pos =
                expression_analyzer::analyze(analyzer, variadic.value, analysis_data, context);
            let spread_type = analysis_data
                .expr_types.get(&spread_pos).cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed);

            // Spread values flow into the new array like any other element
            // (Hakana's add_array_value_dataflow for the unpacked entry).
            if !spread_type.parent_nodes.is_empty() {
                let new_parent_node = DataFlowNode::get_for_array_item(
                    "".to_string(),
                    make_data_flow_node_position(analyzer, spread_pos),
                );
                analysis_data
                    .data_flow_graph
                    .add_node(new_parent_node.clone());
                for parent_node in spread_type.parent_nodes.iter() {
                    analysis_data.data_flow_graph.add_path(
                        &parent_node.id,
                        &new_parent_node.id,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                        vec![],
                        vec![],
                    );
                }
                info.parent_nodes.push(new_parent_node);
            }
            handle_unpacked_array(analyzer, analysis_data, info, variadic.value, &spread_type);
        }
        ArrayElement::Value(value_element) => {
            let value_pos =
                expression_analyzer::analyze(analyzer, value_element.value, analysis_data, context);
            let value_type = analysis_data
                .expr_types.get(&value_pos).cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed);

            info.can_be_empty = false;

            if info.int_offset == i64::MAX {
                emit_issue(
                    analyzer,
                    analysis_data,
                    value_element.span().start.offset,
                    value_element.span().end.offset,
                    IssueKind::InvalidArrayOffset,
                    "Cannot add an item with an offset beyond i64::MAX".to_string(),
                );
                return;
            }

            info.int_offset += 1;
            let key = ArrayKey::Int(info.int_offset);

            // Hakana `analyze_vals_item`: connect the value's parents to a per-item
            // node keyed by the implicit list offset.
            let key_item_type = TUnion::new(TAtomic::TLiteralInt {
                value: info.int_offset,
            });
            add_array_value_dataflow(
                analyzer,
                &value_type,
                analysis_data,
                &key_item_type,
                value_pos,
                info,
            );

            record_literal_key(
                analyzer,
                analysis_data,
                info,
                key,
                value_type,
                true,
                (
                    value_element.span().start.offset,
                    value_element.span().end.offset,
                ),
            );
        }
        ArrayElement::KeyValue(key_value) => {
            let key_pos =
                expression_analyzer::analyze(analyzer, key_value.key, analysis_data, context);
            let value_pos =
                expression_analyzer::analyze(analyzer, key_value.value, analysis_data, context);

            let raw_key_type = analysis_data
                .expr_types.get(&key_pos).cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::array_key);
            let value_type = analysis_data
                .expr_types.get(&value_pos).cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed);

            // Hakana `analyze_keyvals_item`: key and value dataflow per item.
            add_array_key_dataflow(analyzer, &raw_key_type, analysis_data, key_pos, info);
            add_array_value_dataflow(
                analyzer,
                &value_type,
                analysis_data,
                &raw_key_type,
                value_pos,
                info,
            );

            info.can_be_empty = false;

            let normalized_key_type = normalize_array_creation_key_union(
                analyzer,
                analysis_data,
                &raw_key_type,
                key_value.key,
            );

            for atomic in &normalized_key_type.types {
                info.item_key_atomic_types.push(atomic.clone());
            }

            if let Some(literal_key) = get_single_literal_array_key(&normalized_key_type) {
                match literal_key {
                    ArrayKey::Int(offset) => {
                        if offset > info.int_offset {
                            if offset - 1 != info.int_offset {
                                info.all_list = false;
                            }
                            info.int_offset = offset;
                        } else {
                            info.all_list = false;
                        }
                    }
                    ArrayKey::String(_) => {
                        info.all_list = false;
                    }
                }

                record_literal_key(
                    analyzer,
                    analysis_data,
                    info,
                    literal_key,
                    value_type,
                    true,
                    (key_value.span().start.offset, key_value.span().end.offset),
                );
            } else {
                info.all_list = false;
                info.can_create_objectlike = false;
                info.item_value_types.push(value_type);
            }
        }
    }
}

fn record_literal_key(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    info: &mut ArrayCreationInfo,
    key: ArrayKey,
    value_type: TUnion,
    emit_duplicate_issue: bool,
    issue_span: (u32, u32),
) {
    if emit_duplicate_issue && info.seen_keys.contains(&key) {
        let (line, col) = analyzer.get_line_column(issue_span.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::DuplicateArrayKey,
            format!("Key '{}' already exists on array", key_to_string(&key)),
            analyzer.file_path,
            issue_span.0,
            issue_span.1,
            line,
            col,
        ));
    }

    info.seen_keys.insert(key.clone());
    info.item_key_atomic_types.push(match &key {
        ArrayKey::Int(value) => TAtomic::TLiteralInt { value: *value },
        ArrayKey::String(value) => TAtomic::TLiteralString {
            value: value.clone(),
        },
    });

    if info.can_create_objectlike {
        info.property_types.insert(key, value_type.clone());
    }
    info.item_value_types.push(value_type);
}

fn handle_unpacked_array(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    info: &mut ArrayCreationInfo,
    unpack_expr: &Expression<'_>,
    unpacked_array_type: &TUnion,
) {
    let mut all_non_empty = true;

    for unpacked_atomic_type in &unpacked_array_type.types {
        if !is_definitely_non_empty_iterable(unpacked_atomic_type) {
            all_non_empty = false;
        }

        if let TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } = unpacked_atomic_type
        {
            let mut had_optional = false;

            for (key, property_value) in properties.iter() {
                if property_value.possibly_undefined {
                    had_optional = true;
                    continue;
                }

                let normalized_key = match key {
                    ArrayKey::Int(_) => {
                        if info.int_offset == i64::MAX {
                            emit_issue(
                                analyzer,
                                analysis_data,
                                unpack_expr.span().start.offset,
                                unpack_expr.span().end.offset,
                                IssueKind::InvalidArrayOffset,
                                "Cannot add an item with an offset beyond i64::MAX".to_string(),
                            );
                            continue;
                        }
                        info.int_offset += 1;
                        ArrayKey::Int(info.int_offset)
                    }
                    ArrayKey::String(value) => {
                        info.all_list = false;
                        ArrayKey::String(value.clone())
                    }
                };

                record_literal_key(
                    analyzer,
                    analysis_data,
                    info,
                    normalized_key,
                    property_value.clone(),
                    false,
                    (
                        unpack_expr.span().start.offset,
                        unpack_expr.span().end.offset,
                    ),
                );
            }

            if !had_optional && fallback_key_type.is_none() && fallback_value_type.is_none() {
                continue;
            }
        }

        let Some((iter_key_type, iter_value_type)) =
            extract_unpacked_iterable_params(analyzer, unpacked_atomic_type)
        else {
            info.can_create_objectlike = false;
            info.all_list = false;
            info.item_key_atomic_types.push(TAtomic::TArrayKey);
            info.item_value_types.push(TUnion::mixed());
            emit_issue(
                analyzer,
                analysis_data,
                unpack_expr.span().start.offset,
                unpack_expr.span().end.offset,
                IssueKind::InvalidOperand,
                format!(
                    "Cannot use spread operator on non-iterable type {}",
                    unpacked_array_type.get_id(Some(analyzer.interner))
                ),
            );
            continue;
        };

        if !union_contains_only_array_key(analyzer, &iter_key_type) {
            emit_issue(
                analyzer,
                analysis_data,
                unpack_expr.span().start.offset,
                unpack_expr.span().end.offset,
                IssueKind::InvalidOperand,
                format!(
                    "Cannot use spread operator on iterable with key type {}",
                    iter_key_type.get_id(Some(analyzer.interner))
                ),
            );
            continue;
        }

        info.can_create_objectlike = false;

        let key_can_be_string = iter_key_type.has_string()
            || iter_key_type.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TTemplateParam { as_type, .. } if as_type.has_string()
                )
            });
        if key_can_be_string {
            info.all_list = false;
        }

        merge_property_types_for_spread(
            analyzer,
            &mut info.property_types,
            &iter_key_type,
            &iter_value_type,
        );

        info.item_key_atomic_types
            .extend(iter_key_type.types.clone());
        info.item_value_types.push(iter_value_type);
    }

    if all_non_empty {
        info.can_be_empty = false;
    }
}

fn is_definitely_non_empty_iterable(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => true,
        TAtomic::TKeyedArray { properties, .. } => properties
            .values()
            .any(|property_type| !property_type.possibly_undefined),
        _ => false,
    }
}

fn extract_unpacked_iterable_params(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<(TUnion, TUnion)> {
    match atomic {
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
        } => Some(((**key_type).clone(), (**value_type).clone())),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            Some((TUnion::int(), (**value_type).clone()))
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => Some(get_keyed_array_generic_params(
            properties,
            fallback_key_type.as_deref(),
            fallback_value_type.as_deref(),
        )),
        TAtomic::TNamedObject { name, type_params , .. } => {
            if !named_object_is_traversable(analyzer, *name) {
                return None;
            }

            if let Some(class_info) = analyzer.codebase.get_class(*name) {
                let mut template_result =
                    function_call_analyzer::get_class_template_defaults(class_info);
                function_call_analyzer::infer_class_template_replacements_from_extended_params(
                    &mut template_result,
                    class_info,
                );
                function_call_analyzer::overlay_template_replacements(
                    &mut template_result,
                    function_call_analyzer::infer_class_template_replacements_from_type_params(
                        class_info,
                        type_params.as_deref(),
                    ),
                );

                if let Some(extended_traversable_template_map) =
                    class_info.template_extended_params.get(&StrId::TRAVERSABLE)
                {
                    if let Some(traversable_info) = analyzer.codebase.get_class(StrId::TRAVERSABLE)
                    {
                        let mut ordered = Vec::new();
                        for template_type in &traversable_info.template_types {
                            if let Some(resolved_union) =
                                extended_traversable_template_map.get(&template_type.name)
                            {
                                ordered.push(function_call_analyzer::replace_templates_in_union(
                                    resolved_union,
                                    &template_result,
                                ));
                            }
                        }

                        if ordered.len() >= 2 {
                            return Some((ordered[0].clone(), ordered[1].clone()));
                        }

                        if let Some(single_value_type) = ordered.first() {
                            return Some((TUnion::mixed(), single_value_type.clone()));
                        }
                    }
                }

                if let Some(extended_traversable_params) = class_info
                    .template_extended_offsets
                    .get(&StrId::TRAVERSABLE)
                {
                    if extended_traversable_params.len() >= 2 {
                        let key_type = function_call_analyzer::replace_templates_in_union(
                            &extended_traversable_params[0],
                            &template_result,
                        );
                        let value_type = function_call_analyzer::replace_templates_in_union(
                            &extended_traversable_params[1],
                            &template_result,
                        );
                        return Some((key_type, value_type));
                    }

                    if let Some(single_value_type) = extended_traversable_params.first() {
                        let value_type = function_call_analyzer::replace_templates_in_union(
                            single_value_type,
                            &template_result,
                        );
                        return Some((TUnion::mixed(), value_type));
                    }
                }

                if let Some(type_params) = type_params {
                    if type_params.len() >= 2 {
                        return Some((type_params[0].clone(), type_params[1].clone()));
                    }
                    if let Some(value_type) = type_params.first() {
                        return Some((TUnion::mixed(), value_type.clone()));
                    }
                }
            } else if let Some(type_params) = type_params {
                if type_params.len() >= 2 {
                    return Some((type_params[0].clone(), type_params[1].clone()));
                }
                if let Some(value_type) = type_params.first() {
                    return Some((TUnion::mixed(), value_type.clone()));
                }
            }

            Some((TUnion::mixed(), TUnion::mixed()))
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut key_type: Option<TUnion> = None;
            let mut value_type: Option<TUnion> = None;

            for nested_atomic in &as_type.types {
                let Some((nested_key_type, nested_value_type)) =
                    extract_unpacked_iterable_params(analyzer, nested_atomic)
                else {
                    continue;
                };

                key_type = Some(if let Some(existing) = key_type {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });

                value_type = Some(if let Some(existing) = value_type {
                    combine_union_types(&existing, &nested_value_type, false)
                } else {
                    nested_value_type
                });
            }

            Some((
                key_type.unwrap_or_else(TUnion::mixed),
                value_type.unwrap_or_else(TUnion::mixed),
            ))
        }
        TAtomic::TObjectIntersection { types } => {
            let mut key_type: Option<TUnion> = None;
            let mut value_type: Option<TUnion> = None;

            for nested_atomic in types {
                let Some((nested_key_type, nested_value_type)) =
                    extract_unpacked_iterable_params(analyzer, nested_atomic)
                else {
                    continue;
                };

                key_type = Some(if let Some(existing) = key_type {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });

                value_type = Some(if let Some(existing) = value_type {
                    combine_union_types(&existing, &nested_value_type, false)
                } else {
                    nested_value_type
                });
            }

            Some((
                key_type.unwrap_or_else(TUnion::mixed),
                value_type.unwrap_or_else(TUnion::mixed),
            ))
        }
        _ => None,
    }
}

pub(crate) fn get_keyed_array_generic_params(
    properties: &FxHashMap<ArrayKey, TUnion>,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,
) -> (TUnion, TUnion) {
    let mut key_type = fallback_key_type.cloned().unwrap_or_else(TUnion::nothing);
    let mut value_type = fallback_value_type.cloned().unwrap_or_else(TUnion::nothing);

    for (key, property_type) in properties.iter() {
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

        value_type = if value_type.is_nothing() {
            property_type.clone()
        } else {
            combine_union_types(&value_type, property_type, false)
        };
    }

    (
        if key_type.is_nothing() {
            TUnion::array_key()
        } else {
            key_type
        },
        if value_type.is_nothing() {
            TUnion::mixed()
        } else {
            value_type
        },
    )
}

fn merge_property_types_for_spread(
    analyzer: &StatementsAnalyzer<'_>,
    properties: &mut FxHashMap<ArrayKey, TUnion>,
    spread_key_type: &TUnion,
    spread_value_type: &TUnion,
) {
    let mut keys_to_update = Vec::new();

    for (property_key, property_value) in properties.iter() {
        let literal_key_type = array_key_to_union(property_key);
        let mut comparison_result = TypeComparisonResult::new();

        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &literal_key_type,
            spread_key_type,
            false,
            false,
            &mut comparison_result,
        ) {
            keys_to_update.push((property_key.clone(), property_value.clone()));
        }
    }

    for (property_key, property_value) in keys_to_update {
        properties.insert(
            property_key,
            combine_union_types(&property_value, spread_value_type, false),
        );
    }
}

fn array_key_to_union(key: &ArrayKey) -> TUnion {
    match key {
        ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
        ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
            value: value.clone(),
        }),
    }
}

fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
    if name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
    {
        return true;
    }

    analyzer.codebase.get_class(name).is_some_and(|class_info| {
        class_info.interfaces.contains(&StrId::TRAVERSABLE)
            || class_info
                .all_parent_interfaces
                .iter()
                .any(|interface| *interface == StrId::TRAVERSABLE)
    })
}

fn union_contains_only_array_key(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let mut comparison_result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        union,
        &TUnion::array_key(),
        false,
        false,
        &mut comparison_result,
    )
}

fn normalize_array_creation_key_union(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    key_type: &TUnion,
    key_expr: &Expression<'_>,
) -> TUnion {
    let mut good_types = Vec::new();
    let mut saw_mixed = false;
    let mut saw_invalid = false;

    for atomic in &key_type.types {
        match atomic {
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                saw_mixed = true;
                good_types.push(TAtomic::TArrayKey);
            }
            TAtomic::TNull => {
                good_types.push(TAtomic::TLiteralString {
                    value: String::new(),
                });
            }
            TAtomic::TLiteralString { value } => {
                if let Some(int_value) = literal_array_key_to_int(value) {
                    good_types.push(TAtomic::TLiteralInt { value: int_value });
                } else {
                    good_types.push(atomic.clone());
                }
            }
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TArrayKey
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TDependentGetClass { .. }
            | TAtomic::TDependentGetType { .. }
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. } => {
                good_types.push(atomic.clone());
            }
            TAtomic::TFalse => {
                saw_invalid = true;
                good_types.push(TAtomic::TLiteralInt { value: 0 });
            }
            TAtomic::TTrue => {
                saw_invalid = true;
                good_types.push(TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TBool => {
                saw_invalid = true;
                good_types.push(TAtomic::TLiteralInt { value: 0 });
                good_types.push(TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TLiteralFloat { value } => {
                saw_invalid = true;
                good_types.push(TAtomic::TLiteralInt {
                    value: *value as i64,
                });
            }
            TAtomic::TFloat => {
                saw_invalid = true;
                good_types.push(TAtomic::TInt);
            }
            _ => {
                saw_invalid = true;
                good_types.push(TAtomic::TArrayKey);
            }
        }
    }

    if saw_mixed {
        emit_issue(
            analyzer,
            analysis_data,
            key_expr.span().start.offset,
            key_expr.span().end.offset,
            IssueKind::MixedArrayOffset,
            "Cannot create mixed offset, expecting array-key".to_string(),
        );
    }

    if saw_invalid {
        emit_issue(
            analyzer,
            analysis_data,
            key_expr.span().start.offset,
            key_expr.span().end.offset,
            IssueKind::InvalidArrayOffset,
            format!(
                "Cannot create offset of type {}, expecting array-key",
                key_type.get_id(Some(analyzer.interner))
            ),
        );
    }

    if good_types.is_empty() {
        TUnion::array_key()
    } else {
        TUnion::from_types(type_combiner::combine(good_types, false))
    }
}

fn get_single_literal_array_key(key_type: &TUnion) -> Option<ArrayKey> {
    if key_type.types.len() != 1 {
        return None;
    }

    match key_type.types.first()? {
        TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => Some(ArrayKey::String(value.clone())),
        _ => None,
    }
}

fn literal_array_key_to_int(literal_array_key: &str) -> Option<i64> {
    if literal_array_key.trim() != literal_array_key {
        return None;
    }

    if literal_array_key.starts_with('+') {
        return None;
    }

    let parsed = literal_array_key.parse::<i64>().ok()?;

    if parsed.to_string() == literal_array_key {
        Some(parsed)
    } else {
        None
    }
}

fn key_to_string(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => value.clone(),
    }
}

fn emit_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start: u32,
    end: u32,
    kind: IssueKind,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(start);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        start,
        end,
        line,
        col,
    ));
}

/// Analyze a list() expression (used as LHS of assignment).
pub fn analyze_list(
    analyzer: &StatementsAnalyzer<'_>,
    list: &List<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // list() is typically used on the LHS of assignment for destructuring
    // When analyzed as an expression, it represents the pattern being matched

    for element in list.elements.iter() {
        match element {
            ArrayElement::Value(val) => {
                // This is a variable or nested list that will receive a value
                let elem_span = val.value.span();
                let elem_pos: Pos = (elem_span.start.offset, elem_span.end.offset);
                let _inner_pos =
                    expression_analyzer::analyze(analyzer, val.value, analysis_data, context);
                analysis_data.expr_types.insert(elem_pos, Rc::new(TUnion::mixed()));
            }
            ArrayElement::KeyValue(kv) => {
                // Keyed destructuring: list('key' => $var)
                let _key_pos =
                    expression_analyzer::analyze(analyzer, kv.key, analysis_data, context);
                let elem_span = kv.value.span();
                let elem_pos: Pos = (elem_span.start.offset, elem_span.end.offset);
                let _inner_pos =
                    expression_analyzer::analyze(analyzer, kv.value, analysis_data, context);
                analysis_data.expr_types.insert(elem_pos, Rc::new(TUnion::mixed()));
            }
            ArrayElement::Missing(_) => {
                // Skipped element
            }
            ArrayElement::Variadic(_) => {
                // Variadic in list() - this is a syntax error in PHP
            }
        }
    }

    // The list expression itself has an array type
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
}

/// Combine multiple types into a union.
fn combine_types(types: Vec<TUnion>) -> TUnion {
    if types.is_empty() {
        return TUnion::mixed();
    }

    let mut result = types[0].clone();
    for t in &types[1..] {
        result = combine_union_types(&result, t, false);
    }
    result
}
