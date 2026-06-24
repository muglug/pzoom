//! Exit/die expression analyzer.

use mago_syntax::cst::cst::construct::{DieConstruct, ExitConstruct};

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr::call::method_call_analyzer::is_mutation_free_context;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Mirror Psalm's `ExitAnalyzer`: calling `exit`/`die` with a non-integer
/// argument from a mutation-free (pure) context is impure, because the argument
/// is printed. Emits `ImpureFunctionCall` when applicable.
fn check_exit_purity(
    analyzer: &StatementsAnalyzer<'_>,
    arg_pos: Pos,
    construct_pos: Pos,
    function_name: &str,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !is_mutation_free_context(analyzer) {
        return;
    }

    let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned() else {
        return;
    };

    let is_int = !arg_type.types.is_empty()
        && arg_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
            )
        });

    if is_int {
        return;
    }

    let (line, col) = analyzer.get_line_column(construct_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::ImpureFunctionCall,
        format!(
            "Cannot call {} with a non-integer argument from a mutation-free context",
            function_name
        ),
        analyzer.file_path,
        construct_pos.0,
        construct_pos.1,
        line,
        col,
    ));
}

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
            let arg_pos =
                expression_analyzer::analyze(analyzer, arg.value(), analysis_data, context);
            check_exit_purity(analyzer, arg_pos, pos, "exit", analysis_data);

            // Psalm `ExitAnalyzer`: exit/die output their argument, with the
            // same sink kinds as echo.
            if analyzer.config.taint_analysis
                && let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned()
            {
                crate::expr::output_constructs::add_output_call_argument_dataflow(
                    analyzer,
                    "exit",
                    0,
                    arg_pos,
                    &arg_type,
                    pos,
                    analysis_data,
                    context,
                );
            }
        }
    }

    // exit/die returns never (nothing)
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::nothing()));
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
            let arg_pos =
                expression_analyzer::analyze(analyzer, arg.value(), analysis_data, context);
            check_exit_purity(analyzer, arg_pos, pos, "die", analysis_data);

            // Psalm `ExitAnalyzer`: exit/die output their argument, with the
            // same sink kinds as echo.
            if analyzer.config.taint_analysis
                && let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned()
            {
                crate::expr::output_constructs::add_output_call_argument_dataflow(
                    analyzer,
                    "exit",
                    0,
                    arg_pos,
                    &arg_type,
                    pos,
                    analysis_data,
                    context,
                );
            }
        }
    }

    // exit/die returns never (nothing)
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::nothing()));
}
