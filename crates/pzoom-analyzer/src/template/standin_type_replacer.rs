//! Template standin type replacement helpers.
//!
//! Mirrors Psalm's TemplateStandinTypeReplacer / hakana-core's standin_type_replacer:
//! replaces template parameters in a type with the concrete types inferred for them
//! (or their declared defaults), expanding `class-string<T>`, indexed-access, and
//! template-param atomics along the way.

use pzoom_code_info::{GenericParent, TAtomic, TUnion, TemplateResult, combine_union_types};
use pzoom_str::StrId;

use crate::statements_analyzer::StatementsAnalyzer;
use crate::template::{
    lower_bounds_get, lower_bounds_get_by_name, lower_bounds_insert_combined,
    template_types_contains_name, template_types_entity_for_name, template_types_get,
    template_types_get_by_name,
};
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Replaces template params in a union with inferred/default concrete types.
pub fn replace(union_type: &TUnion, template_result: &TemplateResult) -> TUnion {
    substitute_templates_in_union(union_type, template_result)
}

pub(crate) fn substitute_templates_in_union(
    union: &TUnion,
    template_result: &TemplateResult,
) -> TUnion {
    let mut replaced_types = Vec::new();
    let mut bound_ignore_nullable = false;
    let mut bound_ignore_falsable = false;

    for atomic in &union.types {
        if let Some(indexed_access_union) =
            resolve_indexed_access_template_union(atomic, template_result)
        {
            for replacement_atomic in indexed_access_union.types {
                if !replaced_types.contains(&replacement_atomic) {
                    replaced_types.push(replacement_atomic);
                }
            }
            continue;
        }

        if let Some(key_value_of_union) =
            resolve_template_key_value_of_union(atomic, template_result)
        {
            for replacement_atomic in key_value_of_union.types {
                if !replaced_types.contains(&replacement_atomic) {
                    replaced_types.push(replacement_atomic);
                }
            }
            continue;
        }

        if let Some(properties_of) = resolve_template_properties_of(atomic, template_result) {
            if !replaced_types.contains(&properties_of) {
                replaced_types.push(properties_of);
            }
            continue;
        }

        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if let Some(class_replacement) =
                    resolve_class_string_template_replacement(as_type, template_result)
                {
                    for replacement_atomic in class_replacement.types {
                        let class_string_atomic = TAtomic::TClassString {
                            as_type: Some(Box::new(replacement_atomic)),
                        };
                        if !replaced_types.contains(&class_string_atomic) {
                            replaced_types.push(class_string_atomic);
                        }
                    }
                    continue;
                }

                let replaced_atomic = substitute_templates_in_atomic(atomic, template_result);
                if !replaced_types.contains(&replaced_atomic) {
                    replaced_types.push(replaced_atomic);
                }
            }
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => {
                let replacement = lower_bounds_get(template_result, *name, *defining_entity)
                    .or_else(|| {
                        template_types_get(template_result, *name, *defining_entity)
                            .map(|mapped_type| (**mapped_type).clone())
                    })
                    .unwrap_or_else(|| (**as_type).clone());

                // The inferred bound's ignore flags travel with it into the
                // substituted union (e.g. a falsable-but-ignored array element
                // bound to a callable's param template).
                bound_ignore_nullable |= replacement.ignore_nullable_issues;
                bound_ignore_falsable |= replacement.ignore_falsable_issues;

                for replacement_atomic in replacement.types {
                    if !replaced_types.contains(&replacement_atomic) {
                        replaced_types.push(replacement_atomic);
                    }
                }
            }
            _ => {
                let replaced_atomic = substitute_templates_in_atomic(atomic, template_result);
                if !replaced_types.contains(&replaced_atomic) {
                    replaced_types.push(replaced_atomic);
                }
            }
        }
    }

    let mut result = if replaced_types.is_empty() {
        TUnion::mixed()
    } else {
        TUnion::from_types(replaced_types)
    };
    result.from_docblock = union.from_docblock;
    result.is_resolved = union.is_resolved;
    result.parent_nodes = union.parent_nodes.clone();
    result.ignore_nullable_issues = union.ignore_nullable_issues || bound_ignore_nullable;
    result.ignore_falsable_issues = union.ignore_falsable_issues || bound_ignore_falsable;
    result
}

fn resolve_class_string_template_replacement(
    as_type: &TAtomic,
    template_result: &TemplateResult,
) -> Option<TUnion> {
    match as_type {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        }
        | TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            ..
        } => lower_bounds_get(template_result, *name, *defining_entity).or_else(|| {
            template_types_get(template_result, *name, *defining_entity)
                .map(|mapped_type| (**mapped_type).clone())
        }),
        TAtomic::TNamedObject {
            name,
            type_params: None,
            ..
        } => lower_bounds_get_by_name(template_result, *name).or_else(|| {
            template_types_get_by_name(template_result, *name)
                .map(|mapped_type| (**mapped_type).clone())
        }),
        _ => None,
    }
}

/// Resolve a deferred `key-of<T>` / `value-of<T>` once `T` has a bound replacement,
/// producing the keys (resp. values) of that replacement. Returns `None` when the
/// template is still unbound, leaving the deferred atomic in place.
fn resolve_template_key_value_of_union(
    atomic: &TAtomic,
    template_result: &TemplateResult,
) -> Option<TUnion> {
    let (param_name, defining_entity, is_key_of) = match atomic {
        TAtomic::TTemplateKeyOf {
            param_name,
            defining_entity,
            ..
        } => (*param_name, *defining_entity, true),
        TAtomic::TTemplateValueOf {
            param_name,
            defining_entity,
            ..
        } => (*param_name, *defining_entity, false),
        _ => return None,
    };

    // Only resolve against a concrete inferred binding (the lower bounds),
    // never against a template's declared bound. During body analysis only the bound is
    // known and `key-of<T>` must stay deferred so a concrete key cannot satisfy it.
    let replacement = lower_bounds_get(template_result, param_name, defining_entity)?;

    let inferred_only = TemplateResult {
        lower_bounds: template_result.lower_bounds.clone(),
        ..Default::default()
    };
    let resolved = substitute_templates_in_union(&replacement, &inferred_only);

    Some(if is_key_of {
        pzoom_code_info::ttype::get_key_of_union(&resolved)
    } else {
        pzoom_code_info::ttype::get_value_of_union(&resolved)
    })
}

