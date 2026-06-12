//! Global statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::global::Global;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::{Issue, IssueKind, TUnion};
use pzoom_code_info::VarName;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a `global $var;` statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    global_stmt: &Global<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Psalm reports InvalidGlobal at top level but still processes the
    // statement (a @var comment can type the variable).
    if analyzer.function_info.is_none() {
        let span = global_stmt.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidGlobal,
            "Cannot use global scope keyword outside a function",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    for variable in global_stmt.variables.iter() {
        let Variable::Direct(direct) = variable else {
            continue;
        };

        let var_id = VarName::new(direct.name);

        // Psalm treats imported globals as in-scope references to external state.
        // Without a shared global context, use mixed to avoid false undefined-variable reports.
        context.remove_reference_binding(&var_id);
        context.mark_external_reference(var_id.clone());
        context.clear_confusing_reference(&var_id);

        // A statement-level `@var` comment types the imported global
        // (Psalm's CommentAnalyzer::getVarComments path); otherwise
        // superglobals get their fixed shapes and other globals clone the
        // top-level variable's type recorded at declaration time (Psalm's
        // global_context lookup), defaulting to mixed when unknown.
        let comment_type = analysis_data.current_stmt_start.and_then(|stmt_start| {
            crate::expr::variable_fetch_analyzer::get_inline_var_annotation_type(
                analyzer, stmt_start, &var_id,
            )
        });
        let mut var_type = comment_type
            .or_else(|| get_superglobal_default_type(direct.name))
            .or_else(|| analysis_data.file_global_types.get(&var_id).cloned())
            .unwrap_or_else(TUnion::mixed);

        // The `global $a;` import is itself an assignment-like declaration:
        // one that is never subsequently read reports UnusedVariable (Psalm's
        // unusedUndeclaredGlobalVariable).
        if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
            && get_superglobal_default_type(direct.name).is_none()
        {
            let span = variable.span();
            let decl_node = pzoom_code_info::DataFlowNode::get_for_variable_source(
                pzoom_code_info::VariableSourceKind::Default,
                pzoom_code_info::VarId(analyzer.interner.intern(&var_id)),
                crate::data_flow::make_data_flow_node_position(
                    analyzer,
                    (span.start.offset, span.end.offset),
                ),
                false,
                false,
                false,
                false,
                false,
            );
            // The import reads the top-level binding: its dataflow parents
            // (from the file_global_types snapshot) feed the declaration, so
            // a function-level use marks the top-level assignment used too.
            for parent_node in &var_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &decl_node.id,
                    pzoom_code_info::PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            analysis_data.data_flow_graph.add_node(decl_node.clone());
            var_type.parent_nodes = vec![decl_node];
        }
        context.set_var_type_direct(var_id, var_type);
    }
}

use crate::expr::variable_fetch_analyzer::get_superglobal_default_type;
