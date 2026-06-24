//! Method parameter providers (mirrors Psalm's `MethodParamsProviderInterface`).
//!
//! Psalm consults `$codebase->methods->params_provider` at the top of
//! `Methods::getMethodParams`; pzoom dispatches from the method-call analyzers
//! right before argument verification.

mod pdo_statement_set_fetch_mode;

use mago_syntax::cst::cst::argument::Argument;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Context passed to a [`MethodParamsProvider`], analogous to Psalm's
/// `MethodParamsProviderEvent`.
pub struct MethodParamsProviderEvent<'a, 'arena> {
    pub analyzer: &'a StatementsAnalyzer<'a>,
    /// The class the method is being called on (after resolution).
    pub class_id: StrId,
    /// The called method name, as written.
    pub method_name: &'a str,
    pub args: &'a [&'a Argument<'arena>],
    pub arg_positions: &'a [Pos],
    pub context: &'a BlockContext,
}

/// A provider that supplies a per-call parameter list for calls to particular
/// class methods.
pub trait MethodParamsProvider: Sync {
    /// Class names this provider applies to (case-sensitive, unqualified).
    fn classlike_names(&self) -> &'static [&'static str];

    fn get_method_params(
        &self,
        event: &MethodParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<Vec<ParamInfo>>;
}

/// The registered method params providers (Psalm's `MethodParamsProvider`
/// constructor registrations).
fn providers() -> &'static [&'static (dyn MethodParamsProvider + 'static)] {
    &[&pdo_statement_set_fetch_mode::PdoStatementSetFetchMode]
}

/// Consult the registered providers for the call. `None` means no provider
/// claims this class/method.
pub fn dispatch_method_params(
    event: &MethodParamsProviderEvent<'_, '_>,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<Vec<ParamInfo>> {
    let class_name_raw = event.analyzer.interner.lookup(event.class_id);
    let class_name = class_name_raw
        .rsplit('\\')
        .next()
        .unwrap_or(class_name_raw.as_ref());
    for provider in providers() {
        if provider
            .classlike_names()
            .iter()
            .any(|name| class_name.eq_ignore_ascii_case(name))
        {
            if let Some(params) = provider.get_method_params(event, analysis_data) {
                return Some(params);
            }
        }
    }
    None
}
