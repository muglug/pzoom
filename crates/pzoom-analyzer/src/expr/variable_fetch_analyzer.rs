//! Variable fetch analyzer.

use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::{DataFlowNode, GraphKind, Issue, IssueKind, TAtomic, TUnion, VarId};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::issue_suppression;
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

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
            let var_id = VarName::new(var_name);

            if var_id == "$this" && context.get_var_type("$this").is_none() {
                if !issue_suppression::is_issue_suppressed_at(
                    analyzer,
                    analysis_data,
                    pos.0,
                    "InvalidScope",
                ) {
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
                analysis_data
                    .expr_types
                    .insert(pos, Rc::new(TUnion::mixed()));
                return;
            }

            // Psalm: referencing `$this` from a `@psalm-pure` context is impure, since a
            // pure function may not depend on instance state.
            if var_id == "$this" && analyzer.function_info.is_some_and(|info| info.is_pure) {
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

            // An inline @var keyed at this exact expression offset overrides the
            // fetch. Statement-level @var docblocks are NOT consulted here:
            // they were assigned into the context once, before the statement
            // (stmt_analyzer::apply_statement_var_annotations) — re-applying
            // per fetch would clobber narrowing inside the statement (Psalm
            // honors an `instanceof` in the same if-condition).
            let inline_annotation_type = get_inline_var_annotation_type(analyzer, pos.0, &var_id);

            if let Some(annotation_type) = inline_annotation_type {
                emit_undefined_docblock_classes_in_annotation(
                    analyzer,
                    &annotation_type,
                    pos,
                    analysis_data,
                );
                context.set_var_type(var_id.clone(), annotation_type.clone());
                analysis_data
                    .expr_types
                    .insert(pos, Rc::new(annotation_type));
                return;
            }

            // Check if we have a type for this variable in context
            if let Some(var_type) = context.get_var_type(&var_id) {
                maybe_emit_possibly_undefined_variable(
                    analyzer,
                    var_name,
                    pos,
                    analysis_data,
                    context,
                );
                let mut expr_type = var_type.clone();
                if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
                    && (context.inside_return
                        || context.inside_call
                        || context.inside_general_use
                        || context.inside_conditional
                        || context.inside_throw
                        || context.inside_isset)
                {
                    let sink_node = DataFlowNode::get_for_variable_sink(
                        VarId(
                            analyzer
                                .interner
                                .find(&var_id)
                                .unwrap_or(pzoom_str::StrId::EMPTY),
                        ),
                        make_data_flow_node_position(analyzer, pos),
                    );
                    analysis_data.data_flow_graph.add_node(sink_node.clone());

                    if expr_type.parent_nodes.is_empty() {
                        // A type with no dataflow parents here is usually a
                        // conditionally-assigned variable materialized by an
                        // isset/empty reconciliation. Psalm's
                        // `registerPossiblyUndefinedVariable` reconnects it by
                        // adding edges from every registered assignment of
                        // the same variable (within this function) to the
                        // fetch.
                        connect_var_assignment_sources(
                            analyzer,
                            analysis_data,
                            &var_id,
                            pos,
                            &sink_node,
                        );
                        expr_type.parent_nodes.push(sink_node);
                    } else {
                        add_default_dataflow_paths(
                            &mut analysis_data.data_flow_graph,
                            &expr_type.parent_nodes,
                            &sink_node,
                        );
                    }
                }

                analysis_data.expr_types.insert(pos, Rc::new(expr_type));
            } else if let Some(mut superglobal_type) = get_superglobal_default_type(var_name) {
                // Request superglobals are taint sources carrying every input
                // taint (Psalm's VariableFetchAnalyzer::taintExternalSuperglobals).
                // Psalm `VariableFetchAnalyzer::taintVariable`: only the four
                // request superglobals carry ALL_INPUT, and the source node
                // has no code location (traces print a bare `$_GET`).
                if analyzer.config.taint_analysis
                    && matches!(
                        var_name.trim_start_matches('$'),
                        "_GET" | "_POST" | "_COOKIE" | "_REQUEST"
                    )
                {
                    let source_node = pzoom_code_info::DataFlowNode {
                        id: pzoom_code_info::data_flow::node::DataFlowNodeId::Var(
                            pzoom_code_info::VarId(
                                analyzer
                                    .interner
                                    .find(var_name)
                                    .unwrap_or(pzoom_str::StrId::EMPTY),
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                        ),
                        kind: pzoom_code_info::data_flow::node::DataFlowNodeKind::TaintSource {
                            pos: None,
                            types: pzoom_code_info::data_flow::node::SinkType::all_input(),
                        },
                    };
                    superglobal_type.parent_nodes.push(source_node.clone());
                    analysis_data.data_flow_graph.add_node(source_node);
                }
                context.set_var_type(var_id.clone(), superglobal_type.clone());
                analysis_data
                    .expr_types
                    .insert(pos, Rc::new(superglobal_type));
            } else if let alt_var_id = get_alternate_var_id(var_name)
                && context.get_var_type(&alt_var_id).is_some()
            {
                if let Some(var_type) = context.get_var_type(&alt_var_id) {
                    maybe_emit_possibly_undefined_variable(
                        analyzer,
                        var_name,
                        pos,
                        analysis_data,
                        context,
                    );
                    analysis_data
                        .expr_types
                        .insert(pos, Rc::new(var_type.clone()));
                } else {
                    maybe_emit_undefined_variable(analyzer, var_name, pos, analysis_data, context);

                    // Variable not yet assigned - could be undefined
                    // For now, treat as mixed
                    analysis_data
                        .expr_types
                        .insert(pos, Rc::new(TUnion::mixed()));
                }
            } else {
                maybe_emit_undefined_variable(analyzer, var_name, pos, analysis_data, context);

                // Variable not yet assigned - could be undefined
                // For now, treat as mixed
                let mut expr_type = TUnion::mixed();
                // Psalm registerPossiblyUndefinedVariable: connect prior
                // conditional assignments of this variable to the fetch so
                // they count as used.
                if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
                    && (context.vars_possibly_in_scope.contains(var_name)
                        || context
                            .vars_possibly_in_scope
                            .contains(&get_alternate_var_id(var_name)))
                {
                    let use_node = DataFlowNode::get_for_variable_sink(
                        VarId(
                            analyzer
                                .interner
                                .find(&var_id)
                                .unwrap_or(pzoom_str::StrId::EMPTY),
                        ),
                        make_data_flow_node_position(analyzer, pos),
                    );
                    analysis_data.data_flow_graph.add_node(use_node.clone());
                    connect_var_assignment_sources(
                        analyzer,
                        analysis_data,
                        &var_id,
                        pos,
                        &use_node,
                    );
                    expr_type.parent_nodes.push(use_node);
                }
                analysis_data.expr_types.insert(pos, Rc::new(expr_type));
            }
        }
        Variable::Indirect(_indirect) => {
            // Variable variables ($$name) - type is unknown at static analysis time
            analysis_data
                .expr_types
                .insert(pos, Rc::new(TUnion::mixed()));
        }
        Variable::Nested(_nested) => {
            // Nested variables - type is unknown at static analysis time
            analysis_data
                .expr_types
                .insert(pos, Rc::new(TUnion::mixed()));
        }
    }
}

