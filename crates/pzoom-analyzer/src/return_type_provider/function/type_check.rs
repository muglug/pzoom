//! `is_string` / `is_int` / ... return-type provider (always `bool`).

use pzoom_code_info::TUnion;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct TypeCheckReturnTypeProvider;

impl FunctionReturnTypeProvider for TypeCheckReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &[
            "is_string", "is_int", "is_integer", "is_long", "is_float", "is_double", "is_real",
            "is_bool", "is_array", "is_object", "is_null", "is_numeric", "is_resource", "is_scalar",
            "is_iterable",
        ]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let asserted_atomic = fca::get_builtin_type_check_atomic(event.function_id)?;
        // When the argument is a reconcilable lvalue (variable, property, array or
        // static-property access), a redundant/impossible check is surfaced by the
        // reconciler (`reconcileInt` et al. via the active-assertion narrowing),
        // which also keeps the in-branch refinement intact. Only for arguments with
        // no such key (e.g. `is_int(returns_int())`) is there nothing to reconcile,
        // so — like Psalm's FunctionCallReturnTypeFetcher — we narrow the result
        // itself to `true`/`false` and let `handle_paradoxical_condition` flag it.
        let arg_has_var_key = event
            .args
            .first()
            .is_some_and(|arg| expression_identifier::get_expression_var_key(arg.value()).is_some());
        fca::infer_builtin_type_check_return_type(
            event.analyzer,
            event.arg_positions,
            analysis_data,
            asserted_atomic,
            arg_has_var_key,
        )
    }
}
