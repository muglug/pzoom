//! Variable fetch analyzer.

use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{DataFlowNode, Issue, IssueKind, TAtomic, TUnion, VarId};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::issue_suppression;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a variable fetch expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    var: &Variable<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match var {
        Variable::Direct(direct) => {
            // Get the variable name from the identifier
            let var_name = direct.name;
            let var_id = analyzer.interner.intern(var_name);

            if var_id == StrId::THIS_VAR && context.get_var_type(StrId::THIS_VAR).is_none() {
                if !issue_suppression::is_issue_suppressed_at(analyzer, pos.0, "InvalidScope") {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidScope,
                        "Invalid reference to $this in a non-class context",
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
                analysis_data.set_expr_type(pos, TUnion::mixed());
                return;
            }

            // Psalm: referencing `$this` from a `@psalm-pure` context is impure, since a
            // pure function may not depend on instance state.
            if var_id == StrId::THIS_VAR
                && analyzer.function_info.is_some_and(|info| info.is_pure)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureVariable,
                    "Cannot reference $this in a pure context",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Inline @var docblocks are keyed by the following statement's start offset.
            // Check both the exact variable offset and current statement start.
            let inline_annotation_type = get_inline_var_annotation_type(analyzer, pos.0, var_id)
                .or_else(|| {
                    analysis_data.current_stmt_start.and_then(|stmt_start| {
                        get_inline_var_annotation_type(analyzer, stmt_start, var_id)
                    })
                });

            if let Some(annotation_type) = inline_annotation_type {
                context.set_var_type(var_id, annotation_type.clone());
                analysis_data.set_expr_type(pos, annotation_type);
                return;
            }

            // Check if we have a type for this variable in context
            if let Some(var_type) = context.get_var_type(var_id) {
                maybe_emit_possibly_undefined_variable(
                    analyzer,
                    var_id,
                    var_name,
                    pos,
                    analysis_data,
                    context,
                );
                let mut expr_type = var_type.clone();
                if context.inside_general_use || context.inside_throw || context.inside_isset {
                    let sink_node = DataFlowNode::get_for_variable_sink(
                        VarId(var_id),
                        make_data_flow_node_position(analyzer, pos),
                    );
                    analysis_data.data_flow_graph.add_node(sink_node.clone());

                    if expr_type.parent_nodes.is_empty() {
                        expr_type.parent_nodes.push(sink_node);
                    } else {
                        add_default_dataflow_paths(
                            &mut analysis_data.data_flow_graph,
                            &expr_type.parent_nodes,
                            &sink_node,
                        );
                    }
                }

                analysis_data.set_expr_type(pos, expr_type);
            } else if let Some(superglobal_type) = get_superglobal_default_type(var_name) {
                context.set_var_type(var_id, superglobal_type.clone());
                analysis_data.set_expr_type(pos, superglobal_type);
            } else if let Some(alt_var_id) = get_alternate_var_id(analyzer, var_name) {
                if let Some(var_type) = context.get_var_type(alt_var_id) {
                    maybe_emit_possibly_undefined_variable(
                        analyzer,
                        alt_var_id,
                        var_name,
                        pos,
                        analysis_data,
                        context,
                    );
                    analysis_data.set_expr_type(pos, var_type.clone());
                } else {
                    maybe_emit_undefined_variable(analyzer, var_name, pos, analysis_data, context);

                    // Variable not yet assigned - could be undefined
                    // For now, treat as mixed
                    analysis_data.set_expr_type(pos, TUnion::mixed());
                }
            } else {
                maybe_emit_undefined_variable(analyzer, var_name, pos, analysis_data, context);

                // Variable not yet assigned - could be undefined
                // For now, treat as mixed
                analysis_data.set_expr_type(pos, TUnion::mixed());
            }
        }
        Variable::Indirect(_indirect) => {
            // Variable variables ($$name) - type is unknown at static analysis time
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
        Variable::Nested(_nested) => {
            // Nested variables - type is unknown at static analysis time
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    }
}

fn maybe_emit_undefined_variable(
    analyzer: &StatementsAnalyzer<'_>,
    var_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !context.check_variables {
        return;
    }

    if context.inside_isset || context.inside_unset {
        return;
    }

    if !should_emit_undefined_variable(var_name) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    if analyzer.function_info.is_none() {
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedGlobalVariable,
            format!(
                "Undefined global variable ${}",
                normalize_var_name(var_name)
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedVariable,
        format!("Undefined variable ${}", normalize_var_name(var_name)),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn maybe_emit_possibly_undefined_variable(
    analyzer: &StatementsAnalyzer<'_>,
    var_id: pzoom_str::StrId,
    var_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !context.check_variables {
        return;
    }

    if context.inside_isset || context.inside_unset {
        return;
    }

    if !context.possibly_assigned_var_ids.contains(&var_id) {
        return;
    }

    if context.assigned_var_ids.contains_key(&var_id) {
        return;
    }

    if !should_emit_undefined_variable(var_name) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    if analyzer.function_info.is_none() {
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedGlobalVariable,
            format!(
                "Undefined global variable ${}",
                normalize_var_name(var_name)
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedVariable,
        format!("Undefined variable ${}", normalize_var_name(var_name)),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn normalize_var_name(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

fn should_emit_undefined_variable(var_name: &str) -> bool {
    let normalized = normalize_var_name(var_name);
    !normalized.eq_ignore_ascii_case("this") && !is_superglobal(normalized)
}

fn get_alternate_var_id(
    analyzer: &StatementsAnalyzer<'_>,
    var_name: &str,
) -> Option<pzoom_str::StrId> {
    if let Some(stripped) = var_name.strip_prefix('$') {
        analyzer.interner.find(stripped)
    } else {
        analyzer.interner.find(&format!("${}", var_name))
    }
}

fn is_superglobal(var_name: &str) -> bool {
    matches!(
        var_name,
        "GLOBALS"
            | "_SERVER"
            | "_GET"
            | "_POST"
            | "_FILES"
            | "_COOKIE"
            | "_SESSION"
            | "_REQUEST"
            | "_ENV"
            | "argc"
            | "argv"
    )
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

fn get_inline_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    offset: u32,
    var_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(offset)?;

    for annotation in annotations {
        match annotation.var_name {
            Some(name) if name == var_id => return Some(annotation.var_type.clone()),
            _ => {}
        }
    }

    None
}
