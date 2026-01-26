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
    // TClosure to TCallable
    if matches!(container_type_part, TAtomic::TCallable { .. }) {
        if matches!(input_type_part, TAtomic::TClosure { .. }) {
            // Closure is always a valid callable
            // TODO: Check param/return type compatibility if specified
            return true;
        }
    }

    // TClosure to TClosure comparison
    if let TAtomic::TClosure {
        params: container_params,
        return_type: container_return,
    } = container_type_part
    {
        if let TAtomic::TClosure {
            params: input_params,
            return_type: input_return,
        } = input_type_part
        {
            return compare_callable_signatures(
                codebase,
                input_params,
                input_return,
                container_params,
                container_return,
                atomic_comparison_result,
            );
        }
    }

    // TCallable to TCallable comparison
    if let TAtomic::TCallable {
        params: container_params,
        return_type: container_return,
    } = container_type_part
    {
        if let TAtomic::TCallable {
            params: input_params,
            return_type: input_return,
        } = input_type_part
        {
            return compare_callable_signatures(
                codebase,
                input_params,
                input_return,
                container_params,
                container_return,
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
    container_params: &Option<Vec<pzoom_code_info::t_atomic::FunctionLikeParameter>>,
    container_return: &Option<Box<pzoom_code_info::TUnion>>,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // If container has no param requirements, any callable matches
    if container_params.is_none() && container_return.is_none() {
        return true;
    }

    // Check params (contravariant)
    if let (Some(container_params), Some(input_params)) = (container_params, input_params) {
        // Input must accept at least as many params as container expects to pass
        // and container's param types must be subtypes of input's param types
        if input_params.len() < container_params.len() {
            // Check if remaining input params are optional or variadic
            let remaining_count = container_params.len() - input_params.len();
            let mut has_variadic = false;
            let mut optional_count = 0;

            for param in input_params.iter().rev() {
                if param.is_variadic {
                    has_variadic = true;
                    break;
                }
                if param.is_optional {
                    optional_count += 1;
                }
            }

            if !has_variadic && remaining_count > optional_count {
                return false;
            }
        }

        for (i, container_param) in container_params.iter().enumerate() {
            if let Some(input_param) = input_params.get(i) {
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
                    return false;
                }
            } else if !container_param.is_optional && !container_param.is_variadic {
                // Input doesn't have enough required params
                // But this should be caught by the length check above for variadic
            }
        }
    }

    // Check return type (covariant)
    if let (Some(container_return), Some(input_return)) = (container_return, input_return) {
        if !container_return.is_mixed()
            && !union_type_comparator::is_contained_by(
                codebase,
                input_return,
                container_return,
                false,
                false,
                atomic_comparison_result,
            )
        {
            return false;
        }
    }

    true
}
