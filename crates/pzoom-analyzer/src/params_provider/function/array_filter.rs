//! Port of Psalm's `ArrayFilterParamsProvider` (callmap-variant rules).
//!
//! Only the variant checks live here for now: a `null` callback is only valid
//! in the two-argument form, so `array_filter($a, null, MODE)` flags the mode
//! as unused. (Psalm's provider additionally rebuilds the callback parameter
//! type per mode; pzoom infers the callback param types in
//! `arguments_analyzer::infer_array_filter_callback_param_type_for_validation`.)

use pzoom_code_info::{Issue, IssueKind};

use crate::function_analysis_data::FunctionAnalysisData;

use super::{FunctionParamsProvider, FunctionParamsProviderEvent, FunctionParamsProviderResult};

pub(super) struct ArrayFilterParamsProvider;

impl FunctionParamsProvider for ArrayFilterParamsProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        // Psalm's ArgumentsAnalyzer::ARRAY_FILTERLIKE; only array_filter takes
        // a third argument.
        &["array_filter"]
    }

    fn get_function_params(
        &self,
        event: &FunctionParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<FunctionParamsProviderResult> {
        if event.args.len() >= 3
            && event
                .args
                .get(1)
                .and_then(|_| analysis_data.expr_types.get(&event.arg_positions[1]).cloned())
                .is_some_and(|arg_type| arg_type.is_null())
        {
            let arg_pos = event.arg_positions[1];
            let (line, col) = event.analyzer.get_line_column(arg_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                "The 3rd argument of array_filter is not used, when the 2nd argument is null"
                    .to_string(),
                event.analyzer.file_path,
                arg_pos.0,
                arg_pos.1,
                line,
                col,
            ));
        }

        None
    }
}
