//! Scalar type comparator.
//!
//! Handles comparison of scalar types: int, float, string, bool, and their subtypes.

use pzoom_code_info::{CodebaseInfo, TAtomic};

use super::type_comparison_result::TypeComparisonResult;

/// Check if an input scalar type is contained by a container scalar type.
pub fn is_contained_by(
    _codebase: &CodebaseInfo,
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
            TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => return true,
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
            | TAtomic::TNonEmptyLowercaseString => return true,
            TAtomic::TLiteralString { value: _ } => {
                // Empty string doesn't match non-empty-string
                // Note: value is StrId, we'd need interner to check if empty
                // For now, assume literal strings are non-empty
                return true;
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
            _ => {}
        }
    }

    // Lowercase string comparisons
    if matches!(container_type_part, TAtomic::TLowercaseString) {
        match input_type_part {
            TAtomic::TLowercaseString | TAtomic::TNonEmptyLowercaseString => return true,
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
            _ => {}
        }
    }

    // Numeric string comparisons
    if matches!(container_type_part, TAtomic::TNumericString) {
        match input_type_part {
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => return true,
            _ => {}
        }
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
        if let TAtomic::TLiteralString { value: input_value } = input_type_part {
            return input_value == container_value;
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
    }

    // Negative int comparisons
    if matches!(container_type_part, TAtomic::TNegativeInt) {
        if let TAtomic::TLiteralInt { value } = input_type_part {
            return *value < 0;
        }
        if matches!(input_type_part, TAtomic::TNegativeInt) {
            return true;
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
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString => return true,
            _ => {}
        }
    }

    false
}