/// Resolve a deferred `properties-of<T>` once `T` is bound to a concrete class, turning
/// it into a `TPropertiesOf` that the type expander later expands to a shape. Mirrors
/// Psalm's `TTemplatePropertiesOf::replaceTemplateTypesWithArgTypes`.
fn resolve_template_properties_of(
    atomic: &TAtomic,
    template_result: &TemplateResult,
) -> Option<TAtomic> {
    let TAtomic::TTemplatePropertiesOf {
        param_name,
        defining_entity,
        visibility_filter,
    } = atomic
    else {
        return None;
    };

    let replacement = lower_bounds_get(template_result, *param_name, *defining_entity)?;
    let classlike_name = single_named_object_name(&replacement)?;

    Some(TAtomic::TPropertiesOf {
        classlike_name,
        visibility_filter: *visibility_filter,
    })
}

fn single_named_object_name(union: &TUnion) -> Option<StrId> {
    match union.get_single()? {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TObjectIntersection { types } => types.iter().find_map(|atomic| {
            if let TAtomic::TNamedObject { name, .. } = atomic {
                Some(*name)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn resolve_indexed_access_template_union(
    atomic: &TAtomic,
    template_result: &TemplateResult,
) -> Option<TUnion> {
    let TAtomic::TNamedObject {
        name,
        type_params: Some(type_params),
        ..
    } = atomic
    else {
        return None;
    };

    if *name != StrId::PZOOM_INDEXED_ACCESS || type_params.len() != 2 {
        return None;
    }

    let array_type = substitute_templates_in_union(&type_params[0], template_result);

    Some(extract_indexed_access_value_type(&array_type))
}

fn extract_indexed_access_value_type(array_type: &TUnion) -> TUnion {
    let mut value_type = TUnion::nothing();

    for atomic in &array_type.types {
        let extracted = match atomic {
            TAtomic::TIterable { value_type, .. } => Some((**value_type).clone()),
            // The unified array atomic: the value type combines every known
            // entry's value with the typed fallback `params.1` (covers generic
            // `array<K, V>` / `list<V>` and shapes alike).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => Some(extract_keyed_array_value_type(
                known_values,
                params.as_deref().map(|(_, value)| value),
            )),
            TAtomic::TTemplateParam { as_type, .. } => {
                Some(extract_indexed_access_value_type(as_type))
            }
            _ => None,
        };

        let Some(extracted) = extracted else {
            continue;
        };

        value_type = if value_type.is_nothing() {
            extracted
        } else {
            combine_union_types(&value_type, &extracted, false)
        };
    }

    if value_type.is_nothing() {
        TUnion::mixed()
    } else {
        value_type
    }
}

fn substitute_templates_in_atomic(atomic: &TAtomic, template_result: &TemplateResult) -> TAtomic {
    match atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            let replacement = lower_bounds_get(template_result, *name, *defining_entity)
                .or_else(|| {
                    template_types_get(template_result, *name, *defining_entity)
                        .map(|mapped_type| (**mapped_type).clone())
                })
                .unwrap_or_else(|| (**as_type).clone());

            replacement
                .get_single()
                .cloned()
                .unwrap_or_else(|| atomic.clone())
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => TAtomic::TIterable {
            key_type: Box::new(substitute_templates_in_union(key_type, template_result)),
            value_type: Box::new(substitute_templates_in_union(value_type, template_result)),
        },
        // The unified array atomic: substitute templates in every known-entry
        // value and the typed fallback `params`, preserving the shape's flags and
        // each entry's possibly-undefined bool (`rebuilt_array` keeps them
        // without re-normalising).
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let mut new_known_values = rustc_hash::FxHashMap::default();
            for (key, (possibly_undefined, value)) in known_values.iter() {
                new_known_values.insert(
                    key.clone(),
                    (
                        *possibly_undefined,
                        substitute_templates_in_union(value, template_result),
                    ),
                );
            }
            atomic.rebuilt_array(
                std::sync::Arc::new(new_known_values),
                params.as_ref().map(|params| {
                    Box::new((
                        substitute_templates_in_union(&params.0, template_result),
                        substitute_templates_in_union(&params.1, template_result),
                    ))
                }),
            )
        }
        TAtomic::TNamedObject {
            name,
            type_params: None,
            ..
        } if template_result.lower_bounds.contains_key(name)
            || template_types_contains_name(template_result, *name) =>
        {
            let replacement = lower_bounds_get_by_name(template_result, *name)
                .or_else(|| {
                    template_types_get_by_name(template_result, *name)
                        .map(|mapped_type| (**mapped_type).clone())
                })
                .unwrap_or_else(TUnion::mixed);

            replacement
                .get_single()
                .cloned()
                .unwrap_or_else(|| atomic.clone())
        }
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static,
            remapped_params,
        } => TAtomic::TNamedObject {
            name: *name,
            type_params: type_params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| substitute_templates_in_union(param, template_result))
                    .collect()
            }),
            is_static: *is_static,
            remapped_params: *remapped_params,
        },
        TAtomic::TObjectWithProperties {
            properties,
            is_stringable,
            is_invokable,
        } => TAtomic::TObjectWithProperties {
            properties: properties
                .iter()
                .map(|(key, (possibly_undefined, value))| {
                    (
                        key.clone(),
                        (
                            *possibly_undefined,
                            substitute_templates_in_union(value, template_result),
                        ),
                    )
                })
                .collect(),
            is_stringable: *is_stringable,
            is_invokable: *is_invokable,
        },
        TAtomic::TObjectIntersection { types } => {
            let mut replaced_types = Vec::with_capacity(types.len());
            for nested_type in types {
                let replaced_type = substitute_templates_in_atomic(nested_type, template_result);
                if !replaced_types.contains(&replaced_type) {
                    replaced_types.push(replaced_type);
                }
            }

            if replaced_types.len() == 1 {
                replaced_types.into_iter().next().unwrap()
            } else {
                TAtomic::TObjectIntersection {
                    types: replaced_types,
                }
            }
        }
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: substitute_templates_in_union(
                            &param.param_type,
                            template_result,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(return_type, template_result))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: substitute_templates_in_union(
                            &param.param_type,
                            template_result,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(return_type, template_result))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type
                .as_ref()
                .map(|as_type| Box::new(substitute_templates_in_atomic(as_type, template_result))),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            if let Some(replacement) = lower_bounds_get(template_result, *name, *defining_entity)
                .or_else(|| {
                    template_types_get(template_result, *name, *defining_entity)
                        .map(|mapped_type| (**mapped_type).clone())
                })
                && let Some(single_replacement) = replacement.get_single()
            {
                single_replacement.clone()
            } else {
                TAtomic::TTemplateParamClass {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(substitute_templates_in_atomic(as_type, template_result)),
                }
            }
        }
        _ => atomic.clone(),
    }
}

