//! Echo/print expression analyzer.

use mago_syntax::ast::ast::echo::Echo;

use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

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
    // Psalm: `echo` writes to output, so it is impure from a `@psalm-pure` context.
    emit_impure_output(analyzer, pos, analysis_data, "echo");

    // Analyze all values being echoed
    for value in echo.values.iter() {
        let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        let value_type = analysis_data.expr_types.get(&value_pos).cloned();

        // Check that value is stringable
        if let Some(t) = value_type {
            check_stringable(analyzer, &t, value_pos, analysis_data, "echo");
        }
    }

    // echo doesn't return a value (void)
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::void()));
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
    let value_type = analysis_data.expr_types.get(&value_pos).cloned();

    // Psalm: `print` writes to output, so it is impure from a `@psalm-pure` context.
    emit_impure_output(analyzer, pos, analysis_data, "print");

    // Check that value is stringable
    if let Some(t) = value_type.as_ref() {
        check_stringable(analyzer, t, value_pos, analysis_data, "print");
    }

    // `print` is a taint sink with the same kinds as `echo` (Psalm
    // PrintAnalyzer), wired Hakana-style through argument dataflow.
    if analyzer.config.taint_analysis
        && let Some(value_type) = value_type.as_ref()
    {
        add_output_call_argument_dataflow(
            analyzer,
            "print",
            0,
            value_pos,
            value_type,
            pos,
            analysis_data,
            context,
        );
    }

    // print always returns 1
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(pzoom_code_info::TAtomic::TLiteralInt { value: 1 })));
}

/// Route an `echo`/`print` argument through Hakana's
/// `argument_analyzer::add_dataflow` with a pseudo-function-like whose param
/// is a taint sink. Sink kinds are Psalm's `EchoAnalyzer`/`PrintAnalyzer`
/// set: html, has_quotes, user_secret, system_secret (`exit`/`die` share
/// them via Psalm's `ExitAnalyzer`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_output_call_argument_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    output_construct: &str,
    argument_offset: usize,
    value_pos: Pos,
    value_type: &TUnion,
    call_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    use pzoom_code_info::data_flow::node::SinkType;

    // Psalm's EchoAnalyzer routes echoed objects through __toString
    // (`castStringAttempt`) before the sink, so the method's return taint
    // reaches the output sink.
    let mut value_type = value_type.clone();
    if value_type.has_object() {
        value_type.parent_nodes.extend(
            crate::expr::cast_analyzer::add_to_string_call_dataflow(
                analyzer,
                analysis_data,
                &value_type,
            ),
        );
    }

    add_construct_argument_dataflow(
        analyzer,
        output_construct,
        &[
            SinkType::Html,
            SinkType::HasQuotes,
            SinkType::UserSecret,
            SinkType::SystemSecret,
        ],
        argument_offset,
        value_pos,
        &value_type,
        call_pos,
        analysis_data,
        context,
    );
}

/// Psalm models language constructs that consume data (`echo`, `print`,
/// `exit`, `eval`, `include`, backtick shell-exec) as pseudo function calls
/// whose first parameter is a taint sink (`EchoAnalyzer`, `ExitAnalyzer`,
/// `EvalAnalyzer`, `IncludeAnalyzer`, `ShellExecAnalyzer`...).
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_construct_argument_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    construct_name: &str,
    sink_kinds: &[pzoom_code_info::data_flow::node::SinkType],
    argument_offset: usize,
    value_pos: Pos,
    value_type: &TUnion,
    call_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    use pzoom_code_info::functionlike_info::ParamInfo;

    let construct_param = ParamInfo {
        sinks: sink_kinds.to_vec(),
        ..Default::default()
    };

    crate::expr::call::argument_analyzer::add_dataflow(
        analyzer,
        &pzoom_code_info::FunctionLikeIdentifier::Function(
            analyzer.interner.intern(construct_name),
        ),
        argument_offset,
        value_pos,
        value_type,
        &construct_param,
        true,
        context,
        analysis_data,
        call_pos,
    );
}

/// Emit `ImpureFunctionCall` when output (`echo`/`print`) occurs in a mutation-free
/// context. Psalm gates this on `$context->mutation_free || $context->external_mutation_free`.
pub(crate) fn emit_impure_output(
    analyzer: &StatementsAnalyzer<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    construct: &str,
) {
    if !crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::ImpureFunctionCall,
        format!("Cannot call {} from a mutation-free context", construct),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

/// Check if a type can be converted to a string for output.
pub(crate) fn check_stringable(
    analyzer: &StatementsAnalyzer<'_>,
    t: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context_name: &str,
) {
    use pzoom_code_info::TAtomic;

    let mut saw_stringable = false;
    let mut saw_scalar_coercible = false;
    let mut non_stringable: Option<String> = None;
    for atomic in &t.types {
        if crate::expr::cast_analyzer::atomic_is_stringable(analyzer, atomic) {
            saw_stringable = true;
            // Non-string scalars reach the param via scalar coercion, which
            // Psalm's ArgumentAnalyzer keeps silent for echo/print (the
            // InvalidScalarArgument arm is suppressed for them, and a
            // scalar match downgrades the whole verdict).
            if matches!(
                atomic,
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
                    | TAtomic::TFloat
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TBool
                    | TAtomic::TTrue
                    | TAtomic::TFalse
                    | TAtomic::TNumeric
                    | TAtomic::TNonEmptyScalar
            ) {
                saw_scalar_coercible = true;
            }
        } else if matches!(atomic, TAtomic::TNull) {
            // null coerces to "" at runtime; Psalm reports nothing for echo
            // when the union also has scalar members (PossiblyNullArgument is
            // not echo's concern here).
            saw_scalar_coercible = true;
        } else if non_stringable.is_none() {
            non_stringable = Some(atomic.get_id(Some(analyzer.interner)));
        }
    }

    let Some(type_desc) = non_stringable else {
        return;
    };

    // Psalm's echo verdict: a scalar-coercible member marks the comparison as
    // a scalar match, which echo/print never report on.
    if saw_scalar_coercible {
        return;
    }

    // When only some members of the union are non-stringable the conversion is
    // merely possibly invalid (Psalm's PossiblyInvalidArgument); it is a hard
    // InvalidArgument only when no member can be converted.
    let issue_kind = if saw_stringable {
        IssueKind::PossiblyInvalidArgument
    } else {
        IssueKind::InvalidArgument
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!("{} cannot convert {} to string", context_name, type_desc),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

