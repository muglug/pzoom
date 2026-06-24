//! Empty expression analyzer.

use mago_syntax::cst::cst::construct::EmptyConstruct;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr::isset_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze an empty() expression.
///
/// empty() returns true if the variable doesn't exist or is falsy.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    empty: &EmptyConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Psalm's EmptyAnalyzer routes through IssetAnalyzer::analyzeIssetVar,
    // which sets inside_isset for the whole inner expression — `empty($x)` on
    // an undefined variable reports like isset() does.
    let value_pos =
        isset_analyzer::analyze_isset_var(analyzer, empty.value, analysis_data, context);

    // Psalm: a config listing `empty` in forbiddenFunctions reports
    // ForbiddenCode for the construct.
    if analyzer.config.forbidden_functions.iter().any(|forbidden| {
        forbidden
            .strip_prefix('\\')
            .unwrap_or(forbidden.as_str())
            .eq_ignore_ascii_case("empty")
    }) {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ForbiddenCode,
            "You have forbidden the use of empty",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Psalm's EmptyAnalyzer: `empty()` on a single non-docblock boolean is
    // almost certainly unintended.
    if let Some(value_type) = analysis_data.expr_types.get(&value_pos).cloned() {
        if value_type.is_single()
            && !value_type.from_docblock
            && value_type
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
        {
            let (line, col) = analyzer.get_line_column(value_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                "Calling empty on a boolean value is almost certainly unintended",
                analyzer.file_path,
                value_pos.0,
                value_pos.1,
                line,
                col,
            ));
        }
    }

    // Psalm's EmptyAnalyzer result typing: empty(always-truthy) is `false`
    // unless the operand is possibly undefined, empty(always-falsy) is `true`
    // (docblock provenance preserved so the surrounding condition reports the
    // docblock-flavoured redundancy), anything else is `bool`.
    let result_type = match analysis_data.expr_types.get(&value_pos).cloned() {
        Some(value_type) => {
            if value_type.is_always_truthy() && !value_type.possibly_undefined_from_try {
                let mut result = TUnion::new(TAtomic::TFalse);
                result.from_docblock = value_type.from_docblock;
                result
            } else if value_type.is_always_falsy() {
                let mut result = TUnion::new(TAtomic::TTrue);
                result.from_docblock = value_type.from_docblock;
                result
            } else {
                TUnion::bool()
            }
        }
        None => TUnion::bool(),
    };
    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}
