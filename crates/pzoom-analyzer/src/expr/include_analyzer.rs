//! Include/require expression analyzer.

use mago_syntax::ast::ast::construct::{
    IncludeConstruct, IncludeOnceConstruct, RequireConstruct, RequireOnceConstruct,
};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an include expression.
pub fn analyze_include(
    analyzer: &StatementsAnalyzer<'_>,
    include: &IncludeConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, include.value, pos, analysis_data, context, false);
}

/// Analyze an include_once expression.
pub fn analyze_include_once(
    analyzer: &StatementsAnalyzer<'_>,
    include: &IncludeOnceConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, include.value, pos, analysis_data, context, false);
}

/// Analyze a require expression.
pub fn analyze_require(
    analyzer: &StatementsAnalyzer<'_>,
    require: &RequireConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, require.value, pos, analysis_data, context, true);
}

/// Analyze a require_once expression.
pub fn analyze_require_once(
    analyzer: &StatementsAnalyzer<'_>,
    require: &RequireOnceConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, require.value, pos, analysis_data, context, true);
}

/// Analyze the path argument of an include/require expression.
fn analyze_path(
    analyzer: &StatementsAnalyzer<'_>,
    path: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_require: bool,
) {
    // Analyze the path expression
    let path_pos = expr_analyzer::analyze(analyzer, path, analysis_data, context);

    // Get the path type
    if let Some(path_type) = analysis_data.get_expr_type(path_pos) {
        // Check if path is a literal string (safe)
        let is_literal_string = path_type.types.iter().all(|t| {
            matches!(
                t,
                TAtomic::TLiteralString { .. } | TAtomic::TLiteralClassString { .. }
            )
        });

        // Check for potential path injection (tainted data in include path)
        if !is_literal_string {
            // Check if path could contain user input (non-literal strings)
            let has_variable_string = path_type.types.iter().any(|t| {
                matches!(
                    t,
                    TAtomic::TString
                        | TAtomic::TNonEmptyString
                        | TAtomic::TMixed
                        | TAtomic::TNumericString
                        | TAtomic::TNonEmptyNumericString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString
                        | TAtomic::TTruthyString
                )
            });

            if has_variable_string {
                // Potential security issue: variable string in include path
                let construct_name = if is_require { "require" } else { "include" };
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::TaintedInput,
                    format!(
                        "Potential path injection: {} path is not a literal string",
                        construct_name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }

        // Check for mixed type in path
        if path_type.is_mixed() {
            let construct_name = if is_require { "require" } else { "include" };
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedArgument,
                format!("{} path is of mixed type", construct_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // include/require returns the return value of the included file,
    // or 1 on success, false on failure (for include)
    // For simplicity, we return mixed since we don't track included file returns
    analysis_data.set_expr_type(pos, TUnion::mixed());
}
