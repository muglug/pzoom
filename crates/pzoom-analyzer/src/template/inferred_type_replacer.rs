//! Inferred template replacement helpers.
//!
//! This module intentionally mirrors Psalm's `TemplateInferredTypeReplacer` role:
//! after standin substitution, replace remaining template atomics from inferred
//! template types.

use pzoom_code_info::{FunctionLikeParameter, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use crate::template::TemplateMap;

/// Replaces remaining template atomics using inferred/default template maps.
pub fn replace(
    union: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TUnion {
    if template_replacements.is_empty() && template_defaults.is_empty() {
        return union.clone();
    }

    replace_union(
        union,
        template_replacements,
        template_defaults,
        &mut FxHashSet::default(),
    )
}

fn replace_union(
    union: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    resolving_templates: &mut FxHashSet<StrId>,
) -> TUnion {
    let mut new_types = Vec::new();

    for atomic_type in &union.types {
        match atomic_type {
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => {
                if let Some(template_type) = resolve_template_union(
                    *name,
                    *defining_entity,
                    as_type,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
                ) {
                    for template_type_part in template_type.types {
                        push_unique(&mut new_types, template_type_part);
                    }
                } else {
                    push_unique(
                        &mut new_types,
                        TAtomic::TTemplateParam {
                            name: *name,
                            defining_entity: *defining_entity,
                            as_type: Box::new(replace_union(
                                as_type,
                                template_replacements,
                                template_defaults,
                                resolving_templates,
                            )),
                        },
                    );
                }
            }
            TAtomic::TTemplateParamClass {
                name,
                defining_entity,
                as_type,
            } => {
                if let Some(class_template_type) = resolve_template_class_union(
                    *name,
                    *defining_entity,
                    as_type,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
                ) {
                    for template_type_part in class_template_type.types {
                        push_unique(&mut new_types, template_type_part);
                    }
                } else {
                    push_unique(
                        &mut new_types,
                        TAtomic::TTemplateParamClass {
                            name: *name,
                            defining_entity: *defining_entity,
                            as_type: Box::new(replace_atomic(
                                as_type,
                                template_replacements,
                                template_defaults,
                                resolving_templates,
                            )),
                        },
                    );
                }
            }
            TAtomic::TTemplateKeyOf {
                param_name,
                defining_entity,
                ..
            }
            | TAtomic::TTemplateValueOf {
                param_name,
                defining_entity,
                ..
            } => {
                let is_key_of = matches!(atomic_type, TAtomic::TTemplateKeyOf { .. });
                if let Some(resolved) = resolve_template_key_value_of(
                    *param_name,
                    *defining_entity,
                    is_key_of,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
                ) {
                    for resolved_part in resolved.types {
                        push_unique(&mut new_types, resolved_part);
                    }
                } else {
                    push_unique(&mut new_types, atomic_type.clone());
                }
            }
            TAtomic::TTemplatePropertiesOf {
                param_name,
                defining_entity,
                visibility_filter,
            } => {
                if let Some(properties_of) = template_replacements
                    .get(*param_name, *defining_entity)
                    .and_then(single_named_object_name)
                    .map(|classlike_name| TAtomic::TPropertiesOf {
                        classlike_name,
                        visibility_filter: *visibility_filter,
                    })
                {
                    push_unique(&mut new_types, properties_of);
                } else {
                    push_unique(&mut new_types, atomic_type.clone());
                }
            }
            _ => {
                push_unique(
                    &mut new_types,
                    replace_atomic(
                        atomic_type,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    ),
                );
            }
        }
    }

    if new_types.is_empty() {
        return union.clone();
    }

    let mut result = TUnion::from_types(new_types);
    result.from_docblock = union.from_docblock;
    result.is_resolved = union.is_resolved;
    result.parent_nodes = union.parent_nodes.clone();
    result.ignore_nullable_issues = union.ignore_nullable_issues;
    result.ignore_falsable_issues = union.ignore_falsable_issues;
    result
}

fn replace_atomic(
    atomic: &TAtomic,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    resolving_templates: &mut FxHashSet<StrId>,
) -> TAtomic {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        } => TAtomic::TArray {
            key_type: Box::new(replace_union(
                key_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
            value_type: Box::new(replace_union(
                value_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
        },
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => TAtomic::TNonEmptyArray {
            key_type: Box::new(replace_union(
                key_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
            value_type: Box::new(replace_union(
                value_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
        },
        TAtomic::TList { value_type } => TAtomic::TList {
            value_type: Box::new(replace_union(
                value_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
        },
        TAtomic::TNonEmptyList { value_type } => TAtomic::TNonEmptyList {
            value_type: Box::new(replace_union(
                value_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
        },
        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            let mut new_properties = FxHashMap::default();
            for (key, value) in properties {
                new_properties.insert(
                    key.clone(),
                    replace_union(
                        value,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    ),
                );
            }

            TAtomic::TKeyedArray {
                properties: new_properties,
                is_list: *is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.as_ref().map(|fallback_key| {
                    Box::new(replace_union(
                        fallback_key,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    ))
                }),
                fallback_value_type: fallback_value_type.as_ref().map(|fallback_value| {
                    Box::new(replace_union(
                        fallback_value,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    ))
                }),
            }
        }
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static,
            remapped_params,
        } => TAtomic::TNamedObject {
            name: *name,
            type_params: type_params.as_ref().map(|type_params| {
                type_params
                    .iter()
                    .map(|type_param| {
                        replace_union(
                            type_param,
                            template_replacements,
                            template_defaults,
                            resolving_templates,
                        )
                    })
                    .collect()
            }),
            is_static: *is_static,
            remapped_params: *remapped_params,
        },
        TAtomic::TObjectIntersection { types } => TAtomic::TObjectIntersection {
            types: types
                .iter()
                .map(|nested_type| {
                    replace_atomic(
                        nested_type,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    )
                })
                .collect(),
        },
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| FunctionLikeParameter {
                        name: param.name,
                        param_type: replace_union(
                            &param.param_type,
                            template_replacements,
                            template_defaults,
                            resolving_templates,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(replace_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
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
                    .map(|param| FunctionLikeParameter {
                        name: param.name,
                        param_type: replace_union(
                            &param.param_type,
                            template_replacements,
                            template_defaults,
                            resolving_templates,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(replace_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TIterable {
            key_type,
            value_type,
        } => TAtomic::TIterable {
            key_type: Box::new(replace_union(
                key_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
            value_type: Box::new(replace_union(
                value_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            )),
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|as_type| {
                Box::new(replace_atomic(
                    as_type,
                    template_replacements,
                    template_defaults,
                    resolving_templates,
                ))
            }),
        },
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            if let Some(template_type) = resolve_template_union(
                *name,
                *defining_entity,
                as_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            ) {
                first_atomic_or_mixed(&template_type)
            } else {
                TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(replace_union(
                        as_type,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    )),
                }
            }
        }
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            if let Some(class_template_type) = resolve_template_class_union(
                *name,
                *defining_entity,
                as_type,
                template_replacements,
                template_defaults,
                resolving_templates,
            ) {
                first_atomic_or_mixed(&class_template_type)
            } else {
                TAtomic::TTemplateParamClass {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(replace_atomic(
                        as_type,
                        template_replacements,
                        template_defaults,
                        resolving_templates,
                    )),
                }
            }
        }
        _ => atomic.clone(),
    }
}

/// Resolve a deferred `key-of<T>` / `value-of<T>` from the inferred bound of `T`,
/// producing the keys (resp. values) of that bound. Mirrors Psalm's
/// `TemplateInferredTypeReplacer::replaceTemplateKeyOfValueOf`.
fn resolve_template_key_value_of(
    template_name: StrId,
    defining_entity: StrId,
    is_key_of: bool,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    resolving_templates: &mut FxHashSet<StrId>,
) -> Option<TUnion> {
    // Resolve only against a concrete inferred binding, never a declared bound, so the
    // deferred `key-of<T>` survives body analysis (where only the bound is known).
    let replacement = template_replacements.get(template_name, defining_entity)?;

    if !resolving_templates.insert(template_name) {
        return None;
    }
    let resolved = replace_union(
        replacement,
        template_replacements,
        template_defaults,
        resolving_templates,
    );
    resolving_templates.remove(&template_name);

    Some(if is_key_of {
        pzoom_code_info::ttype::get_key_of_union(&resolved)
    } else {
        pzoom_code_info::ttype::get_value_of_union(&resolved)
    })
}

fn resolve_template_union(
    template_name: StrId,
    defining_entity: StrId,
    as_type: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    resolving_templates: &mut FxHashSet<StrId>,
) -> Option<TUnion> {
    let replacement = template_replacements
        .get(template_name, defining_entity)
        .or_else(|| template_defaults.get(template_name, defining_entity))?;

    // A self-referential replacement (`T -> T`) means the parameter is bound to
    // itself — typically a `$this`/`self` method call inside the defining class,
    // where the class templates stay abstract. Keep it as the template parameter
    // rather than recursing (which would collapse it to its `as` bound).
    if replacement.types.len() == 1
        && let TAtomic::TTemplateParam {
            name: replacement_name,
            ..
        } = &replacement.types[0]
        && *replacement_name == template_name
    {
        return Some(replacement.clone());
    }

    if !resolving_templates.insert(template_name) {
        return Some(as_type.clone());
    }

    let resolved = replace_union(
        replacement,
        template_replacements,
        template_defaults,
        resolving_templates,
    );
    resolving_templates.remove(&template_name);

    // Psalm: if inferred replacement is mixed but template bound is not mixed, keep the bound.
    if resolved.is_mixed() && !as_type.is_mixed() {
        Some(as_type.clone())
    } else {
        Some(resolved)
    }
}

fn resolve_template_class_union(
    template_name: StrId,
    defining_entity: StrId,
    as_type: &TAtomic,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    resolving_templates: &mut FxHashSet<StrId>,
) -> Option<TUnion> {
    let replacement = template_replacements
        .get(template_name, defining_entity)
        .or_else(|| template_defaults.get(template_name, defining_entity))?;

    if !resolving_templates.insert(template_name) {
        return Some(TUnion::new(TAtomic::TClassString {
            as_type: Some(Box::new(as_type.clone())),
        }));
    }

    let resolved = replace_union(
        replacement,
        template_replacements,
        template_defaults,
        resolving_templates,
    );
    resolving_templates.remove(&template_name);

    let mut class_template_types = Vec::new();
    for template_type_part in resolved.types {
        if let Some(class_template_type) = to_class_string_atomic(&template_type_part) {
            push_unique(&mut class_template_types, class_template_type);
        }
    }

    if class_template_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(class_template_types))
    }
}

fn to_class_string_atomic(atomic: &TAtomic) -> Option<TAtomic> {
    match atomic {
        TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } => Some(atomic.clone()),
        TAtomic::TNamedObject { .. } | TAtomic::TObjectIntersection { .. } => {
            Some(TAtomic::TClassString {
                as_type: Some(Box::new(atomic.clone())),
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let first = first_atomic_or_mixed(as_type);
            Some(TAtomic::TClassString {
                as_type: Some(Box::new(first)),
            })
        }
        TAtomic::TTemplateParamClass { as_type, .. } => Some(TAtomic::TClassString {
            as_type: Some(Box::new((**as_type).clone())),
        }),
        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject => {
            Some(TAtomic::TClassString {
                as_type: Some(Box::new(TAtomic::TObject)),
            })
        }
        _ => None,
    }
}

fn first_atomic_or_mixed(union: &TUnion) -> TAtomic {
    union.types.first().cloned().unwrap_or(TAtomic::TMixed)
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

fn push_unique(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !target.contains(&atomic) {
        target.push(atomic);
    }
}
