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
                .map(|template_type| template_type.as_type.clone())
                .collect();
            &default_params
        }
    };

    // Iterate the INPUT params and stop once the container runs out of params,
    // mirroring Psalm's `foreach ($input_type_params ...) { if (!isset(
    // $container->type_params[$i])) break; }`. Extra input params are tolerated;
    // missing ones simply aren't compared.
    for (index, input_param) in input_type_params.iter().enumerate() {
        let Some(container_param) = container_type_params.get(index) else {
            break;
        };

        // `never`/`nothing` is contained by anything (Psalm short-circuits on
        // `$input_param->isNever()`).
        if input_param.is_nothing() {
            continue;
        }

        let variance = class_template_variances
            .get(index)
            .map(|template_type| template_type.variance)
            .unwrap_or(TemplateVariance::Invariant);

        let mut forward_result = TypeComparisonResult::new();
        let mut reverse_result = TypeComparisonResult::new();

        let matches = match variance {
            TemplateVariance::Covariant => union_type_comparator::is_contained_by(
                codebase,
                input_param,
                container_param,
                false,
                false,
                &mut forward_result,
            ),
            TemplateVariance::Contravariant => union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                false,
                &mut forward_result,
            ),
            TemplateVariance::Invariant => {
                if !union_type_comparator::is_contained_by(
                    codebase,
                    input_param,
                    container_param,
                    false,
                    false,
                    &mut forward_result,
                ) {
                    false
                } else if union_contains_any_literal(input_param)
                    || union_has_template(input_param)
                    || union_has_template(container_param)
                {
                    // Psalm's `GenericTypeComparator` only performs the reverse
                    // (invariance) check when neither param involves a template
                    // and the input does not contain a literal. A literal input
                    // is treated as a generalisation of the container param, so
                    // `Container<array{name: 'x'}>` satisfies
                    // `Container<array{name: string}>` without a coercion.
                    true
                } else {
                    union_type_comparator::is_contained_by(
                        codebase,
                        container_param,
                        input_param,
                        false,
                        false,
                        &mut reverse_result,
                    )
                }
            }
        };

        if !matches {
            // Generator's third template param (`TSend`) defaults to `mixed`, so
            // a narrower send type is coerced from mixed rather than a real
            // mismatch. Psalm exempts `Generator` arg 2 when the failure is a
            // coercion from mixed.
            if class_name == StrId::GENERATOR
                && index == 2
                && (forward_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
                    || reverse_result
                        .type_coerced_from_nested_mixed
                        .unwrap_or(false))
            {
                continue;
            }

            if forward_result.type_coerced.unwrap_or(false)
                || reverse_result.type_coerced.unwrap_or(false)
            {
                atomic_comparison_result.type_coerced = Some(true);
            }

            if forward_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
                || reverse_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
            {
                atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
            }

            return false;
        }
    }

    true
}

/// Recursively determines whether a union contains any literal type, mirroring
/// Psalm's `Union::containsAnyLiteral()` (`ContainsLiteralVisitor`). Literal
/// strings/ints/floats, `true`/`false`, and the empty array all count.
pub(crate) fn union_contains_any_literal(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_any_literal)
}

fn atomic_contains_any_literal(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TTrue
        | TAtomic::TFalse => true,
        TAtomic::TArray { key_type, value_type } => {
            // The empty array (`array<never, never>`) is treated as a literal.
            (key_type.is_nothing() && value_type.is_nothing())
                || union_contains_any_literal(key_type)
                || union_contains_any_literal(value_type)
        }
        TAtomic::TNonEmptyArray { key_type, value_type } => {
            union_contains_any_literal(key_type) || union_contains_any_literal(value_type)
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_contains_any_literal(value_type)
        }
        TAtomic::TIterable { key_type, value_type } => {
            union_contains_any_literal(key_type) || union_contains_any_literal(value_type)
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            properties.values().any(union_contains_any_literal)
                || fallback_key_type
                    .as_deref()
                    .is_some_and(union_contains_any_literal)
                || fallback_value_type
                    .as_deref()
                    .is_some_and(union_contains_any_literal)
        }
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
        TAtomic::TArray { key_type, value_type }
        | TAtomic::TNonEmptyArray { key_type, value_type }
        | TAtomic::TIterable { key_type, value_type } => {
            union_has_template(key_type) || union_has_template(value_type)
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_has_template(value_type)
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            properties.values().any(union_has_template)
                || fallback_key_type.as_deref().is_some_and(union_has_template)
                || fallback_value_type
                    .as_deref()
                    .is_some_and(union_has_template)
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().any(union_has_template),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_has_template),
        _ => false,
    }
}
