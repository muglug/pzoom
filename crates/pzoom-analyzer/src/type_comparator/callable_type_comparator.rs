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
                | TAtomic::TClassString { .. }
        ) {
            // Only accept if container has no param requirements
            if container_params.is_none() {
                return true;
            }
        }

        // Arrays can be callable ([class, method] or [object, method])
        if matches!(
            input_type_part,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
        ) {
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
        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
        return false;
    }

    if container_return.is_some() && input_return.is_none() {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
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

            // Param types are contravariant: container param must be subtype of input param
            if !input_param.param_type.is_mixed()
                && !union_type_comparator::is_contained_by(
                    codebase,
                    &container_param.param_type,
                    &input_param.param_type,
                    false,
                    false,
                    atomic_comparison_result,
                )
            {
                if is_scalar_union(&container_param.param_type)
                    && is_scalar_union(&input_param.param_type)
                {
                    atomic_comparison_result.scalar_type_match_found = true;
                }
                return false;
            }
        }

        if let Some(input_variadic_param_idx) = input_variadic_param_idx {
            if let Some(input_param) = input_params.get(input_variadic_param_idx) {
                for container_param in container_params.iter().skip(input_variadic_param_idx) {
                    if !container_param.param_type.is_mixed()
                        && !union_type_comparator::is_contained_by(
                            codebase,
                            &container_param.param_type,
                            &input_param.param_type,
                            false,
                            false,
                            atomic_comparison_result,
                        )
                    {
                        if is_scalar_union(&container_param.param_type)
                            && is_scalar_union(&input_param.param_type)
                        {
                            atomic_comparison_result.scalar_type_match_found = true;
                        }
                        return false;
                    }
                }
            }
        }
    }

    // Check return type (covariant)
    if let (Some(container_return), Some(input_return)) = (container_return, input_return) {
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
                atomic_comparison_result.scalar_type_match_found = true;
            }
            return false;
        }
    }

    true
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
