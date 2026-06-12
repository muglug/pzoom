//! Class template-parameter collector.
//!
//! Mirrors Hakana's `expr/call/class_template_param_collector.rs` and Psalm's
//! `ClassTemplateParamCollector`. Given a class, the late-static-bound class and
//! the receiver type (`$obj` in `$obj->method()`), collect the mapping from each
//! of the class's template names to its concrete type.
//!
//! pzoom's [`TemplateMap`] mirrors Psalm's/Hakana's two-level
//! `[$param_name][$defining_class]` keying (Hakana:
//! `IndexMap<StrId, FxHashMap<GenericParent, TUnion>>`); the `template name`
//! defaults are tracked separately (`get_class_template_defaults`), so `collect`
//! returns the *replacements* only. The same three Hakana entry points are
//! provided — [`collect`], [`resolve_template_param`] and [`expand_type`] — and
//! [`collect`] is wired into the method-call template context the same way
//! Hakana wires it from the existing-method-call analyzer.


use pzoom_code_info::class_like_info::{ClassLikeInfo, TemplateType};
use pzoom_code_info::{CodebaseInfo, GenericParent, TAtomic, TUnion, TemplateBound, TemplateResult};
use pzoom_str::StrId;
use indexmap::IndexMap;
use rustc_hash::FxHashMap;

/// The shape of `TemplateResult::lower_bounds` (Hakana's `collect` returns the
/// map that seeds it).
pub(crate) type LowerBounds = IndexMap<StrId, FxHashMap<GenericParent, Vec<TemplateBound>>>;

fn bounds_insert(bounds: &mut LowerBounds, name: StrId, entity: GenericParent, union: TUnion) {
    bounds
        .entry(name)
        .or_default()
        .insert(entity, vec![TemplateBound::new(union, 0, None, None)]);
}

fn bounds_insert_combined(bounds: &mut LowerBounds, name: StrId, entity: GenericParent, union: TUnion) {
    bounds
        .entry(name)
        .or_default()
        .entry(entity)
        .or_default()
        .push(TemplateBound::new(union, 0, None, None));
}

/// Collect a class's template-parameter replacement map for a call, combining
/// the receiver's supplied type params with any `@extends`/`@implements`
/// mappings. Returns `None` when the class declares no templates.
pub(crate) fn collect(
    codebase: &CodebaseInfo,
    class_storage: &ClassLikeInfo,
    static_class_storage: &ClassLikeInfo,
    lhs_type_part: Option<&TAtomic>,
    self_call: bool,
) -> Option<LowerBounds> {
    if class_storage.template_types.is_empty() {
        return None;
    }

    let mut class_template_params = LowerBounds::default();

    let e = &static_class_storage.template_extended_params;

    // `@extends`/`@implements` mappings. Psalm reads these off the *static*
    // class's `template_extended_params`, keyed by the ancestor that declares
    // each template (`$class_template_params[$type_name][$candidate->name]`),
    // which the populator has merged transitively, so a method declared higher
    // up the chain still resolves through the receiver's view of the
    // hierarchy. Fold them in first so receiver type params can override below.
    for (declaring_entity, template_map) in e {
        for (template_name, extended_type) in template_map {
            let expanded = TUnion::from_types(expand_type(
                extended_type,
                e,
                &static_class_storage.name,
                &static_class_storage.template_types,
            ));

            bounds_insert_combined(
                &mut class_template_params,
                *template_name,
                GenericParent::ClassLike(*declaring_entity),
                expanded,
            );
        }
    }

    let lhs_type_params = match lhs_type_part {
        Some(TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        }) => Some(type_params.as_slice()),
        _ => None,
    };

    if let Some(lhs_type_params) = lhs_type_params {
        if class_storage.name == static_class_storage.name {
            // Same class: pair templates with the receiver's params by order
            // (these override the extended mappings).
            for (i, template_type) in class_storage.template_types.iter().enumerate() {
                if let Some(type_param) = lhs_type_params.get(i) {
                    bounds_insert(
                        &mut class_template_params,
                        template_type.name,
                        GenericParent::ClassLike(class_storage.name),
                        type_param.clone(),
                    );
                }
            }
        } else {
            // Subclass receiver: resolve each template through the extended
            // mapping against the static class's params.
            for template_type in &class_storage.template_types {
                if let Some(input_type_extends) = e
                    .get(&class_storage.name)
                    .and_then(|map| map.get(&template_type.name))
                {
                    if let Some(output_type_extends) = resolve_template_param(
                        codebase,
                        input_type_extends,
                        static_class_storage,
                        lhs_type_params,
                    ) {
                        bounds_insert(
                            &mut class_template_params,
                            template_type.name,
                            GenericParent::ClassLike(class_storage.name),
                            output_type_extends,
                        );
                    }
                }
            }
        }
    }

    // Psalm's trailing fallback: a declaring-class template that found no
    // mapping resolves to its constraint
    // (`$class_template_params[$type_name][$class_storage->name] = $type`)
    // unless this is a `$this` call, where templates must stay unreplaced.
    if !self_call {
        for template_type in &class_storage.template_types {
            // Name-level guard, matching Psalm's
            // `!isset($class_template_params[$type_name])`.
            if !class_template_params.contains_key(&template_type.name) {
                bounds_insert(
                    &mut class_template_params,
                    template_type.name,
                    GenericParent::ClassLike(class_storage.name),
                    template_type.as_type.clone(),
                );
            }
        }
    }

    Some(class_template_params)
}

