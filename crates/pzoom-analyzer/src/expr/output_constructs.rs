//! Shared analysis for output-producing language constructs.
//!
//! Psalm models constructs that consume data (`echo`, `print`, `exit`,
//! `eval`, `include`, backtick shell-exec) as pseudo function calls whose
//! first parameter is a taint sink, and reports their argument types through
//! `ArgumentAnalyzer::verifyType`; these helpers are that machinery's shared
//! core on the pzoom side.

use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

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
        value_type
            .parent_nodes
            .extend(crate::expr::cast_analyzer::add_to_string_call_dataflow(
                analyzer,
                analysis_data,
                &value_type,
            ));
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
            analyzer
                .interner
                .find(construct_name)
                .unwrap_or(pzoom_str::StrId::EMPTY),
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

/// Report an output construct's argument type the way Psalm's
/// `ArgumentAnalyzer::verifyType` does against the pseudo-param `string $var`:
/// `MixedArgument` for mixed values (with the dataflow origin attached),
/// `InvalidArgument`/`PossiblyInvalidArgument` for non-stringable values,
/// silence for scalar coercions and null.
pub(crate) fn verify_output_argument_type(
    analyzer: &StatementsAnalyzer<'_>,
    t: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    construct_name: &str,
    argument_offset: usize,
) {
    use pzoom_code_info::TAtomic;

    // Psalm: a mixed input (`Union::hasMixed`, any mixed-family member —
    // including from-loop-isset placeholders) reports MixedArgument and
    // skips the containment checks entirely. Where pzoom's union carries a
    // spurious mixed member Psalm's inference avoids (the post-if loop merge
    // unions a concrete branch type with a placeholder where Psalm marks the
    // impossible isset branch failed_reconciliation), the resulting report is
    // a known pzoom divergence — kept anyway for Psalm fidelity of the check
    // itself.
    if t.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset
        )
    }) {
        let (line, col) = analyzer.get_line_column(pos.0);
        let origin_secondary =
            crate::data_flow::mixed_origin_secondary(analyzer, analysis_data, t, pos.0);
        analysis_data.add_issue(
            Issue::new(
                IssueKind::MixedArgument,
                format!(
                    "Argument {} of {} cannot be mixed, expecting string",
                    argument_offset + 1,
                    construct_name
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            )
            .with_secondary_opt(origin_secondary),
        );
        return;
    }

    check_stringable(
        analyzer,
        t,
        pos,
        analysis_data,
        construct_name,
        argument_offset,
    );
}

/// Check if a type can be converted to a string for output.
pub(crate) fn check_stringable(
    analyzer: &StatementsAnalyzer<'_>,
    t: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    construct_name: &str,
    argument_offset: usize,
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
    let (issue_kind, message) = if saw_stringable {
        (
            IssueKind::PossiblyInvalidArgument,
            format!(
                "Argument {} of {} expects string, but possibly different type {} provided",
                argument_offset + 1,
                construct_name,
                type_desc
            ),
        )
    } else {
        (
            IssueKind::InvalidArgument,
            format!(
                "Argument {} of {} expects string, but {} provided",
                argument_offset + 1,
                construct_name,
                type_desc
            ),
        )
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

/// Psalm reports `ForbiddenCode` when a construct name appears in the
/// config's forbiddenFunctions.
pub(crate) fn is_forbidden_construct(analyzer: &StatementsAnalyzer<'_>, name: &str) -> bool {
    analyzer.config.forbidden_functions.iter().any(|forbidden| {
        forbidden
            .strip_prefix('\\')
            .unwrap_or(forbidden.as_str())
            .eq_ignore_ascii_case(name)
    })
}
