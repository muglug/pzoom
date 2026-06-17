//! Generic (templated) type comparator.
//!
//! Compares the type parameters of two generic class instances, honoring each
//! template's declared variance. Mirrors Psalm's `GenericTypeComparator` and
//! Hakana's `generic_type_comparator`.

use pzoom_code_info::class_like_info::TemplateVariance;
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::StrId;

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    class_name: StrId,
    input_type_params: Option<&[TUnion]>,
    container_type_params: Option<&[TUnion]>,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let Some(container_type_params) = container_type_params else {
        return true;
    };

    let class_template_variances = codebase
        .get_class(class_name)
        .map(|class_info| class_info.template_types.as_slice())
        .unwrap_or(&[]);

    // A bare `Foo` (no explicit type params) is treated as `Foo<...>` filled with
    // each template's `as` bound (`mixed` for an unbounded `@template T`), matching
    // Psalm — rather than flagging a coercion against a parameterized container.
    let default_params: Vec<TUnion>;
    let input_type_params = match input_type_params {
        Some(params) => params,
        None => {
            default_params = class_template_variances
                .iter()
                .map(|template_type| {
                    let mut as_type = template_type.as_type.clone();
                    // Psalm's from_template_default: a slot filled from the
                    // template's declared bound coerces leniently (as-mixed);
                    // @template declarations are docblock constructs.
                    as_type.from_template_default = true;
                    as_type.from_docblock = true;
                    as_type
                })
                .collect();
            &default_params
        }
    };

    let mut all_types_contain = true;

    // Iterate the INPUT params and stop once the container runs out of params,
    // mirroring Psalm's `foreach ($input_type_params ...) { if (!isset(
    // $container->type_params[$i])) break; }`. Extra input params are tolerated;
    // missing ones simply aren't compared, and like Psalm the loop runs every
    // param rather than bailing at the first mismatch.
    for (i, input_param) in input_type_params.iter().enumerate() {
        let Some(container_param) = container_type_params.get(i) else {
            break;
        };

        // `never`/`nothing` is contained by anything (Psalm short-circuits on
        // `$input_param->isNever()`).
        if input_param.is_nothing() {
            continue;
        }

        let variance = class_template_variances
            .get(i)
            .map(|template_type| template_type.variance)
            .unwrap_or(TemplateVariance::Invariant);

        let mut param_comparison_result = TypeComparisonResult::new();

        let contained = match variance {
            // Psalm's `@template-contravariant` params flip the comparison
            // direction; everything else compares input-in-container first.
            TemplateVariance::Contravariant => union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                false,
                &mut param_comparison_result,
            ),
            _ => union_type_comparator::is_contained_by(
                codebase,
                input_param,
                container_param,
                false,
                false,
                &mut param_comparison_result,
            ),
        };

        if !contained {
            // Generator's third template param (`TSend`) defaults to `mixed`, so
            // a narrower send type is coerced from mixed rather than a real
            // mismatch. Psalm exempts `Generator` arg 2 when the failure is a
            // coercion from mixed.
            if class_name == StrId::GENERATOR
                && i == 2
                && param_comparison_result
                    .type_coerced_from_mixed
                    .unwrap_or(false)
            {
                continue;
            }

            // Psalm's overwrite-style merge: a failing param sets each flag to
            // its own result, but a flag already forced to `false` stays false.
            atomic_comparison_result.type_coerced = Some(
                param_comparison_result.type_coerced == Some(true)
                    && atomic_comparison_result.type_coerced != Some(false),
            );

            atomic_comparison_result.type_coerced_from_mixed = Some(
                param_comparison_result.type_coerced_from_mixed == Some(true)
                    && atomic_comparison_result.type_coerced_from_mixed != Some(false),
            );

            atomic_comparison_result.type_coerced_from_as_mixed = Some(
                param_comparison_result.type_coerced_from_as_mixed == Some(true)
                    && atomic_comparison_result.type_coerced_from_as_mixed != Some(false),
            );

            // Psalm keeps `$all_types_contain` when the only failure was a
            // coercion from a template's as-mixed bound (non-iterable
            // containers).
            if !param_comparison_result
                .type_coerced_from_as_mixed
                .unwrap_or(false)
            {
                all_types_contain = false;
            }
        } else if !matches!(variance, TemplateVariance::Covariant)
            && !union_contains_any_literal(input_param)
            && !union_has_template(input_param)
            && !union_has_template(container_param)
        {
            // Invariant generic params constrain a type variable from both
            // sides (Hakana's generic comparator): a forward upper bound also
            // becomes an equality lower bound on the container class.
            atomic_comparison_result
                .type_variable_lower_bounds
                .extend(param_comparison_result.type_variable_lower_bounds);

            atomic_comparison_result.type_variable_lower_bounds.extend(
                param_comparison_result
                    .type_variable_upper_bounds
                    .clone()
                    .into_iter()
                    .map(|(name, mut bound)| {
                        bound.equality_bound_classlike = Some(class_name);
                        (name, bound)
                    }),
            );

            atomic_comparison_result
                .type_variable_upper_bounds
                .extend(param_comparison_result.type_variable_upper_bounds);

            let mut param_comparison_result = TypeComparisonResult::new();

            // Psalm's "Make sure types are basically the same" invariance
            // check: the container param must be contained right back, without
            // coercion. A literal input is treated as a generalisation of the
            // container param, so `Container<array{name: 'x'}>` satisfies
            // `Container<array{name: string}>`, and templates skip the check.
            if !union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                false,
                &mut param_comparison_result,
            ) || param_comparison_result.type_coerced.unwrap_or(false)
            {
                all_types_contain = false;
                atomic_comparison_result.type_coerced = Some(false);
            }
        }
    }

    all_types_contain
}