pub(crate) fn extract_keyed_array_value_type(
    known_values: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, (bool, TUnion)>,
    fallback_value_type: Option<&TUnion>,
) -> TUnion {
    let mut value_type = fallback_value_type.cloned().unwrap_or_else(TUnion::nothing);

    for (_possibly_undefined, property_type) in known_values.values() {
        value_type = if value_type.is_nothing() {
            property_type.clone()
        } else {
            combine_union_types(&value_type, property_type, false)
        };
    }

    if value_type.is_nothing() {
        TUnion::mixed()
    } else {
        value_type
    }
}

pub(crate) fn infer_template_replacements_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
    arg_type: &TUnion,
    template_result: &mut TemplateResult,
) {
    // When a parameter union mixes explicit non-template branches with top-level template
    // branches (e.g. `T|int`), infer templates from the unmatched argument remainder.
    let template_branches: Vec<&TAtomic> = param_type
        .types
        .iter()
        .filter(|atomic| is_top_level_template_atomic(atomic))
        .collect();
    let non_template_branches: Vec<&TAtomic> = param_type
        .types
        .iter()
        .filter(|atomic| !is_top_level_template_atomic(atomic))
        .collect();

    if !template_branches.is_empty() && !non_template_branches.is_empty() {
        let mut unmatched_arg_atomics: Vec<&TAtomic> = Vec::new();

        'arg_atomic: for arg_atomic in &arg_type.types {
            for non_template_branch in &non_template_branches {
                let mut comparison_result = TypeComparisonResult::new();
                if union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &TUnion::new((*arg_atomic).clone()),
                    &TUnion::new((*non_template_branch).clone()),
                    false,
                    false,
                    &mut comparison_result,
                ) {
                    continue 'arg_atomic;
                }
            }

            unmatched_arg_atomics.push(arg_atomic);
        }

        if !unmatched_arg_atomics.is_empty() {
            for template_branch in template_branches {
                for arg_atomic in &unmatched_arg_atomics {
                    infer_template_replacements_from_atomic(
                        analyzer,
                        template_branch,
                        arg_atomic,
                        template_result,
                    );
                }
            }
            return;
        }
    }

    for param_atomic in &param_type.types {
        // A union carrying ignore-nullable/falsable flags binds whole, so the
        // flags survive into the bound (Psalm binds the entire input union as
        // `$generic_param`, flags included); splitting per atomic would shed
        // them.
        if (arg_type.ignore_nullable_issues || arg_type.ignore_falsable_issues)
            && let TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } = param_atomic
        {
            bind_template_param_to_arg_union(
                analyzer,
                *name,
                *defining_entity,
                as_type,
                arg_type,
                template_result,
            );
            continue;
        }
        for arg_atomic in &arg_type.types {
            infer_template_replacements_from_atomic(
                analyzer,
                param_atomic,
                arg_atomic,
                template_result,
            );
        }
    }
}

fn bind_template_param_to_arg_union(
    analyzer: &StatementsAnalyzer<'_>,
    name: StrId,
    defining_entity: GenericParent,
    as_type: &TUnion,
    arg_union: &TUnion,
    template_result: &mut TemplateResult,
) {
    let bound = template_types_get(template_result, name, defining_entity)
        .map(|mapped_type| (**mapped_type).clone())
        .unwrap_or_else(|| as_type.clone());

    bind_template_replacement(
        analyzer,
        name,
        defining_entity,
        arg_union,
        &bound,
        template_result,
    );
}

