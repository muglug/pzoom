//! Atomic method-call analyzer.
//!
//! Resolves and checks an instance method call against a single receiver type:
//! method resolution up the hierarchy, magic `__call`/magic-property calls,
//! visibility, template/`static` localization, inherited return/param types,
//! return-type-provider adjustments, argument verification, and post-call
//! assertions. Mirrors Psalm's `AtomicMethodCallAnalyzer` / `ExistingAtomicMethodCallAnalyzer`.

use crate::type_expander::localize_special_class_type_union;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::functionlike_info::AssertionType;
use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{
    can_access_internal, format_caller_context, format_internal_scope_phrase,
};
use crate::issue_suppression;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::attribute_analyzer;

use super::argument_analyzer;
use super::{
    arguments_analyzer,
    existing_atomic_method_call_analyzer, function_call_analyzer,
};

use super::method_call_analyzer::*;

use super::method_call_return_type_fetcher::*;
use super::method_visibility_analyzer::*;
use super::method_call_prohibition_analyzer::*;
use super::method_call_purity_analyzer::*;
use super::missing_method_call_handler::*;
use crate::template::TemplateMap;

pub(crate) fn get_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    obj_type: &TUnion,
    method_name: &str,
    pos: Pos,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    enforce_mutation_free: bool,
    suppress_possibly_null_reference_issue: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    if method_name.eq_ignore_ascii_case("__construct") {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::DirectConstructorCall,
            "Constructors should only be called with new",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let reconciled_receiver_type =
        get_reconciled_receiver_type_for_expression(analyzer, context, object_expr)
            .and_then(|tracked_type| {
                assertion_reconciler::intersect_union_with_union(obj_type, &tracked_type)
            })
            .unwrap_or_else(|| obj_type.clone());
    let expanded_obj_type = expand_template_object_union(&reconciled_receiver_type);

    let mut resolved_method: Option<(
        pzoom_str::StrId,
        pzoom_str::StrId,
        Option<Vec<TUnion>>,
        pzoom_code_info::FunctionLikeInfo,
    )> = None;
    let mut has_unsealed_magic_call = false;
    let mut magic_call_return_type: Option<TUnion> = None;
    let mut has_valid_receiver = false;
    let mut has_null_receiver = false;
    let mut has_false_receiver = false;
    let mut has_invalid_receiver = false;
    let mut has_receiver_without_method = false;
    let mut first_missing_interface: Option<StrId> = None;
    let is_this_call =
        expression_identifier::get_expression_var_key(object_expr).as_deref() == Some("$this");
    let calling_class = analyzer.get_declaring_class();

    for atomic in &expanded_obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, type_params , .. } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if let Some((resolved_class, resolved_type_params, method_info)) =
                        resolve_named_object_instance_method(
                            analyzer,
                            class_info,
                            type_params.as_deref(),
                            method_name,
                        )
                    {
                        has_valid_receiver = true;
                        if let Some(existing) = &mut resolved_method {
                            let existing_is_interface = analyzer
                                .codebase
                                .get_class(existing.1)
                                .is_some_and(|info| info.kind == ClassLikeKind::Interface);
                            if class_info.kind == ClassLikeKind::Interface
                                && existing_is_interface
                                && method_has_more_specific_return(
                                    analyzer,
                                    &method_info,
                                    &existing.3,
                                )
                            {
                                *existing =
                                    (*name, resolved_class, resolved_type_params, method_info);
                            }
                        } else {
                            resolved_method =
                                Some((*name, resolved_class, resolved_type_params, method_info));
                        }
                    } else if class_info.kind == ClassLikeKind::Interface
                        && !class_info.override_method_visibility
                        && first_missing_interface.is_none()
                    {
                        first_missing_interface = Some(*name);
                    } else if !(class_has_magic_call(class_info)
                        && !class_has_sealed_methods(class_info))
                    {
                        has_receiver_without_method = true;
                    }

                    if class_has_magic_call(class_info) && !class_has_sealed_methods(class_info) {
                        has_valid_receiver = true;
                        has_unsealed_magic_call = true;

                        if let Some(magic_call_info) = class_info.methods.get(&StrId::CALL) {
                            analyze_pending_closure_args_for_method(
                                analyzer,
                                args,
                                arg_positions,
                                magic_call_info,
                                class_info,
                                type_params.as_deref(),
                                *name,
                                *name,
                                class_info.parent_class,
                                analysis_data,
                                context,
                            );

                            let (template_defaults, template_replacements) =
                                existing_atomic_method_call_analyzer::build_method_template_context(
                                    analyzer,
                                    class_info,
                                    type_params.as_deref(),
                                    magic_call_info,
                                    is_this_call,
                                    args,
                                    arg_positions,
                                    analysis_data,
                                    context,
                                );

                            let resolved_magic_return = resolve_effective_method_return_type(
                                analyzer,
                                *name,
                                "__call",
                                magic_call_info,
                                &template_defaults,
                                &template_replacements,
                                args.len(),
                            );

                            let localized_magic_return = localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
                                &resolved_magic_return,
                                *name,
                                *name,
                                class_info.parent_class,
                            );

                            magic_call_return_type =
                                Some(if let Some(existing) = magic_call_return_type {
                                    combine_union_types(&existing, &localized_magic_return, false)
                                } else {
                                    localized_magic_return
                                });
                        }
                    }
                }
            }
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_resolved: Option<(
                    pzoom_str::StrId,
                    pzoom_str::StrId,
                    Option<Vec<TUnion>>,
                    pzoom_code_info::FunctionLikeInfo,
                )> = None;

                for nested in types {
                    let TAtomic::TNamedObject { name, type_params , .. } = nested else {
                        continue;
                    };

                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        continue;
                    };

                    let Some((resolved_class, resolved_type_params, method_info)) =
                        resolve_named_object_instance_method(
                            analyzer,
                            class_info,
                            type_params.as_deref(),
                            method_name,
                        )
                    else {
                        // For intersections (e.g. A&I), missing a method on one component
                        // does not mean the concrete object cannot provide it.
                        continue;
                    };

                    has_valid_receiver = true;
                    if let Some(existing) = &mut intersection_resolved {
                        // Prefer the intersection part that supplies generic
                        // params (e.g. `IParent<C>` over a bare `IChild`):
                        // Psalm keeps those bindings via the atomic's
                        // extra_types, which pzoom's intersection model splits
                        // into separate parts.
                        if (existing.2.is_none() && resolved_type_params.is_some())
                            || method_has_more_specific_return(analyzer, &method_info, &existing.3)
                        {
                            *existing = (*name, resolved_class, resolved_type_params, method_info);
                        }
                    } else {
                        intersection_resolved =
                            Some((*name, resolved_class, resolved_type_params, method_info));
                    }
                }

                if resolved_method.is_none() {
                    resolved_method = intersection_resolved;
                }
            }
            TAtomic::TObject | TAtomic::TMixed => {
                has_valid_receiver = true;
            }
            TAtomic::TNull | TAtomic::TVoid => {
                has_null_receiver = true;
            }
            TAtomic::TFalse => {
                has_false_receiver = true;
            }
            _ => {
                has_invalid_receiver = true;
            }
        }
    }

    if resolved_method.is_some() && has_receiver_without_method {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyUndefinedMethod,
            format!(
                "Method {} may not exist on one or more possible object types",
                method_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if is_this_call
        && let Some(calling_class_id) = calling_class
        && resolved_method
            .as_ref()
            .is_some_and(|(_, resolved_class_id, _, _)| *resolved_class_id != calling_class_id)
    {
        let existing_type_params = resolved_method
            .as_ref()
            .and_then(|(_, _, type_params, _)| type_params.clone());

        if let Some(calling_class_info) = analyzer.codebase.get_class(calling_class_id)
            && let Some((self_resolved_class_id, _, self_method_info)) =
                resolve_named_object_instance_method(
                    analyzer,
                    calling_class_info,
                    None,
                    method_name,
                )
        {
            resolved_method = Some((
                calling_class_id,
                self_resolved_class_id,
                existing_type_params,
                self_method_info,
            ));
        }
    }

    if let Some((receiver_class_id, class_id, object_type_params, method_info)) = resolved_method {
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

            if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                let scope_phrase = format_internal_scope_phrase(analyzer, &class_info.internal);
                let caller_phrase = format_caller_context(analyzer, Some(context));
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

        let (template_defaults, template_replacements) =
            if let Some(class_info) = analyzer.codebase.get_class(class_id) {
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

                existing_atomic_method_call_analyzer::build_method_template_context(
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
                let template_defaults = function_call_analyzer::get_template_defaults(&method_info);
                let template_replacements =
                    function_call_analyzer::infer_template_replacements_from_args(
                        analyzer,
                        args,
                        arg_positions,
                        &method_info.params,
                        &template_defaults,
                        analysis_data,
                        context,
                    );
                (template_defaults, template_replacements)
            };

        verify_method_arguments(
            analyzer,
            args,
            arg_positions,
            &method_info,
            class_name.as_ref(),
            method_name,
            analysis_data,
            context,
            pos,
            &template_defaults,
            &template_replacements,
            class_id,
            receiver_class_id,
            parent_class_id,
        );

        apply_post_call_assertions(
            analyzer,
            object_expr,
            args,
            &method_info,
            context,
            &template_defaults,
            &template_replacements,
            class_id,
            receiver_class_id,
            parent_class_id,
        );

        existing_atomic_method_call_analyzer::maybe_emit_if_this_is_mismatch(
            analyzer,
            &method_info,
            receiver_class_id,
            object_type_params.as_deref(),
            &template_defaults,
            &template_replacements,
            parent_class_id,
            pos,
            analysis_data,
        );

        attribute_analyzer::analyze_reflection_get_attributes_call(
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
                            &expanded_obj_type,
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
                            format!("Method {}::{} does not exist", class_name, method_name)
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
                            &expanded_obj_type,
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
            pos,
            enforce_mutation_free,
            receiver_is_pure_compatible,
            analysis_data,
        );

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

        let resolved_return_type =
            crate::return_type_provider::dispatch_method_return_type(
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
                    resolve_effective_method_return_type(
                        analyzer,
                        class_id,
                        method_name,
                        &method_info,
                        &template_defaults,
                        &template_replacements,
                        args.len(),
                    )
                });

        let static_class_id =
            find_concrete_receiver_class_id(analyzer, obj_type).unwrap_or(receiver_class_id);

        let mut localized_return_type = localize_special_class_type_union(analyzer.codebase, analyzer.interner,
            &resolved_return_type,
            class_id,
            static_class_id,
            parent_class_id,
        );

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

        if let Some(tracked_type) = get_cached_no_arg_method_call_type(
            analyzer,
            context,
            object_expr,
            method_name,
            args.len(),
        ) {
            if let Some(intersection) = assertion_reconciler::intersect_union_with_union(
                &localized_return_type,
                &tracked_type,
            ) {
                localized_return_type = intersection;
            }
        }

        let method_is_mutation_free = analyzer
            .codebase
            .get_class(class_id)
            .map(|class_info| method_is_mutation_free(&method_info, class_info))
            .unwrap_or(method_info.is_mutation_free);

        if !method_is_mutation_free {
            invalidate_property_narrowings_after_mutation(analyzer, context);

            // Mirror Psalm's MethodCallPurityAnalyzer with the default config
            // (`remember_property_assignments_after_call = true`): a non-mutation-free
            // call only invalidates the specific `$lhs->prop` narrowings for properties
            // the called method actually assigns to (its `this_property_mutations`).
            if !method_info.this_property_mutations.is_empty() {
                if let Some(object_key) =
                    expression_identifier::get_expression_var_key(object_expr)
                {
                    // Collect the reference cluster for the receiver variable so a
                    // mutation through one alias (`$ref = &$obj`) also invalidates the
                    // narrowing held under the other alias.
                    let mut root_names: Vec<String> = vec![object_key.clone()];
                    if let Some(object_var_id) = analyzer.interner.find(&object_key) {
                        if let Some(target_id) =
                            context.references_in_scope.get(&object_var_id)
                        {
                            root_names.push(analyzer.interner.lookup(*target_id).to_string());
                        }
                        for (ref_id, target_id) in &context.references_in_scope {
                            if *target_id == object_var_id {
                                root_names.push(analyzer.interner.lookup(*ref_id).to_string());
                            }
                        }
                    }

                    for prop_name in &method_info.this_property_mutations {
                        let prop = analyzer.interner.lookup(*prop_name);
                        for root in &root_names {
                            let mutation_var = format!("{}->{}", root, prop);
                            if let Some(var_id) = analyzer.interner.find(&mutation_var) {
                                context.locals.remove(&var_id);
                            }
                        }
                    }
                }
            }
        }

        if args.is_empty() {
            // Mirror Psalm's MethodCallPurityAnalyzer: a mutation-free method's
            // result is only memoizable when the mutation-free status was
            // *declared* (not inferred from the body), or the method is final or
            // private. An inferred-mutation-free, overridable method can't be
            // memoized — a subclass may override it impurely.
            let can_memoize = method_is_mutation_free
                && (!method_info.mutation_free_inferred
                    || method_info.is_final
                    || matches!(method_info.visibility, Visibility::Private));

            if can_memoize {
                if let Some(object_key) = expression_identifier::get_expression_var_key(object_expr)
                {
                    let call_key = format!("{}->{}()", object_key, method_name);
                    let call_id = analyzer.interner.intern(&call_key);
                    context
                        .locals
                        .insert(call_id, localized_return_type.clone());
                }
            }
        }

        if has_null_receiver
            && !suppress_possibly_null_reference_issue
            && !expanded_obj_type.ignore_nullable_issues
            && !issue_suppression::is_issue_suppressed_at(analyzer, pos.0, "PossiblyNullReference")
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

        if has_false_receiver && !expanded_obj_type.ignore_falsable_issues {
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

        if has_invalid_receiver {
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

        if let Some(interface_id) = first_missing_interface
            && !context.inside_conditional
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedInterfaceMethod,
                format!(
                    "Method {}::{} does not exist",
                    analyzer.interner.lookup(interface_id),
                    method_name
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        let call_node = DataFlowNode::get_for_call(
            FunctionLikeIdentifier::Method(class_id, analyzer.interner.intern(method_name)),
            make_data_flow_node_position(analyzer, pos),
        );
        analysis_data.data_flow_graph.add_node(call_node.clone());

        add_default_dataflow_paths(
            &mut analysis_data.data_flow_graph,
            &expanded_obj_type.parent_nodes,
            &call_node,
        );

        for arg_pos in arg_positions {
            if let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) {
                add_default_dataflow_paths(
                    &mut analysis_data.data_flow_graph,
                    &arg_type.parent_nodes,
                    &call_node,
                );
            }
        }

        pzoom_code_info::ttype::extend_dataflow_uniquely(
            &mut localized_return_type.parent_nodes,
            vec![call_node],
        );

        return Some(localized_return_type);
    }

    if has_unsealed_magic_call {
        return Some(magic_call_return_type.unwrap_or_else(TUnion::mixed));
    }

    if !has_valid_receiver && !has_unsealed_magic_call {
        // A preceding `method_exists($obj, 'foo')` guard proves the method exists at
        // runtime even though it is absent from the declared class, so treat the call as
        // returning `mixed` rather than reporting UndefinedMethod (matching Psalm).
        if is_method_guarded_by_method_exists(analyzer, context, object_expr, method_name) {
            return Some(TUnion::mixed());
        }

        let mut saw_named_object = false;
        let mut saw_non_interface = false;
        let mut first_interface: Option<StrId> = None;

        for atomic in &expanded_obj_type.types {
            match atomic {
                TAtomic::TNamedObject { name, .. } => {
                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        saw_non_interface = true;
                        continue;
                    };

                    saw_named_object = true;
                    if class_info.kind != ClassLikeKind::Interface {
                        saw_non_interface = true;
                        continue;
                    }

                    if first_interface.is_none() {
                        first_interface = Some(*name);
                    }
                }
                TAtomic::TObjectIntersection { types } => {
                    for nested in types {
                        let TAtomic::TNamedObject { name, .. } = nested else {
                            continue;
                        };

                        let Some(class_info) = analyzer.codebase.get_class(*name) else {
                            saw_non_interface = true;
                            continue;
                        };

                        saw_named_object = true;
                        if class_info.kind != ClassLikeKind::Interface {
                            saw_non_interface = true;
                            continue;
                        }

                        if first_interface.is_none() {
                            first_interface = Some(*name);
                        }
                    }
                }
                _ => {}
            }
        }

        if saw_named_object && !saw_non_interface && !context.inside_conditional {
            let interface_name = analyzer
                .interner
                .lookup(first_interface.unwrap_or(StrId::EMPTY));
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedInterfaceMethod,
                format!("Method {}::{} does not exist", interface_name, method_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            return None;
        }
    }

    for atomic in &expanded_obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if is_datetime_interface_add(analyzer, *name, method_name) {
                        return Some(TUnion::new(TAtomic::TNamedObject {
                            name: *name,
                            type_params: None,
                        is_static: false, remapped_params: false }));
                    }

                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(pos.0);

                    if class_has_magic_call(class_info) {
                        if class_has_sealed_methods(class_info) {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedMagicMethod,
                                format!(
                                    "Magic method {}::{} does not exist",
                                    class_name, method_name
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        } else {
                            return Some(TUnion::mixed());
                        }
                    } else {
                        if let Some(visibility_scope) =
                            find_private_method_visibility_scope(analyzer, *name, method_name)
                        {
                            let issue_kind = if should_report_private_method_as_undefined(
                                analyzer,
                                calling_class,
                                visibility_scope,
                            ) {
                                IssueKind::UndefinedMethod
                            } else {
                                IssueKind::InaccessibleMethod
                            };

                            let message = if issue_kind == IssueKind::UndefinedMethod {
                                format!("Method {}::{} does not exist", class_name, method_name)
                            } else {
                                format!(
                                    "Cannot access private method {}::{}",
                                    analyzer.interner.lookup(visibility_scope),
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
                        } else {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedMethod,
                                format!("Method {}::{} does not exist", class_name, method_name),
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
            TAtomic::TObjectIntersection { .. } => {}
            TAtomic::TObject => {
                // Generic object - can't look up method, just return mixed
            }
            TAtomic::TMixed => {
                if matches!(object_expr.unparenthesized(), Expression::ArrayAccess(_)) {
                    continue;
                }
                if !analyzer.config.is_issue_suppressed("MixedMethodCall") {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedMethodCall,
                        format!("Cannot call method {} on mixed type", method_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
            TAtomic::TNull | TAtomic::TVoid => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullReference,
                    format!("Cannot call method {} on null", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            _ => {
                let type_desc = atomic.get_id(Some(analyzer.interner));
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidMethodCall,
                    format!("Cannot call method {} on {}", method_name, type_desc),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    None
}

pub(crate) fn resolve_named_object_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if let Some(method_info) = get_method_info_case_insensitive(analyzer, class_info, method_name) {
        let visibility_scope_class_id =
            get_method_visibility_scope_class_id(class_info, method_info);

        if method_info.visibility != Visibility::Private
            || visibility_scope_class_id == class_info.name
        {
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info.clone(),
            ));
        }
    }

    if class_info.kind == ClassLikeKind::Interface || class_has_magic_call(class_info) {
        if let Some(method_info) =
            get_pseudo_method_info_case_insensitive(analyzer, class_info, method_name)
        {
            return Some((
                class_info.name,
                object_type_params.map(|p| p.to_vec()),
                method_info.clone(),
            ));
        }
    }

    resolve_named_mixin_instance_method(analyzer, class_info, object_type_params, method_name)
}

pub(crate) fn resolve_named_mixin_instance_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if class_info.named_mixins.is_empty() {
        return None;
    }

    let class_template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut class_template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut class_template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    for mixin_atomic in &class_info.named_mixins {
        let localized_mixin = function_call_analyzer::replace_templates_in_union(
            &TUnion::new(mixin_atomic.clone()),
            &class_template_replacements,
            &class_template_defaults,
        );

        for localized_atomic in localized_mixin.types {
            let TAtomic::TNamedObject {
                name: mixin_class_id,
                type_params: mixin_type_params,
            .. } = localized_atomic
            else {
                continue;
            };

            let Some(mixin_class_info) = analyzer.codebase.get_class(mixin_class_id) else {
                continue;
            };

            if let Some(method_info) =
                get_method_info_case_insensitive(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }

            if let Some(method_info) =
                get_pseudo_method_info_case_insensitive(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }
        }
    }

    None
}

pub(crate) fn get_literal_string_argument(
    analysis_data: &FunctionAnalysisData,
    arg_pos: Option<&Pos>,
) -> Option<String> {
    let arg_pos = *arg_pos?;
    let arg_type = analysis_data.get_expr_type(arg_pos)?;
    let atomic = arg_type.get_single()?;

    if let TAtomic::TLiteralString { value } = atomic {
        return Some(value.clone());
    }

    None
}

pub(crate) fn find_concrete_receiver_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    object_type: &TUnion,
) -> Option<StrId> {
    for atomic in &object_type.types {
        if let Some(class_id) = find_concrete_receiver_class_id_in_atomic(analyzer, atomic) {
            return Some(class_id);
        }
    }

    None
}

pub(crate) fn find_concrete_receiver_class_id_in_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .and_then(|class_info| (class_info.kind == ClassLikeKind::Class).then_some(*name)),
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                if let Some(class_id) = find_concrete_receiver_class_id_in_atomic(analyzer, nested)
                {
                    return Some(class_id);
                }
            }
            None
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            find_concrete_receiver_class_id(analyzer, as_type)
        }
        _ => None,
    }
}

pub(crate) fn union_contains_static_reference(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_static_reference)
}

pub(crate) fn atomic_contains_static_reference(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
            if *name == StrId::STATIC {
                return true;
            }

            type_params.as_ref().is_some_and(|type_params| {
                type_params
                    .iter()
                    .any(|type_param| union_contains_static_reference(type_param))
            })
        }
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_contains_static_reference)
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_static_reference(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_contains_static_reference(as_type),
        _ => false,
    }
}

pub(crate) fn collect_receiver_named_types(receiver_type: &TUnion) -> Vec<TAtomic> {
    let mut named_types = Vec::new();
    for atomic in &receiver_type.types {
        collect_receiver_named_types_in_atomic(atomic, &mut named_types);
    }
    named_types
}

pub(crate) fn collect_receiver_named_types_in_atomic(atomic: &TAtomic, target: &mut Vec<TAtomic>) {
    match atomic {
        TAtomic::TNamedObject { .. } => {
            if !target.contains(atomic) {
                target.push(atomic.clone());
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                collect_receiver_named_types_in_atomic(nested, target);
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested in &as_type.types {
                collect_receiver_named_types_in_atomic(nested, target);
            }
        }
        _ => {}
    }
}

pub(crate) fn verify_method_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_pos: Pos,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    let callable_name = format!("{}::{}", class_name, method_name);
    let arg_param_indices = arguments_analyzer::check_arguments_match(
        analyzer,
        args,
        arg_positions,
        method_info,
        &callable_name,
        analysis_data,
        context,
        Some(template_defaults),
        Some(template_replacements),
        call_pos,
        false,
        false,
    );

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = method_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let issue_pos = arg_positions.first().copied().unwrap_or(call_pos);
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to method {}, {} expected, {} provided",
                callable_name,
                required_params,
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = method_info.params.last().is_some_and(|p| p.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > method_info.params.len() {
        let issue_pos = arg_positions
            .get(method_info.params.len())
            .copied()
            .or_else(|| arg_positions.first().copied())
            .unwrap_or((0, 0));
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to method {}, {} expected, {} provided",
                callable_name,
                method_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        if arg.is_unpacked() {
            if let Some(arg_type) =
                arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
            {
                argument_analyzer::verify_unpacked_argument(
                    analyzer,
                    arg_pos,
                    &arg_type,
                    &callable_name,
                    method_info.no_named_arguments,
                    analysis_data,
                );
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| method_info.params.get(mapped_index))
            .or_else(|| method_info.params.last().filter(|p| p.is_variadic));

        if let (Some(param), Some(arg_type)) = (
            param,
            arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos),
        ) {
            let mut effective_param = param.clone();
            if let Some(inherited_param_type) =
                get_inherited_method_param_type(analyzer, self_class_id, method_name, idx)
            {
                let can_auto_inherit_docblock = inherited_param_type.from_docblock
                    && !method_has_docblock_param_types(method_info)
                    && !method_has_docblock_return_type(method_info);
                let can_inherit_interface_contract =
                    inherited_param_type.source_is_interface && !param.has_docblock_type;

                // Only fall back to the inherited type when the override declares
                // no type of its own (neither native nor docblock); an explicit
                // native param type on the override takes precedence (e.g. a child
                // widening `string` to `?string`). The docblock-propagation cases
                // below still apply when the override lacks a docblock refinement.
                let should_use_inherited = effective_param.get_type().is_none()
                    || (method_info.inherits_docblock && !param.has_docblock_type)
                    || (can_auto_inherit_docblock && !param.has_docblock_type)
                    || can_inherit_interface_contract;

                if should_use_inherited {
                    effective_param.param_type = Some(inherited_param_type.param_type);
                }
            }

            if let Some(param_type) = effective_param.get_type() {
                let replaced_param_type =
                    if template_defaults.is_empty() && template_replacements.is_empty() {
                        param_type.clone()
                    } else {
                        function_call_analyzer::replace_templates_in_union(
                            param_type,
                            template_replacements,
                            template_defaults,
                        )
                    };

                effective_param.param_type = Some(localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
                    &replaced_param_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ));
            }

            argument_analyzer::verify_type(
                analyzer,
                arg,
                arg_pos,
                &arg_type,
                &effective_param,
                param_index.unwrap_or(idx),
                &callable_name,
                analysis_data,
                context,
            );
        }
    }
}

pub(crate) fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    if method_info.assertions.is_empty() {
        return;
    }

    for assertion in &method_info.assertions {
        let resolved_assertion_type = replace_and_localize_assertion_type(
            analyzer.codebase,
            analyzer.interner,
            &assertion.assertion_type,
            template_replacements,
            template_defaults,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if assertion_name.as_ref() == "$this" {
            apply_assertion_to_expression(analyzer, object_expr, &resolved_assertion_type, context);
            continue;
        }

        // Property-path assertion on the receiver, e.g. `@psalm-assert !null
        // $this->other`. Rebase the `$this` prefix onto the actual receiver
        // expression and narrow that property in scope, seeding the declared
        // property type when it isn't a local yet. Mirrors Psalm applying
        // `$this->prop` assertions via the reconciler.
        if let Some(prop_suffix) = assertion_name.strip_prefix("$this->") {
            if let Some(receiver_key) =
                expression_identifier::get_expression_var_key(object_expr)
            {
                let full_key = format!("{}->{}", receiver_key, prop_suffix);
                let var_id = analyzer.interner.intern(&full_key);
                let existing_type = context
                    .locals
                    .get(&var_id)
                    .cloned()
                    .or_else(|| {
                        crate::reconciler::resolve_key_type(&full_key, context, analyzer)
                    })
                    .unwrap_or_else(TUnion::mixed);
                let narrowed_type =
                    apply_functionlike_assertion_to_union(&existing_type, &resolved_assertion_type);
                context.locals.insert(var_id, narrowed_type);
            }
            continue;
        }

        let Some(param_idx) =
            find_assertion_param_index(analyzer, &method_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(argument) = args.get(param_idx) else {
            continue;
        };

        apply_assertion_to_expression(
            analyzer,
            argument.value(),
            &resolved_assertion_type,
            context,
        );
    }
}

pub(crate) fn replace_and_localize_assertion_type(
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    assertion_type: &AssertionType,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => {
            AssertionType::IsType(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsEqual(asserted_type) => {
            AssertionType::IsEqual(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsLooselyEqual(asserted_type) => {
            AssertionType::IsLooselyEqual(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotType(asserted_type) => {
            AssertionType::IsNotType(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotEqual(asserted_type) => {
            AssertionType::IsNotEqual(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotLooselyEqual(asserted_type) => {
            AssertionType::IsNotLooselyEqual(localize_special_class_type_union(codebase, interner, 
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_replacements,
                    template_defaults,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

pub(crate) fn apply_assertion_to_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
) {
    let Some(var_key) = expression_identifier::get_expression_var_key(expr) else {
        return;
    };

    let var_id = analyzer.interner.intern(&var_key);
    let existing_type = context
        .locals
        .get(&var_id)
        .cloned()
        .unwrap_or_else(TUnion::mixed);
    let narrowed_type = apply_functionlike_assertion_to_union(&existing_type, assertion_type);
    context.locals.insert(var_id, narrowed_type);
}

pub(crate) fn find_assertion_param_index(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertion_var_id: pzoom_str::StrId,
) -> Option<usize> {
    let assertion_name = analyzer.interner.lookup(assertion_var_id);
    let normalized_assertion = assertion_name
        .strip_prefix('$')
        .unwrap_or(assertion_name.as_ref());

    params.iter().position(|param| {
        if param.name == assertion_var_id {
            return true;
        }

        let param_name = analyzer.interner.lookup(param.name);
        let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name.as_ref());
        normalized_param == normalized_assertion
    })
}

pub(crate) fn apply_functionlike_assertion_to_union(
    existing_type: &TUnion,
    assertion_type: &AssertionType,
) -> TUnion {
    match assertion_type {
        AssertionType::IsType(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsEqual(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsLooselyEqual(_) => existing_type.clone(),
        AssertionType::IsNotType(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotEqual(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotLooselyEqual(_) => existing_type.clone(),
        AssertionType::Truthy | AssertionType::NotEmpty => existing_type.clone(),
        AssertionType::Falsy => existing_type.clone(),
        AssertionType::NotNull => subtract_union(existing_type, &TUnion::new(TAtomic::TNull)),
    }
}

pub(crate) fn subtract_union(existing_type: &TUnion, type_to_remove: &TUnion) -> TUnion {
    let filtered_types: Vec<_> = existing_type
        .types
        .iter()
        .filter(|atomic| !type_to_remove.types.contains(atomic))
        .cloned()
        .collect();

    if filtered_types.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered_types)
    }
}
