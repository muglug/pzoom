//! Try/catch statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::r#try::Try;
use mago_syntax::ast::ast::type_hint::Hint;

use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::StrId;
use pzoom_syntax::resolve_hint;

use crate::context::BlockContext;
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

    // Analyze the try block
    let mut try_context = original_context.clone();
    stmt_analyzer::analyze_stmts(
        analyzer,
        try_stmt.block.statements.as_slice(),
        analysis_data,
        &mut try_context,
    )?;

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

    let mut all_catches_leave = !try_stmt.catch_clauses.is_empty();

    // Analyze each catch clause
    for catch in try_stmt.catch_clauses.iter() {
        let mut catch_context = finally_entry_context.clone();
        catch_context.has_returned = false;

        let raw_catch_type = resolve_catch_hint_union(analyzer, &catch.hint, context);
        maybe_emit_invalid_catch_issue(analyzer, &catch.hint, &raw_catch_type, analysis_data);
        let catch_var_type = augment_catch_union_with_throwable(analyzer, &raw_catch_type);

        // Add the exception variable to context if it exists
        if let Some(var) = &catch.variable {
            let var_name = var.name;
            let var_name_id = analyzer.interner.intern(var_name);
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

        // Catch state can feed finally regardless of whether it exits.
        finally_entry_context.merge(&catch_context);

        if !catch_exits {
            continuing_branch_contexts.push(catch_context);
        }
    }

    // Merge contexts from non-exiting try/catch branches.
    if continuing_branch_contexts.is_empty() {
        context.has_returned = true;
    } else {
        let mut merged_context = continuing_branch_contexts.remove(0);
        for branch_ctx in &continuing_branch_contexts {
            merged_context.merge(branch_ctx);
        }
        *context = merged_context;
    }

    // Analyze the finally block if present
    let mut finally_has_returned = false;
    if let Some(finally) = &try_stmt.finally_clause {
        let mut finally_context = finally_entry_context.clone();
        finally_context.has_returned = false;
        finally_context.inside_try = true;
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
            context.merge(&finally_context);
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

    for atomic in &catch_type.types {
        if !atomic_is_throwable(analyzer, atomic) {
            has_non_throwable = true;
            break;
        }
    }

    if !has_non_throwable {
        return;
    }

    let span = hint.span();
    if crate::issue_suppression::is_issue_suppressed_at(
        analyzer,
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
                        is_static: false, remapped_params: false },
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
        let stripped_id = analyzer.interner.intern(stripped);
        return stripped_id == StrId::THROWABLE
            || object_type_comparator::is_class_subtype_of(
                stripped_id,
                StrId::THROWABLE,
                analyzer.codebase,
            );
    }

    false
}