fn infer_template_replacements_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    param_atomic: &TAtomic,
    arg_atomic: &TAtomic,
    template_result: &mut TemplateResult,
) {
    // Hakana's `find_matching_atomic_types_for_template`: a type-variable
    // argument matches a same-named type-variable parameter as itself, and
    // anything else as mixed ("todo we can probably do better here").
    if let TAtomic::TTypeVariable { name: arg_name } = arg_atomic {
        if let TAtomic::TTypeVariable { name: param_name } = param_atomic
            && arg_name == param_name
        {
            return;
        }

        infer_template_replacements_from_atomic(
            analyzer,
            param_atomic,
            &TAtomic::TMixed,
            template_result,
        );
        return;
    }

    if let TAtomic::TTemplateParam {
        as_type: arg_as_type,
        ..
    } = arg_atomic
    {
        if let TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type: param_as_type,
        } = param_atomic
        {
            let bound = template_types_get(template_result, *name, *defining_entity)
                .map(|mapped_type| (**mapped_type).clone())
                .unwrap_or_else(|| (**param_as_type).clone());
            let arg_union = TUnion::new(arg_atomic.clone());
            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                &bound,
                template_result,
            );
            infer_template_replacements_from_union(analyzer, &bound, arg_as_type, template_result);
            return;
        }

        infer_template_replacements_from_union(
            analyzer,
            &TUnion::new(param_atomic.clone()),
            arg_as_type,
            template_result,
        );
        return;
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = arg_atomic {
        infer_template_replacements_from_atomic(analyzer, param_atomic, as_type, template_result);
        return;
    }

    match param_atomic {
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            let bound = template_types_get(template_result, *name, *defining_entity)
                .map(|mapped_type| (**mapped_type).clone())
                .unwrap_or_else(|| TUnion::new((**as_type).clone()));
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                &bound,
                template_result,
            );

            infer_template_replacements_from_union(analyzer, &bound, &arg_union, template_result);
        }
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            let bound = template_types_get(template_result, *name, *defining_entity)
                .map(|mapped_type| (**mapped_type).clone())
                .unwrap_or_else(|| (**as_type).clone());
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                &bound,
                template_result,
            );

            // Bounds mined from a NOMINAL as-clause (`TIterator as
            // RecursiveIterator<TKey, TValue>`, matched against the arg's
            // @implements chain) are the weakest FALLBACKS: they insert at
            // depth 2 so that bounds from a callable's declared param types
            // (depth 1, contravariant) take precedence in get_relevant_bounds
            // instead of unioning with the iterator's element types.
            // Structural as-clauses (`TArray as array<T>`) stay authoritative
            // at depth 0 — Psalm's usort tests expect the array's element type
            // to win over the (depth-1) callback parameter bound.
            let bound_is_nominal = bound.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TNamedObject {
                        type_params: Some(_),
                        ..
                    }
                )
            });
            let previous_depth = template_result.bound_insertion_depth;
            if bound_is_nominal {
                template_result.bound_insertion_depth = previous_depth + 2;
            }
            infer_template_replacements_from_union(analyzer, &bound, &arg_union, template_result);
            template_result.bound_insertion_depth = previous_depth;
        }
        TAtomic::TNamedObject {
            name,
            type_params: None,
            ..
        } if template_types_contains_name(template_result, *name) => {
            // A template name that parsed as a plain object reference: the
            // declared template types know which entity declared it.
            if let (Some(bound), Some(defining_entity)) = (
                template_types_get_by_name(template_result, *name)
                    .map(|mapped_type| (**mapped_type).clone()),
                template_types_entity_for_name(template_result, *name),
            ) {
                bind_template_replacement(
                    analyzer,
                    *name,
                    defining_entity,
                    &TUnion::new(arg_atomic.clone()),
                    &bound,
                    template_result,
                );
            }
        }
        TAtomic::TClassString {
            as_type: Some(param_as_type),
        } => {
            if let Some(arg_as_type) = extract_class_string_atomic(analyzer, arg_atomic) {
                // `class-string<T>` naming a *concrete* class names `T`
                // exactly (`new ReflectionClass(Foo::class)` is a reflection
                // *of* Foo): record the binding as an equality bound, which
                // pins any type variable minted for `T` to precise bounds. A
                // template-valued class-string (forwarding a generic) is not
                // an exact naming and binds normally.
                if let TAtomic::TTemplateParam {
                    name,
                    defining_entity,
                    as_type,
                } = param_as_type.as_ref()
                    && let TAtomic::TNamedObject {
                        name: equality_classlike,
                        ..
                    } = &arg_as_type
                {
                    let bound = template_types_get(template_result, *name, *defining_entity)
                        .map(|mapped_type| (**mapped_type).clone())
                        .unwrap_or_else(|| (**as_type).clone());
                    let arg_union = TUnion::new(arg_as_type.clone());
                    bind_template_replacement_as_equality(
                        analyzer,
                        *name,
                        *defining_entity,
                        &arg_union,
                        &bound,
                        *equality_classlike,
                        template_result,
                    );

                    // A nominal as-clause carrying sibling templates
                    // (`TWrapper of Wrapper<TInner>`) mines them from the
                    // named class's inheritance chain, exactly as the
                    // TTemplateParam arm below does for non-class-string
                    // arguments.
                    let bound_is_nominal = bound.types.iter().any(|atomic| {
                        matches!(
                            atomic,
                            TAtomic::TNamedObject {
                                type_params: Some(_),
                                ..
                            }
                        )
                    });
                    let previous_depth = template_result.bound_insertion_depth;
                    if bound_is_nominal {
                        template_result.bound_insertion_depth = previous_depth + 2;
                    }
                    infer_template_replacements_from_union(
                        analyzer,
                        &bound,
                        &arg_union,
                        template_result,
                    );
                    template_result.bound_insertion_depth = previous_depth;
                    return;
                }

                infer_template_replacements_from_atomic(
                    analyzer,
                    param_as_type,
                    &arg_as_type,
                    template_result,
                );
            }
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            // A mixed argument binds the nested templates to mixed (Psalm's
            // standin replacer adds a mixed lower bound, the source of
            // `type_coerced_from_mixed`).
            let Some((arg_key_type, arg_value_type)) = extract_array_like_key_value(arg_atomic)
                .or_else(|| {
                    matches!(arg_atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
                        .then(|| (TUnion::mixed(), TUnion::mixed()))
                })
            else {
                return;
            };

            infer_template_replacements_from_union(
                analyzer,
                key_type,
                &arg_key_type,
                template_result,
            );
            infer_template_replacements_from_union(
                analyzer,
                value_type,
                &arg_value_type,
                template_result,
            );
        }
        // The unified array atomic as a *param*. A generic, non-list array
        // (`array<K, V>` / `non-empty-array<K, V>`) infers both key and value
        // templates; a generic list (`list<V>` / `non-empty-list<V>`) infers
        // only the value (the list key is always `int`); a shape param (known
        // entries present) records no bound here, matching the old code which
        // had no `TKeyedArray` param arm.
        TAtomic::TArray {
            known_values,
            params,
            is_list,
            ..
        } if known_values.is_empty() => {
            if *is_list {
                // List param: infer the value template from the argument's value.
                let arg_value_type = match arg_atomic {
                    TAtomic::TArray {
                        known_values: arg_known_values,
                        params: arg_params,
                        ..
                    } => {
                        if arg_known_values.is_empty() {
                            // Generic array/list arg: its value is `params.1`, or
                            // `never` for the empty literal (no typed fallback).
                            arg_params
                                .as_deref()
                                .map(|(_, value)| value.clone())
                                .unwrap_or_else(TUnion::nothing)
                        } else {
                            // Shape arg: combine the fallback value with every
                            // known entry value.
                            let mut combined = arg_params
                                .as_deref()
                                .map(|(_, value)| value.clone())
                                .unwrap_or_else(TUnion::mixed);
                            for (_, property_type) in arg_known_values.values() {
                                combined = combine_union_types(&combined, property_type, false);
                            }
                            combined
                        }
                    }
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed => TUnion::mixed(),
                    _ => return,
                };

                if let Some(params) = params {
                    infer_template_replacements_from_union(
                        analyzer,
                        &params.1,
                        &arg_value_type,
                        template_result,
                    );
                }
            } else {
                // Generic array param: infer both key and value templates. A
                // mixed argument binds the nested templates to mixed.
                let Some((arg_key_type, arg_value_type)) = extract_array_like_key_value(arg_atomic)
                    .or_else(|| {
                        matches!(arg_atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
                            .then(|| (TUnion::mixed(), TUnion::mixed()))
                    })
                else {
                    return;
                };

                if let Some(params) = params {
                    infer_template_replacements_from_union(
                        analyzer,
                        &params.0,
                        &arg_key_type,
                        template_result,
                    );
                    infer_template_replacements_from_union(
                        analyzer,
                        &params.1,
                        &arg_value_type,
                        template_result,
                    );
                }
            }
        }
        // `object{value: T}` params bind T from the argument's matching
        // property — a shape property or a (possibly templated) class
        // property read through the argument's type params.
        TAtomic::TObjectWithProperties {
            properties: param_properties,
            ..
        } => {
            for (key, (_, param_property_type)) in param_properties {
                let arg_property_type = match arg_atomic {
                    TAtomic::TObjectWithProperties {
                        properties: arg_properties,
                        ..
                    } => arg_properties.get(key).map(|(_, value)| value.clone()),
                    TAtomic::TNamedObject {
                        name: arg_name,
                        type_params: arg_type_params,
                        ..
                    } => {
                        let pzoom_code_info::ArrayKey::String(property_name) = key else {
                            continue;
                        };
                        analyzer.codebase.get_class(*arg_name).and_then(|class_info| {
                            let property_id =
                                class_info.property_name_lookup.get(property_name).copied()?;
                            let property_info = class_info.properties.get(&property_id)?;
                            Some(
                                crate::expr::fetch::atomic_property_fetch_analyzer::substitute_class_template_params(
                                    class_info,
                                    arg_type_params.as_deref(),
                                    &property_info
                                        .get_type()
                                        .cloned()
                                        .unwrap_or_else(TUnion::mixed),
                                ),
                            )
                        })
                    }
                    _ => None,
                };
                if let Some(arg_property_type) = arg_property_type {
                    infer_template_replacements_from_union(
                        analyzer,
                        param_property_type,
                        &arg_property_type,
                        template_result,
                    );
                }
            }
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(param_type_params),
            ..
        } => {
            if let TAtomic::TNamedObject {
                name: arg_name,
                type_params: arg_type_params,
                ..
            } = arg_atomic
            {
                if name == arg_name
                    || (is_traversable_template_target(*name)
                        && crate::expr::call::function_call_analyzer::named_object_is_traversable(
                            analyzer, *arg_name,
                        ))
                {
                    if let Some(arg_type_params) = arg_type_params {
                        for (param, arg) in param_type_params.iter().zip(arg_type_params.iter()) {
                            infer_template_replacements_from_union(
                                analyzer,
                                param,
                                arg,
                                template_result,
                            );
                        }
                    }
                } else {
                    infer_named_object_template_replacements_from_extended_params(
                        analyzer,
                        *name,
                        param_type_params,
                        *arg_name,
                        arg_type_params.as_deref(),
                        template_result,
                    );
                }
            }
        }
        TAtomic::TObjectIntersection { types: param_types } => {
            if let TAtomic::TObjectIntersection { types: arg_types } = arg_atomic {
                for param_type in param_types {
                    for arg_type in arg_types {
                        infer_template_replacements_from_atomic(
                            analyzer,
                            param_type,
                            arg_type,
                            template_result,
                        );
                    }
                }
            } else {
                for param_type in param_types {
                    infer_template_replacements_from_atomic(
                        analyzer,
                        param_type,
                        arg_atomic,
                        template_result,
                    );
                }
            }
        }
        TAtomic::TCallable {
            params: Some(param_params),
            return_type: param_return_type,
            ..
        }
        | TAtomic::TClosure {
            params: Some(param_params),
            return_type: param_return_type,
            ..
        } => {
            let (arg_params, arg_return_type) = match arg_atomic {
                TAtomic::TCallable {
                    params: Some(params),
                    return_type,
                    ..
                }
                | TAtomic::TClosure {
                    params: Some(params),
                    return_type,
                    ..
                } => (params, return_type),
                // A signature-less `callable`/`Closure` could return anything:
                // its return binds the param's return templates to mixed
                // (Psalm infers mixed for `call_user_func($plain_callable)`),
                // never leaving them unbound (which would resolve to never
                // and cut control flow after the call).
                TAtomic::TCallable { params: None, .. }
                | TAtomic::TClosure { params: None, .. }
                | TAtomic::TCallableString => {
                    if let Some(param_return_type) = param_return_type {
                        infer_template_replacements_from_union(
                            analyzer,
                            param_return_type,
                            &TUnion::mixed(),
                            template_result,
                        );
                    }
                    return;
                }
                _ => return,
            };

            // A callable's PARAMETER positions are contravariant: a bound mined
            // from them (e.g. usort's `callable(T,T)` matched against an
            // `fn(array,array)` callback, binding `T = array`) is a fallback
            // that must lose to a covariant bound for the same template from
            // another argument — the sorted array's element type. Insert these
            // one level deeper so get_relevant_bounds keeps the array element
            // (depth 0) over the callback parameter (depth 1); nominal
            // as-clause bounds sit deeper still (depth 2).
            let previous_depth = template_result.bound_insertion_depth;
            template_result.bound_insertion_depth = previous_depth + 1;
            for (param_param, arg_param) in param_params.iter().zip(arg_params.iter()) {
                infer_template_replacements_from_union(
                    analyzer,
                    &param_param.param_type,
                    &arg_param.param_type,
                    template_result,
                );
            }
            template_result.bound_insertion_depth = previous_depth;

            // The return type is covariant and stays authoritative (depth 0).
            if let (Some(param_return_type), Some(arg_return_type)) =
                (param_return_type, arg_return_type)
            {
                infer_template_replacements_from_union(
                    analyzer,
                    param_return_type,
                    arg_return_type,
                    template_result,
                );
            }
        }
        _ => {}
    }
}

