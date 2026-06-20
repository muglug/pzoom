//! Existing atomic method call helpers.
//!
//! Mirrors Psalm/Hakana's split where the main method-call analyzer delegates
//! method-template and `if_this_is` handling to a dedicated module.

use mago_syntax::ast::ast::argument::Argument;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use super::atomic_method_call_analyzer::*;
use super::method_call_analyzer::*;
use super::method_call_return_type_fetcher::*;
use super::method_visibility_analyzer::*;
use super::missing_method_call_handler::*;
use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::{
    atomic_type_comparator, object_type_comparator, union_type_comparator,
};
use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use pzoom_code_info::TemplateResult;
use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::Visibility;

pub(crate) fn maybe_emit_if_this_is_mismatch(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    receiver_class_id: StrId,
    receiver_type_params: Option<&[TUnion]>,
    template_result: &TemplateResult,
    parent_class_id: Option<StrId>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(if_this_is_type) = &method_info.if_this_is_type else {
        return;
    };

    let resolved_if_this_is = if crate::template::template_result_is_empty(template_result) {
        if_this_is_type.clone()
    } else {
        function_call_analyzer::replace_templates_in_union(if_this_is_type, template_result)
    };

    let expected_receiver_type = crate::type_expander::localize_special_class_type_union(
        analyzer.codebase,
        analyzer.interner,
        &resolved_if_this_is,
        receiver_class_id,
        receiver_class_id,
        parent_class_id,
    );

    // Type-variable receiver params resolve through their lower bounds for
    // this check: `@psalm-if-this-is a<int>` on a receiver `a<`_0 >: string>`
    // must compare the concrete binding, not record yet another bound.
    let actual_receiver_type = TUnion::new(TAtomic::TNamedObject {
        name: receiver_class_id,
        type_params: receiver_type_params.map(|params| {
            params
                .iter()
                .map(|param| {
                    crate::template::resolve_type_variables_in_union(
                        param,
                        &analysis_data.type_variable_bounds,
                    )
                })
                .collect()
        }),
        is_static: false,
        remapped_params: false,
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
            ..
        },
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: expected_params,
            ..
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
    self_call: bool,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> TemplateResult {
    // For an inherited method the templates in its signature belong to the
    // class that declares it — Psalm's
    // `$codebase->methods->getClassLikeStorageForMethod($method_id)`.
    // `class_info` stays the static/receiver class (for mixins the mixin class
    // itself, mirroring Psalm's rewritten `$lhs_type_part`).
    let declaring_class_info = analyzer
        .codebase
        .get_classlike_storage_for_method(class_info.name, method_info.name)
        .unwrap_or(class_info);

    let mut template_result =
        function_call_analyzer::get_class_template_defaults(declaring_class_info);
    for template_type in &method_info.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

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
    if let Some(collected) = super::class_template_param_collector::collect(
        analyzer.codebase,
        declaring_class_info,
        class_info,
        Some(&lhs_type_part),
        self_call,
    ) {
        template_result.lower_bounds = collected;
    }

    if let Some(if_this_is_type) = &method_info.if_this_is_type {
        let method_template_names: FxHashSet<_> =
            method_info.template_types.iter().map(|t| t.name).collect();

        if !method_template_names.is_empty() {
            let class_template_names: FxHashSet<_> =
                class_info.template_types.iter().map(|t| t.name).collect();

            let class_template_result = TemplateResult {
                template_types: template_result
                    .template_types
                    .iter()
                    .filter(|(name, _)| class_template_names.contains(*name))
                    .map(|(name, entries)| (*name, entries.clone()))
                    .collect(),
                lower_bounds: template_result
                    .lower_bounds
                    .iter()
                    .filter(|(name, _)| class_template_names.contains(*name))
                    .map(|(name, entities)| (*name, entities.clone()))
                    .collect(),
                ..Default::default()
            };

            let expected_receiver_type =
                if crate::template::template_result_is_empty(&class_template_result) {
                    if_this_is_type.clone()
                } else {
                    function_call_analyzer::replace_templates_in_union(
                        if_this_is_type,
                        &class_template_result,
                    )
                };

            let actual_receiver_type = TUnion::new(TAtomic::TNamedObject {
                name: class_info.name,
                type_params: object_type_params.map(|params| params.to_vec()),
                is_static: false,
                remapped_params: false,
            });

            let inferred_if_this_is_replacements = infer_if_this_is_template_replacements(
                analyzer,
                &expected_receiver_type,
                &actual_receiver_type,
                &method_template_names,
            );

            function_call_analyzer::overlay_template_replacements(
                &mut template_result,
                inferred_if_this_is_replacements,
            );
        }
    }

    let mut arg_template_result = TemplateResult {
        template_types: template_result.template_types.clone(),
        ..Default::default()
    };
    function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        &method_info.params,
        &mut arg_template_result,
        analysis_data,
        context,
    );
    // A class template parameter used by a method is fixed by the receiver's type
    // arguments (e.g. calling `create()` on `FileManager<ImageFile>` binds `T` to
    // `ImageFile`); argument-based inference must not override such a binding, so
    // it only fills templates the receiver left unbound. This matches Psalm, where
    // an argument that contradicts the receiver's binding is an InvalidArgument
    // rather than a re-inference of the template.
    for (name, entity, replacement) in
        crate::template::lower_bounds_iter(&arg_template_result).collect::<Vec<_>>()
    {
        match crate::template::lower_bounds_get(&template_result, name, entity) {
            // A concrete receiver binding wins; a degenerate `never` binding
            // (e.g. from an empty-array generic) is refined by the argument.
            Some(existing) if !existing.is_nothing() => {}
            _ => {
                crate::template::lower_bounds_insert(
                    &mut template_result,
                    name,
                    entity,
                    replacement,
                );
            }
        }
    }

    template_result
}

fn infer_if_this_is_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    expected_receiver_type: &TUnion,
    actual_receiver_type: &TUnion,
    method_template_names: &FxHashSet<StrId>,
) -> TemplateResult {
    let mut template_replacements = TemplateResult::default();
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
    template_replacements: &mut TemplateResult,
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
    template_replacements: &mut TemplateResult,
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
            crate::template::lower_bounds_insert_combined(
                template_replacements,
                *name,
                *defining_entity,
                actual_union,
            );
        }
        TAtomic::TNamedObject {
            name: expected_name,
            type_params: Some(expected_type_params),
            ..
        } => {
            let TAtomic::TNamedObject {
                name: actual_name,
                type_params: Some(actual_type_params),
                ..
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    obj_type: &TUnion,
    expanded_obj_type: &TUnion,
    method_name: &str,
    pos: Pos,
    method_name_pos: Pos,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    enforce_mutation_free: bool,
    suppress_possibly_null_reference_issue: bool,
    is_this_call: bool,
    calling_class: Option<StrId>,
    any_candidate_accepts_arg_count: bool,
    has_null_receiver: bool,
    has_false_receiver: bool,
    has_invalid_receiver: bool,
    first_missing_interface: Option<StrId>,
    receiver_atomics: &[TAtomic],
    receiver_class_id: StrId,
    class_id: StrId,
    object_type_params: Option<Vec<TUnion>>,
    method_info: pzoom_code_info::FunctionLikeInfo,
    is_primary: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    // Psalm's Methods::getMethodParams resolves documenting-ancestor
    // docblock params before argument analysis, so templates the
    // documenting method declares bind from args during standin
    // replacement. Apply the same inheritance up front.
    let method_info = {
        let mut method_info = method_info;
        if let Some(inherited_params) =
            apply_inherited_method_param_types(analyzer, class_id, method_name, &method_info)
        {
            method_info.params = inherited_params;

            // The documenting ancestor's method-level templates come with
            // its param types: surfacing them lets the args bind them
            // during standin replacement (Psalm analyzes the call against
            // the declaring method's storage, templates included).
            if method_info.template_types.is_empty()
                && let Some(inherited_templates) =
                    inherited_method_template_types(analyzer, class_id, method_name)
            {
                method_info.template_types = inherited_templates;
            }
        }
        method_info
    };
    // While re-analysing a constructor to collect property initialisations,
    // a `$this->method()` call is followed in place so the method's
    // `$this->prop` writes land flow-sensitively (Psalm's
    // `CallAnalyzer::collectSpecialInformation`, instance branch).
    if context.collect_initializations && is_this_call {
        crate::init_collector::follow_instance_init_call(analyzer, context, class_id, &method_info);
    }
    let class_name = analyzer.interner.lookup(class_id);
    let parent_class_id = analyzer
        .codebase
        .get_class(class_id)
        .and_then(|class_info| class_info.parent_class);

    if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        if class_info.is_deprecated
            && analyzer
                .get_declaring_class()
                .is_none_or(|declaring_class| declaring_class != class_id)
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::DeprecatedClass,
                format!("{} is marked deprecated", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if !crate::internal_access::can_access_internal(analyzer, &class_info.internal, Some(context)) {
            let scope_phrase = crate::internal_access::format_internal_scope_phrase(analyzer, &class_info.internal);
            let caller_phrase = crate::internal_access::format_caller_context(analyzer, Some(context));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InternalMethod,
                format!(
                    "The method {}::{} is internal to {} but called from {}",
                    class_name, method_name, scope_phrase, caller_phrase
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    let template_result = if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        analyze_pending_closure_args_for_method(
            analyzer,
            args,
            arg_positions,
            &method_info,
            class_info,
            object_type_params.as_deref(),
            class_id,
            receiver_class_id,
            parent_class_id,
            analysis_data,
            context,
        );

        build_method_template_context(
            analyzer,
            class_info,
            object_type_params.as_deref(),
            &method_info,
            is_this_call,
            args,
            arg_positions,
            analysis_data,
            context,
        )
    } else {
        let mut template_result = function_call_analyzer::get_template_defaults(&method_info);
        function_call_analyzer::infer_template_replacements_from_args(
            analyzer,
            args,
            arg_positions,
            &method_info.params,
            &mut template_result,
            analysis_data,
            context,
        );
        template_result
    };

    // Consult the method params providers (Psalm checks
    // `$codebase->methods->params_provider` at the top of
    // Methods::getMethodParams) — a provider may rebuild the parameter
    // list from the call site (e.g. PDOStatement::setFetchMode's mode-
    // dependent tail).
    let provider_adjusted_method_info = crate::params_provider::dispatch_method_params(
        &crate::params_provider::MethodParamsProviderEvent {
            analyzer,
            class_id,
            method_name,
            args,
            arg_positions,
            context,
        },
        analysis_data,
    )
    .map(|params| {
        let mut adjusted = method_info.clone();
        adjusted.params = params;
        adjusted
    });
    let method_info_for_args: &pzoom_code_info::FunctionLikeInfo = provider_adjusted_method_info
        .as_ref()
        .unwrap_or(&method_info);

    verify_method_arguments(
        analyzer,
        args,
        arg_positions,
        method_info_for_args,
        class_name.as_ref(),
        method_name,
        analysis_data,
        context,
        pos,
        &template_result,
        class_id,
        receiver_class_id,
        parent_class_id,
        any_candidate_accepts_arg_count,
        obj_type
            .types
            .iter()
            .find(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. })),
    );

    // By-ref / @param-out write-backs: Psalm applies the same out-type
    // machinery to method calls as to named functions, so a by-ref array
    // arg widens to the param's declared type after the call.
    super::arguments_analyzer::apply_param_out_types(
        analyzer,
        method_info.name,
        &method_info_for_args.template_types,
        args,
        arg_positions,
        &method_info_for_args.params,
        analysis_data,
        context,
        &template_result,
        pos,
    );

    apply_post_call_assertions(
        analyzer,
        analysis_data,
        object_expr,
        args,
        &method_info,
        context,
        &template_result,
        class_id,
        receiver_class_id,
        parent_class_id,
    );

    maybe_emit_if_this_is_mismatch(
        analyzer,
        &method_info,
        receiver_class_id,
        object_type_params.as_deref(),
        &template_result,
        parent_class_id,
        pos,
        analysis_data,
    );

    crate::stmt::attribute_analyzer::analyze_reflection_get_attributes_call(
        analyzer,
        class_id,
        method_name,
        args,
        arg_positions,
        analysis_data,
    );

    if let Some(resolved_class_info) = analyzer.codebase.get_class(class_id) {
        let visibility_scope_class_id =
            get_method_visibility_scope_class_id(resolved_class_info, &method_info);

        match method_info.visibility {
            Visibility::Public => {}
            Visibility::Private => {
                let is_same_class = calling_class
                    .is_some_and(|caller_class| caller_class == visibility_scope_class_id);

                if !is_same_class
                    && !receiver_allows_method_visibility_override(
                        analyzer,
                        expanded_obj_type,
                        visibility_scope_class_id,
                    )
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    let issue_kind = if should_report_private_method_as_undefined(
                        analyzer,
                        calling_class,
                        visibility_scope_class_id,
                    ) {
                        IssueKind::UndefinedMethod
                    } else {
                        IssueKind::InaccessibleMethod
                    };
                    let message = if issue_kind == IssueKind::UndefinedMethod {
                        crate::class_casing::undefined_method_message(
                            analyzer,
                            &class_name,
                            method_name,
                        )
                    } else {
                        format!(
                            "Cannot access private method {}::{}",
                            analyzer.interner.lookup(visibility_scope_class_id),
                            method_name
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
            }
            Visibility::Protected => {
                let can_access = calling_class.is_some_and(|caller_class| {
                    can_access_protected_member_visibility(
                        analyzer,
                        caller_class,
                        visibility_scope_class_id,
                    )
                });

                if !can_access
                    && !receiver_allows_method_visibility_override(
                        analyzer,
                        expanded_obj_type,
                        visibility_scope_class_id,
                    )
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleMethod,
                        format!(
                            "Cannot access protected method {}::{}",
                            analyzer.interner.lookup(visibility_scope_class_id),
                            method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    // Deprecated / @internal method prohibition (Psalm MethodCallProhibitionAnalyzer).
    super::method_call_prohibition_analyzer::analyze(
        analyzer,
        &method_info,
        class_name.as_ref(),
        method_name,
        pos,
        analysis_data,
        context,
    );

    // Impure-call purity check (Psalm MethodCallPurityAnalyzer). A
    // reference-free receiver (e.g. a freshly-`new`'d externally-mutation
    // -free object) is "pure compatible", so its mutating methods may be
    // called from a pure context.
    let receiver_is_pure_compatible = obj_type.is_reference_free();
    super::method_call_purity_analyzer::analyze(
        analyzer,
        class_id,
        &method_info,
        class_name.as_ref(),
        method_name,
        method_name_pos,
        enforce_mutation_free,
        receiver_is_pure_compatible,
        analysis_data,
    );

    // Psalm MethodCallPurityAnalyzer's unused branch: a mutation-free
    // method's discarded result reports UnusedMethodCall under
    // find_unused_variables. The external-mutation-free arm requires the
    // receiver to be a FRESH expression (Psalm checks the receiver node's
    // 'external_mutation_free'/'pure' attributes, set on `new`/pure-call
    // nodes) — mutating an object held in a variable is observable later.
    if is_primary && analyzer.config.find_unused_code {
        // A call on a union receiver (`A|B`) resolves a method on each
        // member, and Psalm's Methods::methodExists records a reference for
        // every one of them. Record each member's resolved method, not only
        // the single best candidate, so e.g. `B::foo` stays alive when the
        // call is typed `A|B`.
        let mut recorded_any = false;
        for atomic in receiver_atomics {
            if let TAtomic::TNamedObject {
                name, type_params, ..
            } = atomic
                && let Some(atomic_class_info) = analyzer.codebase.get_class(*name)
                && let Some((member_class_id, _, member_method_info)) =
                    resolve_named_object_instance_method(
                        analyzer,
                        atomic_class_info,
                        type_params.as_deref(),
                        method_name,
                        Some(&analysis_data.type_variable_bounds),
                    )
            {
                record_method_reference(
                    analyzer,
                    member_class_id,
                    member_method_info.declaring_class,
                    method_name,
                    context,
                    analysis_data,
                );
                recorded_any = true;
            }
        }
        // Fall back to the resolved candidate (e.g. template/intersection
        // receivers that the per-atomic walk above doesn't cover).
        if !recorded_any {
            record_method_reference(
                analyzer,
                class_id,
                method_info.declaring_class,
                method_name,
                context,
                analysis_data,
            );
        }
    }

    let receiver_is_fresh_pure_value = receiver_is_pure_compatible
        && matches!(
            object_expr.unparenthesized(),
            Expression::Instantiation(_) | Expression::Call(_)
        );
    if is_primary
        && analyzer.config.report_unused
        && !context.inside_unset
        && !context.inside_conditional
        && !context.inside_general_use
        && !context.inside_throw
        && !context.inside_assignment
        && !context.inside_call
        && !context.inside_return
        && !context.inside_isset
        && method_info.assertions.is_empty()
        && method_info.if_true_assertions.is_empty()
        && method_info.if_false_assertions.is_empty()
        && !method_info.has_throws
        && analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| {
                super::method_call_purity_analyzer::method_is_mutation_free(
                    &method_info,
                    class_info,
                ) || (method_info.is_external_mutation_free && receiver_is_fresh_pure_value)
            })
    {
        // Psalm points at the method name node.
        let (line, col) = analyzer.get_line_column(method_name_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::UnusedMethodCall,
            format!(
                "The call to {}::{} is not used",
                class_name.as_ref(),
                method_name
            ),
            analyzer.file_path,
            method_name_pos.0,
            method_name_pos.1,
            line,
            col,
        ));
    }

    if let Some(magic_property_return) = analyze_magic_property_method_call(
        analyzer,
        class_id,
        object_type_params.as_deref(),
        method_name,
        object_expr,
        args,
        arg_positions,
        pos,
        analysis_data,
    ) {
        return Some(magic_property_return);
    }

    let resolved_return_type = crate::return_type_provider::dispatch_method_return_type(
        &crate::return_type_provider::MethodReturnTypeProviderEvent {
            analyzer,
            class_id,
            method_name,
            args,
            arg_positions,
            analysis_data,
        },
    )
    .unwrap_or_else(|| {
        let param_arg_types = super::function_call_return_type_fetcher::collect_param_arg_types(
            &method_info_for_args.params,
            arg_positions,
            analysis_data,
        );
        crate::methods::get_method_return_type(
            analyzer,
            class_id,
            method_name,
            &method_info,
            &template_result,
            &param_arg_types,
            args.len(),
        )
    });

    let static_class_id =
        find_concrete_receiver_class_id(analyzer, obj_type).unwrap_or(receiver_class_id);

    // Psalm's MethodCallReturnTypeFetcher: `static` in the return type
    // binds firmly when the receiver's concrete class is final, and a
    // template-typed receiver (`T as Model`) late-binds `static` to the
    // template itself ($static_type carries the lhs type part).
    let receiver_template_binding = obj_type
        .get_single()
        .filter(|receiver_atomic| matches!(receiver_atomic, TAtomic::TTemplateParam { .. }));
    let receiver_is_final = analyzer
        .codebase
        .get_class(static_class_id)
        .is_some_and(|receiver_info| receiver_info.is_final);
    let mut localized_return_type = if let Some(receiver_template) = receiver_template_binding {
        crate::type_expander::localize_special_class_type_union_with_static_object(
            analyzer.codebase,
            analyzer.interner,
            &resolved_return_type,
            class_id,
            receiver_template.clone(),
            parent_class_id,
        )
    } else {
        crate::type_expander::localize_special_class_type_union_final(
            analyzer.codebase,
            analyzer.interner,
            &resolved_return_type,
            class_id,
            static_class_id,
            parent_class_id,
            receiver_is_final,
        )
    };

    if should_strip_false_from_datetime_modify_return(
        analyzer,
        receiver_class_id,
        method_name,
        arg_positions,
        analysis_data,
    ) || should_strip_false_from_pdo_prepare_return(analyzer, receiver_class_id, method_name)
    {
        localized_return_type
            .types
            .retain(|atomic| !matches!(atomic, TAtomic::TFalse));
    }

    if union_contains_static_reference(&resolved_return_type) {
        localized_return_type =
            merge_receiver_intersection_into_return_type(&localized_return_type, obj_type);
    }

    // Psalm keeps the receiver intersection through `@return self` too:
    // ExpectationInterface::andReturn (`@return self`) called on
    // `ExpectationInterface&MockInterface` still yields the intersection,
    // so a chained shouldReceive resolves (Mockery chains). `self`
    // resolves to the declaring class at scan time, so the declaring
    // class reappearing in the return type is the closest available
    // signal here.
    if obj_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TObjectIntersection { .. }))
    {
        localized_return_type =
            intersect_self_return_with_receiver(&localized_return_type, obj_type, class_id);
    }

    let method_is_mutation_free = analyzer
        .codebase
        .get_class(class_id)
        .map(|class_info| super::method_call_purity_analyzer::method_is_mutation_free(&method_info, class_info))
        .unwrap_or(method_info.is_mutation_free);

    // Mirror Psalm's MethodCallPurityAnalyzer: a mutation-free method's
    // result is only memoizable when the mutation-free status was
    // *declared* (not inferred from the body), or the method is final or
    // private. An inferred-mutation-free, overridable method can't be
    // memoized — a subclass may override it impurely.
    let can_memoize = method_is_mutation_free
        && (!method_info.mutation_free_inferred
            || method_info.is_final
            || matches!(method_info.visibility, Visibility::Private));

    // Psalm's MethodCallAnalyzer only consults the tracked `$x->m()` entry
    // when the call can be memoized — a narrowed entry for an impure (or
    // overridable inferred-pure) method must not stand in for a fresh call.
    if is_primary
        && can_memoize
        && let Some(tracked_type) =
            get_cached_no_arg_method_call_type(context, object_expr, method_name, args.len())
        && let Some(intersection) =
            assertion_reconciler::intersect_union_with_union(&localized_return_type, &tracked_type)
    {
        localized_return_type = intersection;
    }

    // Each impure member invalidates the `$lhs->prop` narrowings it mutates
    // (run per member — Hakana's per-atomic existing-method analysis), not
    // only for the primary member.
    if !method_is_mutation_free {
        invalidate_property_narrowings_after_mutation(context);

        // Mirror Psalm's MethodCallPurityAnalyzer with the default config
        // (`remember_property_assignments_after_call = true`): a non-mutation-free
        // call only invalidates the specific `$lhs->prop` narrowings for properties
        // the called method actually assigns to (its `this_property_mutations`).
        //
        // While collecting constructor initialisations the call was instead
        // *followed* in place (`init_collector`), which already established the
        // authoritative post-call `$this->prop` scope — dropping those keys here
        // would discard exactly the initialisations the follow just recorded.
        if !context.collect_initializations
            && !method_info.this_property_mutations.is_empty()
            && let Some(object_key) = expression_identifier::get_expression_var_key(object_expr)
        {
            // Collect the reference cluster for the receiver variable so a
            // mutation through one alias (`$ref = &$obj`) also invalidates the
            // narrowing held under the other alias.
            let mut root_names: Vec<String> = vec![object_key.to_string()];
            if let Some(target_id) = context.references_in_scope.get(object_key.as_str()) {
                root_names.push(target_id.to_string());
            }
            for (ref_id, target_id) in &context.references_in_scope {
                if target_id == &object_key {
                    root_names.push(ref_id.to_string());
                }
            }

            for prop_name in &method_info.this_property_mutations {
                let prop = analyzer.interner.lookup(*prop_name);
                for root in &root_names {
                    let mutation_var = format!("{}->{}", root, prop);
                    context.locals.remove(mutation_var.as_str());
                }
            }
        }
    }

    if is_primary
        && args.is_empty()
        && can_memoize
        && let Some(object_key) = expression_identifier::get_expression_var_key(object_expr)
    {
        let call_key = format!("{}->{}()", object_key, method_name.to_ascii_lowercase());
        let call_id = analyzer.interner.intern(&call_key);
        context.locals.insert(
            VarName::new(&analyzer.interner.lookup(call_id)),
            localized_return_type.clone(),
        );
        // Psalm marks the node `memoizable` so getExtendedVarId
        // (and through it the assertion finder) keys on the call.
        analysis_data.memoizable_method_call_offsets.insert(pos.0);
    }

    if is_primary
        && has_null_receiver
        && !suppress_possibly_null_reference_issue
        && !expanded_obj_type.ignore_nullable_issues
        && !crate::issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            pos.0,
            "PossiblyNullReference",
        )
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullReference,
            format!("Cannot call method {} on possibly null value", method_name),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if is_primary && has_false_receiver && !expanded_obj_type.ignore_falsable_issues {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyFalseReference,
            format!("Cannot call method {} on possibly false value", method_name),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if is_primary && has_invalid_receiver {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyInvalidMethodCall,
            format!(
                "Cannot call method {} on possibly invalid type",
                method_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if is_primary
        && let Some(interface_id) = first_missing_interface
        && !context.inside_conditional
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedInterfaceMethod,
            crate::class_casing::undefined_method_message(
                analyzer,
                analyzer.interner.lookup(interface_id),
                method_name,
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let object_span = object_expr.span();
    let receiver_var_key = crate::expression_identifier::get_expression_var_key(object_expr);
    localized_return_type = add_method_call_dataflow_with_receiver(
        analyzer,
        localized_return_type,
        Some((object_span.start.offset, object_span.end.offset)),
        receiver_var_key.as_deref(),
        Some(context),
        // The receiver's class: taints route `Declaring::m → Receiver::m`
        // inside add_method_call_dataflow (Hakana get_tainted_method_node).
        receiver_class_id,
        analyzer.interner.intern(method_name),
        Some(&method_info),
        arg_positions,
        analysis_data,
        pos,
    );

    // Psalm marks the return of a declared external-mutation-free method
    // reference_free (FunctionLikeAnalyzer), so chained builder calls
    // like `$x->getBuilder()->setTypes(...)` stay pure-compatible.
    if (method_info.is_external_mutation_free
        || method_info.is_mutation_free
        || method_info.is_pure)
        && !method_info.mutation_free_inferred
    {
        localized_return_type.reference_free = true;
    }

    Some(localized_return_type)
}