/// Resolve a template's `@extends` type against the static class's supplied
/// type params, recursing through nested template references. Mirrors Hakana's
/// `resolve_template_param`.
fn resolve_template_param(
    codebase: &CodebaseInfo,
    input_type_extends: &TUnion,
    static_class_storage: &ClassLikeInfo,
    type_params: &[TUnion],
) -> Option<TUnion> {
    let mut output_type_extends: Option<TUnion> = None;

    for type_extends_atomic in &input_type_extends.types {
        if let TAtomic::TTemplateParam {
            name: param_name,
            defining_entity: GenericParent::ClassLike(defining_entity),
            ..
        } = type_extends_atomic
        {
            if let Some(mapped_offset) = static_class_storage
                .template_types
                .iter()
                .position(|template_type| template_type.name == *param_name)
            {
                if let Some(type_param) = type_params.get(mapped_offset) {
                    output_type_extends =
                        Some(add_optional_union_type(type_param, output_type_extends.as_ref()));
                }
            } else if let Some(nested_input_type_extends) = static_class_storage
                .template_extended_params
                .get(defining_entity)
                .and_then(|map| map.get(param_name))
            {
                if let Some(nested_output_type) = resolve_template_param(
                    codebase,
                    nested_input_type_extends,
                    static_class_storage,
                    type_params,
                ) {
                    output_type_extends = Some(add_optional_union_type(
                        &nested_output_type,
                        output_type_extends.as_ref(),
                    ));
                }
            }
        } else {
            output_type_extends = Some(add_optional_union_type(
                &TUnion::new(type_extends_atomic.clone()),
                output_type_extends.as_ref(),
            ));
        }
    }

    output_type_extends
}

/// Expand a template's `@extends` type, following nested template references
/// through the extended-params map (except those defined by the static class
/// itself). Mirrors Hakana's `expand_type`.
fn expand_type(
    input_type_extends: &TUnion,
    e: &IndexMap<StrId, IndexMap<StrId, TUnion>>,
    static_classlike_name: &StrId,
    static_template_types: &[TemplateType],
) -> Vec<TAtomic> {
    let mut output_type_extends = Vec::new();

    for type_extends_atomic in &input_type_extends.types {
        let extended_type = if let TAtomic::TTemplateParam {
            name: param_name,
            defining_entity: GenericParent::ClassLike(defining_entity),
            ..
        } = type_extends_atomic
        {
            if static_classlike_name != defining_entity
                || !static_template_types
                    .iter()
                    .any(|template_type| template_type.name == *param_name)
            {
                e.get(defining_entity).and_then(|map| map.get(param_name))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(extended_type) = extended_type {
            output_type_extends.extend(expand_type(
                extended_type,
                e,
                static_classlike_name,
                static_template_types,
            ));
        } else {
            output_type_extends.push(type_extends_atomic.clone());
        }
    }

    output_type_extends
}

fn add_optional_union_type(new_type: &TUnion, existing: Option<&TUnion>) -> TUnion {
    match existing {
        Some(existing) => pzoom_code_info::combine_union_types(existing, new_type, false),
        None => new_type.clone(),
    }
}

/// Pair a class's template names with the receiver's supplied type params by
/// declaration order (the same-class core of [`collect`]). Retained under its
/// historical name for the many call sites that only need this mapping.
pub(crate) fn infer_class_template_replacements_from_type_params(
    class_info: &ClassLikeInfo,
    type_params: Option<&[TUnion]>,
) -> TemplateResult {
    let mut class_template_params = TemplateResult::default();

    let Some(type_params) = type_params else {
        return class_template_params;
    };

    for (i, template_type) in class_info.template_types.iter().enumerate() {
        if let Some(type_param) = type_params.get(i) {
            crate::template::lower_bounds_insert(
                &mut class_template_params,
                template_type.name,
                GenericParent::ClassLike(class_info.name),
                type_param.clone(),
            );
        }
    }

    class_template_params
}
