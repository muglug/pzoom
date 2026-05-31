//! Template standin type replacement helpers.
//!
//! Mirrors Psalm's TemplateStandinTypeReplacer / hakana-core's standin_type_replacer:
//! replaces template parameters in a type with the concrete types inferred for them
//! (or their declared defaults), expanding `class-string<T>`, indexed-access, and
//! template-param atomics along the way.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use crate::template::TemplateMap;

/// Replaces template params in a union with inferred/default concrete types.
pub fn replace(
    union_type: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TUnion {
    substitute_templates_in_union(union_type, template_replacements, template_defaults)
}

pub(crate) fn substitute_templates_in_union(
    union: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TUnion {
    let mut replaced_types = Vec::new();

    for atomic in &union.types {
        if let Some(indexed_access_union) =
            resolve_indexed_access_template_union(atomic, template_replacements, template_defaults)
        {
            for replacement_atomic in indexed_access_union.types {
                if !replaced_types.contains(&replacement_atomic) {
                    replaced_types.push(replacement_atomic);
                }
            }
            continue;
        }

        if let Some(key_value_of_union) =
            resolve_template_key_value_of_union(atomic, template_replacements)
        {
            for replacement_atomic in key_value_of_union.types {
                if !replaced_types.contains(&replacement_atomic) {
                    replaced_types.push(replacement_atomic);
                }
            }
            continue;
        }

        if let Some(properties_of) = resolve_template_properties_of(atomic, template_replacements) {
            if !replaced_types.contains(&properties_of) {
                replaced_types.push(properties_of);
            }
            continue;
        }

        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if let Some(class_replacement) = resolve_class_string_template_replacement(
                    as_type,
                    template_replacements,
                    template_defaults,
                ) {
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

                let replaced_atomic = substitute_templates_in_atomic(
                    atomic,
                    template_replacements,
                    template_defaults,
                );
                if !replaced_types.contains(&replaced_atomic) {
                    replaced_types.push(replaced_atomic);
                }
            }
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => {
                let replacement = template_replacements
                    .get(*name, *defining_entity)
                    .cloned()
                    .or_else(|| template_defaults.get(*name, *defining_entity).cloned())
                    .unwrap_or_else(|| (**as_type).clone());

                for replacement_atomic in replacement.types {
                    if !replaced_types.contains(&replacement_atomic) {
                        replaced_types.push(replacement_atomic);
                    }
                }
            }
            _ => {
                let replaced_atomic = substitute_templates_in_atomic(
                    atomic,
                    template_replacements,
                    template_defaults,
                );
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
    result.ignore_nullable_issues = union.ignore_nullable_issues;
    result.ignore_falsable_issues = union.ignore_falsable_issues;
    result
}

fn resolve_class_string_template_replacement(
    as_type: &TAtomic,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
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
        } => template_replacements
            .get(*name, *defining_entity)
            .cloned()
            .or_else(|| template_defaults.get(*name, *defining_entity).cloned()),
        TAtomic::TNamedObject {
            name,
            type_params: None,
        .. } => template_replacements
            .get_by_name(*name)
            .cloned()
            .or_else(|| template_defaults.get_by_name(*name).cloned()),
        _ => None,
    }
}

/// Resolve a deferred `key-of<T>` / `value-of<T>` once `T` has a bound replacement,
/// producing the keys (resp. values) of that replacement. Returns `None` when the
/// template is still unbound, leaving the deferred atomic in place.
fn resolve_template_key_value_of_union(
    atomic: &TAtomic,
    template_replacements: &TemplateMap,
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

    // Only resolve against a concrete inferred binding (call-site `template_replacements`),
    // never against a template's declared bound. During body analysis only the bound is
    // known and `key-of<T>` must stay deferred so a concrete key cannot satisfy it.
    let replacement = template_replacements.get(param_name, defining_entity)?;

    let resolved =
        substitute_templates_in_union(replacement, template_replacements, &TemplateMap::new());

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
    template_replacements: &TemplateMap,
) -> Option<TAtomic> {
    let TAtomic::TTemplatePropertiesOf {
        param_name,
        defining_entity,
        visibility_filter,
    } = atomic
    else {
        return None;
    };

    let replacement = template_replacements.get(*param_name, *defining_entity)?;
    let classlike_name = single_named_object_name(replacement)?;

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
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> Option<TUnion> {
    let TAtomic::TNamedObject {
        name,
        type_params: Some(type_params),
    .. } = atomic
    else {
        return None;
    };

    if *name != StrId::PZOOM_INDEXED_ACCESS || type_params.len() != 2 {
        return None;
    }

    let array_type =
        substitute_templates_in_union(&type_params[0], template_replacements, template_defaults);

    Some(extract_indexed_access_value_type(&array_type))
}

fn extract_indexed_access_value_type(array_type: &TUnion) -> TUnion {
    let mut value_type = TUnion::nothing();

    for atomic in &array_type.types {
        let extracted = match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TIterable { value_type, .. } => Some((**value_type).clone()),
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                Some((**value_type).clone())
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => Some(extract_keyed_array_value_type(
                properties,
                fallback_value_type.as_deref(),
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

fn substitute_templates_in_atomic(
    atomic: &TAtomic,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TAtomic {
    match atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            let replacement = template_replacements
                .get(*name, *defining_entity)
                .cloned()
                .or_else(|| template_defaults.get(*name, *defining_entity).cloned())
                .unwrap_or_else(|| (**as_type).clone());

            replacement
                .get_single()
                .cloned()
                .unwrap_or_else(|| atomic.clone())
        }
        TAtomic::TArray {
            key_type,
            value_type,
        } => TAtomic::TArray {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => TAtomic::TNonEmptyArray {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TIterable {
            key_type,
            value_type,
        } => TAtomic::TIterable {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TList { value_type } => TAtomic::TList {
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNonEmptyList { value_type } => TAtomic::TNonEmptyList {
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNamedObject {
            name,
            type_params: None,
        .. } if template_replacements.contains_name(*name) || template_defaults.contains_name(*name) => {
            let replacement = template_replacements
                .get_by_name(*name)
                .cloned()
                .or_else(|| template_defaults.get_by_name(*name).cloned())
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
                    .map(|param| {
                        substitute_templates_in_union(
                            param,
                            template_replacements,
                            template_defaults,
                        )
                    })
                    .collect()
            }),
            is_static: *is_static,
            remapped_params: *remapped_params,
        },
        TAtomic::TObjectIntersection { types } => {
            let mut replaced_types = Vec::with_capacity(types.len());
            for nested_type in types {
                let replaced_type = substitute_templates_in_atomic(
                    nested_type,
                    template_replacements,
                    template_defaults,
                );
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
        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            let mut new_properties = rustc_hash::FxHashMap::default();
            for (key, value) in properties {
                new_properties.insert(
                    key.clone(),
                    substitute_templates_in_union(value, template_replacements, template_defaults),
                );
            }

            TAtomic::TKeyedArray {
                properties: new_properties,
                is_list: *is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.as_ref().map(|key_type| {
                    Box::new(substitute_templates_in_union(
                        key_type,
                        template_replacements,
                        template_defaults,
                    ))
                }),
                fallback_value_type: fallback_value_type.as_ref().map(|value_type| {
                    Box::new(substitute_templates_in_union(
                        value_type,
                        template_replacements,
                        template_defaults,
                    ))
                }),
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
                            template_replacements,
                            template_defaults,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                ))
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
                            template_replacements,
                            template_defaults,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|as_type| {
                Box::new(substitute_templates_in_atomic(
                    as_type,
                    template_replacements,
                    template_defaults,
                ))
            }),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            if let Some(replacement) = template_replacements
                .get(*name, *defining_entity)
                .cloned()
                .or_else(|| template_defaults.get(*name, *defining_entity).cloned())
                && let Some(single_replacement) = replacement.get_single()
            {
                single_replacement.clone()
            } else {
                TAtomic::TTemplateParamClass {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(substitute_templates_in_atomic(
                        as_type,
                        template_replacements,
                        template_defaults,
                    )),
                }
            }
        }
        _ => atomic.clone(),
    }
}

pub(crate) fn extract_keyed_array_value_type(
    properties: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, TUnion>,
    fallback_value_type: Option<&TUnion>,
) -> TUnion {
    let mut value_type = fallback_value_type.cloned().unwrap_or_else(TUnion::nothing);

    for property_type in properties.values() {
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
    template_defaults: &TemplateMap,
    template_replacements: &mut TemplateMap,
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
                        template_defaults,
                        template_replacements,
                    );
                }
            }
            return;
        }
    }

    for param_atomic in &param_type.types {
        for arg_atomic in &arg_type.types {
            infer_template_replacements_from_atomic(
                analyzer,
                param_atomic,
                arg_atomic,
                template_defaults,
                template_replacements,
            );
        }
    }
}

pub(crate) fn infer_template_replacements_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    param_atomic: &TAtomic,
    arg_atomic: &TAtomic,
    template_defaults: &TemplateMap,
    template_replacements: &mut TemplateMap,
) {
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
            let bound = template_defaults.get(*name, *defining_entity).unwrap_or(param_as_type);
            let arg_union = TUnion::new(arg_atomic.clone());
            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                bound,
                template_replacements,
            );
            infer_template_replacements_from_union(
                analyzer,
                bound,
                arg_as_type,
                template_defaults,
                template_replacements,
            );
            return;
        }

        infer_template_replacements_from_union(
            analyzer,
            &TUnion::new(param_atomic.clone()),
            arg_as_type,
            template_defaults,
            template_replacements,
        );
        return;
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = arg_atomic {
        infer_template_replacements_from_atomic(
            analyzer,
            param_atomic,
            as_type,
            template_defaults,
            template_replacements,
        );
        return;
    }

    match param_atomic {
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            let bound = template_defaults
                .get(*name, *defining_entity)
                .cloned()
                .unwrap_or_else(|| TUnion::new((**as_type).clone()));
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                &bound,
                template_replacements,
            );

            infer_template_replacements_from_union(
                analyzer,
                &bound,
                &arg_union,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            let bound = template_defaults.get(*name, *defining_entity).unwrap_or(as_type);
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(
                analyzer,
                *name,
                *defining_entity,
                &arg_union,
                bound,
                template_replacements,
            );

            infer_template_replacements_from_union(
                analyzer,
                bound,
                &arg_union,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TNamedObject {
            name,
            type_params: None,
        .. } if template_defaults.contains_name(*name) => {
            // A template name that parsed as a plain object reference: the
            // defaults map knows which entity declared it.
            if let (Some(bound), Some(defining_entity)) = (
                template_defaults.get_by_name(*name),
                template_defaults.entity_for_name(*name),
            ) {
                bind_template_replacement(
                    analyzer,
                    *name,
                    defining_entity,
                    &TUnion::new(arg_atomic.clone()),
                    bound,
                    template_replacements,
                );
            }
        }
        TAtomic::TClassString {
            as_type: Some(param_as_type),
        } => {
            if let Some(arg_as_type) = extract_class_string_atomic(analyzer, arg_atomic) {
                infer_template_replacements_from_atomic(
                    analyzer,
                    param_as_type,
                    &arg_as_type,
                    template_defaults,
                    template_replacements,
                );
            }
        }
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
        } => {
            let Some((arg_key_type, arg_value_type)) = extract_array_like_key_value(arg_atomic)
            else {
                return;
            };

            infer_template_replacements_from_union(
                analyzer,
                key_type,
                &arg_key_type,
                template_defaults,
                template_replacements,
            );
            infer_template_replacements_from_union(
                analyzer,
                value_type,
                &arg_value_type,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            let arg_value_type = match arg_atomic {
                TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                    (**value_type).clone()
                }
                TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                    (**value_type).clone()
                }
                TAtomic::TKeyedArray {
                    properties,
                    fallback_value_type,
                    ..
                } => {
                    let mut combined = fallback_value_type
                        .as_ref()
                        .map(|value_type| (**value_type).clone())
                        .unwrap_or_else(TUnion::mixed);

                    for property_type in properties.values() {
                        combined = combine_union_types(&combined, property_type, false);
                    }

                    combined
                }
                _ => return,
            };

            infer_template_replacements_from_union(
                analyzer,
                value_type,
                &arg_value_type,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(param_type_params),
        .. } => {
            if let TAtomic::TNamedObject {
                name: arg_name,
                type_params: arg_type_params,
            .. } = arg_atomic
            {
                if name == arg_name
                    || (is_traversable_template_target(*name)
                        && crate::expr::call::function_call_analyzer::named_object_is_traversable(analyzer, *arg_name))
                {
                    if let Some(arg_type_params) = arg_type_params {
                        for (param, arg) in param_type_params.iter().zip(arg_type_params.iter()) {
                            infer_template_replacements_from_union(
                                analyzer,
                                param,
                                arg,
                                template_defaults,
                                template_replacements,
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
                        template_defaults,
                        template_replacements,
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
                            template_defaults,
                            template_replacements,
                        );
                    }
                }
            } else {
                for param_type in param_types {
                    infer_template_replacements_from_atomic(
                        analyzer,
                        param_type,
                        arg_atomic,
                        template_defaults,
                        template_replacements,
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
                _ => return,
            };

            for (param_param, arg_param) in param_params.iter().zip(arg_params.iter()) {
                infer_template_replacements_from_union(
                    analyzer,
                    &param_param.param_type,
                    &arg_param.param_type,
                    template_defaults,
                    template_replacements,
                );
            }

            if let (Some(param_return_type), Some(arg_return_type)) =
                (param_return_type, arg_return_type)
            {
                infer_template_replacements_from_union(
                    analyzer,
                    param_return_type,
                    arg_return_type,
                    template_defaults,
                    template_replacements,
                );
            }
        }
        _ => {}
    }
}

pub(crate) fn infer_named_object_template_replacements_from_extended_params(
    analyzer: &StatementsAnalyzer<'_>,
    param_class_name: StrId,
    param_type_params: &[TUnion],
    arg_class_name: StrId,
    arg_type_params: Option<&[TUnion]>,
    template_defaults: &TemplateMap,
    template_replacements: &mut TemplateMap,
) {
    let Some(arg_class_info) = analyzer.codebase.get_class(arg_class_name) else {
        return;
    };
    let Some(param_class_info) = analyzer.codebase.get_class(param_class_name) else {
        return;
    };
    let Some(extended_param_map) = arg_class_info
        .template_extended_params
        .get(&param_class_name)
    else {
        return;
    };

    let arg_template_defaults = crate::expr::call::function_call_analyzer::get_class_template_defaults(arg_class_info);
    let arg_template_replacements =
        crate::expr::call::function_call_analyzer::infer_class_template_replacements_from_type_params(arg_class_info, arg_type_params);

    for (idx, template_type) in param_class_info.template_types.iter().enumerate() {
        let Some(param_type_param) = param_type_params.get(idx) else {
            continue;
        };

        let mapped_arg_type = extended_param_map
            .get(&template_type.name)
            .cloned()
            .unwrap_or_else(|| template_type.as_type.clone());

        let resolved_arg_type =
            if arg_template_defaults.is_empty() && arg_template_replacements.is_empty() {
                mapped_arg_type
            } else {
                crate::expr::call::function_call_analyzer::replace_templates_in_union(
                    &mapped_arg_type,
                    &arg_template_replacements,
                    &arg_template_defaults,
                )
            };

        infer_template_replacements_from_union(
            analyzer,
            param_type_param,
            &resolved_arg_type,
            template_defaults,
            template_replacements,
        );
    }
}

pub(crate) fn is_traversable_template_target(name: StrId) -> bool {
    name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
}

pub(crate) fn is_top_level_template_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
    )
}

pub(crate) fn extract_class_string_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    arg_atomic: &TAtomic,
) -> Option<TAtomic> {
    match arg_atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some((**as_type).clone()),
        TAtomic::TLiteralClassString { name } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(name),
            type_params: None,
        is_static: false, remapped_params: false }),
        TAtomic::TLiteralString { value } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(value),
            type_params: None,
        is_static: false, remapped_params: false }),
        _ => None,
    }
}