/// Recursively determines whether a union contains any literal type, mirroring
/// Psalm's `Union::containsAnyLiteral()` (`ContainsLiteralVisitor`). Literal
/// strings/ints/floats, `true`/`false`, and the empty array all count.
fn union_contains_any_literal(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_any_literal)
}

fn atomic_contains_any_literal(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TTrue
        | TAtomic::TFalse => true,
        TAtomic::TArray {
            known_values,
            params,
            is_nonempty,
            ..
        } => {
            // The empty array literal (Psalm's `array<never, never>` / `[]`) is
            // treated as a literal: a generic array (no known entries) that is
            // not non-empty and whose typed fallback is absent or `never`/`never`.
            let is_empty_array_literal = known_values.is_empty()
                && !*is_nonempty
                && params
                    .as_deref()
                    .is_none_or(|(key, value)| key.is_nothing() && value.is_nothing());
            is_empty_array_literal
                || known_values
                    .values()
                    .any(|(_, value_type)| union_contains_any_literal(value_type))
                || params.as_deref().is_some_and(|(key, value)| {
                    union_contains_any_literal(key) || union_contains_any_literal(value)
                })
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => union_contains_any_literal(key_type) || union_contains_any_literal(value_type),
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().any(union_contains_any_literal),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_any_literal),
        _ => false,
    }
}

/// Recursively determines whether a union references a template parameter,
/// mirroring Psalm's `Union::hasTemplate()` (`TemplateTypeCollector`).
pub(crate) fn union_has_template(union: &TUnion) -> bool {
    union.types.iter().any(atomic_has_template)
}

fn atomic_has_template(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TTemplateParam { .. }
        | TAtomic::TTemplateParamClass { .. }
        | TAtomic::TTemplateKeyOf { .. }
        | TAtomic::TTemplateValueOf { .. }
        | TAtomic::TTemplatePropertiesOf { .. } => true,
        TAtomic::TIterable {
            key_type,
            value_type,
        } => union_has_template(key_type) || union_has_template(value_type),
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            known_values
                .values()
                .any(|(_, value_type)| union_has_template(value_type))
                || params.as_deref().is_some_and(|(key, value)| {
                    union_has_template(key) || union_has_template(value)
                })
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().any(union_has_template),
        // `class-string<T>` references the template through its constraint
        // (Psalm's TemplateTypeCollector visits TClassString's as-type).
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => atomic_has_template(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_has_template),
        _ => false,
    }
}
