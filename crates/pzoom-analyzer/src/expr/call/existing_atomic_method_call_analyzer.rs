//! Existing atomic method call helpers.
//!
//! Mirrors Psalm/Hakana's split where the main method-call analyzer delegates
//! method-template and `if_this_is` handling to a dedicated module.

use mago_syntax::ast::ast::argument::Argument;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::template::TemplateMap;
use crate::type_comparator::{
    atomic_type_comparator, object_type_comparator, union_type_comparator,
};

pub(crate) fn maybe_emit_if_this_is_mismatch(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    receiver_class_id: StrId,
    receiver_type_params: Option<&[TUnion]>,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
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

    let expected_receiver_type = crate::type_expander::localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
        &resolved_if_this_is,
        receiver_class_id,
        receiver_class_id,
        parent_class_id,
    );

    let actual_receiver_type = TUnion::new(TAtomic::TNamedObject {
        name: receiver_class_id,
        type_params: receiver_type_params.map(|params| params.to_vec()),
    is_static: false, remapped_params: false });

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
        .. },
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: expected_params,
        .. },
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
    self_call: bool,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> (TemplateMap, TemplateMap) {
    // For an inherited method the templates in its signature belong to the
    // class that declares it — Psalm's
    // `$codebase->methods->getClassLikeStorageForMethod($method_id)`.
    // `class_info` stays the static/receiver class (for mixins the mixin class
    // itself, mirroring Psalm's rewritten `$lhs_type_part`).
    let declaring_class_info = analyzer
        .codebase
        .get_classlike_storage_for_method(class_info.name, method_info.name)
        .unwrap_or(class_info);

    let mut template_defaults =
        function_call_analyzer::get_class_template_defaults(declaring_class_info);
    template_defaults.extend_overlay(function_call_analyzer::get_template_defaults(method_info));

    // Class-level template replacements (extended params + receiver type params),
    // via the class template-param collector (Psalm/Hakana
    // ClassTemplateParamCollector::collect). Like Psalm, the declaring class of
    // the method and the receiver's class are passed separately so templates
    // resolve through the receiver's `template_extended_params`.
    let lhs_type_part = TAtomic::TNamedObject {
        name: class_info.name,
        type_params: object_type_params.map(|params| params.to_vec()),
        is_static: false,
        remapped_params: false,
    };
    let mut template_replacements = super::class_template_param_collector::collect(
        analyzer.codebase,
        declaring_class_info,
        class_info,
        Some(&lhs_type_part),
        self_call,
    )
    .unwrap_or_default();

    if let Some(if_this_is_type) = &method_info.if_this_is_type {
        let method_template_names: FxHashSet<_> =
            method_info.template_types.iter().map(|t| t.name).collect();

        if !method_template_names.is_empty() {
            let class_template_names: FxHashSet<_> =
                class_info.template_types.iter().map(|t| t.name).collect();

            let class_template_defaults =
                template_defaults.filter_names(|name| class_template_names.contains(&name));
            let class_template_replacements =
                template_replacements.filter_names(|name| class_template_names.contains(&name));

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
            is_static: false, remapped_params: false });

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
    // A class template parameter used by a method is fixed by the receiver's type
    // arguments (e.g. calling `create()` on `FileManager<ImageFile>` binds `T` to
    // `ImageFile`); argument-based inference must not override such a binding, so
    // it only fills templates the receiver left unbound. This matches Psalm, where
    // an argument that contradicts the receiver's binding is an InvalidArgument
    // rather than a re-inference of the template.
    for (name, entity, replacement) in arg_template_replacements.iter() {
        match template_replacements.get(name, entity) {
            // A concrete receiver binding wins; a degenerate `never` binding
            // (e.g. from an empty-array generic) is refined by the argument.
            Some(existing) if !existing.is_nothing() => {}
            _ => {
                template_replacements.insert(name, entity, replacement.clone());
            }
        }
    }

    (template_defaults, template_replacements)
}

fn infer_if_this_is_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    expected_receiver_type: &TUnion,
    actual_receiver_type: &TUnion,
    method_template_names: &FxHashSet<StrId>,
) -> TemplateMap {
    let mut template_replacements = TemplateMap::new();
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
    template_replacements: &mut TemplateMap,
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
    template_replacements: &mut TemplateMap,
) {
    match expected_atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        } => {
            if !method_template_names.contains(name) {
                return;
            }

            let actual_union = TUnion::new(actual_atomic.clone());
            template_replacements.insert_combined(*name, *defining_entity, actual_union);
        }
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: Some(expected_type_params),
        .. } => {
            let TAtomic::TNamedObject {
                name: actual_name,
                type_params: Some(actual_type_params),
            .. } = actual_atomic
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
