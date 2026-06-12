//! Scalar type comparator.
//!
//! Handles comparison of scalar types: int, float, string, bool, and their subtypes.

use pzoom_code_info::{CodebaseInfo, TAtomic, t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE};

use super::type_comparison_result::TypeComparisonResult;

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
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TLiteralInt { .. }            | TAtomic::TIntRange { .. }
            | TAtomic::TNonEmptyScalar => return true,
            _ => {}
        }
    }

    // literal-int (Psalm's TNonspecificLiteralInt): literal ints are
    // contained; plain int is its parent (coercion).
    if matches!(container_type_part, TAtomic::TNonspecificLiteralInt) {
        match input_type_part {
            TAtomic::TNonspecificLiteralInt | TAtomic::TLiteralInt { .. } => return true,
            TAtomic::TInt | TAtomic::TIntRange { .. } => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Float comparisons
    if matches!(container_type_part, TAtomic::TFloat) {
        match input_type_part {
            TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TInt
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TLiteralInt { .. }            | TAtomic::TIntRange { .. }
            | TAtomic::TNonEmptyScalar => return true,
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
            | TAtomic::TCallableString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString => return true,
            // `scalar` is a less-specific version of `string`, so flag it as a
            // coercion rather than a flat mismatch (matches Psalm's
            // ScalarTypeComparator type_coerced_from_scalar). `array-key` (int|string)
            // is NOT coerced to string — its int branch is a genuine mismatch.
            TAtomic::TScalar => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Non-empty string comparisons
    if matches!(container_type_part, TAtomic::TNonEmptyString) {
        match input_type_part {
            TAtomic::TNonEmptyString
            | TAtomic::TTruthyString
            | TAtomic::TCallableString
            | TAtomic::TNonEmptyNumericString
            // A numeric-string is always non-empty ("" is not numeric), so it is
            // contained by non-empty-string. Matches Psalm ScalarTypeComparator.
            | TAtomic::TNumericString
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
            TAtomic::TString | TAtomic::TLowercaseString => {
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
            TAtomic::TTruthyString
            | TAtomic::TCallableString
            | TAtomic::TNonEmptyNumericString => return true,
            // A known literal is contained iff its value is truthy: Psalm's
            // ScalarTypeComparator rejects '' (non-empty containers) and '0'
            // (TNonFalsyString) and then accepts any literal string.
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }
                return !value.is_empty() && value != "0";
            }
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
            // A class-string is never guaranteed lowercase: flat mismatch (matches Psalm).
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } => return false,
            // A wider string-like input is a less-specific version of lowercase-string,
            // so flag it as a coercion rather than a flat mismatch (matches Psalm).
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TTruthyString => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
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
            // A class-string is never guaranteed lowercase: flat mismatch (matches Psalm).
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } => return false,
            // A wider string-like input is a less-specific version of
            // non-empty-lowercase-string, so flag it as a coercion (matches Psalm).
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TTruthyString => {
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
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TLiteralInt { .. }            | TAtomic::TIntRange { .. }
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
            if matches!(input_type_part, TAtomic::TLiteralString { .. }) {
                return true;
            }
            // A general string-like type is a less-specific (wider) version of
            // `literal-string`, so flag it as a coercion rather than a flat mismatch.
            if matches!(
                input_type_part,
                TAtomic::TString
                    | TAtomic::TNonEmptyString
                    | TAtomic::TLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
            ) {
                atomic_comparison_result.type_coerced = Some(true);
            }
            return false;
        }
    }

    // callable-string container (Psalm's TCallableString rules): an equal
    // callable-string matches; a literal string is accepted as a possible
    // callable name; wider strings coerce like class-string containers.
    if matches!(container_type_part, TAtomic::TCallableString) {
        match input_type_part {
            TAtomic::TCallableString => return true,
            TAtomic::TLiteralString { value } => {
                // Psalm resolves the literal to a callable
                // (CallableTypeComparator::getCallableFromAtomic): a known
                // function or a real Class::method; otherwise not contained.
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    atomic_comparison_result.type_coerced = Some(true);
                    return false;
                }
                if let Some((class_name, _method_name)) = value.split_once("::") {
                    // Method existence needs an interner; accept when the
                    // class resolves (lenient half of Psalm's check).
                    return codebase.resolve_classlike_name(class_name).is_some();
                }
                return codebase.resolve_functionlike_name(value).is_some();
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Class-like string comparisons
    if is_class_string_like_scalar(container_type_part)
        && is_class_string_like_scalar(input_type_part)
    {
        return super::class_like_string_comparator::is_contained_by(
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
            // A plain string literal naming an existing class is a *coercion*
            // into class-string (Psalm: `return "A"` against `@return
            // class-string` is LessSpecificReturnStatement; the argument
            // analyzer separately tolerates such literals as call arguments).
            let literal_class_string = TAtomic::TLiteralClassString {
                name: value.clone(),
            };
            let mut inner_result = TypeComparisonResult::new();
            if super::class_like_string_comparator::is_contained_by(
                codebase,
                &literal_class_string,
                container_type_part,
                &mut inner_result,
            ) {
                atomic_comparison_result.type_coerced = Some(true);
            }
            return false;
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

        // Psalm: a plain int against a literal-int container is a coercion
        // from the wider scalar (ArgumentTypeCoercion), not a plain mismatch.
        if matches!(input_type_part, TAtomic::TInt) {
            atomic_comparison_result.type_coerced = Some(true);
            atomic_comparison_result.type_coerced_from_scalar = Some(true);
            return false;
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
                // Psalm flags a wider string against a literal container as a
                // coercion from the scalar (type_coerced_from_scalar), which
                // keyed-array offset checks treat as a possible match.
                atomic_comparison_result.type_coerced = Some(true);
                atomic_comparison_result.type_coerced_from_scalar = Some(true);
                return false;
            }
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => {
                if container_value.parse::<f64>().is_ok() {
                    atomic_comparison_result.type_coerced = Some(true);
                    atomic_comparison_result.type_coerced_from_scalar = Some(true);
                }
                return false;
            }
            _ => {}
        }
    }

    // Int range comparisons. `positive-int`, `negative-int`,
    // `non-negative-int` and `non-positive-int` are all `TIntRange` atomics, so
    // they flow through here too (mirroring Psalm's single `TIntRange` path).
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
            // Psalm's ScalarTypeComparator delegates range-vs-range purely to
            // IntegerRangeComparator — a failed (even overlapping) range is a
            // plain mismatch, never a coercion.
            return super::integer_range_comparator::is_contained_by(
                *input_min,
                *input_max,
                *container_min,
                *container_max,
            );
        }

        if matches!(input_type_part, TAtomic::TInt) {
            if container_min.is_none() && container_max.is_none() {
                return true;
            }
            // Psalm: `int` is the parent type of any bounded range.
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        }
    }

    // Numeric (int|float) comparisons
    if matches!(container_type_part, TAtomic::TNumeric) {
        match input_type_part {
            TAtomic::TNumeric
            | TAtomic::TInt
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TFloat
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }            | TAtomic::TIntRange { .. } => return true,
            _ => {}
        }
    }

    // Scalar comparisons (int|float|string|bool)
    if matches!(container_type_part, TAtomic::TScalar) {
        match input_type_part {
            TAtomic::TScalar
            | TAtomic::TInt
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumeric
            | TAtomic::TArrayKey            | TAtomic::TNumericString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TTruthyString => return true,
            _ => {}
        }
    }

    // array-key input narrowed to int/string is a coercion from a
    // mixed-ish origin (Psalm AtomicTypeComparator's TArrayKey arm).
    if matches!(input_type_part, TAtomic::TArrayKey)
        && matches!(
            container_type_part,
            TAtomic::TInt
                | TAtomic::TString
                | TAtomic::TIntRange { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TTruthyString
        )
    {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_mixed = Some(true);
        return false;
    }

    // Array key (int|string) comparisons
    if matches!(container_type_part, TAtomic::TArrayKey) {
        match input_type_part {
            TAtomic::TArrayKey
            | TAtomic::TInt
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TString
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TCallableString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNumericString => return true,
            // `scalar`/`numeric` are wider than `array-key` (they include float/bool),
            // so a flat mismatch is reported as a coercion (matches Psalm).
            TAtomic::TScalar | TAtomic::TNumeric => {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
            _ => {}
        }
    }

    // Psalm ScalarTypeComparator catch-all: both input and container are scalar
    // types (guaranteed by the dispatch in atomic_type_comparator) but no specific
    // arm matched. For non-literal scalar containers this is a scalar-vs-scalar
    // mismatch, which Psalm flags as `scalar_type_match_found` so the caller emits
    // `InvalidScalarArgument` rather than a flat `InvalidArgument`. A bare `scalar`
    // input additionally records `type_coerced_from_scalar`.
    if !matches!(
        container_type_part,
        TAtomic::TLiteralInt { .. } | TAtomic::TLiteralString { .. } | TAtomic::TLiteralFloat { .. }
    ) {
        if matches!(input_type_part, TAtomic::TScalar) {
            atomic_comparison_result.type_coerced = Some(true);
            atomic_comparison_result.type_coerced_from_scalar = Some(true);
        }
        atomic_comparison_result.scalar_type_match_found = Some(true);
    }

    false
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

