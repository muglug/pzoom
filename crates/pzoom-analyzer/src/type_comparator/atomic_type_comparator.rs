//! Atomic type comparator.
//!
//! The main entry point for comparing atomic types. Delegates to specialized
//! comparators for scalars, objects, arrays, and callables.

use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::{CodebaseInfo, TAtomic};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use super::{
    array_type_comparator, callable_type_comparator, object_type_comparator,
    scalar_type_comparator, type_comparison_result::TypeComparisonResult,
};
use crate::type_comparator::union_type_comparator;

/// Check if an input atomic type is contained by a container atomic type.
///
/// This is the main entry point for atomic type comparison.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    is_contained_by_in_context(
        codebase,
        input_type_part,
        container_type_part,
        false,
        atomic_comparison_result,
    )
}

/// `is_contained_by` with Psalm's `$allow_interface_equality` flag threaded
/// through: equality-tolerant contexts (identity checks, param defaults,
/// assertion filtering) let a template-param container accept any input
/// fitting its bound.
pub fn is_contained_by_in_context(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // The dependent `get_class()`/`gettype()` atomics are class-string/string
    // subtypes; compare them as their plain supertype (Psalm models them via
    // class inheritance, so they are transparently comparable as strings).
    if let Some(input_equiv) = input_type_part.dependent_string_equivalent() {
        return is_contained_by_in_context(
            codebase,
            &input_equiv,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }
    if let Some(container_equiv) = container_type_part.dependent_string_equivalent() {
        return is_contained_by_in_context(
            codebase,
            input_type_part,
            &container_equiv,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    // An unresolved conditional type compares through its branches (Psalm's
    // AtomicTypeComparator): as a container, the input must fit one branch
    // atomic; as an input, one branch atomic must fit the container.
    if let TAtomic::TConditional(container_conditional) = container_type_part {
        return container_conditional
            .if_true_type
            .types
            .iter()
            .chain(container_conditional.if_false_type.types.iter())
            .any(|container_branch_part| {
                is_contained_by_in_context(
                    codebase,
                    input_type_part,
                    container_branch_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                )
            });
    }
    if let TAtomic::TConditional(input_conditional) = input_type_part {
        return input_conditional
            .if_true_type
            .types
            .iter()
            .chain(input_conditional.if_false_type.types.iter())
            .any(|input_branch_part| {
                is_contained_by_in_context(
                    codebase,
                    input_branch_part,
                    container_type_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                )
            });
    }

    // A `class-string-map` compares as `array<class-string<placeholder>, value>`
    // on either side (Psalm's ArrayTypeComparator substitutes
    // `new TArray([getStandinKeyParam(), value_param])` before comparing).
    if let Some(input_equiv) = input_type_part.get_class_string_map_as_array() {
        return is_contained_by_in_context(
            codebase,
            &input_equiv,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }
    if let Some(container_equiv) = container_type_part.get_class_string_map_as_array() {
        return is_contained_by_in_context(
            codebase,
            input_type_part,
            &container_equiv,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TObjectIntersection { types } = container_type_part {
        for intersection_type in types {
            let mut intersection_result = TypeComparisonResult::new();
            if !is_contained_by_in_context(
                codebase,
                input_type_part,
                intersection_type,
                allow_interface_equality,
                &mut intersection_result,
            ) {
                if intersection_result.type_coerced.unwrap_or(false) {
                    atomic_comparison_result.type_coerced = Some(true);
                }
                if intersection_result
                    .type_coerced_from_mixed
                    .unwrap_or(false)
                {
                    atomic_comparison_result.type_coerced_from_mixed = Some(true);
                }
                return false;
            }
        }

        return true;
    }

    if let TAtomic::TObjectIntersection { types } = input_type_part {
        for intersection_type in types {
            if is_contained_by_in_context(
                codebase,
                intersection_type,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            ) {
                return true;
            }
        }

        return false;
    }

    // Two template params compare *shallowly* (Psalm's ObjectComparator::
    // isShallowlyContainedBy): identity, declared-bound links, or an
    // `@extends` mapping — never via mutual bound containment.
    if let (
        TAtomic::TTemplateParam {
            name: input_name,
            defining_entity: input_entity,
            as_type: input_as,
        },
        TAtomic::TTemplateParam {
            name: container_name,
            defining_entity: container_entity,
            as_type: container_as,
        },
    ) = (input_type_part, container_type_part)
    {
        return template_param_shallowly_contained_by(
            codebase,
            (*input_name, input_entity, input_as),
            (*container_name, container_entity, container_as),
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    // An input template param compares using its bound ("as") type — a value
    // of type `T as Foo` is always a Foo (Psalm AtomicTypeComparator's
    // input-TTemplateParam arm).
    if let TAtomic::TTemplateParam { as_type, .. } = input_type_part {
        return union_type_comparator::is_contained_by_in_context(
            codebase,
            as_type,
            &pzoom_code_info::TUnion::new(container_type_part.clone()),
            false,
            false,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    // A template param *container* is rigid: it stands for whatever type the
    // caller chooses, so a concrete input only satisfies it in
    // equality-tolerant contexts (Psalm's container-TTemplateParam arm honors
    // a bound match only under allow_interface_equality). The bound
    // comparison always runs so coercion flags accumulate (LessSpecific vs
    // Invalid classification).
    if let TAtomic::TTemplateParam { as_type, .. } = container_type_part {
        // Psalm: `mixed` input is contained by an as-mixed template container.
        if matches!(
            input_type_part,
            TAtomic::TMixed | TAtomic::TNonEmptyMixed
        ) {
            if as_type.is_mixed() {
                return true;
            }
            atomic_comparison_result.type_coerced = Some(true);
            atomic_comparison_result.type_coerced_from_mixed = Some(true);
            return false;
        }
        // Psalm: `never` input is contained by anything.
        if matches!(input_type_part, TAtomic::TNothing) {
            return true;
        }
        // Psalm: `null` input is contained by a template with a nullable (or
        // mixed) bound.
        if matches!(input_type_part, TAtomic::TNull) {
            return as_type.is_nullable() || as_type.is_mixed();
        }

        let fits_bound = union_type_comparator::is_contained_by_in_context(
            codebase,
            &pzoom_code_info::TUnion::new(input_type_part.clone()),
            as_type,
            false,
            false,
            allow_interface_equality,
            atomic_comparison_result,
        );
        return allow_interface_equality && fits_bound;
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = input_type_part {
        let class_string_input = TAtomic::TClassString {
            as_type: Some(Box::new((**as_type).clone())),
        };
        return is_contained_by_in_context(
            codebase,
            &class_string_input,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = container_type_part {
        let class_string_container = TAtomic::TClassString {
            as_type: Some(Box::new((**as_type).clone())),
        };
        return is_contained_by_in_context(
            codebase,
            input_type_part,
            &class_string_container,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    // `key-of<T>` / `value-of<T>` on an unresolved template. Two deferred types of the
    // same template parameter are interchangeable; otherwise they compare via the keys
    // (resp. values) of the template's bound. A concrete value is never contained by a
    // deferred template key-of/value-of (the real keys/values depend on the eventual
    // instantiation), mirroring Psalm's TTemplateKeyOf/TTemplateValueOf handling.
    if let TAtomic::TTemplateKeyOf {
        param_name: input_param,
        as_type: input_as,
        ..
    } = input_type_part
    {
        if let TAtomic::TTemplateKeyOf {
            param_name: container_param,
            ..
        } = container_type_part
            && input_param == container_param
        {
            return true;
        }
        return union_type_comparator::is_contained_by(
            codebase,
            &pzoom_code_info::ttype::get_key_of_union(input_as),
            &pzoom_code_info::TUnion::new(container_type_part.clone()),
            false,
            false,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TTemplateValueOf {
        param_name: input_param,
        as_type: input_as,
        ..
    } = input_type_part
    {
        if let TAtomic::TTemplateValueOf {
            param_name: container_param,
            ..
        } = container_type_part
            && input_param == container_param
        {
            return true;
        }
        return union_type_comparator::is_contained_by(
            codebase,
            &pzoom_code_info::ttype::get_value_of_union(input_as),
            &pzoom_code_info::TUnion::new(container_type_part.clone()),
            false,
            false,
            atomic_comparison_result,
        );
    }

    // A non-template input is not contained by a deferred template key-of/value-of:
    // the actual keys/values are unknown until the template is bound.
    if matches!(
        container_type_part,
        TAtomic::TTemplateKeyOf { .. } | TAtomic::TTemplateValueOf { .. }
    ) {
        return false;
    }

    // Identical types are always contained
    if input_type_part == container_type_part {
        return true;
    }

    // Enum cases are valid instances of their declaring enum class — and of
    // anything the enum implements (Psalm treats the case as the enum's
    // storage for object containment).
    if let (
        TAtomic::TEnumCase {
            enum_name: input_enum,
            ..
        },
        TAtomic::TNamedObject {
            name: container_name,
            ..
        },
    ) = (input_type_part, container_type_part)
        && (input_enum == container_name
            || object_type_comparator::is_class_subtype_of(
                *input_enum,
                *container_name,
                codebase,
            ))
    {
        return true;
    }

    if let (
        TAtomic::TEnum { name: input_enum },
        TAtomic::TNamedObject {
            name: container_name,
            ..
        },
    ) = (input_type_part, container_type_part)
        && (input_enum == container_name
            || object_type_comparator::is_class_subtype_of(
                *input_enum,
                *container_name,
                codebase,
            ))
    {
        return true;
    }

    // Enum cases and enum classes are objects (Psalm's TEnumCase extends
    // TNamedObject), so a bare `object` container always accepts them.
    if matches!(container_type_part, TAtomic::TObject)
        && matches!(
            input_type_part,
            TAtomic::TEnumCase { .. } | TAtomic::TEnum { .. }
        )
    {
        return true;
    }

    // Mixed contains everything
    if matches!(
        container_type_part,
        TAtomic::TMixed | TAtomic::TNonEmptyMixed
    ) {
        return true;
    }

    // Nothing is contained by everything (never type)
    if matches!(input_type_part, TAtomic::TNothing) {
        return true;
    }

    // Mixed input requires coercion
    if matches!(input_type_part, TAtomic::TMixed | TAtomic::TNonEmptyMixed) {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_mixed = Some(true);
        return false;
    }

    // A template whose bound is mixed is as good as mixed (Psalm
    // AtomicTypeComparator: template-as-mixed inputs coerce from mixed); the
    // as-mixed flag marks the template origin, which suppresses Mixed*
    // reporting downstream.
    if let TAtomic::TTemplateParam { as_type, .. } = input_type_part
        && as_type.is_mixed()
    {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_mixed = Some(true);
        atomic_comparison_result.type_coerced_from_as_mixed = Some(true);
        return false;
    }

    // Null comparisons
    if matches!(input_type_part, TAtomic::TNull) {
        // Null only contained by null or mixed (handled above)
        return false;
    }

    // Void comparisons
    if matches!(input_type_part, TAtomic::TVoid) {
        // Void only contained by void or mixed
        return false;
    }

    // Scalar type comparisons
    if is_scalar_type(input_type_part) && is_scalar_type(container_type_part) {
        return scalar_type_comparator::is_contained_by(
            codebase,
            input_type_part,
            container_type_part,
            atomic_comparison_result,
        );
    }

    // Scalar is contained by TScalar
    if matches!(container_type_part, TAtomic::TScalar) && is_scalar_type(input_type_part) {
        return true;
    }

    // Arrays satisfy Countable.
    if let TAtomic::TNamedObject { name, .. } = container_type_part {
        if *name == StrId::COUNTABLE && is_array_type(input_type_part) {
            return true;
        }
    }

    // Object type comparisons
    if is_object_type(input_type_part) || is_object_type(container_type_part) {
        if is_object_type(input_type_part) && is_object_type(container_type_part) {
            return object_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                atomic_comparison_result,
            );
        }

        // TObject contains any object-like type
        if matches!(container_type_part, TAtomic::TObject) {
            if is_object_type(input_type_part)
                || matches!(input_type_part, TAtomic::TClosure { .. })
            {
                return true;
            }
        }

        // TClosure is contained by TNamedObject { name: Closure }
        if let TAtomic::TNamedObject { name, .. } = container_type_part {
            if *name == StrId::CLOSURE && matches!(input_type_part, TAtomic::TClosure { .. }) {
                return true;
            }
        }
    }

    // Array type comparisons
    if is_array_type(input_type_part) || is_array_type(container_type_part) {
        if is_array_type(input_type_part) && is_array_type(container_type_part) {
            // Array element / shape value types are always declared in docblocks,
            // and Psalm gates `scalar_type_match_found` on the container element's
            // `from_docblock` flag — so a scalar mismatch inside an array never
            // counts as a scalar match (it stays an `InvalidArgument`, not
            // `InvalidScalarArgument`). pzoom has no per-atomic docblock flag, so we
            // preserve the incoming value across the nested element comparisons.
            let saved_scalar_match = atomic_comparison_result.scalar_type_match_found;
            let keyed_shape_pair = matches!(input_type_part, TAtomic::TKeyedArray { .. })
                && matches!(container_type_part, TAtomic::TKeyedArray { .. });
            atomic_comparison_result.scalar_type_match_found = None;
            let result = array_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                atomic_comparison_result,
            );
            // Shape-vs-shape comparisons compute the flag deliberately
            // (KeyedArrayComparator's per-property propagation); every other
            // nested element comparison keeps the incoming value.
            if !keyed_shape_pair || atomic_comparison_result.scalar_type_match_found.is_none() {
                atomic_comparison_result.scalar_type_match_found = saved_scalar_match;
            }
            return result;
        }
    }

    // Callable/Closure comparisons
    if is_callable_type(input_type_part) || is_callable_type(container_type_part) {
        if is_callable_type(container_type_part) {
            // A Closure object instance is callable.
            if matches!(
                input_type_part,
                TAtomic::TNamedObject { name, .. } if *name == StrId::CLOSURE
            ) {
                return true;
            }

            // A `callable-object` (Psalm's TCallableObject) is callable.
            if matches!(
                input_type_part,
                TAtomic::TObjectWithProperties {
                    is_invokable: true,
                    ..
                }
            ) {
                return true;
            }

            // Check if input can be callable
            if is_callable_type(input_type_part) {
                return callable_type_comparator::is_contained_by(
                    codebase,
                    input_type_part,
                    container_type_part,
                    atomic_comparison_result,
                );
            }

            // Strings can be callable
            if matches!(
                input_type_part,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TClassString { .. }
            ) {
                return true;
            }

            // Arrays can be callable
            if is_array_type(input_type_part) {
                return true;
            }

            // An object declaring __invoke is callable when its signature
            // satisfies the container's.
            if let TAtomic::TNamedObject { name, .. } = input_type_part
                && codebase
                    .get_class(*name)
                    .is_some_and(|class_info| class_info.methods.contains_key(&StrId::INVOKE))
            {
                return callable_type_comparator::is_contained_by(
                    codebase,
                    input_type_part,
                    container_type_part,
                    atomic_comparison_result,
                );
            }
        }
    }

    // Iterable type comparisons
    if let TAtomic::TIterable {
        key_type: container_key,
        value_type: container_value,
    } = container_type_part
    {
        // Arrays are iterable; check that the element key/value types are compatible.
        if is_array_type(input_type_part) {
            if container_key.is_mixed() && container_value.is_mixed() {
                return true;
            }

            // A keyed shape checks each entry against the container's value
            // union individually: combining distinct entry shapes first would
            // merge them into one all-optional shape matching neither union
            // branch (Psalm compares per property).
            if let TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } = input_type_part
            {
                use pzoom_code_info::ArrayKey;

                let key_contained = |key_union: &pzoom_code_info::TUnion,
                                     atomic_comparison_result: &mut TypeComparisonResult| {
                    container_key.is_mixed()
                        || union_type_comparator::is_contained_by(
                            codebase,
                            key_union,
                            container_key,
                            false,
                            false,
                            atomic_comparison_result,
                        )
                };
                let value_contained = |value_union: &pzoom_code_info::TUnion,
                                       atomic_comparison_result: &mut TypeComparisonResult| {
                    container_value.is_mixed()
                        || union_type_comparator::is_contained_by(
                            codebase,
                            value_union,
                            container_value,
                            false,
                            false,
                            atomic_comparison_result,
                        )
                };

                for (key, value) in properties.iter() {
                    let key_union = match key {
                        ArrayKey::Int(i) => {
                            pzoom_code_info::TUnion::new(TAtomic::TLiteralInt { value: *i })
                        }
                        ArrayKey::String(s) => {
                            pzoom_code_info::TUnion::new(TAtomic::TLiteralString {
                                value: s.to_string(),
                            })
                        }
                    };
                    if !key_contained(&key_union, atomic_comparison_result)
                        || !value_contained(value, atomic_comparison_result)
                    {
                        return false;
                    }
                }
                if let Some(fallback_key_type) = fallback_key_type
                    && !key_contained(fallback_key_type, atomic_comparison_result)
                {
                    return false;
                }
                if let Some(fallback_value_type) = fallback_value_type
                    && !value_contained(fallback_value_type, atomic_comparison_result)
                {
                    return false;
                }
                return true;
            }

            if let Some((input_key, input_value)) = array_atomic_key_value_types(input_type_part) {
                let key_ok = container_key.is_mixed()
                    || union_type_comparator::is_contained_by(
                        codebase,
                        &input_key,
                        container_key,
                        false,
                        false,
                        atomic_comparison_result,
                    );
                let value_ok = container_value.is_mixed()
                    || union_type_comparator::is_contained_by(
                        codebase,
                        &input_value,
                        container_value,
                        false,
                        false,
                        atomic_comparison_result,
                    );
                return key_ok && value_ok;
            }

            return true;
        }

        // Only classes implementing Traversable (or descendants) are iterable.
        if let TAtomic::TNamedObject { name, type_params , .. } = input_type_part {
            if named_object_is_iterable(codebase, *name) {
                if container_key.is_mixed() && container_value.is_mixed() {
                    return true;
                }

                // The built-in iterables expose <TKey, TValue> as their first two
                // type parameters, so their iteration types can be checked directly.
                if matches!(
                    *name,
                    StrId::GENERATOR
                        | StrId::TRAVERSABLE
                        | StrId::ITERATOR
                        | StrId::ITERATOR_AGGREGATE
                ) {
                    if let Some(params) = type_params
                        && params.len() >= 2
                    {
                        let key_ok = container_key.is_mixed()
                            || union_type_comparator::is_contained_by(
                                codebase,
                                &params[0],
                                container_key,
                                false,
                                false,
                                atomic_comparison_result,
                            );
                        let value_ok = container_value.is_mixed()
                            || union_type_comparator::is_contained_by(
                                codebase,
                                &params[1],
                                container_value,
                                false,
                                false,
                                atomic_comparison_result,
                            );
                        return key_ok && value_ok;
                    }

                    // No type params: the input's iteration types are its
                    // template defaults. A container slot that is itself a
                    // mixed-bound template param accepts them (Psalm compares
                    // bound-to-bound and succeeds); a concrete slot (e.g.
                    // `int`) reports a real coercion from mixed.
                    let accepts_unknown = |slot: &pzoom_code_info::TUnion| {
                        slot.is_mixed()
                            || slot.types.iter().all(|atomic| {
                                matches!(
                                    atomic,
                                    TAtomic::TTemplateParam { as_type, .. } if as_type.is_mixed()
                                )
                            })
                    };
                    if accepts_unknown(container_key) && accepts_unknown(container_value) {
                        return true;
                    }
                    atomic_comparison_result.type_coerced = Some(true);
                    atomic_comparison_result.type_coerced_from_mixed = Some(true);
                    return false;
                }

                return true;
            }
        }

        // Another iterable
        if let TAtomic::TIterable {
            key_type: input_key,
            value_type: input_value,
        } = input_type_part
        {
            let key_ok = container_key.is_mixed()
                || super::union_type_comparator::is_contained_by(
                    codebase,
                    input_key,
                    container_key,
                    false,
                    false,
                    atomic_comparison_result,
                );
            let value_ok = container_value.is_mixed()
                || super::union_type_comparator::is_contained_by(
                    codebase,
                    input_value,
                    container_value,
                    false,
                    false,
                    atomic_comparison_result,
                );
            return key_ok && value_ok;
        }
    }

    // Enum comparisons
    if let TAtomic::TEnum {
        name: container_name,
    } = container_type_part
    {
        if let TAtomic::TEnum { name: input_name } = input_type_part {
            return input_name == container_name;
        }
        if let TAtomic::TEnumCase {
            enum_name: input_name,
            ..
        } = input_type_part
        {
            return input_name == container_name;
        }
    }

    if let TAtomic::TEnumCase {
        enum_name: container_enum,
        case_name: container_case,
    } = container_type_part
    {
        if let TAtomic::TEnumCase {
            enum_name: input_enum,
            case_name: input_case,
        } = input_type_part
        {
            return input_enum == container_enum && input_case == container_case;
        }
    }

    // Resource comparisons
    if matches!(container_type_part, TAtomic::TResource) {
        if matches!(
            input_type_part,
            TAtomic::TResource | TAtomic::TClosedResource
        ) {
            return true;
        }
    }

    // Class-string comparisons
    if let TAtomic::TClassString {
        as_type: container_as,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TClassString { as_type: input_as } => {
                if let Some(container_as) = container_as {
                    if let Some(input_as) = input_as {
                        return is_contained_by_in_context(
                            codebase,
                            input_as,
                            container_as,
                            allow_interface_equality,
                            atomic_comparison_result,
                        );
                    }

                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }

                return true;
            }
            TAtomic::TLiteralClassString { .. } => {
                // Only unconstrained class-string can safely accept literal class-string here.
                return container_as.is_none();
            }
            _ => {}
        }
    }

    false
}

fn named_object_is_iterable(codebase: &CodebaseInfo, class_name: StrId) -> bool {
    if matches!(
        class_name,
        StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
    ) {
        return true;
    }

    let mut to_visit = vec![class_name];
    let mut visited = FxHashSet::default();

    while let Some(current) = to_visit.pop() {
        if !visited.insert(current) {
            continue;
        }

        if matches!(
            current,
            StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
        ) {
            return true;
        }

        let Some(class_info) = codebase.get_class(current) else {
            continue;
        };

        if let Some(parent_class) = class_info.parent_class {
            to_visit.push(parent_class);
        }

        to_visit.extend(class_info.interfaces.iter().copied());
        to_visit.extend(class_info.all_parent_interfaces.iter().copied());
    }

    false
}

/// Check if a type is a scalar type.
fn is_scalar_type(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TCallableString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TNumeric
            | TAtomic::TArrayKey
            | TAtomic::TScalar
    )
}

/// Check if a type is an object type.
fn is_object_type(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TObjectIntersection { .. }
            | TAtomic::TObjectWithProperties { .. }
    )
}

/// Check if a type is an array type.
/// Extract the (key, value) element types of an array-like atomic, for comparing
/// against an iterable's type parameters.
fn array_atomic_key_value_types(
    atomic: &TAtomic,
) -> Option<(pzoom_code_info::TUnion, pzoom_code_info::TUnion)> {
    use pzoom_code_info::{ArrayKey, TUnion, combine_union_types};

    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
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
            let mut key_union: Option<TUnion> =
                fallback_key_type.as_ref().map(|k| (**k).clone());
            let mut value_union: Option<TUnion> =
                fallback_value_type.as_ref().map(|v| (**v).clone());

            for (key, value) in properties.iter() {
                let key_atomic = match key {
                    ArrayKey::Int(i) => TAtomic::TLiteralInt { value: *i },
                    ArrayKey::String(s) => TAtomic::TLiteralString { value: s.clone() },
                };
                let key_t = TUnion::new(key_atomic);
                key_union = Some(match key_union {
                    Some(existing) => combine_union_types(&existing, &key_t, false),
                    None => key_t,
                });
                value_union = Some(match value_union {
                    Some(existing) => combine_union_types(&existing, value, false),
                    None => value.clone(),
                });
            }

            Some((
                key_union.unwrap_or_else(TUnion::array_key),
                value_union.unwrap_or_else(TUnion::mixed),
            ))
        }
        _ => None,
    }
}

fn is_array_type(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
    )
}

/// Check if a type is a callable type.
fn is_callable_type(atomic: &TAtomic) -> bool {
    matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. })
}

fn named_objects_might_overlap(codebase: &CodebaseInfo, left: StrId, right: StrId) -> bool {
    if object_type_comparator::is_class_subtype_of(left, right, codebase)
        || object_type_comparator::is_class_subtype_of(right, left, codebase)
    {
        return true;
    }

    let Some(left_info) = codebase.get_class(left) else {
        return true;
    };
    let Some(right_info) = codebase.get_class(right) else {
        return true;
    };

    left_info.kind == ClassLikeKind::Interface || right_info.kind == ClassLikeKind::Interface
}

/// Check if two atomic types can be identical (used for type assertions).
pub fn can_be_identical(codebase: &CodebaseInfo, type1: &TAtomic, type2: &TAtomic) -> bool {
    // Same types can always be identical
    if type1 == type2 {
        return true;
    }

    // Compare the dependent get_class()/gettype() atomics as their string supertype.
    if let Some(equiv) = type1.dependent_string_equivalent() {
        return can_be_identical(codebase, &equiv, type2);
    }
    if let Some(equiv) = type2.dependent_string_equivalent() {
        return can_be_identical(codebase, type1, &equiv);
    }

    // A `class-string-map` behaves as `array<class-string<placeholder>, value>`.
    if let Some(equiv) = type1.get_class_string_map_as_array() {
        return can_be_identical(codebase, &equiv, type2);
    }
    if let Some(equiv) = type2.get_class_string_map_as_array() {
        return can_be_identical(codebase, type1, &equiv);
    }

    // Mixed can be identical to anything
    if matches!(type1, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
        || matches!(type2, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
    {
        return true;
    }

    // A type variable can be identical to anything (Hakana's
    // `can_be_identical`); its constraints are reconciled at function end.
    if matches!(type1, TAtomic::TTypeVariable { .. })
        || matches!(type2, TAtomic::TTypeVariable { .. })
    {
        return true;
    }

    if is_array_type(type1) && is_array_type(type2) {
        return true;
    }

    if matches!(
        type1,
        TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
    ) && matches!(
        type2,
        TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
    ) {
        return true;
    }

    if let (
        TAtomic::TNamedObject {
            name: left_name, ..
        },
        TAtomic::TNamedObject {
            name: right_name, ..
        },
    ) = (type1, type2)
        && named_objects_might_overlap(codebase, *left_name, *right_name)
    {
        return true;
    }

    // `===` requires the same runtime scalar kind. Assignment containment allows
    // int->float widening, which is not valid for identity comparisons.
    if let (Some(left_family), Some(right_family)) = (
        strict_scalar_identity_family(type1),
        strict_scalar_identity_family(type2),
    ) && left_family != right_family
    {
        return false;
    }

    // Check if either is contained by the other. Psalm's canBeIdentical
    // passes allow_interface_equality=true, so a template param can be
    // identical to anything fitting its bound.
    let mut result = TypeComparisonResult::new();
    if is_contained_by_in_context(codebase, type1, type2, true, &mut result) {
        return true;
    }
    if is_contained_by_in_context(codebase, type2, type1, true, &mut result) {
        return true;
    }

    false
}

/// Two template params compare shallowly — a port of the template-vs-template
/// rules in Psalm's `ObjectComparator::isShallowlyContainedBy` /
/// `isIntersectionShallowlyContainedBy` (with pzoom's standing
/// `allow_interface_equality = false`):
///
/// 1. The same param (name + defining entity) is contained by itself.
/// 2. Two function-defined templates from *different* functions are
///    interchangeable.
/// 3. `T1 as T2` is contained by `T2` (the input's bound names the container).
/// 4. Templates from different entities with single-atomic bounds compare via
///    those bounds when both are named objects (shallowly) or both `mixed`
///    (true) — unless the input is a method template of the container's own
///    class (Psalm extracts the class from the `fn-class::method` id; pzoom's
///    interner-free comparator can't, an accepted leniency).
/// 5. A class-defined input whose class fills the container's template via
///    `@extends`/`@implements` is contained by it.
fn template_param_shallowly_contained_by(
    codebase: &CodebaseInfo,
    (input_name, input_entity, input_as): (
        StrId,
        &pzoom_code_info::GenericParent,
        &pzoom_code_info::TUnion,
    ),
    (container_name, container_entity, container_as): (
        StrId,
        &pzoom_code_info::GenericParent,
        &pzoom_code_info::TUnion,
    ),
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    use pzoom_code_info::GenericParent;

    // Psalm's isShallowlyContainedBy pre-check: different defining entities
    // with single-atomic bounds compare via the bounds when the input is not
    // defined on (a method of) the container's class.
    if input_entity != container_entity
        && input_as.types.len() == 1
        && container_as.types.len() == 1
    {
        let container_in_fn = matches!(container_entity, GenericParent::FunctionLike(_));

        if !container_in_fn {
            match (input_as.types.first(), container_as.types.first()) {
                (
                    Some(input_bound @ TAtomic::TNamedObject { .. }),
                    Some(container_bound @ TAtomic::TNamedObject { .. }),
                ) => {
                    return is_contained_by_in_context(
                        codebase,
                        input_bound,
                        container_bound,
                        allow_interface_equality,
                        atomic_comparison_result,
                    );
                }
                (
                    Some(TAtomic::TMixed | TAtomic::TNonEmptyMixed),
                    Some(TAtomic::TMixed | TAtomic::TNonEmptyMixed),
                ) => {
                    return true;
                }
                _ => {}
            }
        }
    }

    // Psalm's function-template special rules apply only outside
    // equality-tolerant contexts (gated on !allow_interface_equality).
    if !allow_interface_equality {
        // Two function-defined templates from different functions.
        if let (GenericParent::FunctionLike(_), GenericParent::FunctionLike(_)) =
            (input_entity, container_entity)
            && input_entity != container_entity
        {
            return true;
        }

        // `T1 as T2` satisfies `T2` — the input's bound names the container
        // template exactly.
        if input_as.types.iter().any(|input_as_atomic| {
            matches!(
                input_as_atomic,
                TAtomic::TTemplateParam {
                    name,
                    defining_entity,
                    ..
                } if *name == container_name && defining_entity == container_entity
            )
        }) {
            return true;
        }
    }

    // The same template param.
    if input_name == container_name && input_entity == container_entity {
        return true;
    }

    // A class-defined input template whose class fills the container's
    // template through the `@extends`/`@implements` chain.
    if let (GenericParent::ClassLike(input_class), GenericParent::ClassLike(container_class)) =
        (input_entity, container_entity)
        && let Some(input_class_info) = codebase.get_class(*input_class)
        && input_class_info
            .template_extended_params
            .get(container_class)
            .is_some_and(|template_map| template_map.contains_key(&container_name))
    {
        return true;
    }

    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StrictScalarIdentityFamily {
    Int,
    Float,
    String,
    Bool,
}

fn strict_scalar_identity_family(atomic: &TAtomic) -> Option<StrictScalarIdentityFamily> {
    match atomic {
        TAtomic::TInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TIntRange { .. } => Some(StrictScalarIdentityFamily::Int),
        TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => Some(StrictScalarIdentityFamily::Float),
        TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => Some(StrictScalarIdentityFamily::Bool),
        TAtomic::TString
        | TAtomic::TLiteralString { .. }
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TNonEmptyString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TCallableString => Some(StrictScalarIdentityFamily::String),
        _ => None,
    }
}
