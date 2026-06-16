//! Statement analyzer - dispatches to specific statement type analyzers.

use mago_span::HasSpan;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::node::{Node, NodeKind};
use pzoom_code_info::{Issue, IssueKind};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

// Import statement-specific analyzers
use crate::stmt::class_analyzer::ClassLikeDeclaration;
use crate::stmt::{
    break_analyzer, class_analyzer, continue_analyzer, declare_analyzer, do_analyzer, echo_analyzer,
    expression_stmt_analyzer, for_analyzer, foreach_analyzer, function_analyzer, global_analyzer,
    if_else_analyzer, interface_analyzer, return_analyzer, static_analyzer, switch_analyzer,
    trait_analyzer, try_analyzer, unset_analyzer, while_analyzer,
};

/// Returns true if any statement in `statements` contains a `yield`/`yield from`,
/// without descending into nested function-like scopes (which have their own
/// generator context). Used to determine whether the enclosing function is a
/// generator.
pub fn body_contains_yield(statements: &[Statement<'_>]) -> bool {
    let mut stack: Vec<Node> = statements.iter().map(Node::Statement).collect();

    while let Some(node) = stack.pop() {
        match node.kind() {
            NodeKind::Yield | NodeKind::YieldFrom | NodeKind::YieldValue | NodeKind::YieldPair => {
                return true;
            }
            // Nested function-like scopes are generators in their own right.
            NodeKind::Closure
            | NodeKind::ArrowFunction
            | NodeKind::AnonymousClass
            | NodeKind::Function
            | NodeKind::Class
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Enum => {}
            _ => stack.extend(node.children()),
        }
    }

    false
}

/// Analyze a sequence of statements.
pub fn analyze_stmts(
    analyzer: &StatementsAnalyzer<'_>,
    stmts: &[Statement<'_>],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    for stmt in stmts {
        // `__halt_compiler()` ends the program text: everything after it is
        // raw data, never code (Psalm's parser produces nothing past it).
        if matches!(stmt, Statement::HaltCompiler(_)) {
            break;
        }

        // Check if control flow has already returned (unreachable code)
        // Skip certain statements that are allowed after return (like function/class declarations)
        if context.has_returned {
            match stmt {
                Statement::Function(_)
                | Statement::Class(_)
                | Statement::Interface(_)
                | Statement::Trait(_)
                | Statement::Enum(_)
                | Statement::Noop(_) => {
                    // These are allowed after return
                }
                _ => {
                    // Unreachable code - skip but don't break (allow analyzing further).
                    // Psalm's StatementsAnalyzer reports it when unused-variable
                    // detection is on ("Expressions after return/throw/continue").
                    if analyzer.config.report_unused {
                        let span = stmt.span();
                        let (line, col) = analyzer.get_line_column(span.start.offset);
                        analysis_data.add_issue(
                            pzoom_code_info::issue::Issue::new(
                                pzoom_code_info::issue::IssueKind::UnevaluatedCode,
                                "Expressions after return/throw/continue",
                                analyzer.file_path,
                                span.start.offset,
                                span.end.offset,
                                line,
                                col,
                            ),
                        );
                    }
                    continue;
                }
            }
        }

        analyze_stmt(analyzer, stmt, analysis_data, context)?;
    }

    Ok(())
}

/// Analyze a single statement, dispatching to the appropriate analyzer.
pub fn analyze_stmt(
    analyzer: &StatementsAnalyzer<'_>,
    stmt: &Statement<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let span = stmt.span();
    analysis_data.current_stmt_start = Some(span.start.offset);
    analysis_data.current_stmt_end = Some(span.end.offset);
    emit_invalid_inline_var_annotation_issues(analyzer, span.start.offset, analysis_data);
    apply_statement_var_annotations(analyzer, stmt, span.start.offset, analysis_data, context);
    apply_statement_scope_this_annotation(analyzer, span.start.offset, analysis_data, context);

    // Psalm's StatementsAnalyzer registers a statement docblock's
    // `@psalm-suppress` issues for the duration of the statement's analysis
    // (nested statements included). Record the docblock + statement spans so
    // the file-level filter can apply them to any issue inside the statement.
    record_stmt_docblock_suppressions(analyzer, span, analysis_data);

    let stmt_result = match stmt {
        // Control flow statements
        Statement::Return(ret) => return_analyzer::analyze(analyzer, ret, analysis_data, context),

        Statement::If(if_stmt) => {
            if_else_analyzer::analyze(analyzer, if_stmt, analysis_data, context)
        }

        Statement::While(while_stmt) => {
            while_analyzer::analyze(analyzer, while_stmt, analysis_data, context)
        }

        Statement::Foreach(foreach_stmt) => {
            foreach_analyzer::analyze(analyzer, foreach_stmt, analysis_data, context)
        }

        // Expression statements
        Statement::Expression(expr_stmt) => {
            expression_stmt_analyzer::analyze(analyzer, expr_stmt, analysis_data, context)
        }

        Statement::Echo(echo) => echo_analyzer::analyze(analyzer, echo, analysis_data, context),

        // Block statement
        Statement::Block(block) => analyze_stmts(
            analyzer,
            block.statements.as_slice(),
            analysis_data,
            context,
        ),

        // Namespace handling - process statements inside namespace with the namespace context
        Statement::Namespace(ns) => analyze_namespace(analyzer, ns, analysis_data, context),

        // Use statements don't need runtime analysis
        Statement::Use(_) => Ok(()),

        // Class-like declarations - analyze method bodies
        Statement::Class(class_stmt) => class_analyzer::analyze(
            analyzer,
            ClassLikeDeclaration::Class(class_stmt),
            analysis_data,
            context,
        ),
        Statement::Trait(trait_stmt) => {
            trait_analyzer::analyze(analyzer, trait_stmt, analysis_data, context)
        }
        Statement::Interface(interface_stmt) => {
            interface_analyzer::analyze(analyzer, interface_stmt, analysis_data, context)
        }
        Statement::Enum(enum_stmt) => class_analyzer::analyze(
            analyzer,
            ClassLikeDeclaration::Enum(enum_stmt),
            analysis_data,
            context,
        ),

        // Function declarations - analyze function body
        Statement::Function(func_stmt) => {
            function_analyzer::analyze(analyzer, func_stmt, analysis_data, context)
        }

        // Constant declarations are handled during scanning
        Statement::Constant(_) => Ok(()),

        // Opening/closing tags and inline HTML
        Statement::OpeningTag(_) | Statement::ClosingTag(_) | Statement::Inline(_) => Ok(()),

        // TODO: Implement these
        Statement::For(for_stmt) => {
            for_analyzer::analyze(analyzer, for_stmt, analysis_data, context)
        }
        Statement::DoWhile(do_while_stmt) => {
            do_analyzer::analyze(analyzer, do_while_stmt, analysis_data, context)
        }
        Statement::Switch(switch_stmt) => {
            switch_analyzer::analyze(analyzer, switch_stmt, analysis_data, context)
        }
        Statement::Try(try_stmt) => {
            try_analyzer::analyze(analyzer, try_stmt, analysis_data, context)
        }
        Statement::Declare(declare_stmt) => {
            declare_analyzer::analyze(analyzer, declare_stmt, analysis_data)
        }
        Statement::Goto(_) => Ok(()),
        Statement::Label(_) => Ok(()),
        Statement::Continue(continue_stmt) => {
            continue_analyzer::analyze(analyzer, continue_stmt, analysis_data, context);
            Ok(())
        }
        Statement::Break(break_stmt) => {
            break_analyzer::analyze(analyzer, break_stmt, analysis_data, context);
            Ok(())
        }
        Statement::Global(global_stmt) => {
            global_analyzer::analyze(analyzer, global_stmt, analysis_data, context);
            Ok(())
        }
        Statement::Static(static_stmt) => {
            static_analyzer::analyze(analyzer, static_stmt, analysis_data, context);
            Ok(())
        }
        Statement::Unset(unset_stmt) => {
            unset_analyzer::analyze(analyzer, unset_stmt, analysis_data, context)
        }
        Statement::HaltCompiler(_) => Ok(()),
        Statement::EchoTag(tag) => echo_analyzer::analyze_tag(analyzer, tag, analysis_data, context),
        Statement::Noop(_) => Ok(()),
        // Non-exhaustive enum - catch future variants
        _ => Ok(()),
    };

    // `@psalm-check-type` is evaluated against the variable state *after* the
    // annotated statement (the assertion's docblock precedes the statement it
    // describes), so this runs once the statement has updated the context.
    emit_check_type_annotations(
        analyzer,
        span.start.offset,
        span.end.offset,
        analysis_data,
        context,
    );

    // `@psalm-trace` likewise reports the variable state *after* the annotated
    // statement (Psalm's StatementsAnalyzer emits traces post-analysis).
    emit_inline_trace_annotations(analyzer, span.start.offset, analysis_data, context);

    stmt_result
}

/// Record the spans of a `@psalm-suppress` docblock immediately preceding
/// `stmt_span`, so the file-level issue filter suppresses matching issues
/// anywhere within the statement (Psalm's addSuppressedIssues /
/// removeSuppressedIssues around statement analysis).
fn record_stmt_docblock_suppressions(
    analyzer: &StatementsAnalyzer<'_>,
    stmt_span: mago_span::Span,
    analysis_data: &mut FunctionAnalysisData,
) {
    let source = analyzer.source;
    let stmt_start = (stmt_span.start.offset as usize).min(source.len());
    let Some((docblock_start, docblock)) =
        crate::issue_suppression::preceding_docblock(&source[..stmt_start])
    else {
        return;
    };
    if !docblock.contains("@psalm-suppress") && !docblock.contains("@psalm-fixme") {
        return;
    }
    let entry = (
        docblock_start as u32,
        (docblock_start + docblock.len()) as u32,
        stmt_span.start.offset,
        stmt_span.end.offset,
    );
    // Loop fixpoints re-analyze the same statement; record each span once.
    if !analysis_data.stmt_suppression_ranges.contains(&entry) {
        analysis_data.stmt_suppression_ranges.push(entry);
    }
}

fn emit_invalid_inline_var_annotation_issues(
    analyzer: &StatementsAnalyzer<'_>,
    start_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(file_info) = analyzer.codebase.files.get(&analyzer.file_path) else {
        return;
    };

    for (annotation_offset, annotations) in &file_info.inline_annotations.var_annotations {
        // Match the annotation's target statement exactly (see
        // emit_inline_trace_annotations) so enclosing compound statements
        // don't re-report it.
        if *annotation_offset != start_offset {
            continue;
        }

        let (line, col) = analyzer.get_line_column(*annotation_offset);

        // Psalm's name-first `@var $x Type` form: IncorrectDocblockException
        // ("Misplaced variable") surfaces as MissingDocblockType.
        if annotations
            .iter()
            .any(|annotation| annotation.is_misplaced_variable)
        {
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingDocblockType,
                "Misplaced variable",
                analyzer.file_path,
                *annotation_offset,
                *annotation_offset,
                line,
                col,
            ));
        }

        if annotations.iter().any(|annotation| annotation.is_invalid) {
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidDocblock,
                "Invalid docblock type",
                analyzer.file_path,
                *annotation_offset,
                *annotation_offset,
                line,
                col,
            ));
        }
    }
}

