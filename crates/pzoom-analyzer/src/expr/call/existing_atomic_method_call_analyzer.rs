//! Existing atomic method call helpers.
//!
//! Mirrors Psalm/Hakana's split where the main method-call analyzer delegates
//! method-template and `if_this_is` handling to a dedicated module.

use mago_syntax::ast::ast::argument::Argument;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::{
    atomic_type_comparator, object_type_comparator, union_type_comparator,
};

pub(crate) fn maybe_emit_if_this_is_mismatch(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    receiver_class_id: StrId,
    receiver_type_params: Option<&[TUnion]>,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    parent_class_id: Option<StrId>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(if_this_is_type) = &method_info.if_this_is_type else {
        return;
    };

    let resolved_if_this_is = if template_defaults.is_empty() && template_replacements.is_empty() {
        if_this_is_type.clone()
    } else {
        function_call_analyzer::replace_templates_in_union(
            if_this_is_type,
            template_replacements,
            template_defaults,
        )
    };

    let expected_receiver_type = super::method_call_analyzer::localize_special_class_type_union(
        &resolved_if_this_is,
        receiver_class_id,
        receiver_class_id,
        parent_class_id,
    );

    let actual_receiver_type = TUnion::new(TAtomic::TNamedObject {
        name: receiver_class_id,
        type_params: receiver_type_params.map(|params| params.to_vec()),
    });

    if receiver_type_satisfies_if_this_is(analyzer, &actual_receiver_type, &expected_receiver_type)
    {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::IfThisIsMismatch,
        format!(
            "Class type must be {}, current type {}",
            expected_receiver_type.get_id(Some(analyzer.interner)),
            actual_receiver_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn receiver_type_satisfies_if_this_is(
    analyzer: &StatementsAnalyzer<'_>,
    actual_receiver_type: &TUnion,
    expected_receiver_type: &TUnion,
) -> bool {
    for actual_atomic in &actual_receiver_type.types {
        let mut matched = false;

        for expected_atomic in &expected_receiver_type.types {
            let expected_is_named_with_type_params = matches!(
                expected_atomic,
                TAtomic::TNamedObject {
                    type_params: Some(_),
                    ..
                }
            );

            if named_object_with_type_params_matches(analyzer, actual_atomic, expected_atomic) {
                matched = true;
                break;
            }

            if expected_is_named_with_type_params {
                continue;
            }

            let mut comparison_result = TypeComparisonResult::new();
            if atomic_type_comparator::is_contained_by(
                analyzer.codebase,
                actual_atomic,
                expected_atomic,
                &mut comparison_result,
            ) {
                matched = true;
                break;
            }
        }

        if !matched {
            return false;
        }
    }

    true
}

fn named_object_with_type_params_matches(
    analyzer: &StatementsAnalyzer<'_>,
    actual_atomic: &TAtomic,
    expected_atomic: &TAtomic,
) -> bool {
    let (
        TAtomic::TNamedObject {
            name: actual_name,
            type_params: actual_params,
        },
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: expected_params,
        },
    ) = (actual_atomic, expected_atomic)
    else {
        return false;
    };

    if !object_type_comparator::is_class_subtype_of(*actual_name, *expected_name, analyzer.codebase)
    {
        return false;
    }

    let Some(expected_params) = expected_params.as_deref() else {
        return true;
    };
    let Some(actual_params) = actual_params.as_deref() else {
        return false;
    };

    if expected_params.len() != actual_params.len() {
        return false;
    }

    for (actual_param, expected_param) in actual_params.iter().zip(expected_params.iter()) {
        let mut comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            actual_param,
            expected_param,
            false,
            false,
            &mut comparison_result,
        ) {
            return false;
        }
    }

    true
}

pub(crate) fn build_method_template_context(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> (FxHashMap<StrId, TUnion>, FxHashMap<StrId, TUnion>) {
    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    template_defaults.extend(function_call_analyzer::get_template_defaults(method_info));

    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    if let Some(if_this_is_type) = &method_info.if_this_is_type {
        let method_template_names: FxHashSet<_> =
            method_info.template_types.iter().map(|t| t.name).collect();

        if !method_template_names.is_empty() {
            let class_template_names: FxHashSet<_> =
                class_info.template_types.iter().map(|t| t.name).collect();

            let class_template_defaults: FxHashMap<_, _> = template_defaults
                .iter()
                .filter_map(|(name, default)| {
                    class_template_names
                        .contains(name)
                        .then_some((*name, default.clone()))
                })
                .collect();
            let class_template_replacements: FxHashMap<_, _> = template_replacements
                .iter()
                .filter_map(|(name, replacement)| {
                    class_template_names
                        .contains(name)
                        .then_some((*name, replacement.clone()))
                })
                .collect();

            let expected_receiver_type =
                if class_template_defaults.is_empty() && class_template_replacements.is_empty() {
                    if_this_is_type.clone()
                } else {
                    function_call_analyzer::replace_templates_in_union(
                        if_this_is_type,
                        &class_template_replacements,
                        &class_template_defaults,
                    )
                };

            let actual_receiver_type = TUnion::new(TAtomic::TNamedObject {
                name: class_info.name,
                type_params: object_type_params.map(|params| params.to_vec()),
            });

            let inferred_if_this_is_replacements = infer_if_this_is_template_replacements(
                analyzer,
                &expected_receiver_type,
                &actual_receiver_type,
                &method_template_names,
            );

            function_call_analyzer::overlay_template_replacements(
                &mut template_replacements,
                inferred_if_this_is_replacements,
            );
        }
    }

    let arg_template_replacements = function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        &method_info.params,
        &template_defaults,
        analysis_data,
        context,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        arg_template_replacements,
    );

    (template_defaults, template_replacements)
}

fn infer_if_this_is_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    expected_receiver_type: &TUnion,
    actual_receiver_type: &TUnion,
    method_template_names: &FxHashSet<StrId>,
) -> FxHashMap<StrId, TUnion> {
    let mut template_replacements = FxHashMap::default();
    infer_if_this_is_union_replacements(
        analyzer,
        expected_receiver_type,
        actual_receiver_type,
        method_template_names,
        &mut template_replacements,
    );
    template_replacements
}

