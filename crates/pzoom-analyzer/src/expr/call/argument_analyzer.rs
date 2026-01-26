//! Single argument analyzer.

use mago_syntax::ast::ast::argument::Argument;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a single function/method argument.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    argument: &Argument<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    let arg_pos = expr_analyzer::analyze(analyzer, argument.value(), analysis_data, context);

    // Check if this is a named argument
    if let Argument::Named(named) = argument {
        // Named arguments are handled differently in argument resolution
        // The name is available via named.name
        let _ = named.name;
    }

    // Check if this is a variadic/spread argument (...$arg)
    if argument.is_unpacked() {
        // Spread arguments should be arrays/iterables
        if let Some(arg_type) = analysis_data.get_expr_type(arg_pos) {
            let is_iterable = arg_type.types.iter().any(|t| {
                matches!(
                    t,
                    TAtomic::TArray { .. }
                        | TAtomic::TNonEmptyArray { .. }
                        | TAtomic::TList { .. }
                        | TAtomic::TNonEmptyList { .. }
                        | TAtomic::TKeyedArray { .. }
                        | TAtomic::TIterable { .. }
                        | TAtomic::TMixed
                )
            });

            if !is_iterable {
                let (line, col) = analyzer.get_line_column(arg_pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidArgument,
                    "Spread operator requires an array or iterable".to_string(),
                    analyzer.file_path,
                    arg_pos.0,
                    arg_pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    arg_pos
}

/// Verify that an argument type matches the expected parameter type.
pub fn verify_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    param_type: &TUnion,
    arg_pos: Pos,
    argument_offset: usize,
    function_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
) {
    // If param accepts mixed, any type is valid
    if param_type.is_mixed() {
        return;
    }

    let (line, col) = analyzer.get_line_column(arg_pos.0);

    // Check for mixed argument type
    if arg_type.is_mixed() {
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArgument,
            format!(
                "Argument {} of {} expects {}, mixed provided",
                argument_offset + 1,
                function_name,
                param_type.get_id()
            ),
            analyzer.file_path,
            arg_pos.0,
            arg_pos.1,
            line,
            col,
        ));
        return;
    }

    // Check type compatibility using proper type comparator
    let mut comparison_result = TypeComparisonResult::new();
    let is_contained = union_type_comparator::is_contained_by(
        analyzer.codebase,
        arg_type,
        param_type,
        false,
        false,
        &mut comparison_result,
    );

    if !is_contained {
        // Check for type coercion
        if comparison_result.type_coerced.unwrap_or(false) {
            analysis_data.add_issue(Issue::new(
                IssueKind::ArgumentTypeCoercion,
                format!(
                    "Argument {} of {} expects {}, parent type {} provided",
                    argument_offset + 1,
                    function_name,
                    param_type.get_id(),
                    arg_type.get_id()
                ),
                analyzer.file_path,
                arg_pos.0,
                arg_pos.1,
                line,
                col,
            ));
        } else {
            // Check if any value could be valid (possibly invalid)
            let can_be_contained = union_type_comparator::can_be_contained_by(
                analyzer.codebase,
                arg_type,
                param_type,
            );

            if can_be_contained {
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyInvalidArgument,
                    format!(
                        "Argument {} of {} expects {}, possibly different type {} provided",
                        argument_offset + 1,
                        function_name,
                        param_type.get_id(),
                        arg_type.get_id()
                    ),
                    analyzer.file_path,
                    arg_pos.0,
                    arg_pos.1,
                    line,
                    col,
                ));
            } else {
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidArgument,
                    format!(
                        "Argument {} of {} expects {}, {} provided",
                        argument_offset + 1,
                        function_name,
                        param_type.get_id(),
                        arg_type.get_id()
                    ),
                    analyzer.file_path,
                    arg_pos.0,
                    arg_pos.1,
                    line,
                    col,
                ));
            }
        }
    }
}
