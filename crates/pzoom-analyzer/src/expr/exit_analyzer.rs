//! Exit/die expression analyzer.

use mago_syntax::ast::ast::construct::{DieConstruct, ExitConstruct};

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an exit() expression.
///
/// exit terminates script execution. The optional argument is either
/// an integer exit code or a string message.
pub fn analyze_exit(
    analyzer: &StatementsAnalyzer<'_>,
    exit: &ExitConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the arguments if present
    if let Some(ref args) = exit.arguments {
        for arg in args.arguments.iter() {
            let _arg_pos = expr_analyzer::analyze(analyzer, arg.value(), analysis_data, context);
        }
    }

    // exit/die returns never (nothing)
    analysis_data.set_expr_type(pos, TUnion::nothing());
}

/// Analyze a die() expression.
///
/// die terminates script execution. The optional argument is either
/// an integer exit code or a string message.
pub fn analyze_die(
    analyzer: &StatementsAnalyzer<'_>,
    die: &DieConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the arguments if present
    if let Some(ref args) = die.arguments {
        for arg in args.arguments.iter() {
            let _arg_pos = expr_analyzer::analyze(analyzer, arg.value(), analysis_data, context);
        }
    }

    // exit/die returns never (nothing)
    analysis_data.set_expr_type(pos, TUnion::nothing());
}
