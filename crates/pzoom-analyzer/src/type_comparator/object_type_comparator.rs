//! Object type comparator.
//!
//! Handles comparison of object/class types, checking class hierarchy.

use pzoom_code_info::class_like_info::TemplateVariance;
use pzoom_code_info::{CodebaseInfo, TAtomic};
use pzoom_str::StrId;

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

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
        type_params: container_type_params,
        ..
    } = container_type_part
    {
        if let TAtomic::TNamedObject {
            name: input_name,
            type_params: input_type_params,
            ..
        } = input_type_part
        {
            // Same class
            if input_name == container_name {
                if !named_object_template_params_are_contained_by(
                    codebase,
                    *container_name,
                    input_type_params.as_deref(),
                    container_type_params.as_deref(),
                    atomic_comparison_result,
                ) {
                    return false;
                }

                return true;
            }

            // Generator is always traversable and iterator-like in Psalm semantics.
            if *input_name == StrId::GENERATOR
                && (*container_name == StrId::TRAVERSABLE || *container_name == StrId::ITERATOR)
            {
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

fn named_object_template_params_are_contained_by(
    codebase: &CodebaseInfo,
    class_name: StrId,
    input_type_params: Option<&[pzoom_code_info::TUnion]>,
    container_type_params: Option<&[pzoom_code_info::TUnion]>,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let Some(container_type_params) = container_type_params else {
        return true;
    };

    let Some(input_type_params) = input_type_params else {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    };

    if input_type_params.len() < container_type_params.len() {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    let class_template_variances = codebase
        .get_class(class_name)
        .map(|class_info| class_info.template_types.as_slice())
        .unwrap_or(&[]);

    for (index, container_param) in container_type_params.iter().enumerate() {
        let Some(input_param) = input_type_params.get(index) else {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        };

        let variance = class_template_variances
            .get(index)
            .map(|template_type| template_type.variance)
            .unwrap_or(TemplateVariance::Invariant);

        let mut forward_result = TypeComparisonResult::new();
        let mut reverse_result = TypeComparisonResult::new();

        let matches = match variance {
            TemplateVariance::Covariant => union_type_comparator::is_contained_by(
                codebase,
                input_param,
                container_param,
                false,
                false,
                &mut forward_result,
            ),
            TemplateVariance::Contravariant => union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                false,
                &mut forward_result,
            ),
            TemplateVariance::Invariant => {
                union_type_comparator::is_contained_by(
                    codebase,
                    input_param,
                    container_param,
                    false,
                    false,
                    &mut forward_result,
                ) && union_type_comparator::is_contained_by(
                    codebase,
                    container_param,
                    input_param,
                    false,
                    false,
                    &mut reverse_result,
                )
            }
        };

        if !matches {
            if forward_result.type_coerced.unwrap_or(false)
                || reverse_result.type_coerced.unwrap_or(false)
            {
                atomic_comparison_result.type_coerced = Some(true);
            }

            if forward_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
                || reverse_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
            {
                atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }
    }

    true
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
