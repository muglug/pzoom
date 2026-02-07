//! Scalar type comparator.
//!
//! Handles comparison of scalar types: int, float, string, bool, and their subtypes.

use pzoom_code_info::{CodebaseInfo, TAtomic, t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE};

use super::{object_type_comparator, type_comparison_result::TypeComparisonResult};

/// Check if an input scalar type is contained by a container scalar type.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // Identical types
    if input_type_part == container_type_part {
        return true;
    }

    // Int comparisons
    if matches!(container_type_part, TAtomic::TInt) {
        match input_type_part {
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. } => return true,
            _ => {}
        }
    }

    // Float comparisons
    if matches!(container_type_part, TAtomic::TFloat) {
        match input_type_part {
            TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. } => return true,
            _ => {}
        }
    }

    // String comparisons
    if matches!(container_type_part, TAtomic::TString) {
        match input_type_part {
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString => return true,
            _ => {}
        }
    }

    // Non-empty string comparisons
    if matches!(container_type_part, TAtomic::TNonEmptyString) {
        match input_type_part {
            TAtomic::TNonEmptyString
            | TAtomic::TTruthyString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. } => return true,
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }
                return !value.is_empty();
            }
            TAtomic::TString | TAtomic::TLowercaseString | TAtomic::TNumericString => {
                // These could be empty, so it's coerced
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Truthy string comparisons
    if matches!(container_type_part, TAtomic::TTruthyString) {
        match input_type_part {
            TAtomic::TTruthyString | TAtomic::TNonEmptyNumericString => return true,
            TAtomic::TNonEmptyString | TAtomic::TNonEmptyLowercaseString => {
                // Non-empty string could be "0" which is falsy
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            TAtomic::TNumericString => {
                // Numeric string can be "0", which is falsy
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Lowercase string comparisons
    if matches!(container_type_part, TAtomic::TLowercaseString) {
        match input_type_part {
            TAtomic::TLowercaseString | TAtomic::TNonEmptyLowercaseString => return true,
            TAtomic::TLiteralString { value } => {
                return value != NON_SPECIFIC_LITERAL_STRING_VALUE
                    && value.eq(&value.to_ascii_lowercase());
            }
            _ => {}
        }
    }

    // Non-empty lowercase string comparisons
    if matches!(container_type_part, TAtomic::TNonEmptyLowercaseString) {
        match input_type_part {
            TAtomic::TNonEmptyLowercaseString => return true,
            TAtomic::TLowercaseString => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            TAtomic::TLiteralString { value } => {
                return !value.is_empty()
                    && value != NON_SPECIFIC_LITERAL_STRING_VALUE
                    && value.eq(&value.to_ascii_lowercase());
            }
            _ => {}
        }
    }

    // Numeric string comparisons
    if matches!(container_type_part, TAtomic::TNumericString) {
        match input_type_part {
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => return true,
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }
                return value.parse::<f64>().is_ok();
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNonEmptyLowercaseString => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Numeric comparisons (int|float|numeric-string)
    if matches!(container_type_part, TAtomic::TNumeric) {
        match input_type_part {
            TAtomic::TNumeric
            | TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString => return true,
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }
                return value.parse::<f64>().is_ok();
            }
            _ => {}
        }
    }

    if let TAtomic::TLiteralString {
        value: container_value,
    } = container_type_part
    {
        if container_value == NON_SPECIFIC_LITERAL_STRING_VALUE {
            return matches!(input_type_part, TAtomic::TLiteralString { .. });
        }
    }

    // Class-like string comparisons
    if is_class_string_like_scalar(container_type_part)
        && is_class_string_like_scalar(input_type_part)
    {
        return classlike_string_is_contained_by(
            codebase,
            input_type_part,
            container_type_part,
            atomic_comparison_result,
        );
    }

    if is_class_string_like_scalar(container_type_part)
        && let TAtomic::TLiteralString { value } = input_type_part
    {
        if value != NON_SPECIFIC_LITERAL_STRING_VALUE
            && codebase.resolve_classlike_name(value).is_some()
        {
            let literal_class_string = TAtomic::TLiteralClassString {
                name: value.clone(),
            };

            return classlike_string_is_contained_by(
                codebase,
                &literal_class_string,
                container_type_part,
                atomic_comparison_result,
            );
        }

        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    if is_class_string_like_scalar(container_type_part) && is_plain_string_like_atomic(input_type_part)
    {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    // Bool comparisons
    if matches!(container_type_part, TAtomic::TBool) {
        match input_type_part {
            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => return true,
            _ => {}
        }
    }

    // Literal int comparisons
    if let TAtomic::TLiteralInt {
        value: container_value,
    } = container_type_part
    {
        if let TAtomic::TLiteralInt { value: input_value } = input_type_part {
            return input_value == container_value;
        }
    }

    // Literal float comparisons
    if let TAtomic::TLiteralFloat {
        value: container_value,
    } = container_type_part
    {
        if let TAtomic::TLiteralFloat { value: input_value } = input_type_part {
            return (input_value - container_value).abs() < f64::EPSILON;
        }
    }

    // Literal string comparisons
    if let TAtomic::TLiteralString {
        value: container_value,
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TLiteralString { value: input_value } => {
                return input_value == container_value;
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNonEmptyLowercaseString => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => {
                if container_value.parse::<f64>().is_ok() {
                    atomic_comparison_result.type_coerced = Some(true);
                }
                return false;
            }
            _ => {}
        }
    }

    // Positive int comparisons
    if matches!(container_type_part, TAtomic::TPositiveInt) {
        if let TAtomic::TLiteralInt { value } = input_type_part {
            return *value > 0;
        }
        if matches!(input_type_part, TAtomic::TPositiveInt) {
            return true;
        }
        if let TAtomic::TIntRange { min, .. } = input_type_part {
            return min.is_some_and(|min| min > 0);
        }
    }

    // Negative int comparisons
    if matches!(container_type_part, TAtomic::TNegativeInt) {
        if let TAtomic::TLiteralInt { value } = input_type_part {
            return *value < 0;
        }
        if matches!(input_type_part, TAtomic::TNegativeInt) {
            return true;
        }
        if let TAtomic::TIntRange { max, .. } = input_type_part {
            return max.is_some_and(|max| max < 0);
        }
    }

    // Int range comparisons
    if let TAtomic::TIntRange {
        min: container_min,
        max: container_max,
    } = container_type_part
    {
        if let TAtomic::TLiteralInt { value } = input_type_part {
            let min_ok = container_min.map_or(true, |m| *value >= m);
            let max_ok = container_max.map_or(true, |m| *value <= m);
            return min_ok && max_ok;
        }
        if let TAtomic::TIntRange {
            min: input_min,
            max: input_max,
        } = input_type_part
        {
            // Input range must be subset of container range
            let min_ok = match (container_min, input_min) {
                (Some(c), Some(i)) => *i >= *c,
                (Some(_), None) => false, // input has no min but container requires one
                (None, _) => true,
            };
            let max_ok = match (container_max, input_max) {
                (Some(c), Some(i)) => *i <= *c,
                (Some(_), None) => false,
                (None, _) => true,
            };
            return min_ok && max_ok;
        }

        if matches!(input_type_part, TAtomic::TPositiveInt) {
            let min_ok = container_min.is_none_or(|m| m <= 1);
            let max_ok = container_max.is_none();
            return min_ok && max_ok;
        }

        if matches!(input_type_part, TAtomic::TNegativeInt) {
            let min_ok = container_min.is_none();
            let max_ok = container_max.is_none_or(|m| m >= -1);
            return min_ok && max_ok;
        }

        if matches!(input_type_part, TAtomic::TInt) {
            return container_min.is_none() && container_max.is_none();
        }
    }

    // Numeric (int|float) comparisons
    if matches!(container_type_part, TAtomic::TNumeric) {
        match input_type_part {
            TAtomic::TNumeric
            | TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. } => return true,
            _ => {}
        }
    }

    // Scalar comparisons (int|float|string|bool)
    if matches!(container_type_part, TAtomic::TScalar) {
        match input_type_part {
            TAtomic::TScalar
            | TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumeric
            | TAtomic::TArrayKey
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString => return true,
            _ => {}
        }
    }

    // Array key (int|string) comparisons
    if matches!(container_type_part, TAtomic::TArrayKey) {
        match input_type_part {
            TAtomic::TArrayKey
            | TAtomic::TInt
            | TAtomic::TString
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString => return true,
            _ => {}
        }
    }

    false
}

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

fn is_class_string_like_scalar(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } | TAtomic::TTemplateParamClass { .. }
    )
}

fn is_plain_string_like_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
    )
}

fn classlike_string_is_contained_by(
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
            }
        }),
        _ => None,
    }
}