fn emit_inline_trace_annotations(
    analyzer: &StatementsAnalyzer<'_>,
    start_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Some(file_info) = analyzer.codebase.files.get(&analyzer.file_path) else {
        return;
    };

    for (annotation_offset, annotations) in &file_info.inline_annotations.trace_annotations {
        // An annotation's recorded offset is the start of the statement the
        // docblock precedes (its attachment target). Matching by exact start —
        // rather than span containment — keeps enclosing compound statements
        // (functions, ifs, loops) from re-emitting the trace with their own,
        // wider scope (Psalm attaches trace docblocks to a single statement).
        if *annotation_offset != start_offset {
            continue;
        }

        for annotation in annotations {
            for var_id in &annotation.var_names {
                let (line, col) = analyzer.get_line_column(*annotation_offset);
                let var_name = analyzer.interner.lookup(*var_id);

                // Psalm reports `UndefinedTrace` when the traced variable is
                // not in scope after the statement.
                let Some(var_type) = context.get_var_type(var_name.as_ref()) else {
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedTrace,
                        format!("Attempt to trace undefined variable {var_name}"),
                        analyzer.file_path,
                        *annotation_offset,
                        *annotation_offset,
                        line,
                        col,
                    ));
                    continue;
                };
                let var_type = var_type.get_id(Some(analyzer.interner));

                analysis_data.add_issue(Issue::new(
                    IssueKind::Trace,
                    format!("{var_name}: {var_type}"),
                    analyzer.file_path,
                    *annotation_offset,
                    *annotation_offset,
                    line,
                    col,
                ));
            }
        }
    }
}

