//! Object type comparator.
//!
//! Handles comparison of object/class types, checking class hierarchy.

use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::StrId;

use super::type_comparison_result::TypeComparisonResult;

/// Check if an input object type is contained by a container object type.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Any named object is contained by generic object
    if matches!(container_type_part, TAtomic::TObject) {
        if matches!(
            input_type_part,
            TAtomic::TNamedObject { .. } | TAtomic::TObject
        ) {
            return true;
        }
    }

    // Generic object going into named object requires coercion
    if matches!(input_type_part, TAtomic::TObject) {
        if matches!(container_type_part, TAtomic::TNamedObject { .. }) {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        }
    }

    // `object{...}` — objects with known properties (Psalm's TObjectWithProperties).
    if let TAtomic::TObjectWithProperties {
        properties: container_props,
        is_stringable,
        ..
    } = container_type_part
    {
        // `stringable-object` (Psalm: methods-only TObjectWithProperties with
        // is_stringable_object_only): satisfied by another stringable-object
        // or a class declaring __toString; everything else is a plain mismatch.
        if *is_stringable {
            return match input_type_part {
                TAtomic::TObjectWithProperties {
                    is_stringable: true,
                    ..
                } => true,
                TAtomic::TNamedObject { .. } => declares_to_string(codebase, input_type_part),
                _ => false,
            };
        }

        match input_type_part {
            // Another `object{...}`: every container property must be present in
            // the input and assignable.
            TAtomic::TObjectWithProperties {
                properties: input_props,
                ..
            } => {
                for (key, container_value) in container_props {
                    let Some(input_value) = input_props.get(key) else {
                        // A missing required property is a plain mismatch
                        // (Psalm reports InvalidReturnStatement, not the
                        // less-specific coercion variants).
                        return false;
                    };
                    if !super::union_type_comparator::is_contained_by(
                        codebase,
                        input_value,
                        container_value,
                        false,
                        false,
                        atomic_comparison_result,
                    ) {
                        return false;
                    }
                }
                return true;
            }
            // A bare `object` may turn out to have these properties at runtime,
            // so it is a coercion (Psalm's ArgumentTypeCoercion).
            TAtomic::TObject => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            // A concrete class must declare each shape property with an
            // assignable type (Psalm's
            // KeyedArrayComparator::isContainedByObjectWithProperties);
            // stdClass accepts dynamic properties, so it always satisfies the
            // shape (Psalm returns true before the property check).
            TAtomic::TNamedObject { name, type_params, .. } => {
                if *name == StrId::STDCLASS {
                    return true;
                }
                let Some(class_info) = codebase.get_class(*name) else {
                    return false;
                };
                let mut all_types_contain = true;
                for (key, container_property_type) in container_props {
                    let property_id = match key {
                        pzoom_code_info::ArrayKey::String(property_name) => {
                            class_info.property_name_lookup.get(property_name).copied()
                        }
                        pzoom_code_info::ArrayKey::Int(_) => None,
                    };
                    let Some(property_info) =
                        property_id.and_then(|property_id| class_info.properties.get(&property_id))
                    else {
                        all_types_contain = false;
                        continue;
                    };
                    // A templated class property reads through the input's
                    // type params (Value<Value<42>>->value: Value<42>).
                    let input_property_type =
                        crate::expr::fetch::atomic_property_fetch_analyzer::substitute_class_template_params(
                            class_info,
                            type_params.as_deref(),
                            &property_info
                                .get_type()
                                .cloned()
                                .unwrap_or_else(pzoom_code_info::TUnion::mixed),
                        );
                    if input_property_type.is_nothing() {
                        continue;
                    }
                    if !super::union_type_comparator::is_contained_by(
                        codebase,
                        &input_property_type,
                        container_property_type,
                        false,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        // The reverse direction holding means coercion, not a
                        // flat mismatch (Psalm sets type_coerced).
                        if super::union_type_comparator::is_contained_by(
                            codebase,
                            container_property_type,
                            &input_property_type,
                            false,
                            false,
                            &mut TypeComparisonResult::new(),
                        ) {
                            atomic_comparison_result.type_coerced = Some(true);
                        }
                        all_types_contain = false;
                    }
                }
                return all_types_contain;
            }
            _ => {}
        }
    }

    // `object{...}` is an object, so it satisfies a bare-object container.
    if matches!(input_type_part, TAtomic::TObjectWithProperties { .. }) {
        if matches!(container_type_part, TAtomic::TObject) {
            return true;
        }
    }

    // PHP 8: a class that declares `__toString` (directly or inherited) implicitly
    // satisfies the native `\Stringable`, even without an explicit `implements
    // Stringable`. Mirrors Psalm's `__tostring` injection. The rule is specific to
    // the built-in interface: a user that *redefines* `Stringable` as their own
    // interface gets ordinary (explicit-implementation) semantics, so we only
    // apply it when `Stringable` resolves to a stub-defined interface.
    if matches!(
        container_type_part,
        TAtomic::TNamedObject { name, .. } if *name == StrId::STRINGABLE
    ) && stringable_is_native(codebase)
        && (declares_to_string(codebase, input_type_part)
            || matches!(
                input_type_part,
                TAtomic::TObjectWithProperties {
                    is_stringable: true,
                    ..
                }
            ))
    {
        return true;
    }

    // Compare named objects
    if let TAtomic::TNamedObject {
        name: container_name,
        type_params: container_type_params,
        is_static: container_is_static,
        ..
    } = container_type_part
    {
        if let TAtomic::TNamedObject {
            name: input_name,
            type_params: input_type_params,
            is_static: input_is_static,
            ..
        } = input_type_part
        {
            // Late-static-binding: two `static` types share the same runtime-class
            // context, so they are mutually compatible regardless of the concrete
            // class recorded in `name`. Otherwise a `static` type behaves like its
            // concrete class for containment, handled by the normal checks below.
            let container_static = *container_is_static || *container_name == StrId::STATIC;
            let input_static = *input_is_static || *input_name == StrId::STATIC;
            if container_static && input_static {
                return true;
            }

            // A `static` container is more specific than its concrete class: it may
            // resolve to a subclass at runtime, so a non-`static` input of the same
            // (or a subclass) is only a coercion, not a clean match — unless the
            // input class is FINAL (no further subclasses; Psalm's
            // ObjectComparator final exemption).
            let input_is_final = codebase
                .get_class(*input_name)
                .is_some_and(|input_info| input_info.is_final);
            let static_guard = move |result: &mut TypeComparisonResult| {
                if container_static && !input_static && !input_is_final {
                    result.type_coerced = Some(true);
                    false
                } else {
                    true
                }
            };

            // Same class
            if input_name == container_name {
                if !super::generic_type_comparator::is_contained_by(
                    codebase,
                    *container_name,
                    input_type_params.as_deref(),
                    container_type_params.as_deref(),
                    atomic_comparison_result,
                ) {
                    return false;
                }

                return static_guard(atomic_comparison_result);
            }

            // Generator is always traversable and iterator-like in Psalm semantics.
            if *input_name == StrId::GENERATOR
                && (*container_name == StrId::TRAVERSABLE || *container_name == StrId::ITERATOR)
            {
                if !compare_remapped_generic_params(
                    codebase,
                    *input_name,
                    input_type_params.as_deref(),
                    *container_name,
                    container_type_params.as_deref(),
                    atomic_comparison_result,
                ) {
                    return false;
                }
                return static_guard(atomic_comparison_result);
            }

            // Check if input extends/implements container
            if is_class_subtype_of(*input_name, *container_name, codebase) {
                if !compare_remapped_generic_params(
                    codebase,
                    *input_name,
                    input_type_params.as_deref(),
                    *container_name,
                    container_type_params.as_deref(),
                    atomic_comparison_result,
                ) {
                    return false;
                }
                return static_guard(atomic_comparison_result);
            }

            // Check if container extends/implements input (coercion)
            if is_class_subtype_of(*container_name, *input_name, codebase) {
                atomic_comparison_result.type_coerced = Some(true);
            }

            return false;
        }
    }

    false
}

