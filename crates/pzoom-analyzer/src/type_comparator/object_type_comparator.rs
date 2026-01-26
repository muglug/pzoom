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

    // Compare named objects
    if let TAtomic::TNamedObject {
        name: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TNamedObject {
            name: input_name, ..
        } = input_type_part
        {
            // Same class
            if input_name == container_name {
                return true;
            }

            // Check if input extends/implements container
            if is_class_subtype_of(*input_name, *container_name, codebase) {
                return true;
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

/// Check if a class is a subtype of another (extends or implements).
pub fn is_class_subtype_of(
    input_class: StrId,
    container_class: StrId,
    codebase: &CodebaseInfo,
) -> bool {
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
