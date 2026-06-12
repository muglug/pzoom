//! Include/require expression analyzer.

use mago_syntax::ast::ast::construct::{
    IncludeConstruct, IncludeOnceConstruct, RequireConstruct, RequireOnceConstruct,
};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

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
    // Analyze the path expression (general use — Hakana's include_analyzer).
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let path_pos = expression_analyzer::analyze(analyzer, path, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    // Get the path type
    if let Some(path_type) = analysis_data.expr_types.get(&path_pos).cloned() {
        // Check if path is a literal string (safe)
        let is_literal_string = path_type.types.iter().all(|t| {
            matches!(
                t,
                TAtomic::TLiteralString { .. } | TAtomic::TLiteralClassString { .. }
            )
        });

        let _ = is_literal_string;

        // Psalm `IncludeAnalyzer`: the include path is an `include`
        // taint sink (TaintedInclude when user input reaches it).
        if analyzer.config.taint_analysis {
            crate::expr::echo_analyzer::add_construct_argument_dataflow(
                analyzer,
                "include",
                &[pzoom_code_info::data_flow::node::SinkType::Include],
                0,
                path_pos,
                &path_type,
                pos,
                analysis_data,
                context,
            );
        }

        // An unresolvable path is Psalm's UnresolvableInclude (never
        // MixedArgument — IncludeAnalyzer has its own issue).
        if path_type.is_mixed() {
            let construct_name = if is_require { "require" } else { "include" };
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnresolvableInclude,
                format!("Cannot resolve {} path", construct_name),
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
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
}
