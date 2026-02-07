//! Statement analyzer - dispatches to specific statement type analyzers.

use mago_span::HasSpan;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::statement::Statement;
use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

// Import statement-specific analyzers
use crate::stmt::{
    class_analyzer, echo_analyzer, expression_stmt_analyzer, for_analyzer, foreach_analyzer,
    function_analyzer, global_analyzer, if_else_analyzer, return_analyzer, static_analyzer,
    switch_analyzer, try_analyzer, unset_analyzer, while_analyzer,
};

/// Analyze a sequence of statements.
pub fn analyze_stmts(
    analyzer: &StatementsAnalyzer<'_>,
    stmts: &[Statement<'_>],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    for stmt in stmts {
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
                    // Unreachable code - skip but don't break (allow analyzing further)
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
    emit_inline_trace_annotations(
        analyzer,
        span.start.offset,
        span.end.offset,
        analysis_data,
        context,
    );
    emit_invalid_inline_var_annotation_issues(
        analyzer,
        span.start.offset,
        span.end.offset,
        analysis_data,
    );

    match stmt {
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
        Statement::Class(class_stmt) => {
            class_analyzer::analyze(analyzer, class_stmt, analysis_data, context)
        }
        Statement::Trait(trait_stmt) => {
            class_analyzer::analyze_trait(analyzer, trait_stmt, analysis_data, context)
        }
        Statement::Interface(interface_stmt) => {
            class_analyzer::analyze_interface(analyzer, interface_stmt, analysis_data, context)
        }
        Statement::Enum(enum_stmt) => {
            class_analyzer::analyze_enum(analyzer, enum_stmt, analysis_data, context)
        }

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
        Statement::DoWhile(_) => Ok(()),
        Statement::Switch(switch_stmt) => {
            switch_analyzer::analyze(analyzer, switch_stmt, analysis_data, context)
        }
        Statement::Try(try_stmt) => {
            try_analyzer::analyze(analyzer, try_stmt, analysis_data, context)
        }
        Statement::Declare(_) => Ok(()),
        Statement::Goto(_) => Ok(()),
        Statement::Label(_) => Ok(()),
        Statement::Continue(_) => Ok(()),
        Statement::Break(_) => Ok(()),
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
        Statement::EchoTag(_) => Ok(()),
        Statement::Noop(_) => Ok(()),
        // Non-exhaustive enum - catch future variants
        _ => Ok(()),
    }
}

fn emit_invalid_inline_var_annotation_issues(
    analyzer: &StatementsAnalyzer<'_>,
    start_offset: u32,
    end_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(file_info) = analyzer.codebase.files.get(&analyzer.file_path) else {
        return;
    };

    for (annotation_offset, annotations) in &file_info.inline_annotations.var_annotations {
        if *annotation_offset < start_offset || *annotation_offset > end_offset {
            continue;
        }

        if !annotations.iter().any(|annotation| annotation.is_invalid) {
            continue;
        }

        let (line, col) = analyzer.get_line_column(*annotation_offset);
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

fn emit_inline_trace_annotations(
    analyzer: &StatementsAnalyzer<'_>,
    start_offset: u32,
    end_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Some(file_info) = analyzer.codebase.files.get(&analyzer.file_path) else {
        return;
    };

    for (annotation_offset, annotations) in &file_info.inline_annotations.trace_annotations {
        if *annotation_offset < start_offset || *annotation_offset > end_offset {
            continue;
        }

        for annotation in annotations {
            for var_id in &annotation.var_names {
                let (line, col) = analyzer.get_line_column(*annotation_offset);
                let var_name = analyzer.interner.lookup(*var_id);
                let var_type = context
                    .get_var_type(*var_id)
                    .map(|t| t.get_id(Some(analyzer.interner)))
                    .unwrap_or_else(|| TUnion::mixed().get_id(Some(analyzer.interner)));

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

    // Create a function/class analyzer context that knows about the namespace
    for stmt in stmts {
        match stmt {
            Statement::Function(func_stmt) => {
                function_analyzer::analyze_with_namespace(
                    analyzer,
                    func_stmt,
                    ns_name,
                    analysis_data,
                    &mut ns_context,
                )?;
            }
            Statement::Class(class_stmt) => {
                class_analyzer::analyze_with_namespace(
                    analyzer,
                    class_stmt,
                    ns_name,
                    analysis_data,
                    &mut ns_context,
                )?;
            }
            Statement::Interface(interface_stmt) => {
                class_analyzer::analyze_interface_with_namespace(
                    analyzer,
                    interface_stmt,
                    ns_name,
                    analysis_data,
                    &mut ns_context,
                )?;
            }
            Statement::Trait(trait_stmt) => {
                class_analyzer::analyze_trait_with_namespace(
                    analyzer,
                    trait_stmt,
                    ns_name,
                    analysis_data,
                    &mut ns_context,
                )?;
            }
            Statement::Enum(enum_stmt) => {
                class_analyzer::analyze_enum_with_namespace(
                    analyzer,
                    enum_stmt,
                    ns_name,
                    analysis_data,
                    &mut ns_context,
                )?;
            }
            _ => {
                analyze_stmt(analyzer, stmt, analysis_data, &mut ns_context)?;
            }
        }
    }

    // Namespace blocks share the same file-level runtime variable scope.
    // Persist context changes (locals/assignments/aliases/etc) while restoring
    // the outer namespace marker.
    let outer_namespace = context.namespace;
    *context = ns_context;
    context.namespace = outer_namespace;

    Ok(())
}