fn infer_named_object_template_replacements_from_extended_params(
    analyzer: &StatementsAnalyzer<'_>,
    param_class_name: StrId,
    param_type_params: &[TUnion],
    arg_class_name: StrId,
    arg_type_params: Option<&[TUnion]>,
    template_result: &mut TemplateResult,
) {
    let Some(arg_class_info) = analyzer.codebase.get_class(arg_class_name) else {
        return;
    };
    // Psalm's findMatchingAtomicTypesForTemplate only treats a named object as
    // matching a generic container of another class when the extends chain
    // links them — without that, no standin bound is recorded.
    if !arg_class_info
        .template_extended_params
        .contains_key(&param_class_name)
    {
        return;
    }

    // Psalm remaps the input's type params onto the container class
    // (GenericTrait::replaceTypeParamsTemplateTypesWithStandins →
    // getMappedGenericTypeParams), then matches the container's written params
    // against the mapped input params by offset.
    let mapped_arg_type_params = get_mapped_generic_type_params(
        analyzer.codebase,
        arg_class_name,
        arg_type_params,
        param_class_name,
    );

    for (idx, param_type_param) in param_type_params.iter().enumerate() {
        let Some(mapped_arg_type) = mapped_arg_type_params.get(idx) else {
            continue;
        };

        infer_template_replacements_from_union(
            analyzer,
            param_type_param,
            mapped_arg_type,
            template_result,
        );
    }
}

