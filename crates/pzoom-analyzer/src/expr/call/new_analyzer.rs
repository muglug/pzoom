//! New (object instantiation) analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::instantiation::Instantiation;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::arguments_analyzer;

/// Analyze a new expression (object instantiation).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    instantiation: &Instantiation<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let _class_pos = expr_analyzer::analyze(analyzer, instantiation.class, analysis_data, context);

    // Try to get the resolved class ID
    let name_id = get_resolved_class_id(analyzer, instantiation.class);
    let classlike_name = name_id.map(|id| analyzer.interner.lookup(id));

    // Analyze constructor arguments if present
    if let Some(ref args) = instantiation.argument_list {
        arguments_analyzer::analyze(analyzer, args, analysis_data, context);
    }

    // Create the result type
    if let (Some(name_id), Some(class_name)) = (name_id, classlike_name) {

        // Check if the class exists
        if analyzer.codebase.get_class(name_id).is_none() {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        let result_type = TUnion::new(TAtomic::TNamedObject {
            name: name_id,
            type_params: None,
        });
        analysis_data.set_expr_type(pos, result_type);
        return;
    }

    // Fall back to generic object
    analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TObject));
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(analyzer: &StatementsAnalyzer<'_>, expr: &Expression<'_>) -> Option<StrId> {
    match expr {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer.get_resolved_name(offset)
        }
        _ => None,
    }
}
