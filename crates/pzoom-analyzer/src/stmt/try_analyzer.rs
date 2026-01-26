//! Try/catch statement analyzer.

use mago_syntax::ast::ast::r#try::Try;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer;

/// Analyze a try/catch statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    try_stmt: &Try<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the try block
    let mut try_context = context.clone();
    stmt_analyzer::analyze_stmts(
        analyzer,
        try_stmt.block.statements.as_slice(),
        analysis_data,
        &mut try_context,
    )?;

    // Collect all branch contexts for merging
    let mut branch_contexts = vec![try_context];

    // Analyze each catch clause
    for catch in try_stmt.catch_clauses.iter() {
        let mut catch_context = context.clone();

        // Add the exception variable to context if it exists
        if let Some(var) = &catch.variable {
            let var_name = var.name;
            let var_name_id = analyzer.interner.intern(var_name);

            // The exception type would be determined from the type hint
            // For now, use Exception as a generic type
            let exception_type = TUnion::new(TAtomic::TNamedObject {
                name: analyzer.interner.intern("Exception"),
                type_params: None,
            });
            catch_context.set_var_type(var_name_id, exception_type);
        }

        // Analyze the catch block
        stmt_analyzer::analyze_stmts(
            analyzer,
            catch.block.statements.as_slice(),
            analysis_data,
            &mut catch_context,
        )?;

        branch_contexts.push(catch_context);
    }

    // Merge contexts from try and all catch branches
    for branch_ctx in &branch_contexts {
        context.merge(branch_ctx);
    }

    // Analyze the finally block if present
    // Finally always runs, so merge its context too
    if let Some(finally) = &try_stmt.finally_clause {
        let mut finally_context = context.clone();
        stmt_analyzer::analyze_stmts(
            analyzer,
            finally.block.statements.as_slice(),
            analysis_data,
            &mut finally_context,
        )?;

        // Finally modifies context regardless of which branch executed
        context.merge(&finally_context);
    }

    Ok(())
}
