//! Clears cached expression types within a loop body before it is re-analyzed in
//! a subsequent fixpoint iteration. Mirrors Hakana's `tast_cleaner::clean_nodes`.

use mago_span::HasSpan;
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::node::Node;

use crate::function_analysis_data::FunctionAnalysisData;

fn clean_node(node: Node<'_, '_>, analysis_data: &mut FunctionAnalysisData) {
    if let Node::Expression(_) = node {
        let span = node.span();
        analysis_data
            .expr_types
            .remove(&(span.start.offset, span.end.offset));
    }

    for child in node.children() {
        clean_node(child, analysis_data);
    }
}

/// Remove cached expression types for every expression inside `stmts`.
pub fn clean_nodes(stmts: &[Statement<'_>], analysis_data: &mut FunctionAnalysisData) {
    for stmt in stmts {
        clean_node(Node::Statement(stmt), analysis_data);
    }
}
