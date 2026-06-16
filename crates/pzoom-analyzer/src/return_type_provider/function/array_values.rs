//! `"array_values"` return-type provider.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator};

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
    let array_type = analysis_data.expr_types.get(&array_pos).cloned()?;
    let array_info = fca::extract_array_like_info_from_union(&array_type)?;

    // Psalm's NamedFunctionCallHandler tests `isContainedBy($arg, Type::getList())`
    // rather than a structural is-list flag, so an empty array (`array<never,never>`,
    // the type of `[]`) — which is contained by `list<mixed>` — is redundant too.
    let list_container = TUnion::new(TAtomic::TList {
        value_type: Box::new(TUnion::mixed()),
    });
    let mut comparison_result = TypeComparisonResult::new();
    let is_contained_by_list = union_type_comparator::is_contained_by(
        analyzer.codebase,
        &array_type,
        &list_container,
        false,
        false,
        &mut comparison_result,
    );

    if is_contained_by_list {
        let (line, col) = analyzer.get_line_column(array_pos.0);
        let type_id = array_type.get_id(Some(analyzer.interner));
        // Psalm's NamedFunctionCallHandler: a redundancy that follows from a
        // docblock-provided list type is the RedundantFunctionCallGivenDocblockType
        // variant, not the plain runtime RedundantFunctionCall.
        let (kind, message) = if array_type.from_docblock {
            (
                IssueKind::RedundantFunctionCallGivenDocblockType,
                format!(
                    "The call to array_values is unnecessary given the list docblock type {type_id}"
                ),
            )
        } else {
            (
                IssueKind::RedundantFunctionCall,
                format!("The call to array_values is unnecessary, {type_id} is already a list"),
            )
        };
        analysis_data.add_issue(Issue::new(
            kind,
            message,
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

    // Psalm has no array_values provider — the type comes from the stub's
    // `@return list<T>` docblock, so downstream redundancies report as the
    // GivenDocblockType kinds.
    let mut result = TUnion::new(atomic);
    result.from_docblock = true;
    Some(result)
}
