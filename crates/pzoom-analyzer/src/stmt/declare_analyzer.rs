//! Declare statement analyzer.
//!
//! Mirrors Psalm's `Internal/Analyzer/Statements/DeclareAnalyzer.php`: validates
//! each `declare()` directive and emits `UnrecognizedStatement` for unknown
//! directives, invalid directive values, and `strict_types` in block mode.

use mago_span::{HasSpan, Span};
use mago_syntax::cst::cst::declare::{Declare, DeclareBody, DeclareItem};
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::literal::Literal;
use mago_syntax::cst::cst::statement::Statement;
use pzoom_code_info::{Issue, IssueKind};

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze a declare statement (Psalm's `DeclareAnalyzer::analyze`).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    stmt: &Declare<'_>,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    for declaration in stmt.items.iter() {
        let declaration_key = pzoom_syntax::bytes_to_str(declaration.name.value);

        if declaration_key == "strict_types" {
            // Psalm flags `$stmt->stmts !== null`: php-parser only leaves the
            // body null for `declare(...);`, so any attached statement/block
            // counts as block mode. In mago a plain `;` parses as a Noop body.
            if has_block_mode_body(stmt) {
                emit_unrecognized_statement(
                    analyzer,
                    analysis_data,
                    "strict_types declaration must not use block mode",
                    stmt.span(),
                );
            }

            analyze_strict_types_declaration(analyzer, declaration, analysis_data);
        } else if declaration_key == "ticks" {
            analyze_ticks_declaration(analyzer, declaration, analysis_data);
        } else if declaration_key == "encoding" {
            analyze_encoding_declaration(analyzer, declaration, analysis_data);
        } else {
            emit_unrecognized_statement(
                analyzer,
                analysis_data,
                &format!("Psalm does not understand the declare statement {declaration_key}"),
                declaration.span(),
            );
        }
    }

    Ok(())
}

/// True when the declare statement carries a body (php-parser's
/// `$stmt->stmts !== null`); `declare(...);` parses to a Noop body in mago.
fn has_block_mode_body(stmt: &Declare<'_>) -> bool {
    match &stmt.body {
        DeclareBody::Statement(body) => !matches!(body, Statement::Noop(_)),
        DeclareBody::ColonDelimited(_) => true,
    }
}

/// Psalm's `DeclareAnalyzer::analyzeStrictTypesDeclaration`. pzoom tracks the
/// strict-types mode itself via `StatementsAnalyzer::file_uses_strict_types`
/// (Psalm sets `$context->strict_types` here).
fn analyze_strict_types_declaration(
    analyzer: &StatementsAnalyzer<'_>,
    declaration: &DeclareItem<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let valid_int_value = matches!(
        &declaration.value,
        Expression::Literal(Literal::Integer(int)) if matches!(int.value, Some(0) | Some(1))
    );

    if !valid_int_value {
        emit_unrecognized_statement(
            analyzer,
            analysis_data,
            "strict_types declaration can only have 1 or 0 as a value",
            declaration.span(),
        );
    }
}

/// Psalm's `DeclareAnalyzer::analyzeTicksDeclaration`.
fn analyze_ticks_declaration(
    analyzer: &StatementsAnalyzer<'_>,
    declaration: &DeclareItem<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !matches!(&declaration.value, Expression::Literal(Literal::Integer(_))) {
        emit_unrecognized_statement(
            analyzer,
            analysis_data,
            "ticks declaration should have integer as a value",
            declaration.span(),
        );
    }
}

/// Psalm's `DeclareAnalyzer::analyzeEncodingDeclaration`.
fn analyze_encoding_declaration(
    analyzer: &StatementsAnalyzer<'_>,
    declaration: &DeclareItem<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !matches!(&declaration.value, Expression::Literal(Literal::String(_))) {
        emit_unrecognized_statement(
            analyzer,
            analysis_data,
            "encoding declaration should have string as a value",
            declaration.span(),
        );
    }
}

fn emit_unrecognized_statement(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    message: &str,
    span: Span,
) {
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::UnrecognizedStatement,
        message,
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
}
