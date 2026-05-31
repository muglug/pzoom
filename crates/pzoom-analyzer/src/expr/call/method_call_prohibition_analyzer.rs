//! Method-call prohibition checks (deprecated / `@internal`). Mirrors Psalm
//! `MethodCallProhibitionAnalyzer::analyze`, which reports a `DeprecatedMethod`
//! for a deprecated declaring method and an `InternalMethod` when the caller is
//! outside the method's `@internal` scope.

use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::{FunctionLikeInfo, Issue, IssueKind};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_caller_context, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;

/// Report deprecation / `@internal` access for a resolved method call.
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if method_info.is_deprecated {
        let message = method_info
            .deprecation_message
            .as_ref()
            .map(|reason| {
                format!(
                    "Method {}::{} is deprecated: {}",
                    class_name, method_name, reason
                )
            })
            .unwrap_or_else(|| format!("Method {}::{} is deprecated", class_name, method_name));
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::DeprecatedMethod,
            message,
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if !can_access_internal(analyzer, &method_info.internal, Some(context)) {
        let scope_phrase = format_internal_scope_phrase(analyzer, &method_info.internal);
        let caller_phrase = format_caller_context(analyzer, Some(context));
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InternalMethod,
            format!(
                "The method {}::{} is internal to {} but called from {}",
                class_name, method_name, scope_phrase, caller_phrase
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }
}

pub(crate) fn class_has_sealed_methods(class_info: &ClassLikeInfo) -> bool {
    class_info.sealed_methods.unwrap_or(false)
}

pub(crate) fn class_has_sealed_properties(class_info: &ClassLikeInfo) -> bool {
    class_info.sealed_properties.unwrap_or(false) && !class_info.no_seal_properties
}
