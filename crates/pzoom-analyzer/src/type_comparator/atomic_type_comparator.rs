//! Atomic type comparator.
//!
//! The main entry point for comparing atomic types. Delegates to specialized
//! comparators for scalars, objects, arrays, and callables.

use pzoom_code_info::{CodebaseInfo, TAtomic};
use pzoom_str::StrId;

use super::{
    array_type_comparator, callable_type_comparator, object_type_comparator,
    scalar_type_comparator, type_comparison_result::TypeComparisonResult,
};

/// Check if an input atomic type is contained by a container atomic type.
///
/// This is the main entry point for atomic type comparison.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Identical types are always contained
    if input_type_part == container_type_part {
        return true;
    }

    // Mixed contains everything
    if matches!(container_type_part, TAtomic::TMixed | TAtomic::TNonEmptyMixed) {
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

        // Generators and Traversables are iterable
        if let TAtomic::TNamedObject { name, .. } = input_type_part {
            // Check if it implements Traversable/Iterator
            if let Some(_class_info) = codebase.get_class(*name) {
                // TODO: properly check if class implements Traversable/Iterator
                // For now, accept any object as potentially iterable
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
    if let TAtomic::TEnum { name: container_name } = container_type_part {
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
        as_type: _container_as,
    } = container_type_part
    {
        if matches!(
            input_type_part,
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
        ) {
            // TODO: Check as_type compatibility
            return true;
        }
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
    matches!(atomic, TAtomic::TObject | TAtomic::TNamedObject { .. })
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

/// Check if two atomic types can be identical (used for type assertions).
pub fn can_be_identical(
    codebase: &CodebaseInfo,
    type1: &TAtomic,
    type2: &TAtomic,
) -> bool {
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
