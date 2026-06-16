//! Unset statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unset::Unset;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{ArrayKey, Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    unset_stmt: &Unset<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    for value in unset_stmt.values.iter() {
        let was_inside_unset = context.inside_unset;
        context.inside_unset = true;
        let _ = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        context.inside_unset = was_inside_unset;

        if let Expression::Variable(Variable::Direct(direct)) = value.unparenthesized() {
            let var_id = VarName::new(direct.name);
            context.remove_var(&var_id);
            continue;
        }

        let Expression::Access(access) = value else {
            if let Expression::ArrayAccess(array_access) = value {
                // `unset($arr[$k])` mutates `$arr` — the base variable's
                // dataflow is consumed (Psalm registers the unset as a use).
                // Capture parents before handle_array_unset rebuilds the type.
                if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
                    && let Expression::Variable(Variable::Direct(base_direct)) =
                        array_access.array.unparenthesized()
                    && let Some(base_type) = context.get_var_type(base_direct.name)
                {
                    let span = array_access.array.span();
                    let unset_sink = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
                        crate::data_flow::make_data_flow_node_position(
                            analyzer,
                            (span.start.offset, span.end.offset),
                        ),
                    );
                    let parent_nodes = base_type.parent_nodes.clone();
                    for parent_node in &parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            &parent_node.id,
                            &unset_sink.id,
                            pzoom_code_info::PathKind::Default,
                            vec![],
                            vec![],
                        );
                    }
                    analysis_data.data_flow_graph.add_node(unset_sink);
                }

                handle_array_unset(array_access, context);
            }
            continue;
        };

        let Access::Property(property_access) = access else {
            continue;
        };

        let ClassLikeMemberSelector::Identifier(property_name) = &property_access.property else {
            continue;
        };

        let object_span = property_access.object.span();
        let Some(object_type) = analysis_data
            .expr_types
            .get(&(object_span.start.offset, object_span.end.offset))
            .cloned()
        else {
            continue;
        };

        let property_id = analyzer.interner.intern(property_name.value);

        for atomic in &object_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };

            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                continue;
            };

            let Some(prop_info) = class_info.properties.get(&property_id) else {
                continue;
            };

            if prop_info.is_readonly || class_info.is_immutable {
                let (line, col) = analyzer.get_line_column(value.span().start.offset);
                let class_name = analyzer.interner.lookup(*name);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InaccessibleProperty,
                    format!(
                        "Cannot unset restricted property {}::${}",
                        class_name, property_name.value
                    ),
                    analyzer.file_path,
                    value.span().start.offset,
                    value.span().end.offset,
                    line,
                    col,
                ));
                break;
            }
        }
    }

    Ok(())
}

fn handle_array_unset(array_access: &ArrayAccess<'_>, context: &mut BlockContext) {
    let Some(base_var_name) = get_root_array_base_var_name(array_access.array) else {
        return;
    };

    let unset_key = get_literal_array_key(array_access.index);

    let base_var_id = VarName::new(base_var_name);
    let Some(existing_type) = context.get_var_type(&base_var_id).cloned() else {
        return;
    };

    // For nested unsets (`$a['x'][$k]`), clear tracked array-path assertions/types for the
    // root variable. This mirrors Psalm's behavior of invalidating prior shape assumptions.
    if !matches!(
        array_access.array.unparenthesized(),
        Expression::Variable(Variable::Direct(_))
    ) {
        // Psalm's UnsetAnalyzer demotes the immediate receiver's type in place
        // (its root_var_id is the dim-fetch receiver, e.g. `$a['x']`): a
        // non-empty array becomes possibly empty, shapes lose the key. Keeping
        // the demoted entry (rather than just dropping it) lets loop merging
        // see the change, so a later `empty()` check stays live.
        let receiver_key = crate::expression_identifier::get_expression_var_key(array_access.array);
        let receiver_type = receiver_key
            .as_ref()
            .and_then(|key| context.get_var_type(key).cloned());

        clear_array_path_types_for_base_var(context, base_var_name);
        remove_var_clauses_from_context(context, base_var_name);
        // Mark the root variable as changed so branch merging can invalidate
        // stale path-based clauses from sibling branches.
        context.set_var_type(base_var_id, existing_type);

        if let (Some(receiver_key), Some(receiver_type)) = (receiver_key, receiver_type) {
            let demoted = demote_array_type_after_unset(&receiver_type, unset_key.as_ref());
            context.set_var_type(receiver_key, demoted);
        }
        return;
    }

    let demoted = demote_array_type_after_unset(&existing_type, unset_key.as_ref());
    context.set_var_type(base_var_id, demoted);
    clear_array_path_types_for_base_var(context, base_var_name);
    remove_var_clauses_from_context(context, base_var_name);
}

