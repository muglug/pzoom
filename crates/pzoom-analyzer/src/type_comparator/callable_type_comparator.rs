//! Callable type comparator.
//!
//! Handles comparison of callable and closure types.

use pzoom_code_info::{CodebaseInfo, TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

/// Check if an input callable type is contained by a container callable type.
pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // A `callable-object` (the object half of a callable after `is_array`
    // subtraction, Psalm's TCallableObject) satisfies any callable container.
    if matches!(
        container_type_part,
        TAtomic::TCallable { .. } | TAtomic::TClosure { .. }
    ) && matches!(
        input_type_part,
        TAtomic::TObjectWithProperties {
            is_invokable: true,
            ..
        }
    ) {
        return true;
    }

    // An object declaring __invoke satisfies a callable container when its
    // __invoke signature does (Psalm compares the invokable's signature).
    if let TAtomic::TCallable {
        params: container_params,
        return_type: container_return,
        is_pure: container_is_pure,
    } = container_type_part
        && let TAtomic::TNamedObject { name, .. } = input_type_part
        && let Some(invoke_info) = codebase
            .get_class(*name)
            .and_then(|class_info| class_info.methods.get(&pzoom_str::StrId::INVOKE))
    {
        let invoke_params: Vec<pzoom_code_info::t_atomic::FunctionLikeParameter> = invoke_info
            .params
            .iter()
            .map(|param| pzoom_code_info::t_atomic::FunctionLikeParameter {
                name: Some(param.name),
                param_type: param
                    .get_type()
                    .cloned()
                    .unwrap_or_else(pzoom_code_info::TUnion::mixed),
                is_optional: param.is_optional,
                is_variadic: param.is_variadic,
                by_ref: param.by_ref,
            })
            .collect();
        return compare_callable_signatures(
            codebase,
            &Some(invoke_params),
            &invoke_info.get_return_type().cloned().map(Box::new),
            Some(invoke_info.is_pure),
            container_params,
            container_return,
            *container_is_pure,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TCallable {
        params: container_params,
        return_type: container_return,
        is_pure: container_is_pure,
    } = container_type_part
    {
        if let TAtomic::TClosure {
            params: input_params,
            return_type: input_return,
            is_pure: input_is_pure,
        } = input_type_part
        {
            return compare_callable_signatures(
                codebase,
                input_params,
                input_return,
                *input_is_pure,
                container_params,
                container_return,
                *container_is_pure,
                atomic_comparison_result,
            );
        }
    }

    // TClosure to TClosure comparison
    if let TAtomic::TClosure {
        params: container_params,
        return_type: container_return,
        is_pure: container_is_pure,
    } = container_type_part
    {
        if let TAtomic::TClosure {
            params: input_params,
            return_type: input_return,
            is_pure: input_is_pure,
        } = input_type_part
        {
            return compare_callable_signatures(
                codebase,
                input_params,
                input_return,
                *input_is_pure,
                container_params,
                container_return,
                *container_is_pure,
                atomic_comparison_result,
            );
        }
    }

    // A bare `callable` is the wider supertype of a `Closure`: it cannot be guaranteed
    // to be one (it could be a string/array callable), so it is never strictly
    // contained, but when the signatures line up the value might be a Closure. Flag it
    // as a coercion (Psalm's LessSpecific) rather than a flat mismatch.
    if let TAtomic::TClosure {
        params: container_params,
        return_type: container_return,
        is_pure: container_is_pure,
    } = container_type_part
        && let TAtomic::TCallable {
            params: input_params,
            return_type: input_return,
            is_pure: input_is_pure,
        } = input_type_part
    {
        let mut signature_result = TypeComparisonResult::new();
        if compare_callable_signatures(
            codebase,
            input_params,
            input_return,
            *input_is_pure,
            container_params,
            container_return,
            *container_is_pure,
            &mut signature_result,
        ) {
            atomic_comparison_result.type_coerced = Some(true);
        }
        return false;
    }

    // TCallable to TCallable comparison
    if let TAtomic::TCallable {
        params: container_params,
        return_type: container_return,
        is_pure: container_is_pure,
    } = container_type_part
    {
        if let TAtomic::TCallable {
            params: input_params,
            return_type: input_return,
            is_pure: input_is_pure,
        } = input_type_part
        {
            return compare_callable_signatures(
                codebase,
                input_params,
                input_return,
                *input_is_pure,
                container_params,
                container_return,
                *container_is_pure,
                atomic_comparison_result,
            );
        }

        // String can be a callable (function name)
        if matches!(
            input_type_part,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TCallableString
                | TAtomic::TClassString { .. }
        ) {
            // Only accept if container has no param requirements
            if container_params.is_none() {
                return true;
            }
        }

        // Arrays can be callable ([class, method] or [object, method])
        if input_type_part.is_array() {
            // Only accept if container has no param requirements
            if container_params.is_none() {
                return true;
            }
        }
    }

    false
}

/// Compare callable signatures (params and return type).
fn compare_callable_signatures(
    codebase: &CodebaseInfo,
    input_params: &Option<Vec<pzoom_code_info::t_atomic::FunctionLikeParameter>>,
    input_return: &Option<Box<pzoom_code_info::TUnion>>,
    input_is_pure: Option<bool>,
    container_params: &Option<Vec<pzoom_code_info::t_atomic::FunctionLikeParameter>>,
    container_return: &Option<Box<pzoom_code_info::TUnion>>,
    container_is_pure: Option<bool>,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if matches!(container_is_pure, Some(true)) && !matches!(input_is_pure, Some(true)) {
        if input_is_pure.is_none() {
            atomic_comparison_result.type_coerced = Some(true);
        }
        return false;
    }

    // If container has no param requirements, any callable matches
    if container_params.is_none() && container_return.is_none() {
        return true;
    }

    if container_params.is_some() && input_params.is_none() {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_mixed = Some(true);
        return false;
    }

    if container_return.is_some() && input_return.is_none() {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_mixed = Some(true);
        return false;
    }

    // Check params (contravariant)
    let mut input_variadic_param_idx: Option<usize> = None;
    if let (Some(container_params), Some(input_params)) = (container_params, input_params) {
        for (i, input_param) in input_params.iter().enumerate() {
            let mut container_param = container_params.get(i);
            if container_param.is_none() {
                if let Some(last_param) = container_params.last() {
                    if last_param.is_variadic {
                        container_param = Some(last_param);
                    }
                }
            }

            if input_param.is_variadic {
                input_variadic_param_idx = Some(i);
            }

            let Some(container_param) = container_param else {
                if input_param.is_variadic || input_param.is_optional {
                    break;
                }
                return false;
            };

            // Param types are contravariant: container param must be subtype of input param.
            // A `mixed` container param is accepted by anything, so skip the check (matches Psalm).
            if !container_param.param_type.is_mixed() {
                let tv_lower_len = atomic_comparison_result.type_variable_lower_bounds.len();
                let tv_upper_len = atomic_comparison_result.type_variable_upper_bounds.len();
                // The container param is the input side of this contravariant check, so its
                // own ignore flags apply (mirrors Psalm honouring them when it checks array
                // elements against callable params in ArrayFunctionArgumentsAnalyzer).
                let contained = union_type_comparator::is_contained_by(
                    codebase,
                    &container_param.param_type,
                    &input_param.param_type,
                    container_param.param_type.ignore_nullable_issues,
                    container_param.param_type.ignore_falsable_issues,
                    atomic_comparison_result,
                );
                flip_param_position_type_variable_bounds(
                    atomic_comparison_result,
                    tv_lower_len,
                    tv_upper_len,
                    contained,
                );
                if !contained {
                    if is_scalar_union(&container_param.param_type)
                        && is_scalar_union(&input_param.param_type)
                    {
                        atomic_comparison_result.scalar_type_match_found = Some(true);
                    }
                    return false;
                }
            }
        }

        if let Some(input_variadic_param_idx) = input_variadic_param_idx {
            if let Some(input_param) = input_params.get(input_variadic_param_idx) {
                for container_param in container_params.iter().skip(input_variadic_param_idx) {
                    if !container_param.param_type.is_mixed() {
                        let tv_lower_len =
                            atomic_comparison_result.type_variable_lower_bounds.len();
                        let tv_upper_len =
                            atomic_comparison_result.type_variable_upper_bounds.len();
                        let contained = union_type_comparator::is_contained_by(
                            codebase,
                            &container_param.param_type,
                            &input_param.param_type,
                            container_param.param_type.ignore_nullable_issues,
                            container_param.param_type.ignore_falsable_issues,
                            atomic_comparison_result,
                        );
                        flip_param_position_type_variable_bounds(
                            atomic_comparison_result,
                            tv_lower_len,
                            tv_upper_len,
                            contained,
                        );
                        if !contained {
                            if is_scalar_union(&container_param.param_type)
                                && is_scalar_union(&input_param.param_type)
                            {
                                atomic_comparison_result.scalar_type_match_found = Some(true);
                            }
                            return false;
                        }
                    }
                }
            }
        }
    }

    // Check return type (covariant)
    if let (Some(container_return), Some(input_return)) = (container_return, input_return) {
        // A void-returning callable effectively yields null, so it satisfies a
        // container that expects a nullable return. Matches Psalm.
        if input_return.is_void() && container_return.is_nullable() {
            return true;
        }

        if !container_return.is_void()
            && !container_return.is_mixed()
            && !union_type_comparator::is_contained_by(
                codebase,
                input_return,
                container_return,
                false,
                false,
                atomic_comparison_result,
            )
        {
            if is_scalar_union(container_return) && is_scalar_union(input_return) {
                atomic_comparison_result.scalar_type_match_found = Some(true);
            }
            return false;
        }
    }

    true
}

/// Hakana's closure comparator records type-variable bounds from
/// contravariant parameter positions flipped (a lower bound on the param
/// comparison is an upper bound on the variable, and vice versa); on a failed
/// comparison the bounds are dropped with the rest of the param result.
fn flip_param_position_type_variable_bounds(
    result: &mut TypeComparisonResult,
    lower_len: usize,
    upper_len: usize,
    contained: bool,
) {
    let new_lower = result.type_variable_lower_bounds.split_off(lower_len);
    let new_upper = result.type_variable_upper_bounds.split_off(upper_len);
    if contained {
        result.type_variable_lower_bounds.extend(new_upper);
        result.type_variable_upper_bounds.extend(new_lower);
    }
}

fn is_scalar_union(union: &pzoom_code_info::TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    matches!(
        union.get_single(),
        Some(
            TAtomic::TInt
                | TAtomic::TFloat
                | TAtomic::TString
                | TAtomic::TBool
                | TAtomic::TTrue
                | TAtomic::TFalse
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralFloat { .. }
                | TAtomic::TLiteralString { .. }
        )
    )
}