/// Evaluates `@psalm-check-type[-exact]` assertions whose target offset falls
/// within the current statement, comparing the asserted type against the
/// in-scope variable type. Mirrors Psalm's `StatementsAnalyzer` check-type
/// handling. Malformed assertions (missing variable or type) are reported by a
/// separate file-level sweep, so only well-formed assertions are handled here.
fn emit_check_type_annotations(
    analyzer: &StatementsAnalyzer<'_>,
    start_offset: u32,
    end_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Some(file_info) = analyzer.codebase.files.get(&analyzer.file_path) else {
        return;
    };

    for (annotation_offset, annotations) in &file_info.inline_annotations.check_type_annotations {
        // Fire exactly once, for the statement the assertion is directly attached
        // to (its start offset equals the assertion's target). A range check would
        // re-fire for every enclosing statement (function body, block, …) — often
        // with a stale context that hasn't yet defined the variable.
        let _ = end_offset;
        if *annotation_offset != start_offset {
            continue;
        }

        for annotation in annotations {
            // Malformed assertions are handled by the file-level sweep.
            let (Some(var_id), Some(check_type)) = (annotation.var_id, annotation.check_type.as_ref())
            else {
                continue;
            };

            let (line, col) = analyzer.get_line_column(*annotation_offset);
            let var_name = analyzer.interner.lookup(var_id);

            let Some(checked_type) = context.get_var_type(&var_name) else {
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidDocblock,
                    format!("Attempt to check undefined variable {var_name}"),
                    analyzer.file_path,
                    *annotation_offset,
                    *annotation_offset,
                    line,
                    col,
                ));
                continue;
            };

            let mut check_type = check_type.clone();
            check_type.possibly_undefined = annotation.annotation_possibly_undefined;

            let mut forward = TypeComparisonResult::new();
            let mut reverse = TypeComparisonResult::new();
            let contained = union_type_comparator::is_contained_by(
                analyzer.codebase,
                checked_type,
                &check_type,
                false,
                false,
                &mut forward,
            );
            let exact_ok = !annotation.is_exact
                || union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &check_type,
                    checked_type,
                    false,
                    false,
                    &mut reverse,
                );

            let mismatch = check_type.possibly_undefined != checked_type.possibly_undefined
                || !contained
                || !exact_ok;

            if !mismatch {
                continue;
            }

            let checked_var_raw = annotation
                .checked_var_raw
                .clone()
                .unwrap_or_else(|| var_name.to_string());
            let check_var = format!(
                "{}{}",
                var_name,
                if checked_type.possibly_undefined { "?" } else { "" }
            );

            analysis_data.add_issue(Issue::new(
                IssueKind::CheckType,
                format!(
                    "Checked variable {} = {} does not match {} = {}",
                    checked_var_raw,
                    check_type.get_id(Some(analyzer.interner)),
                    check_var,
                    checked_type.get_id(Some(analyzer.interner)),
                ),
                analyzer.file_path,
                *annotation_offset,
                *annotation_offset,
                line,
                col,
            ));
        }
    }
}

