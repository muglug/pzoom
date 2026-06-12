//! Function parameter providers (mirrors Psalm's `FunctionParamsProviderInterface`).
//!
//! The trait, event and dispatch live here; each concrete provider has its own
//! file and colocates any helpers only it uses. Providers receive
//! `&mut FunctionAnalysisData` so they may also emit issues (as Psalm providers
//! do via the issue buffer).

mod array_filter;
mod min_max;
mod array_multisort;
mod array_u_array;

use mago_syntax::ast::ast::argument::Argument;
use pzoom_code_info::functionlike_info::ParamInfo;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Context passed to a [`FunctionParamsProvider`], analogous to Psalm's
/// `FunctionParamsProviderEvent`.
pub struct FunctionParamsProviderEvent<'a, 'arena> {
    pub analyzer: &'a StatementsAnalyzer<'a>,
    /// The called function id, lowercase and without a leading `\`.
    pub function_id: &'a str,
    pub args: &'a [&'a Argument<'arena>],
    pub arg_positions: &'a [Pos],
    pub context: &'a BlockContext,
}

/// What a provider decided about the call.
pub enum FunctionParamsProviderResult {
    /// Use this parameter list for the call instead of the stub's.
    Params(Vec<ParamInfo>),
    /// The provider validated the call itself; skip the generic parameter
    /// validation (Psalm's `null` params, which disable argument checking).
    SkipValidation,
}

/// A provider that supplies a per-call parameter list (and optionally emits
/// issues) for calls to particular functions.
pub trait FunctionParamsProvider: Sync {
    /// Function ids (lowercase, without leading `\`) this provider applies to.
    fn function_ids(&self) -> &'static [&'static str];

    fn get_function_params(
        &self,
        event: &FunctionParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<FunctionParamsProviderResult>;
}

/// The registered function params providers (Psalm's
/// `FunctionParamsProvider` constructor registrations).
fn providers() -> &'static [&'static (dyn FunctionParamsProvider + 'static)] {
    &[
        &array_filter::ArrayFilterParamsProvider,
        &min_max::MinMaxParamsProvider,
        &array_multisort::ArrayMultisortParamsProvider,
        &array_u_array::ArrayUArrayParamsProvider,
    ]
}

/// Consult the registered providers for the call. `None` means no provider
/// claims this function.
pub fn dispatch_function_params(
    event: &FunctionParamsProviderEvent<'_, '_>,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<FunctionParamsProviderResult> {
    let normalized = event.function_id.trim_start_matches('\\');
    for provider in providers() {
        if provider
            .function_ids()
            .iter()
            .any(|id| normalized.eq_ignore_ascii_case(id))
        {
            if let Some(result) = provider.get_function_params(event, analysis_data) {
                return Some(result);
            }
        }
    }
    None
}
