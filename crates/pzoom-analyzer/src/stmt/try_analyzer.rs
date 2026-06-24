//! Try/catch statement analyzer.

use std::rc::Rc;
use mago_span::HasSpan;
use mago_syntax::ast::ast::r#try::Try;
use mago_syntax::ast::ast::type_hint::Hint;

use pzoom_code_info::VarName;
use pzoom_code_info::combine_union_types;
use pzoom_code_info::{
    DataFlowNode, DataFlowNodeId, DataFlowNodeKind, GraphKind, PathKind, TAtomic, TUnion, VarId,
    VariableSourceKind,
};
use pzoom_str::StrId;
use pzoom_syntax::resolve_hint;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{self, ControlAction};
use crate::stmt_analyzer;
use crate::type_comparator::object_type_comparator;

/// Analyze a try/catch statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    try_stmt: &Try<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let original_context = context.clone();

    // Psalm threads a `FinallyScope` through the try/catch (`Context::$finally_scope`)
    // so each control-flow exit (notably `return`) collects its live variables
    // for the finally block. Only created when a finally clause is present.
    let finally_scope = try_stmt.finally_clause.as_ref().map(|_| {
        std::rc::Rc::new(std::cell::RefCell::new(
            crate::scope::FinallyScope::default(),
        ))
    });

    // Analyze the try block
    let mut try_context = original_context.clone();
    let was_inside_try = try_context.inside_try;
    try_context.inside_try = true;
    try_context.finally_scope = finally_scope.clone();
    stmt_analyzer::analyze_stmts(
        analyzer,
        try_stmt.block.statements.as_slice(),
        analysis_data,
        &mut try_context,
    )?;
    try_context.inside_try = was_inside_try;
    try_context.finally_scope = None;

    // Vars newly assigned in the try body (Psalm's $newly_assigned_var_ids):
    // until proven definitely assigned by every continuing catch, they are
    // possibly undefined after the try/catch (the try may have thrown midway).
    // (Psalm iterates the post-try vars_in_scope and marks the ones absent
    // from the pre-try context — reassignments of pre-existing vars stay
    // definite.)
    let newly_assigned_in_try: Vec<VarName> = try_context
        .locals
        .keys()
        .filter(|var_id| !original_context.locals.contains_key(*var_id))
        .cloned()
        .collect();
    let mut definitely_newly_assigned: rustc_hash::FxHashSet<VarName> =
        newly_assigned_in_try.iter().cloned().collect();

    // Collect only branches that can continue after the try/catch.
    let mut continuing_branch_contexts = Vec::new();
    let try_actions = scope_analyzer::get_control_actions(
        try_stmt.block.statements.as_slice(),
        analysis_data,
        &[],
        true,
    );
    let try_exits = try_context.has_returned || !try_actions.contains(&ControlAction::None);
    if !try_exits {
        continuing_branch_contexts.push(try_context.clone());
    }

    // Finally can run both after a normal try path and after a throw path.
    // Keep an entry context that includes pre-try state and branch outcomes.
    let mut finally_entry_context = original_context.clone();
    finally_entry_context.merge(&try_context);
    // Like catch entry, the finally runs after a throw that may have preceded a
    // try assignment, so a try-assigned variable read in the finally is possibly
    // undefined (Psalm). `possibly_undefined_from_try` is the flag
    // VariableFetchAnalyzer gates the in-scope PossiblyUndefinedVariable report
    // on, so set it too — otherwise the read passes silently.
    for var_id in &newly_assigned_in_try {
        if let Some(var_type) = finally_entry_context.locals.get_mut_owned(var_id) {
            var_type.possibly_undefined_from_try = true;
        }
    }

    let mut all_catches_leave = !try_stmt.catch_clauses.is_empty();

    // Analyze each catch clause
    for catch in try_stmt.catch_clauses.iter() {
        let mut catch_context = finally_entry_context.clone();
        catch_context.has_returned = false;
        // A `return` inside a catch also feeds the finally scope.
        catch_context.finally_scope = finally_scope.clone();

        // Psalm's TryAnalyzer marks try-assigned vars possibly undefined on
        // catch entry — the catch runs after a throw that may have preceded
        // the assignment.
        for var_id in &newly_assigned_in_try {
            if let Some(var_type) = catch_context.locals.get_mut_owned(var_id) {
                var_type.possibly_undefined_from_try = true;
            }
        }
        let catch_entry_assigned = catch_context.assigned_var_ids.clone();

        let raw_catch_type = resolve_catch_hint_union(analyzer, &catch.hint, context);
        maybe_emit_invalid_catch_issue(analyzer, &catch.hint, &raw_catch_type, analysis_data);
        let catch_var_type = augment_catch_union_with_throwable(analyzer, &raw_catch_type);

        // Add the exception variable to context if it exists
        if let Some(var) = &catch.variable {
            let var_name = var.name;
            let var_name_id = VarName::new(var_name);
            let mut catch_var_type = catch_var_type;

            // Hakana `try_analyzer`: the catch variable is a fresh variable
            // source (function-body graphs) that immediately flows into an
            // unlabelled variable-use sink, so the binding never reads as
            // unused. (Whole-program graphs use a plain `Var` lvar node.)
            let var_span = var.span();
            let var_pos = make_data_flow_node_position(
                analyzer,
                (var_span.start.offset, var_span.end.offset),
            );

            let new_parent_node = if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
                DataFlowNode::get_for_variable_source(
                    VariableSourceKind::Default,
                    VarId(
                        analyzer
                            .interner
                            .find(&var_name_id)
                            .unwrap_or(pzoom_str::StrId::EMPTY),
                    ),
                    var_pos,
                    false,
                    true,
                    false,
                    false,
                    false,
                )
            } else {
                DataFlowNode::get_for_lvar(
                    VarId(
                        analyzer
                            .interner
                            .find(&var_name_id)
                            .unwrap_or(pzoom_str::StrId::EMPTY),
                    ),
                    var_pos,
                )
            };

            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
                let assignment_node = DataFlowNode {
                    id: DataFlowNodeId::UnlabelledSink(
                        var_pos.file_path,
                        var_pos.start_offset,
                        var_pos.end_offset,
                    ),
                    kind: DataFlowNodeKind::VariableUseSink { pos: var_pos },
                };

                analysis_data.data_flow_graph.add_path(
                    &new_parent_node.id,
                    &assignment_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );

                analysis_data.data_flow_graph.add_node(assignment_node);
            }

            catch_var_type.parent_nodes.push(new_parent_node);

            // The catch variable is a fresh assignment: any memoized path
            // types rooted in it (`$e->getMessage()` from an earlier catch)
            // are stale (mirrors plain-assignment clearing).
            crate::expr::assignment_analyzer::clear_dependent_property_types(
                &mut catch_context,
                var_name,
            );
            crate::expr::assignment_analyzer::clear_array_path_types_for_base_var(
                &mut catch_context,
                var_name,
            );
            crate::expr::assignment_analyzer::clear_dependent_array_access_types(
                &mut catch_context,
                var_name,
            );

            catch_context.set_var_type(var_name_id, catch_var_type);
        }

        // Analyze the catch block
        stmt_analyzer::analyze_stmts(
            analyzer,
            catch.block.statements.as_slice(),
            analysis_data,
            &mut catch_context,
        )?;

        let catch_actions = scope_analyzer::get_control_actions(
            catch.block.statements.as_slice(),
            analysis_data,
            &[],
            true,
        );
        let catch_exits =
            catch_context.has_returned || !catch_actions.contains(&ControlAction::None);
        all_catches_leave &= catch_exits;

        // A continuing catch keeps a var definitely assigned only if it
        // assigns it too (Psalm's $definitely_newly_assigned_var_ids
        // intersection).
        if !catch_exits {
            definitely_newly_assigned.retain(|var_id| {
                catch_context
                    .assigned_var_ids
                    .get(var_id)
                    .copied()
                    .unwrap_or(0)
                    > catch_entry_assigned.get(var_id).copied().unwrap_or(0)
            });
        }

        // Catch state can feed finally regardless of whether it exits. A var
        // the catch introduced (absent from the try's view of the finally
        // scope) is possibly undefined there — and marked from-try, so
        // `isset($exception)` in the finally keeps its isset semantics
        // (Psalm's `$type->setPossiblyUndefined(true, true)` when merging
        // catch vars into the FinallyScope).
        let vars_before_catch_merge: rustc_hash::FxHashSet<VarName> =
            finally_entry_context.locals.keys().cloned().collect();
        finally_entry_context.merge(&catch_context);
        for (var_id, var_type) in finally_entry_context.locals.iter_mut() {
            if !vars_before_catch_merge.contains(var_id) {
                Rc::make_mut(var_type).possibly_undefined_from_try = true;
            }
        }

        if !catch_exits {
            continuing_branch_contexts.push(catch_context);
        }
    }

    // Merge contexts from non-exiting try/catch branches.
    if continuing_branch_contexts.is_empty() {
        context.has_returned = true;
    } else {
        let has_continuing_catch = continuing_branch_contexts.len() > 1;
        let mut merged_context = continuing_branch_contexts.remove(0);
        for branch_ctx in &continuing_branch_contexts {
            merged_context.merge(branch_ctx);
        }
        *context = merged_context;

        // Vars the try assigned but some continuing catch did not are
        // possibly undefined after the statement (Psalm clears the flag only
        // for $definitely_newly_assigned_var_ids).
        if has_continuing_catch {
            for var_id in &newly_assigned_in_try {
                if definitely_newly_assigned.contains(var_id) {
                    continue;
                }
                if let Some(var_type) = context.locals.get_mut_owned(var_id) {
                    var_type.possibly_undefined_from_try = true;
                }
            }
        }
    }

    // Analyze the finally block if present
    let mut finally_has_returned = false;
    if let Some(finally) = &try_stmt.finally_clause {
        let mut finally_context = finally_entry_context.clone();
        finally_context.has_returned = false;

        // Fold in variables collected from try/catch exits (Psalm consumes the
        // FinallyScope here). pzoom's finally entry context already merges the
        // try/catch end states, so only variables it does not already track are
        // added — a variable assigned solely before an early `return` deep in
        // the try, which the end-state merge would otherwise miss. It enters the
        // finally as possibly undefined, since the finally may run on a path
        // that never reached that assignment.
        if let Some(finally_scope) = &finally_scope {
            for (var_id, exit_type) in &finally_scope.borrow().vars_in_scope {
                if !finally_context.locals.contains_key(var_id) {
                    let mut possibly_undefined = exit_type.clone();
                    possibly_undefined.possibly_undefined_from_try = true;
                    finally_context
                        .locals
                        .insert(var_id.clone(), possibly_undefined);
                }
            }
        }

        stmt_analyzer::analyze_stmts(
            analyzer,
            finally.block.statements.as_slice(),
            analysis_data,
            &mut finally_context,
        )?;
        finally_has_returned = finally_context.has_returned;

        // Finally always executes, but only its non-exiting paths continue after this statement.
        let finally_actions = scope_analyzer::get_control_actions(
            finally.block.statements.as_slice(),
            analysis_data,
            &[],
            true,
        );
        let finally_exits =
            finally_context.has_returned || !finally_actions.contains(&ControlAction::None);

        if !finally_exits && !context.has_returned {
            // Finally always runs: union its types into the post-try state
            // without demoting definedness. A var assigned in the try and in
            // scope here stays definitely assigned — the finally's pessimistic
            // entry view (the try may have thrown mid-way) only applies inside
            // the finally block itself. Mirrors Psalm's TryAnalyzer, which
            // combines vars_in_scope without touching assignment bookkeeping.
            for (var_id, finally_type) in &finally_context.locals {
                if let Some(existing) = context.locals.get(var_id) {
                    let existing_defined = !existing.possibly_undefined_from_try;
                    let mut combined = combine_union_types(existing, finally_type, false);
                    // The pessimistic possibly-undefined entry view applies
                    // only INSIDE the finally; the normal path's definedness
                    // wins afterwards (clear the try-block flag too, so a read
                    // after the finally is not reported).
                    if existing_defined {
                        combined.possibly_undefined_from_try = false;
                    }
                    context.locals.insert(var_id.clone(), combined);
                } else {
                    context.locals.insert(var_id.clone(), finally_type.as_ref().clone());
                    context.possibly_assigned_var_ids.insert(var_id.clone());
                }
            }
            for (var_id, count) in &finally_context.assigned_var_ids {
                context
                    .assigned_var_ids
                    .entry(var_id.clone())
                    .or_insert(*count);
            }
            context
                .vars_possibly_in_scope
                .extend(finally_context.vars_possibly_in_scope.iter().cloned());
        }
    }

    let body_has_returned = !try_actions.contains(&ControlAction::None);
    let catches_all_leave_or_absent = if try_stmt.catch_clauses.is_empty() {
        true
    } else {
        all_catches_leave
    };
    context.has_returned = context.has_returned
        || (body_has_returned && catches_all_leave_or_absent)
        || finally_has_returned;

    Ok(())
}