/// Compare a generic input object against a *parameterized ancestor* container
/// by remapping the input's type params onto the container's template slots
/// through the input class's `@template-extends`/`@template-implements` chain.
/// Port of Hakana's `standin_type_replacer::get_mapped_generic_type_params` +
/// the generic param loop (Psalm's AtomicTypeComparator TGenericObject path).
/// Containers without type params (bare ancestors) always match shallowly.
fn compare_remapped_generic_params(
    codebase: &CodebaseInfo,
    input_name: StrId,
    input_type_params: Option<&[TUnion]>,
    container_name: StrId,
    container_type_params: Option<&[TUnion]>,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let Some(container_params) = container_type_params else {
        return true;
    };
    if container_params.is_empty() {
        return true;
    }

    // Psalm only compares params generic-vs-generic: a bare (un-parameterized)
    // input object satisfies a parameterized ancestor shallowly — except a
    // NON-generic class whose @extends/@implements pins the ancestor's params
    // to concrete types (final class CustomEnumSet extends EnumSet<CustomEnum>)
    // compared against a concrete container: those fixed params do compare.
    // Containers mentioning templates keep the shallow pass (their params are
    // solved, not checked, here), as do auto-filled (mixed) extends args.
    let mapped_params = match input_type_params {
        Some(input_params) => {
            get_mapped_generic_type_params(codebase, input_name, input_params, container_name)
        }
        None => {
            let container_mentions_templates = container_params.iter().any(|param| {
                param.is_mixed()
                    || param.types.iter().any(|atomic| {
                        matches!(
                            atomic,
                            TAtomic::TTemplateParam { .. }
                                | TAtomic::TTemplateParamClass { .. }
                                | TAtomic::TTypeVariable { .. }
                        )
                    })
            });
            if !container_mentions_templates
                && codebase.get_class(input_name).is_some_and(|input_info| {
                    input_info.template_types.is_empty()
                        && !input_info.template_extended_params.is_empty()
                })
            {
                get_mapped_generic_type_params(codebase, input_name, &[], container_name)
                    .filter(|mapped| mapped.iter().all(|param| !param.is_mixed()))
            } else {
                return true;
            }
        }
    };

    let Some(mapped_params) = mapped_params else {
        // The ancestor relationship exists but isn't templated (e.g. a plain
        // `implements Traversable` with no @template-implements), or carries
        // no information worth comparing: nothing to check against the
        // container's params.
        return true;
    };

    super::generic_type_comparator::is_contained_by(
        codebase,
        container_name,
        Some(&mapped_params),
        Some(container_params),
        atomic_comparison_result,
    )
}

