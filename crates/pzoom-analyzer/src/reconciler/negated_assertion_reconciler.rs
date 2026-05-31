//! Negated assertion reconciler.
//!
//! Handles negated assertions like `!truthy`, `!isset`, `!== <literal>`, and `!is_<type>()`.
//! This follows Psalm's reconciliation order more closely, using Hakana as the
//! implementation style reference where needed.

use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{Assertion, TAtomic, TUnion};
use pzoom_str::StrId;

use super::simple_negated_assertion_reconciler;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::atomic_type_comparator;
use crate::type_comparator::object_type_comparator;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;

/// Reconciles a negated assertion with an existing type.
pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    analyzer: &StatementsAnalyzer<'_>,
    _inside_loop: bool,
) -> TUnion {
    let is_equality = assertion.has_equality();

    if is_equality {
        if let Some(assertion_atomic) = assertion.get_type() {
            if matches!(
                assertion_atomic,
                TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TEnumCase { .. }
            ) {
                if existing_var_type.is_mixed() {
                    return existing_var_type.clone();
                }

                return handle_literal_negated_equality(
                    existing_var_type,
                    assertion_atomic,
                    analyzer,
                );
            }
        }
    }

    if !is_equality {
        if let Some(assertion_atomic) = assertion.get_type() {
            if let Some(adjusted) =
                reconcile_calculation_numeric_negation(existing_var_type, assertion_atomic)
            {
                return adjusted;
            }
        }
    }

    if let Some(simple_result) = simple_negated_assertion_reconciler::reconcile(
        assertion,
        existing_var_type,
        key,
        negated,
        analysis_data,
        analyzer,
    ) {
        return with_existing_metadata(simple_result, existing_var_type, false);
    }

    match assertion {
        Assertion::IsNotType(atomic) => reconcile_not_type(existing_var_type, atomic, analyzer),
        Assertion::IsNotEqual(atomic) => subtract_literal(existing_var_type, atomic, analyzer),
        Assertion::NotInArray(array_type) => {
            reconcile_not_in_array(existing_var_type, array_type, analyzer)
        }
        Assertion::DoesNotHaveExactCount(_)
        | Assertion::DoesNotHaveNonnullEntryForKey(_)
        | Assertion::ArrayKeyDoesNotExist => existing_var_type.clone(),
        _ => existing_var_type.clone(),
    }
}

fn reconcile_not_type(
    existing_var_type: &TUnion,
    assertion_type: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    if let TAtomic::TClassString { .. } = assertion_type {
        return subtract_class_string(existing_var_type);
    }

    if let TAtomic::TNamedObject { name, .. } = assertion_type {
        if let Some(date_time_result) =
            reconcile_datetime_interface_negation(existing_var_type, *name, analyzer)
        {
            return date_time_result;
        }
    }

    let subtracted = subtract_type(existing_var_type, assertion_type, analyzer);

    if let TAtomic::TNamedObject { name, .. } = assertion_type {
        return remove_matching_enum_cases(&subtracted, *name);
    }

    subtracted
}

fn reconcile_datetime_interface_negation(
    existing_var_type: &TUnion,
    assertion_class_name: StrId,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TUnion> {
    if assertion_class_name != StrId::DATE_TIME
        && assertion_class_name != StrId::DATE_TIME_IMMUTABLE
    {
        return None;
    }

    let date_time_interface_id = analyzer.interner.intern("DateTimeInterface");
    if !existing_var_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TNamedObject { name, .. } if *name == date_time_interface_id
        )
    }) {
        return None;
    }

    let mut acceptable_types = Vec::new();
    for atomic in &existing_var_type.types {
        if matches!(
            atomic,
            TAtomic::TNamedObject { name, .. } if *name == date_time_interface_id
        ) {
            continue;
        }

        acceptable_types.push(atomic.clone());
    }

    let alternate = if assertion_class_name == StrId::DATE_TIME {
        StrId::DATE_TIME_IMMUTABLE
    } else {
        StrId::DATE_TIME
    };

    push_unique_atomic(
        &mut acceptable_types,
        TAtomic::TNamedObject {
            name: alternate,
            type_params: None,
        is_static: false, remapped_params: false },
    );

    let result = if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(type_combiner::combine(acceptable_types, false))
    };

    Some(with_existing_metadata(result, existing_var_type, false))
}