/// Analyze a namespace statement.
fn analyze_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    ns: &Namespace<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the namespace name
    let ns_name = ns.name.as_ref().map(|n| n.value());

    // Set namespace context for function resolution
    let ns_id = ns_name.map(|n| analyzer.interner.intern(n));
    let mut ns_context = context.clone();
    ns_context.namespace = ns_id;

    // Visit statements in namespace
    let stmts = match &ns.body {
        NamespaceBody::Implicit(implicit) => implicit.statements.as_slice(),
        NamespaceBody::BraceDelimited(block) => block.statements.as_slice(),
    };

    // Every per-statement analyzer reads its enclosing namespace from `context`,
    // so a namespaced statement is dispatched exactly like a top-level one —
    // just with the namespace-aware `ns_context`.
    for stmt in stmts {
        analyze_stmt(analyzer, stmt, analysis_data, &mut ns_context)?;
    }

    // Namespace blocks share the same file-level runtime variable scope.
    // Persist context changes (locals/assignments/aliases/etc) while restoring
    // the outer namespace marker.
    let outer_namespace = context.namespace;
    *context = ns_context;
    context.namespace = outer_namespace;

    Ok(())
}


/// Psalm's StatementsAnalyzer: a statement-level `@var` docblock assigns the
/// commented types into the context ONCE, before the statement is analyzed —
/// narrowing inside the statement then proceeds from there (an `instanceof`
/// in an if-condition is not clobbered by re-application at each fetch).
/// Plain-assignment, foreach, and return statements are excluded: their
/// analyzers consume the docblock with their own semantics (Psalm's
/// `!($stmt instanceof Expression && $stmt->expr instanceof Assign) &&
/// !Foreach_ && !Return_` guard). For a plain assignment, annotations naming
/// *other* variables still apply (Psalm's AssignmentAnalyzer does the same
/// for non-target var comments).
/// Psalm's StatementsAnalyzer `psalm-scope-this` handling: from the annotated
/// statement on, `$this` is an instance of the named class. An unknown class
/// reports UndefinedDocblockClass instead.
fn apply_statement_scope_this_annotation(
    analyzer: &StatementsAnalyzer<'_>,
    offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(class_id) = analyzer.get_inline_scope_this_annotation(offset) else {
        return;
    };

    if analyzer.codebase.get_class(class_id).is_none() {
        let (line, col) = analyzer.get_line_column(offset);
        analysis_data.add_issue(pzoom_code_info::Issue::new(
            pzoom_code_info::IssueKind::UndefinedDocblockClass,
            format!(
                "Scope class {} does not exist",
                analyzer.interner.lookup(class_id)
            ),
            analyzer.file_path,
            offset,
            offset.saturating_add(1),
            line,
            col,
        ));
        return;
    }

    context.set_var_type(
        pzoom_code_info::VarName::new("$this"),
        pzoom_code_info::TUnion::new(pzoom_code_info::TAtomic::TNamedObject {
            name: class_id,
            type_params: None,
            is_static: false,
            remapped_params: false,
        }),
    );
}

