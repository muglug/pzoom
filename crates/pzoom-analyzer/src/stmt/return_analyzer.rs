//! Return statement analyzer.

use mago_syntax::ast::ast::r#return::Return;

use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::type_comparator;

/// Analyze a return statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    ret: &Return<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let return_type = if let Some(ref value) = ret.value {
        let value_pos = expr_analyzer::analyze(analyzer, value, analysis_data, context);
        analysis_data
            .get_expr_type(value_pos)
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed)
    } else {
        TUnion::void()
    };

    // Check against expected return type
    if let Some(expected_type) = analyzer.get_expected_return_type() {
        let has_return_value = ret.value.is_some();

        // Check if we're returning a value from a never function
        if has_return_value && expected_type.is_nothing() {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    "Cannot return a value from a function with never return type",
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
        // Check if we're returning a value from a void function
        else if has_return_value && expected_type.is_void() {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    format!(
                        "No return values are expected for this function, but {} was returned",
                        return_type.get_id()
                    ),
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
        // Check type compatibility for non-void/never functions
        else if has_return_value && !expected_type.is_mixed() && !expected_type.is_void() {
            // Skip mixed return validation for now - without docblock parsing,
            // we get too many false positives from untyped parameters
            if return_type.is_mixed() {
                // Continue without error
            } else if !type_comparator::is_contained_by_with_codebase(
                &return_type,
                &expected_type,
                analyzer.codebase,
            ) {
                if let Some(start) = analysis_data.current_stmt_start {
                    let (line, col) = analyzer.get_line_column(start);
                    // Determine the specific issue kind
                    let issue_kind = if return_type.is_nullable && !expected_type.is_nullable {
                        IssueKind::NullableReturnStatement
                    } else if return_type.is_falsable && !expected_type.is_falsable {
                        IssueKind::FalsableReturnStatement
                    } else {
                        IssueKind::InvalidReturnStatement
                    };

                    analysis_data.add_issue(Issue::new(
                        issue_kind,
                        format!(
                            "The type {} does not match the declared return type {}",
                            return_type.get_id(),
                            expected_type.get_id()
                        ),
                        analyzer.file_path,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        line,
                        col,
                    ));
                }
            }
        }
        // Check if we're not returning a value when one is expected
        else if !has_return_value && !expected_type.is_void() && !expected_type.is_mixed() {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    format!(
                        "Empty return statement not expected, function should return {}",
                        expected_type.get_id()
                    ),
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
    }

    // Record the return type for later comparison
    analysis_data.add_return_type(return_type);

    // Mark that control flow has exited
    context.has_returned = true;

    Ok(())
}
