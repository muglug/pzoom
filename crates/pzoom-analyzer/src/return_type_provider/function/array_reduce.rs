//! `"array_reduce"` return-type provider.
//!
//! Mirrors Psalm's `ArrayReduceReturnTypeProvider`: it computes the reduced
//! return type (the initial value combined with the callback's return type) and
//! validates the callback's carry/item parameters against the values that flow
//! into them. The stub types the callback as a plain `callable`, so the generic
//! argument validation does not second-guess the signature — this provider owns
//! that check, exactly as Psalm does.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(super) struct ArrayReduceReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayReduceReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_reduce"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_reduce_return_type(event.analyzer, event.arg_positions, analysis_data)
    }
}

fn infer_array_reduce_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    if arg_positions.len() < 2 {
        return None;
    }

    let array_type = analysis_data.expr_types.get(&arg_positions[0]).cloned()?;
    let callback_type = analysis_data.expr_types.get(&arg_positions[1]).cloned()?;

    // The value type of the input array (the second closure parameter must accept it).
    let array_value_type = fca::extract_array_like_info_from_union(&array_type).map(|info| info.value_type);

    // The initial/carry type: the third argument, or `null` when omitted.
    let initial_type = match arg_positions.get(2) {
        Some(initial_pos) => {
            let initial = analysis_data.expr_types.get(&*initial_pos).cloned()?;
            // A mixed initial means we can't say anything useful — bail to mixed.
            if initial.is_mixed() {
                return Some(TUnion::mixed());
            }
            (*initial).clone()
        }
        None => TUnion::null(),
    };

    // Resolve the callback's closure/callable signature, if known.
    let callable_atomic = callback_type.types.iter().find_map(|atomic| match atomic {
        TAtomic::TClosure {
            params,
            return_type,
            ..
        }
        | TAtomic::TCallable {
            params,
            return_type,
            ..
        } => Some((params.as_ref(), return_type.as_ref())),
        _ => None,
    });

    let (params, return_type) = match callable_atomic {
        Some(parts) => parts,
        // No resolvable callable signature: just produce the combined return type.
        None => return Some(initial_type),
    };

    let closure_return_type = return_type
        .map(|rt| (**rt).clone())
        .unwrap_or_else(TUnion::mixed);
    let closure_return_type = if closure_return_type.is_void() {
        TUnion::null()
    } else {
        closure_return_type
    };

    let reduce_return_type = combine_union_types(&closure_return_type, &initial_type, false);

    let issue_pos = arg_positions[1];

    if let Some(params) = params {
        if params.is_empty() {
            emit_invalid_argument(
                analyzer,
                analysis_data,
                issue_pos,
                "The closure passed to array_reduce needs at least one parameter".to_string(),
            );
            return Some(TUnion::mixed());
        }

        // First (carry) parameter must accept both the initial value and the
        // running reduced value.
        if let Some(carry_type) = params.first().map(|p| &p.param_type) {
            let initial_fits = is_contained_by(analyzer, &initial_type, carry_type);
            let reduce_fits =
                reduce_return_type.is_mixed() || is_contained_by(analyzer, &reduce_return_type, carry_type);
            if !initial_fits || !reduce_fits {
                emit_invalid_argument(
                    analyzer,
                    analysis_data,
                    issue_pos,
                    format!(
                        "The first param of the closure passed to array_reduce must take {} but only accepts {}",
                        reduce_return_type.get_id(Some(analyzer.interner)),
                        carry_type.get_id(Some(analyzer.interner)),
                    ),
                );
                return Some(TUnion::mixed());
            }
        }

        // Second (item) parameter must accept the array's value type.
        if let (Some(item_type), Some(array_value_type)) =
            (params.get(1).map(|p| &p.param_type), &array_value_type)
        {
            if !array_value_type.is_mixed()
                && !is_contained_by(analyzer, array_value_type, item_type)
            {
                emit_invalid_argument(
                    analyzer,
                    analysis_data,
                    issue_pos,
                    format!(
                        "The second param of the closure passed to array_reduce must take {} but only accepts {}",
                        array_value_type.get_id(Some(analyzer.interner)),
                        item_type.get_id(Some(analyzer.interner)),
                    ),
                );
                return Some(TUnion::mixed());
            }
        }
    }

    Some(reduce_return_type)
}

fn is_contained_by(analyzer: &StatementsAnalyzer<'_>, input: &TUnion, container: &TUnion) -> bool {
    let mut result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        input,
        container,
        false,
        false,
        &mut result,
    )
}

fn emit_invalid_argument(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
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