fn infer_if_this_is_union_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    expected_type: &TUnion,
    actual_type: &TUnion,
    method_template_names: &FxHashSet<StrId>,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    for expected_atomic in &expected_type.types {
        for actual_atomic in &actual_type.types {
            infer_if_this_is_atomic_replacements(
                analyzer,
                expected_atomic,
                actual_atomic,
                method_template_names,
                template_replacements,
            );
        }
    }
}

fn infer_if_this_is_atomic_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    expected_atomic: &TAtomic,
    actual_atomic: &TAtomic,
    method_template_names: &FxHashSet<StrId>,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    match expected_atomic {
        TAtomic::TTemplateParam { name, .. } => {
            if !method_template_names.contains(name) {
                return;
            }

            let actual_union = TUnion::new(actual_atomic.clone());
            if let Some(existing) = template_replacements.get(name) {
                template_replacements
                    .insert(*name, combine_union_types(existing, &actual_union, false));
            } else {
                template_replacements.insert(*name, actual_union);
            }
        }
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: Some(expected_type_params),
        } => {
            let TAtomic::TNamedObject {
                name: actual_name,
                type_params: Some(actual_type_params),
            } = actual_atomic
            else {
                return;
            };

            if !object_type_comparator::is_class_subtype_of(
                *actual_name,
                *expected_name,
                analyzer.codebase,
            ) {
                return;
            }

            if expected_type_params.len() != actual_type_params.len() {
                return;
            }

            for (expected_param, actual_param) in
                expected_type_params.iter().zip(actual_type_params.iter())
            {
                infer_if_this_is_union_replacements(
                    analyzer,
                    expected_param,
                    actual_param,
                    method_template_names,
                    template_replacements,
                );
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for expected_intersection_atomic in types {
                infer_if_this_is_atomic_replacements(
                    analyzer,
                    expected_intersection_atomic,
                    actual_atomic,
                    method_template_names,
                    template_replacements,
                );
            }
        }
        _ => {}
    }
}