/// Resolves a template atomic through the `@extends`/`@implements` chain to
/// its terminal atomics (the input class's own templates, or concrete types) —
/// a faithful port of Psalm's `Methods::getExtendedTemplatedTypes`.
fn get_extended_templated_types<'a>(
    atomic: &'a TAtomic,
    extends: &'a indexmap::IndexMap<StrId, indexmap::IndexMap<StrId, TUnion>>,
) -> Vec<&'a TAtomic> {
    let mut extra_added_types = Vec::new();

    let TAtomic::TTemplateParam {
        name,
        defining_entity,
        ..
    } = atomic
    else {
        return extra_added_types;
    };

    if let Some(extended_param) = defining_entity
        .classlike_name()
        .and_then(|entity_class| extends.get(&entity_class))
        .and_then(|map| map.get(name))
    {
        for extended_atomic_type in &extended_param.types {
            if matches!(extended_atomic_type, TAtomic::TTemplateParam { .. }) {
                extra_added_types
                    .extend(get_extended_templated_types(extended_atomic_type, extends));
            } else {
                extra_added_types.push(extended_atomic_type);
            }
        }
    } else {
        extra_added_types.push(atomic);
    }

    extra_added_types
}

/// Maps an input object's type params onto a container class's template
/// params — a faithful port of Psalm's
/// `TemplateStandinTypeReplacer::getMappedGenericTypeParams`.
///
/// (Psalm's `iterable` → `Traversable` container aliasing is omitted: pzoom
/// models `iterable` as `TIterable`, which never reaches the named-object
/// container arm.)
pub(crate) fn get_mapped_generic_type_params(
    codebase: &pzoom_code_info::CodebaseInfo,
    input_name: StrId,
    input_given_type_params: Option<&[TUnion]>,
    container_name: StrId,
) -> Vec<TUnion> {
    let input_class_info = codebase.get_class(input_name);

    let mut input_type_params: Vec<TUnion> = if let Some(params) = input_given_type_params {
        params.to_vec()
    } else if let Some(input_class_info) = input_class_info {
        if input_name == container_name {
            input_class_info
                .template_types
                .iter()
                .map(|template_type| {
                    TUnion::new(TAtomic::TTemplateParam {
                        name: template_type.name,
                        defining_entity: template_type.defining_entity,
                        as_type: Box::new(template_type.as_type.clone()),
                    })
                })
                .collect()
        } else if let Some(extended) = input_class_info
            .template_extended_params
            .get(&container_name)
            .filter(|map| !map.is_empty())
        {
            extended.values().cloned().collect()
        } else {
            vec![TUnion::mixed(); input_class_info.template_types.len()]
        }
    } else {
        Vec::new()
    };

    if input_name != container_name
        && let Some(input_class_info) = input_class_info
    {
        // The input class's own templates map to its given type params, ready
        // to substitute into the extends chain's parameterizations.
        let mut replacement_templates = TemplateResult::default();
        for (i, template_type) in input_class_info.template_types.iter().enumerate() {
            let Some(input_type_param) = input_type_params.get(i) else {
                break;
            };
            crate::template::lower_bounds_insert(
                &mut replacement_templates,
                template_type.name,
                GenericParent::ClassLike(input_name),
                input_type_param.clone(),
            );
        }

        let template_extends = &input_class_info.template_extended_params;

        if let Some(params) = template_extends.get(&container_name) {
            let mut new_input_params = Vec::with_capacity(params.len());

            for extended_input_param_type in params.values() {
                let mut new_input_param: Option<TUnion> = None;

                for extended_template in &extended_input_param_type.types {
                    let extended_templates: Vec<&TAtomic> =
                        if matches!(extended_template, TAtomic::TTemplateParam { .. }) {
                            get_extended_templated_types(extended_template, template_extends)
                                .into_iter()
                                .filter(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
                                .collect()
                        } else {
                            Vec::new()
                        };

                    let mut candidate_param_types: Vec<TUnion> = Vec::new();

                    for template in extended_templates {
                        let TAtomic::TTemplateParam {
                            name: template_name,
                            defining_entity: template_defining_entity,
                            ..
                        } = template
                        else {
                            continue;
                        };

                        let Some(old_params_offset) =
                            input_class_info.template_types.iter().position(|t| {
                                t.name == *template_name
                                    && t.defining_entity == *template_defining_entity
                            })
                        else {
                            continue;
                        };

                        let mut candidate_param_type = input_type_params
                            .get(old_params_offset)
                            .cloned()
                            .unwrap_or_else(TUnion::mixed);
                        candidate_param_type.from_template_default = true;
                        candidate_param_types.push(candidate_param_type);
                    }

                    let candidate = if candidate_param_types.is_empty() {
                        let mut kept = TUnion::new(extended_template.clone());
                        kept.from_template_default = true;
                        kept
                    } else {
                        let mut combined = candidate_param_types[0].clone();
                        for candidate_param_type in &candidate_param_types[1..] {
                            combined = combine_union_types(&combined, candidate_param_type, false);
                        }
                        combined
                    };

                    new_input_param = Some(match new_input_param {
                        Some(existing) => combine_union_types(&existing, &candidate, false),
                        None => candidate,
                    });
                }

                let new_input_param = new_input_param.unwrap_or_else(TUnion::mixed);
                new_input_params.push(crate::template::inferred_type_replacer::replace(
                    &new_input_param,
                    &replacement_templates,
                ));
            }

            input_type_params = new_input_params;
        }
    }

    input_type_params
}

fn is_traversable_template_target(name: StrId) -> bool {
    name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
}

fn is_top_level_template_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
    )
}

fn extract_class_string_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    arg_atomic: &TAtomic,
) -> Option<TAtomic> {
    match arg_atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some((**as_type).clone()),
        // A plain `class-string` names *some* object (Psalm's
        // handleTemplateParamClassStandin binds `new TObject(true)`).
        TAtomic::TClassString { as_type: None } => Some(TAtomic::TObject),
        TAtomic::TLiteralClassString { name } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(name),
            type_params: None,
            is_static: false,
            remapped_params: false,
        }),
        TAtomic::TLiteralString { value } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(value),
            type_params: None,
            is_static: false,
            remapped_params: false,
        }),
        _ => None,
    }
}