fn resolve_catch_hint_union(
    analyzer: &StatementsAnalyzer<'_>,
    hint: &Hint<'_>,
    context: &BlockContext,
) -> TUnion {
    let mut catch_type = resolve_hint(
        hint,
        analyzer.interner,
        context.namespace,
        context.self_class,
        context.parent_class,
        None,
        Some(analyzer.resolved_names),
    );

    for atomic in &mut catch_type.types {
        apply_runtime_class_aliases_to_atomic(atomic, context);
    }

    catch_type
}

fn apply_runtime_class_aliases_to_atomic(atomic: &mut TAtomic, context: &BlockContext) {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            if let Some(target) = context.class_aliases.get(name) {
                *name = *target;
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                apply_runtime_class_aliases_to_atomic(nested, context);
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested in &mut as_type.types {
                apply_runtime_class_aliases_to_atomic(nested, context);
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            apply_runtime_class_aliases_to_atomic(as_type, context);
        }
        _ => {}
    }
}

fn maybe_emit_invalid_catch_issue(
    analyzer: &StatementsAnalyzer<'_>,
    hint: &Hint<'_>,
    catch_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut has_non_throwable = false;
    let mut has_undefined_class = false;

    for atomic in &catch_type.types {
        // An unknown catch class is UndefinedClass, not InvalidCatch — we
        // can't judge whether it extends Throwable.
        if let TAtomic::TNamedObject { name, .. } = atomic
            && analyzer.codebase.get_class(*name).is_none()
            && *name != pzoom_str::StrId::THROWABLE
        {
            has_undefined_class = true;
            let span = hint.span();
            if !crate::issue_suppression::is_issue_suppressed_at(
                analyzer,
                analysis_data,
                span.start.offset,
                "UndefinedClass",
            ) {
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(pzoom_code_info::Issue::new(
                    pzoom_code_info::IssueKind::UndefinedClass,
                    crate::class_casing::undefined_class_message(
                        analyzer,
                        analyzer.interner.lookup(*name),
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
            continue;
        }

        if !atomic_is_throwable(analyzer, atomic) {
            has_non_throwable = true;
            break;
        }
    }

    if has_undefined_class {
        return;
    }

    if !has_non_throwable {
        return;
    }

    let span = hint.span();
    if crate::issue_suppression::is_issue_suppressed_at(
        analyzer,
        analysis_data,
        span.start.offset,
        "InvalidCatch",
    ) {
        return;
    }
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(pzoom_code_info::Issue::new(
        pzoom_code_info::IssueKind::InvalidCatch,
        "Catch type must extend Throwable",
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
}

fn augment_catch_union_with_throwable(
    analyzer: &StatementsAnalyzer<'_>,
    catch_type: &TUnion,
) -> TUnion {
    let mut types = Vec::with_capacity(catch_type.types.len());

    for atomic in &catch_type.types {
        if atomic_is_throwable(analyzer, atomic) {
            types.push(atomic.clone());
            continue;
        }

        match atomic {
            TAtomic::TNamedObject { .. } => {
                types.push(TAtomic::TObjectIntersection {
                    types: vec![
                        atomic.clone(),
                        TAtomic::TNamedObject {
                            name: StrId::THROWABLE,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        },
                    ],
                });
            }
            TAtomic::TObjectIntersection { types: nested } => {
                let mut expanded = nested.clone();
                if !expanded.iter().any(|part| {
                    matches!(part, TAtomic::TNamedObject { name, .. } if *name == StrId::THROWABLE)
                }) {
                    expanded.push(TAtomic::TNamedObject {
                        name: StrId::THROWABLE,
                        type_params: None,
                    is_static: false, remapped_params: false });
                }
                types.push(TAtomic::TObjectIntersection { types: expanded });
            }
            _ => types.push(atomic.clone()),
        }
    }

    TUnion::from_types(types)
}

fn atomic_is_throwable(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => named_object_is_throwable(analyzer, *name),
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|nested| atomic_is_throwable(analyzer, nested)),
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(|nested| atomic_is_throwable(analyzer, nested)),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_is_throwable(analyzer, as_type),
        _ => false,
    }
}

fn named_object_is_throwable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
    if name == StrId::THROWABLE
        || object_type_comparator::is_class_subtype_of(name, StrId::THROWABLE, analyzer.codebase)
    {
        return true;
    }

    let normalized = analyzer.interner.lookup(name);
    if let Some(stripped) = normalized.strip_prefix('\\') {
        let stripped_id = analyzer
            .interner
            .find(stripped)
            .unwrap_or(pzoom_str::StrId::EMPTY);
        return stripped_id == StrId::THROWABLE
            || object_type_comparator::is_class_subtype_of(
                stripped_id,
                StrId::THROWABLE,
                analyzer.codebase,
            );
    }

    false
}
