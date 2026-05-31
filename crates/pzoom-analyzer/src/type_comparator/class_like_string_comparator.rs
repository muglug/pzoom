//! Class-like string comparator.
//!
//! Compares class-string / literal-class-string (and template-param-class) types
//! by reducing them to the class bound they reference. Mirrors Psalm's
//! `ClassLikeStringComparator`. (Hakana inlines this into its scalar comparator.)

use pzoom_code_info::{CodebaseInfo, TAtomic};

use super::{object_type_comparator, type_comparison_result::TypeComparisonResult};

fn object_like_atomic_is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
) -> bool {
    match container_type_part {
        TAtomic::TObject => matches!(
            input_type_part,
            TAtomic::TNamedObject { .. }
                | TAtomic::TObject
                | TAtomic::TObjectIntersection { .. }
                | TAtomic::TTemplateParam { .. }
                | TAtomic::TTemplateParamClass { .. }
        ),
        TAtomic::TNamedObject {
            name: container_name,
            ..
        } => match input_type_part {
            TAtomic::TNamedObject {
                name: input_name, ..
            } => {
                object_type_comparator::is_class_subtype_of(*input_name, *container_name, codebase)
            }
            TAtomic::TObjectIntersection { types } => types.iter().all(|atomic| {
                object_like_atomic_is_contained_by(codebase, atomic, container_type_part)
            }),
            TAtomic::TTemplateParam { as_type, .. } => as_type.types.iter().all(|atomic| {
                object_like_atomic_is_contained_by(codebase, atomic, container_type_part)
            }),
            TAtomic::TTemplateParamClass { as_type, .. } => {
                object_like_atomic_is_contained_by(codebase, as_type, container_type_part)
            }
            _ => false,
        },
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .all(|atomic| object_like_atomic_is_contained_by(codebase, input_type_part, atomic)),
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(|atomic| object_like_atomic_is_contained_by(codebase, input_type_part, atomic)),
        TAtomic::TTemplateParamClass { as_type, .. } => {
            object_like_atomic_is_contained_by(codebase, input_type_part, as_type)
        }
        _ => false,
    }
}

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if let TAtomic::TLiteralClassString { name: container_name } = container_type_part {
        if let TAtomic::TLiteralClassString { name: input_name } = input_type_part {
            return container_name == input_name;
        }

        let Some(container_class_id) = codebase.resolve_classlike_name(container_name) else {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        };

        let Some(fake_input_object) = classlike_string_to_object_bound(codebase, input_type_part)
        else {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        };

        let fake_container_object = TAtomic::TNamedObject {
            name: container_class_id,
            type_params: None,
        is_static: false, remapped_params: false };

        return object_like_atomic_is_contained_by(
            codebase,
            &fake_input_object,
            &fake_container_object,
        ) && object_like_atomic_is_contained_by(
            codebase,
            &fake_container_object,
            &fake_input_object,
        );
    }

    if matches!(
        (container_type_part, input_type_part),
        (TAtomic::TTemplateParamClass { .. }, TAtomic::TClassString { .. })
    ) {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    match container_type_part {
        TAtomic::TClassString {
            as_type: Some(container_bound),
        } if class_string_bound_accepts_unbounded_input(container_bound.as_ref()) => {
            return true;
        }
        TAtomic::TTemplateParamClass { as_type, .. }
            if class_string_bound_accepts_unbounded_input(as_type.as_ref()) =>
        {
            return true;
        }
        _ => {}
    }

    if matches!(container_type_part, TAtomic::TClassString { as_type: None }) {
        return true;
    }

    if class_string_is_unbounded(input_type_part) {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    let Some(fake_container_object) = classlike_string_to_object_bound(codebase, container_type_part)
    else {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    };

    let Some(fake_input_object) = classlike_string_to_object_bound(codebase, input_type_part) else {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    };

    object_like_atomic_is_contained_by(codebase, &fake_input_object, &fake_container_object)
}

fn class_string_is_unbounded(atomic: &TAtomic) -> bool {
    matches!(atomic, TAtomic::TClassString { as_type: None })
}

fn class_string_bound_accepts_unbounded_input(bound: &TAtomic) -> bool {
    match bound {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject => true,
        TAtomic::TTemplateParam { as_type, .. } => {
            as_type.is_mixed()
                || as_type
                    .types
                    .iter()
                    .any(class_string_bound_accepts_unbounded_input)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            class_string_bound_accepts_unbounded_input(as_type)
        }
        _ => false,
    }
}

fn classlike_string_to_object_bound(codebase: &CodebaseInfo, atomic: &TAtomic) -> Option<TAtomic> {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some((**as_type).clone()),
        TAtomic::TTemplateParamClass { as_type, .. } => Some((**as_type).clone()),
        TAtomic::TLiteralClassString { name } => codebase.resolve_classlike_name(name).map(|class_id| {
            TAtomic::TNamedObject {
                name: class_id,
                type_params: None,
            is_static: false, remapped_params: false }
        }),
        _ => None,
    }
}

