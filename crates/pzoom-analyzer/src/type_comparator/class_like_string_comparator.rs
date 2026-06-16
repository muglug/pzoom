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
        // `object&callable(...)` intersections: an invokable class satisfies
        // the callable part via its __invoke signature.
        TAtomic::TCallable { .. } => {
            let mut callable_result = TypeComparisonResult::new();
            super::callable_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                &mut callable_result,
            )
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
    if let TAtomic::TLiteralClassString {
        name: container_name,
    } = container_type_part
    {
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
            is_static: false,
            remapped_params: false,
        };

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

    // A template-typed class-string container (`class-string<T>`) only accepts
    // an input that itself names a template; a concrete `class-string` (even a
    // bounded one like `class-string<C>` from `static::class`) is a coercion
    // (Psalm ClassLikeStringComparator's TTemplateParamClass arm).
    let is_template_class_string = |atomic: &TAtomic| match atomic {
        TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => matches!(as_type.as_ref(), TAtomic::TTemplateParam { .. }),
        _ => false,
    };
    let container_is_template_class_string = is_template_class_string(container_type_part);
    let input_is_template_class_string = is_template_class_string(input_type_part);
    if container_is_template_class_string
        && !input_is_template_class_string
        && matches!(input_type_part, TAtomic::TClassString { .. })
    {
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

    let Some(fake_container_object) =
        classlike_string_to_object_bound(codebase, container_type_part)
    else {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    };

    let Some(fake_input_object) = classlike_string_to_object_bound(codebase, input_type_part)
    else {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    };

    // Psalm compares the bound objects through the full atomic comparator with
    // the result threaded, so coercion/mixed flags from the bounds (e.g.
    // Traversable's implicit mixed params vs iterable<int>) reach the caller.
    super::atomic_type_comparator::is_contained_by(
        codebase,
        &fake_input_object,
        &fake_container_object,
        atomic_comparison_result,
    )
}

fn class_string_is_unbounded(atomic: &TAtomic) -> bool {
    matches!(atomic, TAtomic::TClassString { as_type: None })
}

fn class_string_bound_accepts_unbounded_input(bound: &TAtomic) -> bool {
    // Psalm's accept-anything case is a *plain* `class-string` container
    // (`as === 'object' && !as_type`); a templated bound stays rigid.
    matches!(
        bound,
        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
    )
}

fn classlike_string_to_object_bound(codebase: &CodebaseInfo, atomic: &TAtomic) -> Option<TAtomic> {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some(template_class_string_bound_object(as_type)),
        TAtomic::TTemplateParamClass { as_type, .. } => {
            Some(template_class_string_bound_object(as_type))
        }
        TAtomic::TLiteralClassString { name } => {
            codebase
                .resolve_classlike_name(name)
                .map(|class_id| TAtomic::TNamedObject {
                    name: class_id,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                })
        }
        _ => None,
    }
}

/// Psalm's fake-object construction dissolves a template class-string to the
/// template's *bound* (TTemplateParamClass::$as_type is the bound named
/// object, `object` when unbounded) — the template param atom itself never
/// reaches the object comparison.
fn template_class_string_bound_object(as_type: &TAtomic) -> TAtomic {
    match as_type {
        TAtomic::TTemplateParam { as_type: bound, .. } => {
            bound.get_single().cloned().unwrap_or(TAtomic::TObject)
        }
        other => other.clone(),
    }
}
