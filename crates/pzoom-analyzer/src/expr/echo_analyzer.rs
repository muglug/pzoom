//! Echo/print expression analyzer.

use mago_syntax::ast::ast::echo::Echo;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an echo statement/expression.
///
/// echo outputs one or more expressions and returns void.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    echo: &Echo<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze all values being echoed
    for value in echo.values.iter() {
        let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        let value_type = analysis_data.get_expr_type(value_pos);

        // Check that value is stringable
        if let Some(t) = value_type {
            check_stringable(analyzer, &t, value_pos, analysis_data, "echo");
        }
    }

    // echo doesn't return a value (void)
    analysis_data.set_expr_type(pos, TUnion::void());
}

/// Analyze a print expression.
///
/// print outputs a single expression and returns 1.
pub fn analyze_print(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &mago_syntax::ast::ast::expression::Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the value being printed
    let value_pos = expression_analyzer::analyze(analyzer, expr, analysis_data, context);
    let value_type = analysis_data.get_expr_type(value_pos);

    // Check that value is stringable
    if let Some(t) = value_type {
        check_stringable(analyzer, &t, value_pos, analysis_data, "print");
    }

    // print always returns 1
    analysis_data.set_expr_type(
        pos,
        TUnion::new(pzoom_code_info::TAtomic::TLiteralInt { value: 1 }),
    );
}

/// Check if a type can be converted to a string for output.
pub(crate) fn check_stringable(
    analyzer: &StatementsAnalyzer<'_>,
    t: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context_name: &str,
) {
    for atomic in &t.types {
        if !is_stringable(analyzer, atomic) {
            let type_desc = atomic.get_id(Some(analyzer.interner));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                format!("{} cannot convert {} to string", context_name, type_desc),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }
}

/// Check if an atomic type can be implicitly converted to a string.
fn is_stringable(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TString
        | TAtomic::TLiteralString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TTruthyString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TFloat
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TBool
        | TAtomic::TTrue
        | TAtomic::TFalse
        | TAtomic::TNull
        | TAtomic::TNothing
        | TAtomic::TMixed
        | TAtomic::TNonEmptyMixed
        | TAtomic::TNumeric
        | TAtomic::TScalar
        | TAtomic::TArrayKey => true,
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .is_some_and(|class_info| class_info.methods.contains_key(&StrId::TO_STRING)),
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(|nested| is_stringable(analyzer, nested)),
        TAtomic::TTemplateParamClass { as_type, .. } => is_stringable(analyzer, as_type),
        _ => false,
    }
}