/// Report `UndefinedDocblockClass` for named-object atomics in an applied
/// inline `@var` annotation that reference unknown classes (Psalm validates
/// var comments via checkFullyQualifiedClassLikeName).
pub(crate) fn emit_undefined_docblock_classes_in_annotation(
    analyzer: &StatementsAnalyzer<'_>,
    annotation_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    for atomic in &annotation_type.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
            continue;
        }
        // A class-constant token (`A::TYPE_*`, `A::FOO`) is not a class
        // reference: the type expander resolves it lazily against the
        // codebase, reporting UndefinedConstant there if bogus.
        if analyzer.interner.lookup(*name).contains("::") {
            continue;
        }
        if analyzer.codebase.get_class(*name).is_some() {
            continue;
        }

        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedDocblockClass,
            format!(
                "Docblock-defined class or interface {} does not exist",
                analyzer.interner.lookup(*name)
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
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
        // Psalm's PossiblyUndefined(Global)Variable report (the
        // vars_possibly_in_scope/first-appearance branch of
        // VariableFetchAnalyzer) is gated on
        // `!$context->inside_isset && !$context->inside_unset`: a variable
        // with a prior appearance never reports inside isset()/empty()/unset().
        let possibly_in_scope = context.vars_possibly_in_scope.contains(var_name)
            || context
                .vars_possibly_in_scope
                .contains(&get_alternate_var_id(var_name));
        if possibly_in_scope {
            return;
        }
        // A never-defined variable still reports, except inside
        // isset()/empty() at non-function scope (Psalm's
        // VariableFetchAnalyzer: `!$context->inside_isset ||
        // $statements_analyzer->getSource() instanceof FunctionLikeAnalyzer`
        // gates the Undefined(Global)Variable report; UnsetAnalyzer only sets
        // inside_unset, so unset() reports in both scopes).
        if context.inside_isset && analyzer.function_info.is_none() {
            return;
        }
    }

    // The root of an array assignment is a write position: an undeclared root
    // becomes a fresh array, which Psalm reports as only *possibly* undefined
    // ("first seen" at the assignment itself) in both function and global
    // scope.
    if context.inside_assignment_root {
        if !should_emit_undefined_variable(var_name) {
            return;
        }
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            if analyzer.function_info.is_none() {
                IssueKind::PossiblyUndefinedGlobalVariable
            } else {
                IssueKind::PossiblyUndefinedVariable
            },
            format!(
                "Possibly undefined variable ${}",
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

    if !should_emit_undefined_variable(var_name) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    // A variable assigned on some-but-not-all paths is possibly (not
    // definitely) undefined (Psalm: vars_possibly_in_scope).
    if context.vars_possibly_in_scope.contains(var_name)
        || context
            .vars_possibly_in_scope
            .contains(&get_alternate_var_id(var_name))
    {
        analysis_data.add_issue(Issue::new(
            if analyzer.function_info.is_none() {
                IssueKind::PossiblyUndefinedGlobalVariable
            } else {
                IssueKind::PossiblyUndefinedVariable
            },
            format!(
                "Possibly undefined variable ${}",
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

    // Writing through the variable (array-assignment root) is not a read.
    if context.inside_assignment_root {
        return;
    }

    // Psalm's VariableFetchAnalyzer: a variable present in vars_in_scope is
    // defined — the only in-scope possibly-undefined report is the try-block
    // flag carried on the type itself. (`possibly_assigned_var_ids` is
    // unused-variable bookkeeping in Psalm, never an undefinedness signal;
    // gating on it flagged loop-redefined variables on fixpoint re-analysis.)
    let from_try = context
        .locals
        .get(&VarName::new(var_name))
        .is_some_and(|t| t.possibly_undefined_from_try);
    if !from_try {
        return;
    }

    if !should_emit_undefined_variable(var_name) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    analysis_data.add_issue(Issue::new(
        if analyzer.function_info.is_none() {
            IssueKind::PossiblyUndefinedGlobalVariable
        } else {
            IssueKind::PossiblyUndefinedVariable
        },
        format!(
            "Possibly undefined variable ${} defined in try block",
            normalize_var_name(var_name)
        ),
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

fn get_alternate_var_id(var_name: &str) -> VarName {
    if let Some(stripped) = var_name.strip_prefix('$') {
        VarName::new(stripped)
    } else {
        VarName::from(format!("${}", var_name))
    }
}

pub(crate) fn is_superglobal(var_name: &str) -> bool {
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
            | "http_response_header"
    )
}

pub(crate) fn get_superglobal_default_type(var_name: &str) -> Option<TUnion> {
    let normalized = normalize_var_name(var_name);

    match normalized {
        "_SERVER" | "_GET" | "_POST" | "_FILES" | "_COOKIE" | "_SESSION" | "_REQUEST"
        | "GLOBALS" => Some(TUnion::new(TAtomic::array(
            TUnion::array_key(),
            TUnion::mixed(),
        ))),
        // Psalm types $_ENV entries as scalar (environment values are
        // strings/numbers, never arrays or objects).
        "_ENV" => Some(TUnion::new(TAtomic::array(
            TUnion::array_key(),
            TUnion::new(TAtomic::TScalar),
        ))),
        // Psalm: $argv/$argc exist only in CLI — null otherwise, with
        // ignore_nullable_issues set; $argc is int<1, max>.
        "argc" => {
            let mut argc_type = TUnion::from_types(vec![
                TAtomic::TIntRange {
                    min: Some(1),
                    max: None,
                },
                TAtomic::TNull,
            ]);
            argc_type.ignore_nullable_issues = true;
            Some(argc_type)
        }
        "argv" => {
            let mut argv_type = TUnion::from_types(vec![
                TAtomic::non_empty_list(TUnion::string()),
                TAtomic::TNull,
            ]);
            argv_type.ignore_nullable_issues = true;
            Some(argv_type)
        }
        // Psalm: exists only in the local scope after a successful network
        // request — `non-empty-list<non-falsy-string>`, possibly undefined.
        "http_response_header" => {
            let mut header_list =
                TUnion::new(TAtomic::non_empty_list(TUnion::new(TAtomic::TTruthyString)));
            header_list.possibly_undefined_from_try = true;
            Some(header_list)
        }
        _ => None,
    }
}

pub(crate) fn get_inline_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    offset: u32,
    var_id: &str,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(offset)?;

    for annotation in annotations {
        match annotation.var_name {
            Some(name) if analyzer.interner.lookup(name).as_ref() == var_id => {
                return Some(annotation.var_type.clone());
            }
            _ => {}
        }
    }

    None
}

/// Psalm's `StatementsAnalyzer::registerPossiblyUndefinedVariable`: when a
/// fetched variable's type carries no dataflow parents (a
/// conditionally-assigned variable rebuilt by isset/empty reconciliation),
/// connect every registered assignment node for that variable in the current
/// function to the fetch, so conditional assignments still count as used.
fn connect_var_assignment_sources(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    var_id: &VarName,
    fetch_pos: Pos,
    sink_node: &DataFlowNode,
) {
    use pzoom_code_info::data_flow::node::DataFlowNodeId;

    let var_str_id = analyzer
        .interner
        .find(var_id)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    let function_start = analyzer
        .function_info
        .map(|function_info| function_info.start_offset)
        .unwrap_or(0);

    let matching_sources: Vec<DataFlowNodeId> = analysis_data
        .data_flow_graph
        .sources
        .keys()
        .filter(|id| match id {
            DataFlowNodeId::Var(node_var, file, start, _)
            | DataFlowNodeId::Param(node_var, file, start, _) => {
                node_var.0 == var_str_id
                    && *file == analyzer.file_path
                    && *start >= function_start
                    && *start < fetch_pos.0
            }
            _ => false,
        })
        .cloned()
        .collect();

    for source_id in matching_sources {
        analysis_data.data_flow_graph.add_path(
            &source_id,
            &sink_node.id,
            pzoom_code_info::PathKind::Default,
            vec![],
            vec![],
        );
    }
}