fn apply_statement_var_annotations(
    analyzer: &StatementsAnalyzer<'_>,
    stmt: &Statement<'_>,
    offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::expression::Expression;

    let mut excluded_assignment_target: Option<pzoom_str::StrId> = None;
    match stmt {
        Statement::Foreach(_) | Statement::Return(_) => return,
        Statement::Expression(expr_stmt) => {
            if let Expression::Assignment(assignment) = expr_stmt.expression {
                if let Expression::Variable(
                    mago_syntax::ast::ast::variable::Variable::Direct(direct),
                ) = assignment.lhs.unparenthesized()
                {
                    excluded_assignment_target = Some(analyzer.interner.intern(direct.name));
                }
                // Assignment to a non-variable target (property/array path):
                // the assignment analyzer consumes path-shaped annotations,
                // but named annotations for regular variables still apply
                // here (Psalm's AssignmentAnalyzer sets non-target var
                // comments into scope the same way).
            }
        }
        _ => {}
    }

    let Some(annotations) = analyzer.get_inline_var_annotations(offset) else {
        return;
    };

    let annotations = annotations.clone();
    for annotation in &annotations {
        let Some(var_name) = annotation.var_name else {
            continue;
        };
        if excluded_assignment_target == Some(var_name) {
            continue;
        }

        crate::expr::variable_fetch_analyzer::emit_undefined_docblock_classes_in_annotation(
            analyzer,
            &annotation.var_type,
            (offset, offset),
            analysis_data,
        );
        // Psalm keeps the existing type's dataflow parents on the comment
        // type (`$comment_type->parent_nodes = $existing_var_type->parent_nodes`)
        // so a loop-carried assignment re-pinned by `@var` still counts as
        // used by next-iteration reads.
        let var_key = pzoom_code_info::VarName::new(analyzer.interner.lookup(var_name));
        let mut annotation_type = annotation.var_type.clone();
        if let Some(existing_type) = context.get_var_type(&var_key) {
            annotation_type.parent_nodes = existing_type.parent_nodes.clone();
        }
        context.set_var_type(var_key, annotation_type);
    }
}