pub(crate) fn extract_array_like_key_value(arg_atomic: &TAtomic) -> Option<(TUnion, TUnion)> {
    match arg_atomic {
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
        } => {
            let mut key_union = fallback_key_type
                .as_ref()
                .map(|key_type| (**key_type).clone())
                .unwrap_or_else(TUnion::nothing);
            let mut value_union = fallback_value_type
                .as_ref()
                .map(|value_type| (**value_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for (key, value) in properties {
                let key_union_part = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TUnion::new(TAtomic::TLiteralInt { value: *value })
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                        TUnion::new(TAtomic::TLiteralString {
                            value: value.clone(),
                        })
                    }
                };

                key_union = combine_union_types(&key_union, &key_union_part, false);
                value_union = combine_union_types(&value_union, value, false);
            }

            // An empty array literal (`[]`) is `array<never, never>`; keep the
            // `never` key/value so template inference resolves the params to
            // `never` (matching Psalm) rather than widening to `array-key`/`mixed`.
            Some((key_union, value_union))
        }
        TAtomic::TNamedObject { name, type_params , .. } if *name == StrId::TRAVERSABLE => {
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
        TAtomic::TNamedObject { name, type_params , .. } if *name == StrId::ITERATOR => {
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
        TAtomic::TNamedObject { name, type_params , .. } if *name == StrId::ITERATOR_AGGREGATE => {
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
        TAtomic::TNamedObject { name, type_params , .. } if *name == StrId::GENERATOR => {
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

pub(crate) fn extract_array_like_key_value_from_union(arg_union: &TUnion) -> Option<(TUnion, TUnion)> {
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

pub(crate) fn bind_template_replacement(
    analyzer: &StatementsAnalyzer<'_>,
    template_name: StrId,
    defining_entity: StrId,
    arg_type: &TUnion,
    bound: &TUnion,
    template_replacements: &mut TemplateMap,
) {
    let candidate_arg_type = widen_template_argument_to_bound(arg_type, bound);

    if !bound.is_mixed() {
        let mut bound_comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &candidate_arg_type,
            bound,
            false,
            false,
            &mut bound_comparison_result,
        ) {
            return;
        }
    }

    template_replacements.insert_combined(template_name, defining_entity, candidate_arg_type);
}

pub(crate) fn widen_template_argument_to_bound(arg_type: &TUnion, bound: &TUnion) -> TUnion {
    let mut widened_types = Vec::with_capacity(arg_type.types.len());

    for atomic in &arg_type.types {
        let widened_atomic = match atomic {
            TAtomic::TLiteralInt { .. } if bound_accepts_int_like(bound) => TAtomic::TInt,
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
        TUnion::from_types(widened_types)
    }
}

pub(crate) fn bound_accepts_int_like(bound: &TUnion) -> bool {
    bound.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TInt
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TIntRange { .. }
                | TAtomic::TArrayKey
        )
    })
}

pub(crate) fn bound_accepts_string_like(bound: &TUnion) -> bool {
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

pub(crate) fn bound_contains_literal_string(bound: &TUnion, value: &str) -> bool {
    bound.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralString { value: bound_value } => bound_value == value,
        _ => false,
    })
}

pub(crate) fn bound_accepts_float_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
}

pub(crate) fn bound_accepts_bool_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
}
