//! Global statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::global::Global;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

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
        return;
    }

    for variable in global_stmt.variables.iter() {
        let Variable::Direct(direct) = variable else {
            continue;
        };

        let var_id = analyzer.interner.intern(direct.name);

        // Psalm treats imported globals as in-scope references to external state.
        // Without a shared global context, use mixed to avoid false undefined-variable reports.
        context.remove_reference_binding(var_id);
        context.mark_external_reference(var_id);
        context.clear_confusing_reference(var_id);

        let var_type = get_superglobal_default_type(direct.name).unwrap_or_else(TUnion::mixed);
        context.set_var_type_direct(var_id, var_type);
    }
}

fn normalize_var_name(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

fn get_superglobal_default_type(var_name: &str) -> Option<TUnion> {
    let normalized = normalize_var_name(var_name);

    match normalized {
        "_SERVER" | "_GET" | "_POST" | "_FILES" | "_COOKIE" | "_SESSION" | "_REQUEST" | "_ENV"
        | "GLOBALS" => Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        })),
        "argc" => Some(TUnion::int()),
        "argv" => Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::int()),
            value_type: Box::new(TUnion::string()),
        })),
        _ => None,
    }
}