fn subtract_class_string(existing_var_type: &TUnion) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut removed_class_string = false;

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } => {
                removed_class_string = true;
            }
            _ => acceptable_types.push(atomic.clone()),
        }
    }

    if removed_class_string
        && !acceptable_types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TString))
    {
        acceptable_types.push(TAtomic::TString);
    }

    let result = if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(type_combiner::combine(acceptable_types, false))
    };

    with_existing_metadata(result, existing_var_type, false)
}

fn reconcile_calculation_numeric_negation(
    existing_var_type: &TUnion,
    assertion_type: &TAtomic,
) -> Option<TUnion> {
    if !existing_var_type.from_calculation || !existing_var_type.has_int() {
        return None;
    }

    if !matches!(assertion_type, TAtomic::TInt | TAtomic::TFloat) {
        return None;
    }

    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match assertion_type {
            TAtomic::TInt if is_int_like_atomic(atomic) => {}
            TAtomic::TFloat if is_float_like_atomic(atomic) => {}
            _ => acceptable_types.push(atomic.clone()),
        }
    }

    if matches!(assertion_type, TAtomic::TInt) {
        if !acceptable_types
            .iter()
            .any(|atomic| is_float_like_atomic(atomic))
        {
            acceptable_types.push(TAtomic::TFloat);
        }
    } else if !acceptable_types.iter().any(is_int_like_atomic) {
        acceptable_types.push(TAtomic::TInt);
    }

    let result = if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(type_combiner::combine(acceptable_types, false))
    };

    Some(with_existing_metadata(result, existing_var_type, true))
}