/// Map a generic input object's concrete type params onto an ancestor's
/// template slots via the input class's `@template-extends`/
/// `@template-implements` chain — delegates to the
/// `TemplateStandinTypeReplacer::getMappedGenericTypeParams` port, which
/// resolves multi-level chains. Returns `None` when the ancestor link carries
/// no template substitutions.
pub(crate) fn get_mapped_generic_type_params(
    codebase: &CodebaseInfo,
    input_name: StrId,
    input_params: &[TUnion],
    container_name: StrId,
) -> Option<Vec<TUnion>> {
    if input_name == container_name {
        return Some(input_params.to_vec());
    }

    let input_class_storage = codebase.get_class(input_name)?;
    let extended_params = input_class_storage
        .template_extended_params
        .get(&container_name)?;

    let mapped_params = crate::template::standin_type_replacer::get_mapped_generic_type_params(
        codebase,
        input_name,
        Some(input_params),
        container_name,
    );

    Some(
        mapped_params
            .into_iter()
            .zip(extended_params.values())
            .map(|(mut mapped, extended_param)| {
                // Hakana marks every mapped slot from_template_default; with
                // from_docblock (extends clauses are docblock constructs) it
                // downgrades mixed-coercions to as-mixed. Defaulted slots (no
                // @template-extends — Psalm's populator clears from_docblock
                // on those) keep their non-docblock provenance so the mixed
                // coercion reports as MixedArgumentTypeCoercion.
                mapped.from_template_default = true;
                if extended_param.from_docblock {
                    mapped.from_docblock = true;
                }
                mapped
            })
            .collect(),
    )
}

/// Whether the codebase's `Stringable` is the native (stub-defined) interface
/// rather than a user redefinition. PHP's implicit-Stringable rule only applies
/// to the built-in `\Stringable`.
fn stringable_is_native(codebase: &CodebaseInfo) -> bool {
    match codebase.get_class(StrId::STRINGABLE) {
        Some(info) => codebase
            .files
            .get(&info.file_path)
            .is_none_or(|file| file.is_stub),
        None => true,
    }
}

/// Whether a named-object input declares a `__toString` method — directly, via a
/// parent class, or by implementing `Stringable`. Used to satisfy a `Stringable`
/// container per PHP 8's implicit-Stringable rule.
fn declares_to_string(codebase: &CodebaseInfo, input_type_part: &TAtomic) -> bool {
    let TAtomic::TNamedObject { name, .. } = input_type_part else {
        return false;
    };

    if *name == StrId::STRINGABLE {
        return true;
    }

    let Some(class_info) = codebase.get_class(*name) else {
        return false;
    };

    if class_info.methods.contains_key(&StrId::TO_STRING)
        || class_info.all_parent_interfaces.contains(&StrId::STRINGABLE)
    {
        return true;
    }

    class_info.all_parent_classes.iter().any(|parent| {
        codebase
            .get_class(*parent)
            .is_some_and(|parent_info| parent_info.methods.contains_key(&StrId::TO_STRING))
    })
}

/// Check if a class is a subtype of another (extends or implements).
pub fn is_class_subtype_of(
    input_class: StrId,
    container_class: StrId,
    codebase: &CodebaseInfo,
) -> bool {
    if matches!(input_class, StrId::STATIC | StrId::SELF) {
        if matches!(container_class, StrId::STATIC | StrId::SELF) {
            return true;
        }

        if container_class == StrId::PARENT {
            return false;
        }

        // `static`/`self` are always at least as specific as the current class,
        // so allow containment into concrete named classes.
        return true;
    }

    if matches!(container_class, StrId::STATIC | StrId::SELF) {
        return input_class == container_class;
    }

    if input_class == container_class {
        return true;
    }

    if let Some(class_info) = codebase.get_class(input_class) {
        // Check parent class
        if let Some(parent) = class_info.parent_class {
            if parent == container_class {
                return true;
            }
            // Recursively check parent chain
            if is_class_subtype_of(parent, container_class, codebase) {
                return true;
            }
        }

        // Check interfaces
        if class_info.interfaces.contains(&container_class) {
            return true;
        }

        // Check if any interface extends the container
        for iface in &class_info.interfaces {
            if is_class_subtype_of(*iface, container_class, codebase) {
                return true;
            }
        }
    }

    false
}

/// Check if class/interface exists in codebase.
pub fn class_exists(codebase: &CodebaseInfo, class_name: StrId) -> bool {
    codebase.get_class(class_name).is_some()
}