fn extract_array_like_key_value(arg_atomic: &TAtomic) -> Option<(TUnion, TUnion)> {
    match arg_atomic {
        TAtomic::TIterable {
            key_type,
            value_type,
        } => Some(((**key_type).clone(), (**value_type).clone())),
        // The unified array atomic. The typed fallback `params` seed the key /
        // value (a list's `params.0` is already `int`; an empty literal `[]`
        // has no `params`, seeding `never`/`never`). Every known entry then
        // contributes its literal key and its value.
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let mut key_union = params
                .as_deref()
                .map(|(key, _)| key.clone())
                .unwrap_or_else(TUnion::nothing);
            let mut value_union = params
                .as_deref()
                .map(|(_, value)| value.clone())
                .unwrap_or_else(TUnion::nothing);

            for (key, (_possibly_undefined, value)) in known_values.iter() {
                let key_union_part = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TUnion::new(TAtomic::TLiteralInt { value: *value })
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value)
                    | pzoom_code_info::t_atomic::ArrayKey::ClassString(value) => {
                        TUnion::new(TAtomic::TLiteralString {
                            value: value.clone(),
                        })
                    }
                };

                key_union = combine_union_types(&key_union, &key_union_part, false);
                value_union = combine_union_types(&value_union, value, false);
            }

            // An empty array literal (`[]`) keeps `never` key/value so template
            // inference resolves the params to `never` (matching Psalm) rather
            // than widening to `array-key`/`mixed`.
            Some((key_union, value_union))
        }
        TAtomic::TNamedObject {
            name, type_params, ..
        } if *name == StrId::TRAVERSABLE => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject {
            name, type_params, ..
        } if *name == StrId::ITERATOR => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject {
            name, type_params, ..
        } if *name == StrId::ITERATOR_AGGREGATE => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject {
            name, type_params, ..
        } if *name == StrId::GENERATOR => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TTemplateParam { as_type, .. } => extract_array_like_key_value_from_union(as_type),
        TAtomic::TObjectIntersection { types } => {
            let mut key_type: Option<TUnion> = None;
            let mut value_type: Option<TUnion> = None;

            for intersection_atomic in types {
                let Some((this_key_type, this_value_type)) =
                    extract_array_like_key_value(intersection_atomic)
                else {
                    continue;
                };

                key_type = Some(if let Some(existing) = key_type {
                    combine_union_types(&existing, &this_key_type, false)
                } else {
                    this_key_type
                });
                value_type = Some(if let Some(existing) = value_type {
                    combine_union_types(&existing, &this_value_type, false)
                } else {
                    this_value_type
                });
            }

            match (key_type, value_type) {
                (Some(key_type), Some(value_type)) => Some((key_type, value_type)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn extract_array_like_key_value_from_union(arg_union: &TUnion) -> Option<(TUnion, TUnion)> {
    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for atomic in &arg_union.types {
        let Some((this_key_type, this_value_type)) = extract_array_like_key_value(atomic) else {
            continue;
        };

        key_type = Some(if let Some(existing) = key_type {
            combine_union_types(&existing, &this_key_type, false)
        } else {
            this_key_type
        });
        value_type = Some(if let Some(existing) = value_type {
            combine_union_types(&existing, &this_value_type, false)
        } else {
            this_value_type
        });
    }

    match (key_type, value_type) {
        (Some(key_type), Some(value_type)) => Some((key_type, value_type)),
        _ => None,
    }
}

fn bind_template_replacement(
    analyzer: &StatementsAnalyzer<'_>,
    template_name: StrId,
    defining_entity: GenericParent,
    arg_type: &TUnion,
    bound: &TUnion,
    template_result: &mut TemplateResult,
) {
    let candidate_arg_type = if template_keeps_literals(analyzer, template_name, &defining_entity) {
        arg_type.clone()
    } else {
        widen_template_argument_to_bound(arg_type, bound)
    };

    let Some(candidate_arg_type) = filter_bindable_candidate(analyzer, candidate_arg_type, bound)
    else {
        return;
    };

    lower_bounds_insert_combined(
        template_result,
        template_name,
        defining_entity,
        candidate_arg_type,
    );
}

/// Psalm's `handleTemplateParamStandin` bind gate: the input binds when it
/// *can* be contained by the template's bound (`canBeContainedBy` — any
/// overlap), keeping only the overlapping union members
/// (`$matching_input_keys`). A strict containment gate would refuse to bind
/// `Key as mixed` to a `TKey as array-key` standin, which Psalm accepts.
fn filter_bindable_candidate(
    analyzer: &StatementsAnalyzer<'_>,
    candidate_arg_type: TUnion,
    bound: &TUnion,
) -> Option<TUnion> {
    if bound.is_mixed() {
        return Some(candidate_arg_type);
    }

    // A bound that itself mentions sibling templates (`TArray as
    // array<TKey, mixed>`) must be checked against their constraints —
    // Psalm's comparison effectively sees the templates' as-types, so
    // `array<string, int>` binds to that TArray.
    let bound = dissolve_nested_template_params(bound);
    let bound = &bound;

    let matching: Vec<TAtomic> = candidate_arg_type
        .types
        .iter()
        .filter(|atomic| {
            union_type_comparator::can_be_contained_by(
                analyzer.codebase,
                &TUnion::new((*atomic).clone()),
                bound,
            )
        })
        .cloned()
        .collect();

    if matching.is_empty() {
        return None;
    }

    if matching.len() == candidate_arg_type.types.len() {
        return Some(candidate_arg_type);
    }

    let mut filtered = candidate_arg_type;
    filtered.types = matching;
    Some(filtered)
}

/// Replace template-param atomics with their constraints, recursing through
/// generic containers, so a bound can be compared against concrete inputs.
fn dissolve_nested_template_params(union: &TUnion) -> TUnion {
    fn dissolve_atomics(atomics: &[TAtomic], out: &mut Vec<TAtomic>, depth: usize) {
        for atomic in atomics {
            match atomic {
                TAtomic::TTemplateParam { as_type, .. } if depth < 8 => {
                    dissolve_atomics(&as_type.types, out, depth + 1);
                }
                // A *generic* array (no known entries): dissolve the typed
                // fallback `params`, preserving the flags. Shapes (known entries
                // present) are left untouched — the old code had no `TKeyedArray`
                // arm here, cloning shapes verbatim via the `other` arm.
                TAtomic::TArray {
                    known_values,
                    params,
                    ..
                } if known_values.is_empty() => out.push(atomic.rebuilt_array(
                    known_values.clone(),
                    params.as_ref().map(|params| {
                        Box::new((
                            dissolve_union(&params.0, depth + 1),
                            dissolve_union(&params.1, depth + 1),
                        ))
                    }),
                )),
                TAtomic::TIterable {
                    key_type,
                    value_type,
                } => out.push(TAtomic::TIterable {
                    key_type: Box::new(dissolve_union(key_type, depth + 1)),
                    value_type: Box::new(dissolve_union(value_type, depth + 1)),
                }),
                TAtomic::TNamedObject {
                    name,
                    type_params: Some(type_params),
                    is_static,
                    remapped_params,
                } => out.push(TAtomic::TNamedObject {
                    name: *name,
                    type_params: Some(
                        type_params
                            .iter()
                            .map(|param| dissolve_union(param, depth + 1))
                            .collect(),
                    ),
                    is_static: *is_static,
                    remapped_params: *remapped_params,
                }),
                other => out.push(other.clone()),
            }
        }
    }

    fn dissolve_union(union: &TUnion, depth: usize) -> TUnion {
        if depth >= 8 {
            return union.clone();
        }
        let mut atomics = Vec::with_capacity(union.types.len());
        dissolve_atomics(&union.types, &mut atomics, depth);
        if atomics.is_empty() {
            union.clone()
        } else {
            let mut dissolved = union.clone();
            dissolved.types = atomics;
            dissolved
        }
    }

    dissolve_union(union, 0)
}

/// [`bind_template_replacement`] recording an equality bound (Hakana's
/// `equality_bound_classlike`): the argument names the template's type
/// exactly rather than providing a value of it.
fn bind_template_replacement_as_equality(
    analyzer: &StatementsAnalyzer<'_>,
    template_name: StrId,
    defining_entity: GenericParent,
    arg_type: &TUnion,
    bound: &TUnion,
    equality_classlike: StrId,
    template_result: &mut TemplateResult,
) {
    let candidate_arg_type = if template_keeps_literals(analyzer, template_name, &defining_entity) {
        arg_type.clone()
    } else {
        widen_template_argument_to_bound(arg_type, bound)
    };

    let Some(candidate_arg_type) = filter_bindable_candidate(analyzer, candidate_arg_type, bound)
    else {
        return;
    };

    template_result
        .lower_bounds
        .entry(template_name)
        .or_default()
        .entry(defining_entity)
        .or_default()
        .push(pzoom_code_info::TemplateBound {
            bound_type: candidate_arg_type,
            appearance_depth: 0,
            arg_offset: None,
            equality_bound_classlike: Some(equality_classlike),
            pos: None,
        });
}

/// Whether bounds for this template must keep argument literals: PHP's
/// conditional types (`(T is 1 ? ... : ...)`, a feature Hack lacks)
/// discriminate branches on values, so a template used as a conditional
/// subject is exempt from Hakana's `generalize_literals`.
fn template_keeps_literals(
    analyzer: &StatementsAnalyzer<'_>,
    template_name: StrId,
    defining_entity: &GenericParent,
) -> bool {
    let GenericParent::FunctionLike(id) = defining_entity else {
        return false;
    };

    let template_types = if let Some(function_info) = analyzer.codebase.get_function(*id) {
        &function_info.template_types
    } else {
        // Method templates' defining entity is the combined "Class::method".
        let combined = analyzer.interner.lookup(*id);
        let Some((class_name, method_name)) = combined.split_once("::") else {
            return false;
        };
        let Some(class_id) = analyzer.interner.find(class_name) else {
            return false;
        };
        let Some(method_id) = analyzer.interner.find(method_name) else {
            return false;
        };
        let Some(method_info) = analyzer
            .codebase
            .get_class(class_id)
            .and_then(|class_info| class_info.methods.get(&method_id))
        else {
            return false;
        };
        return method_info
            .template_types
            .iter()
            .any(|t| t.name == template_name && t.conditional_subject);
    };

    template_types
        .iter()
        .any(|t| t.name == template_name && t.conditional_subject)
}

fn widen_template_argument_to_bound(arg_type: &TUnion, bound: &TUnion) -> TUnion {
    let mut widened_types = Vec::with_capacity(arg_type.types.len());

    for atomic in &arg_type.types {
        let widened_atomic = match atomic {
            // A bound carrying literal ints (e.g. preg_match's
            // `TFlags as int-mask<0, 256, 512>`) discriminates on values —
            // keep a matching literal so conditional types can pick a branch
            // (a PHP-side need Hack doesn't have; Psalm's lower bounds keep
            // the arg's literal type). Everything else generalizes at bound
            // insertion (Hakana's `generalize_literals`), including
            // unconstrained (mixed-bounded) templates: binding `T` from
            // `[1, 2, 3]` yields `int`, not `1|2|3`.
            TAtomic::TLiteralInt { value }
                if bound_accepts_int_like(bound) && !bound_contains_literal_int(bound, *value) =>
            {
                TAtomic::TInt
            }
            TAtomic::TLiteralString { value }
                if bound_accepts_string_like(bound)
                    && !bound_contains_literal_string(bound, value) =>
            {
                TAtomic::TString
            }
            TAtomic::TLiteralFloat { .. } if bound_accepts_float_like(bound) => TAtomic::TFloat,
            TAtomic::TTrue | TAtomic::TFalse if bound_accepts_bool_like(bound) => TAtomic::TBool,
            _ => atomic.clone(),
        };

        if !widened_types.contains(&widened_atomic) {
            widened_types.push(widened_atomic);
        }
    }

    if widened_types.is_empty() {
        arg_type.clone()
    } else {
        let mut widened = TUnion::from_types(widened_types);
        widened.ignore_nullable_issues = arg_type.ignore_nullable_issues;
        widened.ignore_falsable_issues = arg_type.ignore_falsable_issues;
        widened
    }
}

fn bound_accepts_int_like(bound: &TUnion) -> bool {
    // `array-key` bounds keep literal keys (Psalm binds `TKey as array-key`
    // to literal `0` from a list argument, and its generic invariance check
    // then exempts the literal param).
    bound.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
        )
    })
}

fn bound_accepts_string_like(bound: &TUnion) -> bool {
    bound.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TClassString { .. }
                | TAtomic::TArrayKey
        )
    })
}

fn bound_contains_literal_string(bound: &TUnion, value: &str) -> bool {
    bound.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralString { value: bound_value } => bound_value == value,
        _ => false,
    })
}

fn bound_contains_literal_int(bound: &TUnion, value: i64) -> bool {
    bound.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralInt { value: bound_value } => *bound_value == value,
        TAtomic::TIntRange { min, max } => {
            min.is_none_or(|min| min <= value) && max.is_none_or(|max| value <= max)
        }
        _ => false,
    })
}

fn bound_accepts_float_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
}

fn bound_accepts_bool_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
}
