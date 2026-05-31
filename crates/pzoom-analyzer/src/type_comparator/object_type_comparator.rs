//! Object type comparator.
//!
//! Handles comparison of object/class types, checking class hierarchy.

use pzoom_code_info::{CodebaseInfo, TAtomic};
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
    } = container_type_part
    {
        match input_type_part {
            // Another `object{...}`: every container property must be present in
            // the input and assignable.
            TAtomic::TObjectWithProperties {
                properties: input_props,
            } => {
                for (key, container_value) in container_props {
                    let Some(input_value) = input_props.get(key) else {
                        atomic_comparison_result.type_coerced = Some(true);
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
            // A concrete class is only assignable when it actually declares the
            // properties. pzoom can't introspect class properties here (no
            // interner in the comparator), and the case that matters in practice
            // is the class *lacking* the property — which Psalm reports as a
            // plain InvalidArgument, not a coercion.
            TAtomic::TNamedObject { .. } => {
                return false;
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
        && declares_to_string(codebase, input_type_part)
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
            // (or a subclass) is only a coercion, not a clean match. Matches Psalm.
            let static_guard = |result: &mut TypeComparisonResult| {
                if container_static && !input_static {
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
                return static_guard(atomic_comparison_result);
            }

            // Check if input extends/implements container
            if is_class_subtype_of(*input_name, *container_name, codebase) {
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
