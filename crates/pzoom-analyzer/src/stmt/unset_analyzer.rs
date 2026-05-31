//! Unset statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unset::Unset;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::algebra::ClauseKey;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{ArrayKey, Issue, IssueKind, TAtomic, TUnion, combine_union_types};

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
            let var_id = analyzer.interner.intern(direct.name);
            context.remove_var(var_id);
            continue;
        }

        let Expression::Access(access) = value else {
            if let Expression::ArrayAccess(array_access) = value {
                handle_array_unset(analyzer, array_access, context);
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
        let Some(object_type) =
            analysis_data.get_expr_type((object_span.start.offset, object_span.end.offset))
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

fn handle_array_unset(
    analyzer: &StatementsAnalyzer<'_>,
    array_access: &ArrayAccess<'_>,
    context: &mut BlockContext,
) {
    let Some(base_var_name) = get_root_array_base_var_name(array_access.array) else {
        return;
    };

    let unset_key = get_literal_array_key(array_access.index);

    let base_var_id = analyzer.interner.intern(base_var_name);
    let Some(existing_type) = context.get_var_type(base_var_id).cloned() else {
        return;
    };

    // For nested unsets (`$a['x'][$k]`), clear tracked array-path assertions/types for the
    // root variable. This mirrors Psalm's behavior of invalidating prior shape assumptions.
    if !matches!(
        array_access.array.unparenthesized(),
        Expression::Variable(Variable::Direct(_))
    ) {
        clear_array_path_types_for_base_var(analyzer, context, base_var_name);
        remove_var_clauses_from_context(context, base_var_name);
        // Mark the root variable as changed so branch merging can invalidate
        // stale path-based clauses from sibling branches.
        context.set_var_type(base_var_id, existing_type);
        return;
    }

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
                if let Some(unset_key) = unset_key.as_ref() {
                    let mut next_properties = properties.clone();
                    next_properties.remove(unset_key);

                    updated_types.push(TAtomic::TKeyedArray {
                        properties: next_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    });
                } else {
                    let mut combined_value: Option<TUnion> = fallback_value_type
                        .as_deref()
                        .map(|value_type| value_type.clone());

                    for property_type in properties.values() {
                        combined_value = Some(match combined_value {
                            Some(existing) => {
                                combine_union_types(&existing, property_type, false)
                            }
                            None => property_type.clone(),
                        });
                    }

                    let combined_key = fallback_key_type
                        .as_deref()
                        .map(|key_type| key_type.clone())
                        .unwrap_or_else(TUnion::array_key);

                    updated_types.push(TAtomic::TArray {
                        key_type: Box::new(combined_key),
                        value_type: Box::new(combined_value.unwrap_or_else(TUnion::mixed)),
                    });
                }
            }
            _ => updated_types.push(atomic.clone()),
        }
    }

    let combined = type_combiner::combine(updated_types, false);
    context.set_var_type(base_var_id, TUnion::from_types(combined));
    clear_array_path_types_for_base_var(analyzer, context, base_var_name);
    remove_var_clauses_from_context(context, base_var_name);
}

fn clear_array_path_types_for_base_var(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
) {
    let prefix = format!("{var_name}[");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            analyzer
                .interner
                .lookup(*var_id)
                .as_ref()
                .starts_with(&prefix)
        })
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

fn remove_var_clauses_from_context(context: &mut BlockContext, assigned_var_name: &str) {
    context.clauses.retain(|clause| {
        !clause
            .possibilities
            .keys()
            .any(|key| matches_assignment_target_key(key, assigned_var_name))
    });
}

fn matches_assignment_target_key(key: &ClauseKey, assigned_var_name: &str) -> bool {
    match key {
        ClauseKey::Name(name) => {
            name == assigned_var_name
                || name.starts_with(&format!("{}[", assigned_var_name))
                || name.starts_with(&format!("{}->", assigned_var_name))
                || name.contains(&format!("[{}]", assigned_var_name))
        }
        ClauseKey::Range(..) => false,
    }
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