fn reconcile_not_in_array(
    existing_var_type: &TUnion,
    array_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut values_to_remove = Vec::new();

    for atomic in &array_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                for value_atomic in &value_type.types {
                    if matches!(
                        value_atomic,
                        TAtomic::TLiteralInt { .. }
                            | TAtomic::TLiteralString { .. }
                            | TAtomic::TLiteralFloat { .. }
                            | TAtomic::TTrue
                            | TAtomic::TFalse
                            | TAtomic::TEnumCase { .. }
                    ) {
                        values_to_remove.push(value_atomic.clone());
                    }
                }
            }
            TAtomic::TKeyedArray { properties, .. } => {
                for value in properties.values() {
                    for value_atomic in &value.types {
                        if matches!(
                            value_atomic,
                            TAtomic::TLiteralInt { .. }
                                | TAtomic::TLiteralString { .. }
                                | TAtomic::TLiteralFloat { .. }
                                | TAtomic::TTrue
                                | TAtomic::TFalse
                                | TAtomic::TEnumCase { .. }
                        ) {
                            values_to_remove.push(value_atomic.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if values_to_remove.is_empty() {
        return existing_var_type.clone();
    }

    let mut result = existing_var_type.clone();
    for value in &values_to_remove {
        result = subtract_literal(&result, value, analyzer);
    }

    result
}

/// Subtracts a type from a union.
pub fn subtract_type(
    existing_var_type: &TUnion,
    type_to_remove: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut remaining_types = Vec::new();

    for atomic in &existing_var_type.types {
        if let Some(narrowed) = narrow_after_subtraction(atomic, type_to_remove, analyzer) {
            push_unique_atomic(&mut remaining_types, narrowed);
            continue;
        }

        if should_subtract(atomic, type_to_remove, analyzer) {
            continue;
        }

        remaining_types.push(atomic.clone());
    }

    // Mirror Hakana: mutate a clone of the existing type's atomics in place so all
    // other metadata (dataflow nodes, docblock origin, etc.) is preserved. This
    // means an identity narrowing (e.g. `string !== "a"`) yields an equal TUnion,
    // so callers' changed-var detection doesn't treat it as a real change.
    let mut result = existing_var_type.clone();
    if remaining_types.is_empty() {
        result.types = vec![TAtomic::TNothing];
    } else if remaining_types.len() > 1 {
        result.types = type_combiner::combine(remaining_types, false);
    } else {
        result.types = remaining_types;
    }

    if matches!(type_to_remove, TAtomic::TNull) {
        result.is_nullable = false;
    }

    result
}

fn subtract_literal(
    existing_var_type: &TUnion,
    literal: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut remaining_types = Vec::new();

    for atomic in &existing_var_type.types {
        match (atomic, literal) {
            (TAtomic::TLiteralInt { value: v1 }, TAtomic::TLiteralInt { value: v2 })
                if v1 == v2 => {}
            (TAtomic::TLiteralString { value: v1 }, TAtomic::TLiteralString { value: v2 })
                if v1 == v2 => {}
            (TAtomic::TLiteralFloat { value: v1 }, TAtomic::TLiteralFloat { value: v2 })
                if v1 == v2 => {}
            (TAtomic::TTrue, TAtomic::TTrue) => {}
            (TAtomic::TFalse, TAtomic::TFalse) => {}
            (TAtomic::TNull, TAtomic::TNull) => {}
            (
                TAtomic::TEnumCase {
                    enum_name: enum_name1,
                    case_name: case_name1,
                },
                TAtomic::TEnumCase {
                    enum_name: enum_name2,
                    case_name: case_name2,
                },
            ) if enum_name1 == enum_name2 && case_name1 == case_name2 => {}
            (TAtomic::TBool, TAtomic::TTrue) => {
                push_unique_atomic(&mut remaining_types, TAtomic::TFalse);
            }
            (TAtomic::TBool, TAtomic::TFalse) => {
                push_unique_atomic(&mut remaining_types, TAtomic::TTrue);
            }
            (TAtomic::TString, TAtomic::TLiteralString { value }) if value.is_empty() => {
                push_unique_atomic(&mut remaining_types, TAtomic::TNonEmptyString);
            }
            (TAtomic::TInt, TAtomic::TLiteralInt { value }) => {
                push_int_except_literal(&mut remaining_types, None, None, *value);
            }
            (TAtomic::TIntRange { min, max }, TAtomic::TLiteralInt { value }) => {
                push_int_except_literal(&mut remaining_types, *min, *max, *value);
            }
            (
                TAtomic::TEnum { name },
                TAtomic::TEnumCase {
                    enum_name,
                    case_name,
                },
            ) if name == enum_name => {
                let mut pushed_any = false;
                if let Some(enum_info) = analyzer.codebase.get_class(*enum_name) {
                    for alt_case in enum_info.constants.keys() {
                        if *alt_case == *case_name {
                            continue;
                        }

                        push_unique_atomic(
                            &mut remaining_types,
                            TAtomic::TEnumCase {
                                enum_name: *enum_name,
                                case_name: *alt_case,
                            },
                        );
                        pushed_any = true;
                    }
                }

                if !pushed_any {
                    remaining_types.push(atomic.clone());
                }
            }
            _ => {
                remaining_types.push(atomic.clone());
            }
        }
    }

    // Mirror Hakana: mutate a clone's atomics in place, preserving other metadata,
    // so an identity narrowing yields an equal TUnion (see subtract_type).
    let mut result = existing_var_type.clone();
    if remaining_types.is_empty() {
        result.types = vec![TAtomic::TNothing];
    } else {
        result.types = remaining_types;
    }

    result
}

/// Remove a literal value (from `!== <literal>` / `!= <literal>`) from a type,
/// mirroring Hakana's `handle_literal_negated_equality`: it walks each atomic and
/// keeps/narrows it as appropriate, tracking whether anything was removed.
fn handle_literal_negated_equality(
    existing_var_type: &TUnion,
    assertion_type: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> TUnion {
    let mut new_var_type = existing_var_type.clone();
    let existing_atomic_types = new_var_type.types.drain(..).collect::<Vec<_>>();
    let mut acceptable_types: Vec<TAtomic> = Vec::new();
    let mut did_remove_type = false;

    for existing_atomic in existing_atomic_types {
        match &existing_atomic {
            TAtomic::TInt | TAtomic::TNumeric => {
                // Psalm keeps plain `int` (the range-splitting there is disabled),
                // only marking the assertion as non-redundant.
                if matches!(assertion_type, TAtomic::TLiteralInt { .. }) {
                    did_remove_type = true;
                }
                acceptable_types.push(existing_atomic);
            }
            TAtomic::TIntRange { min, max } => {
                if let TAtomic::TLiteralInt { value } = assertion_type {
                    did_remove_type = true;
                    // Split the range around the excluded value (Psalm's TIntRange path).
                    push_int_except_literal(&mut acceptable_types, *min, *max, *value);
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TLiteralInt {
                value: existing_value,
            } => {
                if let TAtomic::TLiteralInt { value } = assertion_type {
                    if value == existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic);
                    }
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TArrayKey => {
                if matches!(
                    assertion_type,
                    TAtomic::TLiteralString { .. } | TAtomic::TLiteralInt { .. }
                ) {
                    did_remove_type = true;
                }
                acceptable_types.push(existing_atomic);
            }
            TAtomic::TString | TAtomic::TNonEmptyString | TAtomic::TNumericString => {
                if let TAtomic::TLiteralString { value } = assertion_type {
                    did_remove_type = true;
                    if value.is_empty() && matches!(existing_atomic, TAtomic::TString) {
                        acceptable_types.push(TAtomic::TNonEmptyString);
                    } else {
                        acceptable_types.push(existing_atomic);
                    }
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TLiteralString {
                value: existing_value,
            } => {
                if let TAtomic::TLiteralString { value } = assertion_type {
                    if value == existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic);
                    }
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TEnum {
                name: existing_name,
            } => {
                if let TAtomic::TEnumCase {
                    enum_name,
                    case_name,
                } = assertion_type
                {
                    did_remove_type = true;
                    if enum_name == existing_name {
                        if let Some(enum_info) = analyzer.codebase.get_class(*enum_name) {
                            for alt_case in enum_info.constants.keys() {
                                if alt_case != case_name {
                                    acceptable_types.push(TAtomic::TEnumCase {
                                        enum_name: *enum_name,
                                        case_name: *alt_case,
                                    });
                                }
                            }
                        }
                    } else {
                        acceptable_types.push(existing_atomic);
                    }
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TEnumCase {
                enum_name: existing_enum,
                case_name: existing_case,
            } => {
                if let TAtomic::TEnumCase {
                    enum_name,
                    case_name,
                } = assertion_type
                {
                    if enum_name == existing_enum && case_name == existing_case {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic);
                    }
                } else {
                    acceptable_types.push(existing_atomic);
                }
            }
            TAtomic::TMixed => {
                did_remove_type = true;
                acceptable_types.push(existing_atomic);
            }
            _ => {
                acceptable_types.push(existing_atomic);
            }
        }
    }

    // did_remove_type drives Psalm/Hakana's impossible-condition diagnostics; pzoom
    // emits those via reconcile_keyed_types' redundant-issue path, so it is only
    // informational here.
    let _ = did_remove_type;

    if acceptable_types.is_empty() {
        new_var_type.types = vec![TAtomic::TNothing];
    } else {
        new_var_type.types = acceptable_types;
    }

    new_var_type
}

fn remove_matching_enum_cases(existing_var_type: &TUnion, enum_name: StrId) -> TUnion {
    let mut acceptable_types = Vec::new();

    for atomic in &existing_var_type.types {
        match atomic {
            TAtomic::TEnumCase {
                enum_name: case_enum_name,
                ..
            } if *case_enum_name == enum_name => {}
            _ => acceptable_types.push(atomic.clone()),
        }
    }

    let result = if acceptable_types.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(type_combiner::combine(acceptable_types, false))
    };

    with_existing_metadata(result, existing_var_type, false)
}

fn narrow_after_subtraction(
    existing: &TAtomic,
    to_remove: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> Option<TAtomic> {
    match (existing, to_remove) {
        (TAtomic::TBool, TAtomic::TTrue) => Some(TAtomic::TFalse),
        (TAtomic::TBool, TAtomic::TFalse) => Some(TAtomic::TTrue),
        (TAtomic::TArrayKey, TAtomic::TInt) => Some(TAtomic::TString),
        (TAtomic::TArrayKey, TAtomic::TString) => Some(TAtomic::TInt),
        (TAtomic::TNumeric, TAtomic::TInt) => Some(TAtomic::TFloat),
        (TAtomic::TNumeric, TAtomic::TFloat) => Some(TAtomic::TInt),
        (
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            },
            _,
        ) => {
            let subtracted = subtract_type(as_type, to_remove, analyzer);
            if subtracted.is_nothing() || subtracted == **as_type {
                None
            } else {
                Some(TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(subtracted),
                })
            }
        }
        (
            TAtomic::TIterable {
                key_type,
                value_type,
            },
            TAtomic::TNamedObject { name, .. },
        ) if object_type_comparator::is_class_subtype_of(
            *name,
            StrId::TRAVERSABLE,
            analyzer.codebase,
        ) =>
        {
            Some(TAtomic::TArray {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            })
        }
        (
            TAtomic::TCallable { .. },
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClosure { .. },
        ) => Some(TAtomic::TObject),
        _ => None,
    }
}

fn should_subtract(
    existing: &TAtomic,
    to_remove: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    if existing == to_remove {
        return true;
    }

    let mut comparison_result = TypeComparisonResult::new();
    if atomic_type_comparator::is_contained_by(
        analyzer.codebase,
        existing,
        to_remove,
        &mut comparison_result,
    ) {
        return true;
    }

    matches!(
        (existing, to_remove),
        (
            TAtomic::TEnumCase { enum_name, .. },
            TAtomic::TNamedObject { name, .. }
        ) if enum_name == name
    )
}

fn push_int_except_literal(
    target: &mut Vec<TAtomic>,
    min: Option<i64>,
    max: Option<i64>,
    excluded: i64,
) {
    if let Some(lower_max) = excluded.checked_sub(1) {
        if int_range_has_overlap(min, max, min, Some(lower_max)) {
            push_unique_atomic(target, int_bounds_to_atomic(min, Some(lower_max)));
        }
    }

    if let Some(upper_min) = excluded.checked_add(1) {
        if int_range_has_overlap(min, max, Some(upper_min), max) {
            push_unique_atomic(target, int_bounds_to_atomic(Some(upper_min), max));
        }
    }
}

fn int_range_has_overlap(
    existing_min: Option<i64>,
    existing_max: Option<i64>,
    candidate_min: Option<i64>,
    candidate_max: Option<i64>,
) -> bool {
    let min = match (existing_min, candidate_min) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };

    let max = match (existing_max, candidate_max) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };

    if let (Some(min), Some(max)) = (min, max) {
        return min <= max;
    }

    true
}

fn int_bounds_to_atomic(min: Option<i64>, max: Option<i64>) -> TAtomic {
    match (min, max) {
        (None, None) => TAtomic::TInt,
        (Some(min), Some(max)) if min == max => TAtomic::TLiteralInt { value: min },
        _ => TAtomic::TIntRange { min, max },
    }
}

fn is_int_like_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
    )
}

fn is_float_like_atomic(atomic: &TAtomic) -> bool {
    matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
}

fn push_unique_atomic(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !target.contains(&atomic) {
        target.push(atomic);
    }
}

fn with_existing_metadata(
    mut result: TUnion,
    existing_var_type: &TUnion,
    clear_from_calculation: bool,
) -> TUnion {
    result.from_docblock = existing_var_type.from_docblock;
    result.from_calculation = if clear_from_calculation {
        false
    } else {
        existing_var_type.from_calculation
    };
    result
}
