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
    if let TAtomic::TObjectIntersection { types } = container_type_part {
        for intersection_type in types {
            let mut intersection_result = TypeComparisonResult::new();
            if !is_contained_by(
                codebase,
                input_type_part,
                intersection_type,
                &mut intersection_result,
            ) {
                if intersection_result.type_coerced.unwrap_or(false) {
                    atomic_comparison_result.type_coerced = Some(true);
                }
                if intersection_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
                {
                    atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
                }
                return false;
            }
        }

        return true;
    }

    if let TAtomic::TObjectIntersection { types } = input_type_part {
        for intersection_type in types {
            if is_contained_by(
                codebase,
                intersection_type,
                container_type_part,
                atomic_comparison_result,
            ) {
                return true;
            }
        }

        return false;
    }

    // Template params compare using their bound ("as") type.
    if let TAtomic::TTemplateParam { as_type, .. } = input_type_part {
        return union_type_comparator::is_contained_by(
            codebase,
            as_type,
            &pzoom_code_info::TUnion::new(container_type_part.clone()),
            false,
            false,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TTemplateParam { as_type, .. } = container_type_part {
        return union_type_comparator::is_contained_by(
            codebase,
            &pzoom_code_info::TUnion::new(input_type_part.clone()),
            as_type,
            false,
            false,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = input_type_part {
        let class_string_input = TAtomic::TClassString {
            as_type: Some(Box::new((**as_type).clone())),
        };
        return is_contained_by(
            codebase,
            &class_string_input,
            container_type_part,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = container_type_part {
        let class_string_container = TAtomic::TClassString {
            as_type: Some(Box::new((**as_type).clone())),
        };
        return is_contained_by(
            codebase,
            input_type_part,
            &class_string_container,
            atomic_comparison_result,
        );
    }

    // Identical types are always contained
    if input_type_part == container_type_part {
        return true;
    }

    // Enum cases are valid instances of their declaring enum class.
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
        && input_enum == container_name
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
        && input_enum == container_name
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
        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
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
            return array_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                atomic_comparison_result,
            );
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
        }
    }

    // Iterable type comparisons
    if let TAtomic::TIterable {
        key_type: container_key,
        value_type: container_value,
    } = container_type_part
    {
        // Arrays are iterable
        if is_array_type(input_type_part) {
            return true;
        }

        // Only classes implementing Traversable (or descendants) are iterable.
        if let TAtomic::TNamedObject { name, .. } = input_type_part {
            if named_object_is_iterable(codebase, *name) {
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
                        return is_contained_by(
                            codebase,
                            input_as,
                            container_as,
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
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
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
        TAtomic::TObject | TAtomic::TNamedObject { .. } | TAtomic::TObjectIntersection { .. }
    )
}

/// Check if a type is an array type.
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

    // Mixed can be identical to anything
    if matches!(type1, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
        || matches!(type2, TAtomic::TMixed | TAtomic::TNonEmptyMixed)
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

    // Check if either is contained by the other
    let mut result = TypeComparisonResult::new();
    if is_contained_by(codebase, type1, type2, &mut result) {
        return true;
    }
    if is_contained_by(codebase, type2, type1, &mut result) {
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
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
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
        | TAtomic::TTruthyString => Some(StrictScalarIdentityFamily::String),
        _ => None,
    }
}
