//! `call_user_func` / `call_user_func_array` return-type provider.
//!
//! Psalm rewrites `call_user_func($f, ...)` into a virtual `$f(...)` call;
//! pzoom infers the return from the callable argument's type instead
//! (closures and callables directly, strings/arrays via the resolution the
//! array_map inference shares).

use pzoom_code_info::TUnion;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::callable_validation;
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct CallUserFuncReturnTypeProvider;

impl FunctionReturnTypeProvider for CallUserFuncReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["call_user_func", "call_user_func_array"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let callback_pos = event.arg_positions.first().copied()?;
        let callback_type = analysis_data.expr_types.get(&callback_pos).cloned()?;

        let arg_types: Vec<TUnion> = event
            .arg_positions
            .iter()
            .skip(1)
            .filter_map(|arg_pos| {
                analysis_data
                    .expr_types
                    .get(&*arg_pos)
                    .cloned()
                    .map(|arg_type| (*arg_type).clone())
            })
            .collect();

        callable_validation::infer_array_map_callable_return_type(
            event.analyzer,
            &callback_type,
            &arg_types,
            event.context,
        )
        .or_else(|| callable_validation::infer_callee_return_type(&callback_type))
    }
}
