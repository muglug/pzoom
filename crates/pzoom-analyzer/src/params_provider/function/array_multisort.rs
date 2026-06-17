//! Port of Psalm's `ArrayMultisortParamsProvider`.
//!
//! The parameter list of `array_multisort` is dynamic — any number of arrays
//! (passed by reference) may be interleaved with integer sort-order /
//! sort-flag arguments. The generic signature
//! `array_multisort(&$array, $sort_order, $sort_flags, &...$rest)` cannot
//! express the placement rules, so the provider walks the call and emits
//! `InvalidArgument` when:
//!   * a sort-order/flag argument contains a value that is neither a sort order
//!     nor a sort flag;
//!   * sort-order flags appear before any array argument;
//!   * sort flags appear after a parameter that already had sort flags;
//!   * no array argument is a variable (sorting happens by reference, so the
//!     call would otherwise do nothing);
//!   * arguments after the last by-reference array (and its flags) are redundant.
//!
//! Because the by-reference-ness is decided here (only variable array arguments
//! are by-ref), the provider asks for the generic parameter validation to be
//! skipped — the Psalm equivalent returns `null` params on bail-out, which
//! disables downstream argument checking the same way.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::{FunctionParamsProvider, FunctionParamsProviderEvent, FunctionParamsProviderResult};

pub(super) struct ArrayMultisortParamsProvider;

impl FunctionParamsProvider for ArrayMultisortParamsProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_multisort"]
    }

    fn get_function_params(
        &self,
        event: &FunctionParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<FunctionParamsProviderResult> {
        check_placement_rules(event, analysis_data);
        // The by-reference decisions made here cannot be expressed through the
        // stub signature; skip the generic parameter validation like Psalm
        // does when its provider bails out.
        Some(FunctionParamsProviderResult::SkipValidation)
    }
}

// SORT_* constant values (from the standard extension stub). Kept in sync with
// Psalm's hard-coded sets in ArrayMultisortParamsProvider.
const SORT_ASC: i64 = 4;
const SORT_DESC: i64 = 3;
const SORT_REGULAR: i64 = 0;
const SORT_NUMERIC: i64 = 1;
const SORT_STRING: i64 = 2;
const SORT_LOCALE_STRING: i64 = 5;
const SORT_NATURAL: i64 = 6;
const SORT_FLAG_CASE: i64 = 8;

const SORT_ORDER: [i64; 2] = [SORT_ASC, SORT_DESC];
const SORT_FLAGS: [i64; 7] = [
    SORT_REGULAR,
    SORT_NUMERIC,
    SORT_STRING,
    SORT_LOCALE_STRING,
    SORT_NATURAL,
    SORT_STRING | SORT_FLAG_CASE,
    SORT_NATURAL | SORT_FLAG_CASE,
];

#[derive(Clone, Copy, PartialEq)]
enum PreviousParam {
    None,
    Array,
    SortOrder,
    SortFlags,
}

/// Apply the Psalm placement rules to an `array_multisort` call whose
/// arguments have already been expression-analyzed by the generic flow.
fn check_placement_rules(
    event: &FunctionParamsProviderEvent<'_, '_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let analyzer = event.analyzer;
    let args = event.args;
    let arg_positions = event.arg_positions;

    if args.is_empty() {
        return;
    }

    let mut previous_param = PreviousParam::None;
    let mut last_by_ref_index: i64 = -1;
    let mut first_non_ref_index_after_by_ref: i64 = -1;

    for (key, arg) in args.iter().enumerate() {
        let key_i = key as i64;
        let value_expr = arg.value().unparenthesized();
        let arg_pos = arg_positions.get(key).copied().unwrap_or((0, 0));
        let arg_type = analysis_data.expr_types.get(&arg_pos).cloned();

        // Psalm: function/method calls are assumed to produce (non-ref) arrays;
        // their concrete return type is intentionally not inspected.
        if matches!(value_expr, Expression::Call(_)) {
            if first_non_ref_index_after_by_ref < last_by_ref_index {
                first_non_ref_index_after_by_ref = key_i;
            }
            previous_param = PreviousParam::Array;
            continue;
        }

        let Some(arg_type) = arg_type else {
            // Type unknown — Psalm bails out of the whole provider (returns null).
            return;
        };

        if key == 0 && !union_is_array(&arg_type) {
            return;
        }

        let is_variable_array = union_is_array(&arg_type) && is_writable_variable(value_expr);
        if is_variable_array {
            last_by_ref_index = key_i;
            previous_param = PreviousParam::Array;
            continue;
        }

        if let Some(literal_ints) = all_literal_ints(&arg_type) {
            match classify_sort_argument(
                analyzer,
                &literal_ints,
                key,
                previous_param,
                arg_pos,
                analysis_data,
            ) {
                Some(next) => previous_param = next,
                None => return,
            }
            continue;
        }

        if !union_is_array(&arg_type) {
            // Too complex for now (Psalm bails out).
            return;
        }

        // A non-variable array (e.g. a literal `[...]`): a by-value array param.
        if first_non_ref_index_after_by_ref < last_by_ref_index {
            first_non_ref_index_after_by_ref = key_i;
        }
        previous_param = PreviousParam::Array;
    }

    emit_by_ref_summary_issues(
        analyzer,
        last_by_ref_index,
        first_non_ref_index_after_by_ref,
        arg_positions,
        analysis_data,
    );
}

