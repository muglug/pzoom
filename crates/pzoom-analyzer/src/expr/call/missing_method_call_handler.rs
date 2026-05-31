//! Missing/magic method-call handling (`__call`, pseudo-methods). Mirrors Psalm `MissingMethodCallHandler`.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::{
    Issue, IssueKind, TUnion,
};
use pzoom_str::StrId;

use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;



use super::atomic_method_call_analyzer::*;
use super::method_call_return_type_fetcher::*;
use super::method_call_prohibition_analyzer::*;

pub(crate) fn class_has_magic_call(class_info: &ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::CALL)
}

pub(crate) fn analyze_magic_property_method_call(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
    object_expr: &Expression<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let method_lc = method_name.to_ascii_lowercase();
    let is_this_call =
        expression_identifier::get_expression_var_key(object_expr).as_deref() == Some("$this");

    if method_lc == "__get" {
        let Some(prop_name) = get_literal_string_argument(analysis_data, arg_positions.first())
        else {
            return None;
        };
        let prop_id = analyzer.interner.intern(&prop_name);

        if let Some(pseudo_property_type) = class_info.pseudo_property_get_types.get(&prop_id) {
            return Some(localize_class_union_type(
                class_info,
                object_type_params,
                pseudo_property_type,
            ));
        }

        if class_has_sealed_properties(class_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            let issue_kind = if is_this_call {
                IssueKind::UndefinedThisPropertyFetch
            } else {
                IssueKind::UndefinedMagicPropertyFetch
            };
            let class_name = analyzer.interner.lookup(class_id);
            let message = if is_this_call {
                format!("Property {}::${} does not exist", class_name, prop_name)
            } else {
                format!(
                    "Magic property {}::${} does not exist",
                    class_name, prop_name
                )
            };
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

        return None;
    }

    if method_lc == "__set" {
        let Some(prop_name) = get_literal_string_argument(analysis_data, arg_positions.first())
        else {
            return None;
        };
        let prop_id = analyzer.interner.intern(&prop_name);

        if let Some(pseudo_property_type) = class_info.pseudo_property_set_types.get(&prop_id) {
            if let Some(second_arg_pos) = arg_positions.get(1) {
                if let Some(value_type) = analysis_data.get_expr_type(*second_arg_pos) {
                    // A `mixed` value is universally compatible (Psalm reports it via
                    // MixedAssignment, not a property-value mismatch), so skip the
                    // pseudo-property type check rather than flagging PossiblyInvalid.
                    if value_type.is_mixed() {
                        return None;
                    }
                    let pseudo_property_type = localize_class_union_type(
                        class_info,
                        object_type_params,
                        pseudo_property_type,
                    );
                    let mut comparison_result = TypeComparisonResult::new();
                    let is_contained = union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &value_type,
                        &pseudo_property_type,
                        false,
                        false,
                        &mut comparison_result,
                    );

                    if !is_contained {
                        let can_be_contained = union_type_comparator::can_be_contained_by(
                            analyzer.codebase,
                            &value_type,
                            &pseudo_property_type,
                        );
                        let issue_kind = if can_be_contained {
                            IssueKind::PossiblyInvalidPropertyAssignmentValue
                        } else {
                            IssueKind::InvalidPropertyAssignmentValue
                        };
                        let class_name = analyzer.interner.lookup(class_id);
                        let message = if can_be_contained {
                            format!(
                                "Property {}::${} expects {}, possibly different type {} provided",
                                class_name,
                                prop_name,
                                pseudo_property_type.get_id(Some(analyzer.interner)),
                                value_type.get_id(Some(analyzer.interner))
                            )
                        } else {
                            format!(
                                "Property {}::${} expects {}, got {}",
                                class_name,
                                prop_name,
                                pseudo_property_type.get_id(Some(analyzer.interner)),
                                value_type.get_id(Some(analyzer.interner))
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
                }
            }

            return None;
        }

        if class_has_sealed_properties(class_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            let issue_kind = if is_this_call {
                IssueKind::UndefinedThisPropertyAssignment
            } else {
                IssueKind::UndefinedMagicPropertyAssignment
            };
            let class_name = analyzer.interner.lookup(class_id);
            let message = if is_this_call {
                format!("Property {}::${} does not exist", class_name, prop_name)
            } else {
                format!(
                    "Magic property {}::${} does not exist",
                    class_name, prop_name
                )
            };
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

        let _ = args;
        return None;
    }

    None
}

pub(crate) fn get_pseudo_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.pseudo_methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .pseudo_methods
        .iter()
        .find_map(|(stored_id, method_info)| {
            analyzer
                .interner
                .lookup(*stored_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
                .then_some(method_info)
        })
}

pub(crate) fn get_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .methods
        .iter()
        .find_map(|(stored_id, method_info)| {
            analyzer
                .interner
                .lookup(*stored_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
                .then_some(method_info)
        })
}
