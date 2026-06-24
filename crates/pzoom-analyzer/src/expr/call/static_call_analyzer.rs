//! Static method call analyzer.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::cst::cst::argument::Argument;
use mago_syntax::cst::cst::call::StaticMethodCall;
use mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector;
use mago_syntax::cst::cst::expression::Expression;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashMap;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{
    can_access_internal, format_caller_context, format_internal_scope_phrase,
};
use crate::statements_analyzer::StatementsAnalyzer;

use super::{argument_analyzer, callable_validation, function_call_analyzer};

use super::atomic_static_call_analyzer::*;
use super::existing_atomic_static_call_analyzer::*;
use super::method_call_prohibition_analyzer::class_has_sealed_methods;
use pzoom_code_info::TemplateResult;
use std::rc::Rc;

/// Analyze a static method call expression (Foo::bar()).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the class expression. Psalm's StaticCallAnalyzer flips
    // inside_general_use around it, so `$handler::method()` counts as a use
    // of `$handler`.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let class_pos =
        expression_analyzer::analyze(analyzer, static_call.class, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let class_expr_type = analysis_data.expr_types.get(&class_pos).cloned();

    // Analyze arguments and collect positions
    let args: Vec<_> = static_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();
    // Predeclare by-ref out-params (`Foo::bar(..., &$out)`) before analyzing
    // the argument expressions, mirroring the function-call path — otherwise
    // the out-variable reads as undefined.
    if let Some(method_info) = pre_resolve_static_method_info(analyzer, static_call, context) {
        super::arguments_analyzer::predeclare_by_ref_argument_vars(
            analyzer,
            Some("static-method"),
            Some(method_info),
            &static_call.argument_list.arguments,
            context,
        );
    }

    for arg in &args {
        if is_closure_like_argument(arg) {
            continue;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    if analyzer.get_declaring_class().is_none()
        && let Some(keyword) = get_self_static_or_parent_keyword(analyzer, static_call.class)
    {
        analyze_pending_closure_args_without_context(
            analyzer,
            &args,
            &arg_positions,
            analysis_data,
            context,
        );
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::NonStaticSelfCall,
            format!("Cannot use {} outside class context", keyword),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        analysis_data
            .expr_types
            .insert(pos, Rc::new(TUnion::mixed()));
        return;
    }

    if matches!(static_call.class.unparenthesized(), Expression::Parent(_))
        && analyzer.get_declaring_class().is_some_and(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .is_none_or(|class_info| class_info.parent_class.is_none())
        })
    {
        analyze_pending_closure_args_without_context(
            analyzer,
            &args,
            &arg_positions,
            analysis_data,
            context,
        );
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ParentNotFound,
            "Cannot call method on parent as this class does not extend another",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        analysis_data
            .expr_types
            .insert(pos, Rc::new(TUnion::mixed()));
        return;
    }

    // Try to get the class name using resolved names
    let class_id = get_resolved_class_id(analyzer, static_call.class, context);

    // Get the method name
    let method_name = get_method_name(&static_call.method);

    // Dynamic method selectors (`A::$method()`) consume their inner
    // expression (general use).
    if let ClassLikeMemberSelector::Variable(selector_var) = &static_call.method {
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;
        let _ = expression_analyzer::analyze(
            analyzer,
            &Expression::Variable(selector_var.clone()),
            analysis_data,
            context,
        );
        context.inside_general_use = was_inside_general_use;
    }

    // Try to look up method return type
    if let (Some(class_id), Some(method_name)) = (class_id, method_name) {
        let class_name = analyzer.interner.lookup(class_id);
        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if class_info.kind == ClassLikeKind::Interface
                && matches!(
                    static_call.class.unparenthesized(),
                    Expression::Identifier(_)
                )
            {
                analyze_pending_closure_args_without_context(
                    analyzer,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                );
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    crate::class_casing::undefined_class_message(analyzer, &class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data
                    .expr_types
                    .insert(pos, Rc::new(TUnion::mixed()));
                return;
            }

            if let Some((
                resolved_class_id,
                resolved_type_params,
                method_info,
                allow_non_static_via_magic,
            )) = resolve_named_object_static_method(analyzer, class_info, method_name)
            {
                // While re-analysing a constructor to collect property
                // initialisations, an ancestor `parent::`/`self::`/`static::`
                // call is followed in place (Psalm's
                // `ExistingAtomicStaticCallAnalyzer`, collect_initializations
                // branch) — ungated by visibility, unlike instance calls.
                if context.collect_initializations
                    && crate::init_collector::self_is_or_extends(
                        analyzer.codebase,
                        context,
                        class_id,
                    )
                {
                    crate::init_collector::follow_static_init_call(
                        analyzer,
                        context,
                        resolved_class_id,
                        &method_info,
                    );
                }
                if analyzer.config.find_unused_code {
                    super::atomic_method_call_analyzer::record_method_reference(
                        analyzer,
                        resolved_class_id,
                        method_info.declaring_class,
                        method_name,
                        context,
                        analysis_data,
                    );
                    if context.self_class != Some(class_info.name) {
                        analysis_data.referenced_classes.insert(class_info.name);
                        analysis_data.add_symbol_reference(
                            &context.function_context,
                            class_info.name,
                            false,
                        );
                    }
                }
                let resolved_class_name = analyzer.interner.lookup(resolved_class_id);
                let resolved_class_info = analyzer
                    .codebase
                    .get_class(resolved_class_id)
                    .unwrap_or(class_info);

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
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InternalClass,
                        format!("{} is internal to {}", class_name, scope_phrase),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                let template_result = build_static_method_template_context(
                    analyzer,
                    resolved_class_info,
                    resolved_type_params.as_deref(),
                    analyzer
                        .get_declaring_class()
                        .and_then(|class_id| analyzer.codebase.get_class(class_id)),
                    &method_info,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                );
                analyze_pending_closure_args_for_static_method(
                    analyzer,
                    &args,
                    &arg_positions,
                    &method_info,
                    &template_result,
                    resolved_class_id,
                    class_id,
                    resolved_class_info.parent_class,
                    analysis_data,
                    context,
                );
                // Closure arguments only acquire types in the pending pass
                // above — rebuild the template context so method templates
                // bound from them (`@param callable():T`) resolve, mirroring
                // the named-function path's analyze-then-reinfer ordering.
                let template_result = build_static_method_template_context(
                    analyzer,
                    resolved_class_info,
                    resolved_type_params.as_deref(),
                    analyzer
                        .get_declaring_class()
                        .and_then(|class_id| analyzer.codebase.get_class(class_id)),
                    &method_info,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                );
                verify_method_arguments(
                    analyzer,
                    &args,
                    &arg_positions,
                    &method_info,
                    resolved_class_name.as_ref(),
                    method_name,
                    analysis_data,
                    context,
                    pos,
                    &template_result,
                    resolved_class_id,
                    class_id,
                    resolved_class_info.parent_class,
                );
                crate::expr::call::atomic_method_call_analyzer::apply_post_static_call_assertions(
                    analyzer,
                    analysis_data,
                    &args,
                    &method_info,
                    context,
                    &template_result,
                    resolved_class_id,
                    class_id,
                    resolved_class_info.parent_class,
                );

                // By-ref / @param-out write-backs (same machinery as named
                // function calls — Psalm widens by-ref args for static calls).
                super::arguments_analyzer::apply_param_out_types(
                    analyzer,
                    method_info.name,
                    &method_info.template_types,
                    &args,
                    &arg_positions,
                    &method_info.params,
                    analysis_data,
                    context,
                    &template_result,
                    pos,
                );

                // Check that method is static
                let is_constructor_parent_call = method_name.eq_ignore_ascii_case("__construct")
                    && matches!(
                        static_call.class.unparenthesized(),
                        Expression::Parent(_) | Expression::Self_(_) | Expression::Static(_)
                    );

                let can_call_non_static_in_context =
                    can_call_non_static_via_class_scope(analyzer, class_id, static_call.class);

                if !method_info.is_static
                    && !is_constructor_parent_call
                    && !can_call_non_static_in_context
                    && !allow_non_static_via_magic
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    let issue_kind =
                        if matches!(static_call.class.unparenthesized(), Expression::Self_(_))
                            && analyzer
                                .function_info
                                .is_some_and(|function_info| function_info.is_static)
                        {
                            IssueKind::NonStaticSelfCall
                        } else {
                            IssueKind::InvalidStaticInvocation
                        };

                    analysis_data.add_issue(Issue::new(
                        issue_kind,
                        format!(
                            "Cannot call non-static method {}::{} statically",
                            resolved_class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if method_info.is_abstract
                    && matches!(
                        static_call.class.unparenthesized(),
                        Expression::Identifier(_)
                    )
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::AbstractMethodCall,
                        format!(
                            "Cannot call an abstract static method {}::{} directly",
                            resolved_class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                let visibility_scope_class_id =
                    get_method_visibility_scope_class_id(resolved_class_info, &method_info);

                match method_info.visibility {
                    Visibility::Public => {}
                    Visibility::Private => {
                        let is_same_class =
                            analyzer.get_declaring_class().is_some_and(|calling_class| {
                                calling_class == visibility_scope_class_id
                            });

                        if !is_same_class {
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InaccessibleMethod,
                                format!(
                                    "Cannot access private method {}::{}",
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
                    Visibility::Protected => {
                        let can_access =
                            analyzer.get_declaring_class().is_some_and(|calling_class| {
                                can_access_protected_member_visibility(
                                    analyzer,
                                    calling_class,
                                    visibility_scope_class_id,
                                )
                            });

                        if !can_access {
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

                // Check for deprecated methods
                if method_info.is_deprecated {
                    let message = method_info
                        .deprecation_message
                        .as_ref()
                        .map(|m| {
                            format!(
                                "Method {}::{} is deprecated: {}",
                                resolved_class_name, method_name, m
                            )
                        })
                        .unwrap_or_else(|| {
                            format!(
                                "Method {}::{} is deprecated",
                                resolved_class_name, method_name
                            )
                        });
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedMethod,
                        message,
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if !can_access_internal(analyzer, &method_info.internal, Some(context)) {
                    let scope_phrase =
                        format_internal_scope_phrase(analyzer, &method_info.internal);
                    let caller_phrase = format_caller_context(analyzer, Some(context));
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InternalMethod,
                        format!(
                            "The method {}::{} is internal to {} but called from {}",
                            resolved_class_name, method_name, scope_phrase, caller_phrase
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if enforce_mutation_free
                    && !context.inside_throw
                    && !method_is_mutation_free(&method_info, class_info)
                    && !magic_call_handler_is_mutation_free(analyzer, class_info, method_name)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ImpureMethodCall,
                        format!(
                            "Cannot call a possibly-mutating method {}::{} from a mutation-free context",
                            resolved_class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Return the method's return type
                let method_return_type = crate::return_type_provider::dispatch_method_return_type(
                    &crate::return_type_provider::MethodReturnTypeProviderEvent {
                        analyzer,
                        class_id: resolved_class_id,
                        method_name,
                        args: &args,
                        arg_positions: &arg_positions,
                        analysis_data,
                    },
                )
                .or_else(|| {
                    let param_arg_types =
                        super::function_call_return_type_fetcher::collect_param_arg_types(
                            &method_info.params,
                            &arg_positions,
                            analysis_data,
                        );
                    function_call_analyzer::resolve_functionlike_return_type(
                        analyzer,
                        &method_info,
                        &template_result,
                        &param_arg_types,
                        args.len(),
                    )
                    .or_else(|| {
                        get_inherited_method_return_type(
                            analyzer,
                            resolved_class_id,
                            method_name,
                            &template_result,
                            &param_arg_types,
                            args.len(),
                        )
                    })
                });

                if resolved_class_id == StrId::CLOSURE
                    && method_name.eq_ignore_ascii_case("fromCallable")
                    && let Some(inferred_return_type) = infer_closure_from_callable_return_type(
                        analyzer,
                        &arg_positions,
                        analysis_data,
                    )
                {
                    let inferred_return_type = add_static_call_dataflow(
                        analyzer,
                        analysis_data,
                        resolved_class_id,
                        method_name,
                        Some(&method_info),
                        &arg_positions,
                        pos,
                        inferred_return_type,
                    );
                    analysis_data
                        .expr_types
                        .insert(pos, Rc::new(inferred_return_type));
                    return;
                }

                // A static method without a declared return type still needs
                // call dataflow in whole-program (taint) mode: Psalm taints
                // the call's mixed result so `echo A::rawinput();` flows.
                let method_return_type = method_return_type.or_else(|| {
                    matches!(
                        analysis_data.data_flow_graph.kind,
                        pzoom_code_info::GraphKind::WholeProgram(_)
                    )
                    .then(TUnion::mixed)
                });

                if let Some(resolved_return_type) = method_return_type.as_ref() {
                    let parent_class_id = analyzer
                        .codebase
                        .get_class(resolved_class_id)
                        .and_then(|info| info.parent_class);
                    let static_class_type_name =
                        get_called_class_type_name(analyzer, static_call.class, class_id);
                    // Psalm's ExistingAtomicStaticCallAnalyzer: `static`
                    // binds firmly when the call names a class other than the
                    // enclosing `self`, or the involved class is final.
                    let called_with_keyword = matches!(
                        static_call.class.unparenthesized(),
                        Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_)
                    );
                    let self_class = analyzer.get_declaring_class();
                    let class_is_final = analyzer
                        .codebase
                        .get_class(static_class_type_name)
                        .is_some_and(|info| info.is_final);
                    let function_is_final = if called_with_keyword {
                        class_is_final
                    } else {
                        self_class != Some(static_class_type_name) || class_is_final
                    };
                    let return_type = crate::type_expander::localize_special_class_type_union_final(
                        analyzer.codebase,
                        analyzer.interner,
                        resolved_return_type,
                        resolved_class_id,
                        static_class_type_name,
                        parent_class_id,
                        function_is_final,
                    );
                    let return_type = add_static_call_dataflow(
                        analyzer,
                        analysis_data,
                        // The class named at the call site; the declaring
                        // class's node links in via add_method_call_dataflow.
                        class_id,
                        method_name,
                        Some(&method_info),
                        &arg_positions,
                        pos,
                        return_type,
                    );
                    analysis_data.expr_types.insert(pos, Rc::new(return_type));
                    return;
                }
            } else {
                if matches!(static_call.class.unparenthesized(), Expression::Static(_)) {
                    if let Some(return_type) = resolve_descendant_static_method(
                        analyzer,
                        class_id,
                        method_name,
                        &args,
                        &arg_positions,
                        analysis_data,
                        context,
                        pos,
                    ) {
                        let return_type = add_static_call_dataflow(
                            analyzer,
                            analysis_data,
                            class_id,
                            method_name,
                            None,
                            &arg_positions,
                            pos,
                            return_type,
                        );
                        analysis_data.expr_types.insert(pos, Rc::new(return_type));
                        return;
                    }
                }

                let (line, col) = analyzer.get_line_column(pos.0);

                if is_method_guarded_by_exists(context, method_name) {
                    analyze_pending_closure_args_without_context(
                        analyzer,
                        &args,
                        &arg_positions,
                        analysis_data,
                        context,
                    );
                    analysis_data
                        .expr_types
                        .insert(pos, Rc::new(TUnion::mixed()));
                    return;
                }

                // In an instance context, `parent::`/`self::` calls have
                // non-static semantics, so the instance `__call` handler also
                // applies (Psalm routes these through __call).
                let instance_magic_call = (context.has_this
                    || (context.self_class.is_some() && !analyzer.is_static()))
                    && class_has_magic_call(class_info);

                if class_has_magic_callstatic(class_info) || instance_magic_call {
                    if class_has_sealed_methods(analyzer, class_info) {
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
                        analyze_pending_closure_args_without_context(
                            analyzer,
                            &args,
                            &arg_positions,
                            analysis_data,
                            context,
                        );

                        // Resolve the magic handler's declared return type
                        // (`static` localizes to the calling class).
                        let magic_method_id =
                            if instance_magic_call && !class_has_magic_callstatic(class_info) {
                                StrId::CALL
                            } else {
                                StrId::CALL_STATIC
                            };
                        let magic_return = class_info
                            .methods
                            .get(&magic_method_id)
                            .and_then(|magic_info| {
                                crate::methods::get_method_return_type(
                                    analyzer,
                                    class_info.name,
                                    analyzer.interner.lookup(magic_method_id).as_ref(),
                                    magic_info,
                                    &TemplateResult::default(),
                                    &FxHashMap::default(),
                                    args.len(),
                                )
                                .into()
                            })
                            .map(|magic_return: TUnion| {
                                crate::type_expander::localize_special_class_type_union(
                                    analyzer.codebase,
                                    analyzer.interner,
                                    &magic_return,
                                    context.self_class.unwrap_or(class_info.name),
                                    context.self_class.unwrap_or(class_info.name),
                                    class_info.parent_class,
                                )
                            })
                            .unwrap_or_else(TUnion::mixed);
                        analysis_data.expr_types.insert(pos, Rc::new(magic_return));
                        return;
                    }
                } else {
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedMethod,
                        crate::class_casing::undefined_method_message(
                            analyzer,
                            &class_name,
                            method_name,
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        } else {
            // Class not found
            if !is_class_guarded_by_exists(context, analyzer, class_id)
                && !is_known_class_alias(context, analyzer, class_id)
                && !is_parse_artifact_class_name(class_name.as_ref())
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    crate::class_casing::undefined_class_message(analyzer, &class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    } else if let (Some(class_expr_type), Some(method_name)) = (class_expr_type, method_name) {
        analyze_pending_closure_args_without_context(
            analyzer,
            &args,
            &arg_positions,
            analysis_data,
            context,
        );
        let dynamic_return_type = handle_dynamic_static_call(
            analyzer,
            static_call,
            &class_expr_type,
            method_name,
            pos,
            analysis_data,
            context,
        );
        analysis_data.expr_types.insert(
            pos,
            Rc::new(dynamic_return_type.unwrap_or_else(TUnion::mixed)),
        );
        return;
    }

    analyze_pending_closure_args_without_context(
        analyzer,
        &args,
        &arg_positions,
        analysis_data,
        context,
    );
    // Fall back to mixed
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::mixed()));
}

pub(crate) fn is_closure_like_argument(arg: &Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

pub(crate) fn get_closure_like_argument_offset(arg: &Argument<'_>) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

pub(crate) fn analyze_pending_closure_args_without_context(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for (idx, arg) in args.iter().enumerate() {
        if !is_closure_like_argument(arg) {
            continue;
        }

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        if analysis_data.expr_types.get(&arg_pos).cloned().is_some() {
            continue;
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }
}

pub(crate) fn analyze_pending_closure_args_for_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for (idx, arg) in args.iter().enumerate() {
        let Some(closure_offset) = get_closure_like_argument_offset(arg) else {
            continue;
        };

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        if analysis_data.expr_types.get(&arg_pos).cloned().is_some() {
            continue;
        }

        let param = if idx < method_info.params.len() {
            Some(&method_info.params[idx])
        } else {
            method_info.params.last().filter(|param| param.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
            let replaced_param_type = if crate::template::template_result_is_empty(template_result)
            {
                param_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(param_type, template_result)
            };

            localize_special_class_type_union(
                analyzer.codebase,
                analyzer.interner,
                &replaced_param_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )
        });

        if let Some(expected_param_type) = expected_param_type {
            if callable_validation::union_has_callable(&expected_param_type) {
                context
                    .expected_callable_arg_types
                    .insert(closure_offset, expected_param_type);
            }
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        context.expected_callable_arg_types.remove(&closure_offset);
    }
}

fn get_self_static_or_parent_keyword(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<&'static str> {
    match expr.unparenthesized() {
        Expression::Self_(_) => Some("self"),
        Expression::Static(_) => Some("static"),
        Expression::Parent(_) => Some("parent"),
        Expression::Identifier(id) => {
            let value = pzoom_syntax::bytes_to_str(id.value());
            if value.eq_ignore_ascii_case("self") {
                return Some("self");
            }
            if value.eq_ignore_ascii_case("static") {
                return Some("static");
            }
            if value.eq_ignore_ascii_case("parent") {
                return Some("parent");
            }

            let span = id.span();
            let source_value = analyzer
                .get_source_substring(span.start.offset as usize, span.end.offset as usize)
                .trim();
            if source_value.eq_ignore_ascii_case("self") {
                Some("self")
            } else if source_value.eq_ignore_ascii_case("static") {
                Some("static")
            } else if source_value.eq_ignore_ascii_case("parent") {
                Some("parent")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Get the resolved class ID from an expression using resolved_names.
pub(crate) fn get_resolved_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    let class_id = match expr {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer
                .get_resolved_name(offset)
                // Names inside subtrees the resolver does not visit (e.g.
                // partial applications) fall back to the literal text, with
                // any fully-qualified leading backslash stripped.
                .or_else(|| {
                    Some(
                        analyzer
                            .interner
                            .find(pzoom_syntax::bytes_to_str(id.value()).trim_start_matches('\\'))
                            .unwrap_or(pzoom_str::StrId::EMPTY),
                    )
                })
        }
        Expression::Self_(_) => analyzer.get_declaring_class(),
        Expression::Static(_) => {
            let static_key = VarName::new("@static");
            if let Some(static_type) = context.locals.get(&static_key) {
                if static_type.is_single() {
                    if let Some(TAtomic::TNamedObject { name, .. }) = static_type.get_single() {
                        Some(*name)
                    } else {
                        analyzer.get_declaring_class()
                    }
                } else {
                    analyzer.get_declaring_class()
                }
            } else {
                analyzer.get_declaring_class()
            }
        }
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }?;

    Some(
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .filter(|alias_target| analyzer.codebase.get_class(*alias_target).is_some())
            .unwrap_or(class_id),
    )
}

/// Get the method name from a method selector.
fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(pzoom_syntax::bytes_to_str(id.value)),
        _ => None,
    }
}

pub(crate) fn is_class_guarded_by_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> bool {
    let class_name = analyzer.interner.lookup(class_id);
    let key = format!(
        "@class_exists({})",
        class_name.trim_start_matches('\\').to_ascii_lowercase()
    );
    let key_id = VarName::new(&key);

    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

pub(crate) fn is_known_class_alias(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> bool {
    let class_name = analyzer
        .interner
        .lookup(class_id)
        .trim_start_matches('\\')
        .to_ascii_lowercase();

    context.class_aliases.keys().any(|alias_id| {
        analyzer
            .interner
            .lookup(*alias_id)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case(class_name.as_str())
    })
}

pub(crate) fn is_parse_artifact_class_name(class_name: &str) -> bool {
    class_name.contains(':') && !class_name.contains("::")
}

pub(crate) fn is_method_guarded_by_exists(context: &BlockContext, method_name: &str) -> bool {
    let method_name = method_name.to_ascii_lowercase();
    let suffix = format!(",{})", method_name);

    context.locals.iter().any(|(key_id, guard_type)| {
        if guard_type.is_nothing() || guard_type.is_always_falsy() {
            return false;
        }

        let key = key_id.as_str();
        key.starts_with("@method_exists(") && key.ends_with(&suffix)
    })
}

pub(crate) fn is_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    // One definition of "enforcing purity context" — see
    // method_call_analyzer::is_mutation_free_context for Psalm's semantics
    // (a method's own @mutation-free is NOT body-enforced).
    crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer)
}

/// Best-effort early resolution of a static call's method storage (class
/// hierarchy walk) so by-ref out-params can be predeclared before argument
/// analysis. Full resolution with diagnostics happens later in the call flow.
fn pre_resolve_static_method_info<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let ClassLikeMemberSelector::Identifier(method_identifier) = &static_call.method else {
        return None;
    };

    let class_id =
        match static_call.class.unparenthesized() {
            Expression::Identifier(id) => analyzer
                .get_resolved_name(id.span().start.offset)
                .or_else(|| {
                    Some(
                        analyzer
                            .interner
                            .find(pzoom_syntax::bytes_to_str(id.value()))
                            .unwrap_or(pzoom_str::StrId::EMPTY),
                    )
                })?,
            Expression::Self_(_) | Expression::Static(_) => {
                analyzer.get_declaring_class().or(context.self_class)?
            }
            Expression::Parent(_) => analyzer
                .get_declaring_class()
                .or(context.self_class)
                .and_then(|class_id| analyzer.codebase.get_class(class_id)?.parent_class)?,
            _ => return None,
        };

    let method_id = analyzer
        .interner
        .find(pzoom_syntax::bytes_to_str(method_identifier.value))
        .unwrap_or(pzoom_str::StrId::EMPTY);
    let mut current = Some(class_id);
    while let Some(current_id) = current {
        let class_info = analyzer.codebase.get_class(current_id)?;
        if let Some(method_info) = class_info.methods.get(&method_id) {
            return Some(method_info);
        }
        current = class_info.parent_class;
    }

    None
}