/// Rebuild an array type after `unset($arr[<key>])` (the per-atomic demotion in
/// Psalm's UnsetAnalyzer): shapes lose the key (or mark every entry
/// possibly-undefined for a dynamic key), non-empty arrays/lists become possibly
/// empty, and list contiguity is broken.
fn demote_array_type_after_unset(existing_type: &TUnion, unset_key: Option<&ArrayKey>) -> TUnion {
    let mut updated_types = Vec::with_capacity(existing_type.types.len());

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                if let Some(unset_key) = unset_key {
                    let mut next_properties = (**properties).clone();

                    // Removing a non-last entry from a list (or any entry of an
                    // unsealed list) breaks contiguity (Psalm's UnsetAnalyzer).
                    let mut next_is_list = *is_list;
                    if fallback_value_type.is_some() {
                        next_is_list = false;
                    } else if next_properties.contains_key(unset_key)
                        && *unset_key != ArrayKey::Int(next_properties.len() as i64 - 1)
                    {
                        next_is_list = false;
                    }
                    next_properties.remove(unset_key);

                    if next_properties.is_empty() {
                        // No known entries left: an unsealed shape degrades to
                        // its fallback array, a sealed one to the empty array.
                        if let (Some(fallback_key), Some(fallback_value)) =
                            (fallback_key_type, fallback_value_type)
                        {
                            updated_types.push(TAtomic::TArray {
                                key_type: fallback_key.clone(),
                                value_type: fallback_value.clone(),
                            });
                        } else {
                            updated_types.push(TAtomic::TArray {
                                key_type: Box::new(TUnion::nothing()),
                                value_type: Box::new(TUnion::nothing()),
                            });
                        }
                    } else {
                        updated_types.push(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(next_properties),
                            is_list: next_is_list,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        });
                    }
                } else {
                    // Unknown offset: every known entry may have been the one
                    // removed — Psalm marks them all possibly-undefined and the
                    // shape stops being a list.
                    let mut next_properties = (**properties).clone();
                    for property_type in next_properties.values_mut() {
                        property_type.possibly_undefined = true;
                    }

                    updated_types.push(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(next_properties),
                        is_list: false,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                }
            }
            // Non-emptiness never survives an unset of an arbitrary offset.
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => {
                updated_types.push(TAtomic::TArray {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                });
            }
            TAtomic::TNonEmptyList { value_type } | TAtomic::TList { value_type } => {
                // Unsetting an offset breaks list contiguity (Psalm sets
                // is_list = false), so degrade to an int-keyed array.
                updated_types.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::new(TAtomic::TInt)),
                    value_type: value_type.clone(),
                });
            }
            TAtomic::TNonEmptyMixed => updated_types.push(TAtomic::TMixed),
            _ => updated_types.push(atomic.clone()),
        }
    }

    let combined = type_combiner::combine(updated_types, false);
    TUnion::from_types(combined)
}

fn clear_array_path_types_for_base_var(context: &mut BlockContext, var_name: &str) {
    let prefix = format!("{var_name}[");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.starts_with(&prefix))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

fn remove_var_clauses_from_context(context: &mut BlockContext, assigned_var_name: &str) {
    context.remove_var_name_from_conflicting_clauses(assigned_var_name);
}

fn get_literal_array_key(expr: &Expression<'_>) -> Option<ArrayKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(ArrayKey::Int),
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| ArrayKey::String(value.to_string())),
        _ => None,
    }
}

fn get_root_array_base_var_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name),
        Expression::ArrayAccess(array_access) => get_root_array_base_var_name(array_access.array),
        _ => None,
    }
}
