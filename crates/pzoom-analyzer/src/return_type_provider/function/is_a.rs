//! `is_a()` return-type provider.
//!
//! Mirrors Psalm: emits RedundantFunctionCall when the first argument is a string
//! and the third argument is `false`/absent (so the call always returns false). The
//! return type is left to the stub (`bool`).

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) struct IsAReturnTypeProvider;

impl FunctionReturnTypeProvider for IsAReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["is_a"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        maybe_emit_is_a_redundant_call(
            event.analyzer,
            event.arg_positions,
            analysis_data,
            event.context,
        );
        None
    }
}

fn union_is_string_type(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TNonEmptyString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TClassString { .. }
                    | TAtomic::TLiteralClassString { .. }
            )
        })
}

/// Emit `RedundantFunctionCall` for `is_a($string, ..., false)`: when the first
/// argument is a string and the third argument is `false` (or omitted, which
/// defaults to `false`), `is_a` always returns false. Mirrors Psalm's
/// NamedFunctionCallHandler behaviour.
fn maybe_emit_is_a_redundant_call(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    // The assertion reconciler already emits relevant (but different) issues when
    // `is_a` is used inside a condition.
    if context.inside_conditional {
        return;
    }

    let Some(first_pos) = arg_positions.first().copied() else {
        return;
    };

    let Some(first_arg_type) = analysis_data.expr_types.get(&first_pos).cloned() else {
        return;
    };

    if !union_is_string_type(&first_arg_type) {
        return;
    }

    // Third argument (`$allow_string`) defaults to false. The call is only
    // redundant when it is exactly `false`.
    let third_is_false = match arg_positions.get(2).copied() {
        Some(third_pos) => analysis_data
            .expr_types.get(&third_pos).cloned()
            .is_some_and(|third| matches!(third.get_single(), Some(TAtomic::TFalse))),
        None => true,
    };

    if !third_is_false {
        return;
    }

    let (line, col) = analyzer.get_line_column(first_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::RedundantFunctionCall,
        "Call to is_a always return false when first argument is string \
         unless third argument is true",
        analyzer.file_path,
        first_pos.0,
        first_pos.1,
        line,
        col,
    ));
}
