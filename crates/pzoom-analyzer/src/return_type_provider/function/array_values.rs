//! `"array_values"` return-type provider.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::expr::call::function_call_analyzer as fca;
pub(super) struct ArrayValuesReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayValuesReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_values"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_values_return_type(event.analyzer, event.arg_positions, analysis_data)
    }
}

fn infer_array_values_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let array_info = fca::extract_array_like_info_from_union(&array_type)?;

    if array_info.is_list {
        let (line, col) = analyzer.get_line_column(array_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RedundantFunctionCall,
            "array_values called on a list is redundant",
            analyzer.file_path,
            array_pos.0,
            array_pos.1,
            line,
            col,
        ));
    }

    let atomic = if array_info.is_non_empty {
        TAtomic::TNonEmptyList {
            value_type: Box::new(array_info.value_type),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(array_info.value_type),
        }
    };

    Some(TUnion::new(atomic))
}