/// Classify a sort-order/flag integer argument, emitting `InvalidArgument` for
/// invalid values or placement. Returns the new `previous_param`, or `None` when
/// the call should bail out (Psalm `return null`).
fn classify_sort_argument(
    analyzer: &StatementsAnalyzer<'_>,
    literal_ints: &[i64],
    key: usize,
    previous_param: PreviousParam,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<PreviousParam> {
    // `sort_param` tracks what this argument is so far (Psalm's local state).
    #[derive(Clone, Copy, PartialEq)]
    enum SortParam {
        None,
        SortOrder,
        SortFlags,
        SortOrderFlags,
    }

    let mut sort_param = SortParam::None;

    for &value in literal_ints {
        if SORT_ORDER.contains(&value) {
            sort_param = match sort_param {
                SortParam::SortOrderFlags => SortParam::SortOrderFlags,
                SortParam::SortOrder => SortParam::SortOrder,
                SortParam::SortFlags => SortParam::SortOrderFlags,
                SortParam::None => SortParam::SortOrder,
            };
            continue;
        }

        if SORT_FLAGS.contains(&value) {
            sort_param = match sort_param {
                SortParam::SortOrderFlags => SortParam::SortOrderFlags,
                SortParam::SortFlags => SortParam::SortFlags,
                SortParam::SortOrder => SortParam::SortOrderFlags,
                SortParam::None => SortParam::SortFlags,
            };
            continue;
        }

        emit_invalid_argument(
            analyzer,
            arg_pos,
            analysis_data,
            format!(
                "Argument {} of array_multisort sort order/flag contains an invalid value of {}",
                key + 1,
                value
            ),
        );
    }

    if sort_param == SortParam::None {
        return None;
    }

    if matches!(sort_param, SortParam::SortOrder | SortParam::SortOrderFlags)
        && previous_param != PreviousParam::Array
    {
        emit_invalid_argument(
            analyzer,
            arg_pos,
            analysis_data,
            format!(
                "Argument {} of array_multisort contains sort order flags \
                 and can only be used after an array parameter",
                key + 1
            ),
        );
        return None;
    }

    if sort_param == SortParam::SortFlags
        && previous_param != PreviousParam::Array
        && previous_param != PreviousParam::SortOrder
    {
        emit_invalid_argument(
            analyzer,
            arg_pos,
            analysis_data,
            format!(
                "Argument {} of array_multisort are sort flags \
                 and cannot be used after a parameter with sort flags",
                key + 1
            ),
        );
        return None;
    }

    Some(match sort_param {
        SortParam::SortOrderFlags | SortParam::SortOrder => PreviousParam::SortOrder,
        SortParam::SortFlags => PreviousParam::SortFlags,
        SortParam::None => PreviousParam::None,
    })
}

fn emit_by_ref_summary_issues(
    analyzer: &StatementsAnalyzer<'_>,
    last_by_ref_index: i64,
    first_non_ref_index_after_by_ref: i64,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    let call_pos = arg_positions.first().copied().unwrap_or((0, 0));

    if last_by_ref_index == -1 {
        emit_invalid_argument(
            analyzer,
            call_pos,
            analysis_data,
            "At least 1 array argument of array_multisort must be a variable, \
             since the sorting happens by reference and otherwise this function call does nothing"
                .to_string(),
        );
    } else if first_non_ref_index_after_by_ref > last_by_ref_index {
        emit_invalid_argument(
            analyzer,
            call_pos,
            analysis_data,
            format!(
                "All arguments of array_multisort after argument {}, \
                 which are after the last by reference passed array argument and its flags, \
                 are redundant and can be removed, since the sorting happens by reference",
                first_non_ref_index_after_by_ref
            ),
        );
    }
}

fn emit_invalid_argument(
    analyzer: &StatementsAnalyzer<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidArgument,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

/// A directly-assignable l-value: a variable, property/array access. Function
/// call results are handled separately (they are never by-reference here).
fn is_writable_variable(expr: &Expression<'_>) -> bool {
    matches!(
        expr,
        Expression::Variable(_) | Expression::ArrayAccess(_) | Expression::Access(_)
    )
}

fn union_is_array(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(|atomic| atomic.is_array())
}

/// When every atomic in the union is a literal int, return those values.
/// Otherwise `None` (the argument is not purely an integer literal set).
fn all_literal_ints(union: &TUnion) -> Option<Vec<i64>> {
    if union.types.is_empty() {
        return None;
    }
    let mut values = Vec::with_capacity(union.types.len());
    for atomic in &union.types {
        match atomic {
            TAtomic::TLiteralInt { value } => values.push(*value),
            _ => return None,
        }
    }
    Some(values)
}
