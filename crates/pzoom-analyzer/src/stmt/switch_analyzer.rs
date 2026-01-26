//! Switch statement analyzer.

use mago_syntax::ast::ast::control_flow::switch::{Switch, SwitchBody, SwitchCase};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer;

/// Analyze a switch statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    switch: &Switch<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the switch expression
    let _ = expr_analyzer::analyze(analyzer, switch.expression, analysis_data, context);

    // Get the cases from the switch body
    let cases = match &switch.body {
        SwitchBody::BraceDelimited(body) => body.cases.as_slice(),
        SwitchBody::ColonDelimited(body) => body.cases.as_slice(),
    };

    // Collect all case contexts for merging
    let mut case_contexts = Vec::new();
    let mut has_default = false;

    // Analyze each case
    for case in cases {
        match case {
            SwitchCase::Expression(expr_case) => {
                // Analyze the case expression
                let _ = expr_analyzer::analyze(analyzer, expr_case.expression, analysis_data, context);

                // Create a new context for the case body
                let mut case_context = context.clone();

                // Analyze the case statements
                stmt_analyzer::analyze_stmts(
                    analyzer,
                    expr_case.statements.as_slice(),
                    analysis_data,
                    &mut case_context,
                )?;

                case_contexts.push(case_context);
            }
            SwitchCase::Default(default_case) => {
                has_default = true;

                // Create a new context for the default body
                let mut case_context = context.clone();

                // Analyze the default statements
                stmt_analyzer::analyze_stmts(
                    analyzer,
                    default_case.statements.as_slice(),
                    analysis_data,
                    &mut case_context,
                )?;

                case_contexts.push(case_context);
            }
        }
    }

    // Merge contexts from all case branches
    if has_default && !case_contexts.is_empty() {
        for case_ctx in &case_contexts {
            context.merge(case_ctx);
        }
    }

    Ok(())
}
