//! Function return-type providers (mirrors Psalm's `FunctionReturnTypeProviderInterface`).
//!
//! The trait, event and dispatch live here; each concrete provider has its own file
//! and colocates any helpers only it uses. Providers receive `&mut FunctionAnalysisData`
//! so they may also emit issues (as Psalm providers do via the issue buffer).

mod array_column;
mod array_combine;
mod array_fill;
mod array_filter;
mod array_key_first_last;
mod array_keys;
mod array_map;
mod array_merge;
mod array_pointer;
mod array_rand;
mod array_reduce;
mod array_reverse;
mod array_shift_pop;
mod array_splice;
mod array_values;
mod call_user_func;
mod count;
mod filter_var;
mod get_object_vars;
mod hrtime;
mod is_a;
mod iterator_to_array;
mod microtime;
mod min_max;
mod parse_url;
mod preg_replace;
mod preg_split;
mod rand;
mod range;
mod simple;
mod sprintf;
mod str_replace;
mod trigger_error;
mod type_check;
mod var_export;

use mago_syntax::ast::ast::argument::Argument;
use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Context passed to a [`FunctionReturnTypeProvider`], analogous to Psalm's
/// `FunctionReturnTypeProviderEvent`.
pub struct FunctionReturnTypeProviderEvent<'a, 'arena> {
    pub analyzer: &'a StatementsAnalyzer<'a>,
    /// The called function id, lowercase and without a leading `\`.
    pub function_id: &'a str,
    pub args: &'a [&'a Argument<'arena>],
    pub arg_positions: &'a [Pos],
    pub context: &'a BlockContext,
}

/// A provider that supplies a return type (and optionally emits issues) for calls to
/// particular functions.
pub trait FunctionReturnTypeProvider: Sync {
    /// Function ids (lowercase, without leading `\`) this provider applies to.
    fn function_ids(&self) -> &'static [&'static str];

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion>;
}

/// The registered function return-type providers, in priority order.
fn providers() -> &'static [&'static (dyn FunctionReturnTypeProvider + 'static)] {
    &[
        &type_check::TypeCheckReturnTypeProvider,
        &is_a::IsAReturnTypeProvider,
        &sprintf::SprintfReturnTypeProvider,
        &get_object_vars::GetObjectVarsReturnTypeProvider,
        &simple::Utf8EncodeReturnTypeProvider,
        &simple::TmpfileReturnTypeProvider,
        &simple::FopenReturnTypeProvider,
        &simple::FilterInputArrayReturnTypeProvider,
        &str_replace::StrReplaceReturnTypeProvider,
        &preg_replace::PregReplaceReturnTypeProvider,
        &preg_split::PregSplitReturnTypeProvider,
        &array_keys::ArrayKeysReturnTypeProvider,
        &array_values::ArrayValuesReturnTypeProvider,
        &array_key_first_last::ArrayKeyFirstLastReturnTypeProvider,
        &array_shift_pop::ArrayShiftPopReturnTypeProvider,
        &array_rand::ArrayRandReturnTypeProvider,
        &filter_var::FilterVarReturnTypeProvider,
        &min_max::MinMaxReturnTypeProvider,
        &rand::RandReturnTypeProvider,
        &range::RangeReturnTypeProvider,
        &iterator_to_array::IteratorToArrayReturnTypeProvider,
        &count::CountReturnTypeProvider,
        &array_filter::ArrayFilterReturnTypeProvider,
        &array_fill::ArrayFillReturnTypeProvider,
        &array_map::ArrayMapReturnTypeProvider,
        &var_export::VarExportReturnTypeProvider,
        &array_combine::ArrayCombineReturnTypeProvider,
        &array_merge::ArrayMergeReturnTypeProvider,
        &array_reverse::ArrayReverseReturnTypeProvider,
        &array_splice::ArraySpliceReturnTypeProvider,
        &call_user_func::CallUserFuncReturnTypeProvider,
        &parse_url::ParseUrlReturnTypeProvider,
        &trigger_error::TriggerErrorReturnTypeProvider,
        &microtime::MicrotimeReturnTypeProvider,
        &hrtime::HrtimeReturnTypeProvider,
        &array_pointer::ArrayPointerReturnTypeProvider,
        &array_reduce::ArrayReduceReturnTypeProvider,
        &array_column::ArrayColumnReturnTypeProvider,
    ]
}

/// Dispatch a function call to the registered providers, returning the first match.
///
/// A provider may emit issues and still return `None` (e.g. `sprintf`/`is_a`), in
/// which case dispatch continues and the declared/stub return type is used.
pub fn dispatch_function_return_type(
    event: &FunctionReturnTypeProviderEvent<'_, '_>,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    for provider in providers() {
        if provider
            .function_ids()
            .iter()
            .any(|id| id.eq_ignore_ascii_case(event.function_id))
            && let Some(return_type) = provider.get_function_return_type(event, analysis_data)
        {
            return Some(return_type);
        }
    }

    None
}
