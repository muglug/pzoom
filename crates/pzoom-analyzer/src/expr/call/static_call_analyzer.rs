//! Static method call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::call::StaticMethodCall;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{
    can_access_internal, format_caller_context, format_internal_scope_phrase,
};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

use super::{
    argument_analyzer, arguments_analyzer, callable_validation, function_call_analyzer,
    method_call_return_type_fetcher,
};

/// Analyze a static method call expression (Foo::bar()).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the class expression
    let class_pos =
        expression_analyzer::analyze(analyzer, static_call.class, analysis_data, context);
    let class_expr_type = analysis_data.get_expr_type(class_pos);

    // Analyze arguments and collect positions
    let args: Vec<_> = static_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();
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
        analysis_data.set_expr_type(pos, TUnion::mixed());
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
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    }

    // Try to get the class name using resolved names
    let class_id = get_resolved_class_id(analyzer, static_call.class, context);

    // Get the method name
    let method_name = get_method_name(&static_call.method);

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
                    format!("Class {} does not exist", class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, TUnion::mixed());
                return;
            }

            if let Some((
                resolved_class_id,
                resolved_type_params,
                method_info,
                allow_non_static_via_magic,
            )) = resolve_named_object_static_method(analyzer, class_info, method_name)
            {
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

                let (template_defaults, template_replacements) =
                    build_static_method_template_context(
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
                    &template_defaults,
                    &template_replacements,
                    resolved_class_id,
                    class_id,
                    resolved_class_info.parent_class,
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
                    &template_defaults,
                    &template_replacements,
                    resolved_class_id,
                    class_id,
                    resolved_class_info.parent_class,
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

                if enforce_mutation_free && !method_is_mutation_free(&method_info, class_info) {
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
                let method_return_type = method_call_return_type_fetcher::fetch(
                    analyzer,
                    resolved_class_id,
                    method_name,
                )
                .or_else(|| {
                    function_call_analyzer::resolve_functionlike_return_type(
                        analyzer,
                        &method_info,
                        &template_defaults,
                        &template_replacements,
                        args.len(),
                    )
                })
                .or_else(|| {
                    get_inherited_method_return_type(
                        analyzer,
                        resolved_class_id,
                        method_name,
                        &template_defaults,
                        &template_replacements,
                        args.len(),
                    )
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
                        class_expr_type.as_deref(),
                        &arg_positions,
                        pos,
                        inferred_return_type,
                    );
                    analysis_data.set_expr_type(pos, inferred_return_type);
                    return;
                }

                if let Some(resolved_return_type) = method_return_type.as_ref() {
                    let parent_class_id = analyzer
                        .codebase
                        .get_class(resolved_class_id)
                        .and_then(|info| info.parent_class);
                    let static_class_type_name =
                        get_called_class_type_name(analyzer, static_call.class, class_id);
                    let return_type = localize_special_class_type_union(
                        resolved_return_type,
                        resolved_class_id,
                        static_class_type_name,
                        parent_class_id,
                    );
                    let return_type = add_static_call_dataflow(
                        analyzer,
                        analysis_data,
                        resolved_class_id,
                        method_name,
                        class_expr_type.as_deref(),
                        &arg_positions,
                        pos,
                        return_type,
                    );
                    analysis_data.set_expr_type(pos, return_type);
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
                            class_expr_type.as_deref(),
                            &arg_positions,
                            pos,
                            return_type,
                        );
                        analysis_data.set_expr_type(pos, return_type);
                        return;
                    }
                }

                let (line, col) = analyzer.get_line_column(pos.0);

                if is_method_guarded_by_exists(context, analyzer, method_name) {
                    analyze_pending_closure_args_without_context(
                        analyzer,
                        &args,
                        &arg_positions,
                        analysis_data,
                        context,
                    );
                    analysis_data.set_expr_type(pos, TUnion::mixed());
                    return;
                }

                if class_has_magic_callstatic(class_info) {
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
                        analyze_pending_closure_args_without_context(
                            analyzer,
                            &args,
                            &arg_positions,
                            analysis_data,
                            context,
                        );
                        analysis_data.set_expr_type(pos, TUnion::mixed());
                        return;
                    }
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
        } else {
            // Class not found
            if !is_class_guarded_by_exists(context, analyzer, class_id)
                && !is_known_class_alias(context, analyzer, class_id)
                && !is_parse_artifact_class_name(class_name.as_ref())
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    format!("Class {} does not exist", class_name),
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
        analysis_data.set_expr_type(pos, dynamic_return_type.unwrap_or_else(TUnion::mixed));
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
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

fn add_static_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    class_id: StrId,
    method_name: &str,
    class_expr_type: Option<&TUnion>,
    arg_positions: &[Pos],
    pos: Pos,
    mut return_type: TUnion,
) -> TUnion {
    let call_node = DataFlowNode::get_for_call(
        FunctionLikeIdentifier::Method(class_id, analyzer.interner.intern(method_name)),
        make_data_flow_node_position(analyzer, pos),
    );
    analysis_data.data_flow_graph.add_node(call_node.clone());

    if let Some(class_expr_type) = class_expr_type {
        add_default_dataflow_paths(
            &mut analysis_data.data_flow_graph,
            &class_expr_type.parent_nodes,
            &call_node,
        );
    }

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
        &mut return_type.parent_nodes,
        vec![call_node],
    );
    return_type
}

fn can_call_non_static_via_class_scope(
    analyzer: &StatementsAnalyzer<'_>,
    called_class: StrId,
    class_expr: &Expression<'_>,
) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    if function_info.is_static || function_info.declaring_class.is_none() {
        return false;
    }

    if matches!(
        class_expr.unparenthesized(),
        Expression::Parent(_) | Expression::Self_(_) | Expression::Static(_)
    ) {
        return true;
    }

    let Some(calling_class) = function_info.declaring_class else {
        return false;
    };

    if calling_class == called_class {
        return true;
    }

    analyzer
        .codebase
        .get_class(calling_class)
        .is_some_and(|class_info| class_info.all_parent_classes.contains(&called_class))
}

fn resolve_named_object_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
    bool,
)> {
    if let Some(method_info) = get_method_info_case_insensitive(analyzer, class_info, method_name) {
        return Some((class_info.name, None, method_info.clone(), false));
    }

    if class_info.kind == ClassLikeKind::Interface
        || class_has_magic_callstatic(class_info)
        || class_has_magic_call(class_info)
    {
        if let Some(method_info) =
            get_pseudo_static_method_info_case_insensitive(analyzer, class_info, method_name)
        {
            return Some((class_info.name, None, method_info.clone(), false));
        }

        if let Some(method_info) =
            get_pseudo_method_info_case_insensitive(analyzer, class_info, method_name)
        {
            return Some((class_info.name, None, method_info.clone(), false));
        }
    }

    resolve_named_mixin_static_method(analyzer, class_info, method_name).map(
        |(resolved_class_id, resolved_type_params, method_info)| {
            (
                resolved_class_id,
                resolved_type_params,
                method_info,
                class_has_magic_callstatic(class_info),
            )
        },
    )
}

fn resolve_named_mixin_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
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
    let class_template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);

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
            } = localized_atomic
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

            if let Some(method_info) = get_pseudo_static_method_info_case_insensitive(
                analyzer,
                mixin_class_info,
                method_name,
            ) {
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

fn resolve_descendant_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    call_pos: Pos,
) -> Option<TUnion> {
    let descendants = analyzer.codebase.all_classlike_descendants.get(&class_id)?;
    let mut combined_return_type: Option<TUnion> = None;
    let mut found = false;

    for descendant_id in descendants {
        let Some(descendant_info) = analyzer.codebase.get_class(*descendant_id) else {
            continue;
        };
        let Some(method_info) =
            get_method_info_case_insensitive(analyzer, descendant_info, method_name)
        else {
            continue;
        };
        if !method_info.is_static {
            continue;
        }

        found = true;
        let descendant_name = analyzer.interner.lookup(*descendant_id);
        let (template_defaults, template_replacements) = build_static_method_template_context(
            analyzer,
            descendant_info,
            None,
            analyzer
                .get_declaring_class()
                .and_then(|class_id| analyzer.codebase.get_class(class_id)),
            method_info,
            args,
            arg_positions,
            analysis_data,
            context,
        );
        analyze_pending_closure_args_for_static_method(
            analyzer,
            args,
            arg_positions,
            method_info,
            &template_defaults,
            &template_replacements,
            *descendant_id,
            class_id,
            descendant_info.parent_class,
            analysis_data,
            context,
        );
        verify_method_arguments(
            analyzer,
            args,
            arg_positions,
            method_info,
            descendant_name.as_ref(),
            method_name,
            analysis_data,
            context,
            call_pos,
            &template_defaults,
            &template_replacements,
            *descendant_id,
            class_id,
            descendant_info.parent_class,
        );

        let return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            method_info,
            &template_defaults,
            &template_replacements,
            args.len(),
        )
        .unwrap_or_else(TUnion::mixed);
        combined_return_type = Some(if let Some(existing) = combined_return_type {
            combine_union_types(&existing, &return_type, false)
        } else {
            return_type
        });
    }

    if found {
        Some(combined_return_type.unwrap_or_else(TUnion::mixed))
    } else {
        None
    }
}

fn get_method_info_case_insensitive<'a>(
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

fn get_pseudo_method_info_case_insensitive<'a>(
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

fn get_pseudo_static_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.pseudo_static_methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .pseudo_static_methods
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

fn class_has_magic_callstatic(class_info: &ClassLikeInfo) -> bool {
    class_info
        .methods
        .contains_key(&pzoom_str::StrId::CALL_STATIC)
}

fn class_has_magic_call(class_info: &ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::CALL)
}

fn is_closure_like_argument(arg: &Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

fn get_closure_like_argument_offset(arg: &Argument<'_>) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn analyze_pending_closure_args_without_context(
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
        if analysis_data.get_expr_type(arg_pos).is_some() {
            continue;
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }
}

fn analyze_pending_closure_args_for_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
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
        if analysis_data.get_expr_type(arg_pos).is_some() {
            continue;
        }

        let param = if idx < method_info.params.len() {
            Some(&method_info.params[idx])
        } else {
            method_info.params.last().filter(|param| param.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
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

            localize_special_class_type_union(
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

fn class_has_sealed_methods(class_info: &ClassLikeInfo) -> bool {
    class_info.sealed_methods.unwrap_or(false)
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
            let value = id.value();
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

fn get_method_visibility_scope_class_id(
    class_info: &ClassLikeInfo,
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> StrId {
    class_info
        .appearing_method_ids
        .get(&method_info.name)
        .copied()
        .or(method_info.declaring_class)
        .unwrap_or(class_info.name)
}

fn can_access_protected_member_visibility(
    analyzer: &StatementsAnalyzer<'_>,
    caller_class: StrId,
    visibility_scope_class: StrId,
) -> bool {
    caller_class == visibility_scope_class
        || object_type_comparator::is_class_subtype_of(
            caller_class,
            visibility_scope_class,
            analyzer.codebase,
        )
        || object_type_comparator::is_class_subtype_of(
            visibility_scope_class,
            caller_class,
            analyzer.codebase,
        )
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    let class_id = match expr {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer
                .get_resolved_name(offset)
                .or_else(|| Some(analyzer.interner.intern(id.value())))
        }
        Expression::Self_(_) => analyzer.get_declaring_class(),
        Expression::Static(_) => {
            let static_key = analyzer.interner.intern("@static");
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

fn get_called_class_type_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    resolved_class_id: StrId,
) -> StrId {
    match expr.unparenthesized() {
        Expression::Self_(_) => StrId::STATIC,
        Expression::Static(_) => StrId::STATIC,
        Expression::Parent(_) => StrId::PARENT,
        Expression::Identifier(id) => {
            let value = id.value();
            if value.eq_ignore_ascii_case("self") {
                StrId::STATIC
            } else if value.eq_ignore_ascii_case("static") {
                StrId::STATIC
            } else if value.eq_ignore_ascii_case("parent") {
                StrId::PARENT
            } else {
                let span = id.span();
                let source_value = analyzer
                    .get_source_substring(span.start.offset as usize, span.end.offset as usize)
                    .trim();
                if source_value.eq_ignore_ascii_case("self") {
                    StrId::STATIC
                } else if source_value.eq_ignore_ascii_case("static") {
                    StrId::STATIC
                } else if source_value.eq_ignore_ascii_case("parent") {
                    StrId::PARENT
                } else {
                    resolved_class_id
                }
            }
        }
        _ => resolved_class_id,
    }
}

/// Get the method name from a method selector.
fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn build_static_method_template_context(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    called_type_params: Option<&[TUnion]>,
    invoking_class_info: Option<&pzoom_code_info::ClassLikeInfo>,
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
            called_type_params,
        ),
    );

    if let Some(invoking_class_info) = invoking_class_info {
        if let Some(invoking_template_map) = invoking_class_info
            .template_extended_params
            .get(&class_info.name)
        {
            function_call_analyzer::overlay_template_replacements(
                &mut template_replacements,
                invoking_template_map.clone(),
            );
        }
    }

    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_template_replacements_from_args(
            analyzer,
            args,
            arg_positions,
            &method_info.params,
            &template_defaults,
            analysis_data,
            context,
        ),
    );

    (template_defaults, template_replacements)
}

fn method_is_mutation_free(
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    method_info.is_pure
        || method_info.is_mutation_free
        || (class_info.is_immutable && !method_info.is_static)
}

fn is_class_guarded_by_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> bool {
    let class_name = analyzer.interner.lookup(class_id);
    let key = format!(
        "@class_exists({})",
        class_name.trim_start_matches('\\').to_ascii_lowercase()
    );
    let key_id = analyzer.interner.intern(&key);

    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

fn is_known_class_alias(
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

fn is_parse_artifact_class_name(class_name: &str) -> bool {
    class_name.contains(':') && !class_name.contains("::")
}

fn is_method_guarded_by_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    method_name: &str,
) -> bool {
    let method_name = method_name.to_ascii_lowercase();
    let suffix = format!(",{})", method_name);

    context.locals.iter().any(|(key_id, guard_type)| {
        if guard_type.is_nothing() || guard_type.is_always_falsy() {
            return false;
        }

        let key = analyzer.interner.lookup(*key_id);
        key.starts_with("@method_exists(") && key.ends_with(&suffix)
    })
}

fn handle_dynamic_static_call(
    analyzer: &StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    class_expr_type: &TUnion,
    method_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut emitted_invalid_string_class = false;
    let mut emitted_invalid_static_invocation = false;
    let mut emitted_undefined_class = false;
    let mut emitted_mixed_method_call = false;
    let mut emitted_undefined_method = false;
    let mut combined_return_type: Option<TUnion> = None;

    let mut flattened_targets = Vec::new();
    for atomic in &class_expr_type.types {
        collect_dynamic_static_call_target_atomics(analyzer, atomic, &mut flattened_targets, false);
    }

    for atomic in &flattened_targets {
        match atomic {
            TAtomic::TString | TAtomic::TLiteralString { .. } => {
                if emitted_invalid_string_class {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidStringClass,
                    "String cannot be used as a class",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_invalid_string_class = true;
            }
            TAtomic::TNamedObject { name, .. } => {
                let Some(class_info) = analyzer.codebase.get_class(*name) else {
                    if emitted_undefined_class {
                        continue;
                    }

                    if is_parse_artifact_class_name(analyzer.interner.lookup(*name).as_ref()) {
                        continue;
                    }

                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedClass,
                        format!("Class {} does not exist", analyzer.interner.lookup(*name)),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_class = true;
                    continue;
                };

                if let Some((resolved_class_id, _, method_info, allow_non_static_via_magic)) =
                    resolve_named_object_static_method(analyzer, class_info, method_name)
                {
                    let resolved_return_type = method_info
                        .return_type
                        .as_ref()
                        .or(method_info.signature_return_type.as_ref())
                        .cloned()
                        .unwrap_or_else(TUnion::mixed);
                    let parent_class_id = analyzer
                        .codebase
                        .get_class(resolved_class_id)
                        .and_then(|resolved_info| resolved_info.parent_class);
                    let localized_return_type = localize_special_class_type_union(
                        &resolved_return_type,
                        resolved_class_id,
                        resolved_class_id,
                        parent_class_id,
                    );

                    combined_return_type = Some(if let Some(existing) = combined_return_type {
                        combine_union_types(&existing, &localized_return_type, false)
                    } else {
                        localized_return_type
                    });

                    if !method_info.is_static && !allow_non_static_via_magic {
                        if emitted_invalid_static_invocation {
                            continue;
                        }

                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidStaticInvocation,
                            format!(
                                "Cannot call non-static method {}::{} statically",
                                analyzer.interner.lookup(resolved_class_id),
                                method_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                        emitted_invalid_static_invocation = true;
                    }
                } else if !emitted_undefined_method
                    && !is_method_guarded_by_exists(context, analyzer, method_name)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedMethod,
                        format!(
                            "Method {}::{} does not exist",
                            analyzer.interner.lookup(*name),
                            method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_method = true;
                }
            }
            TAtomic::TObject => {
                if emitted_invalid_static_invocation {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidStaticInvocation,
                    format!("Cannot call non-static method {} statically", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_invalid_static_invocation = true;
            }
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_found_method = false;

                for intersection_type in types {
                    let TAtomic::TNamedObject { name, .. } = intersection_type else {
                        continue;
                    };

                    let Some(class_info) = analyzer.codebase.get_class(*name) else {
                        continue;
                    };

                    if let Some((resolved_class_id, _, method_info, allow_non_static_via_magic)) =
                        resolve_named_object_static_method(analyzer, class_info, method_name)
                    {
                        intersection_found_method = true;

                        let resolved_return_type = method_info
                            .return_type
                            .as_ref()
                            .or(method_info.signature_return_type.as_ref())
                            .cloned()
                            .unwrap_or_else(TUnion::mixed);
                        let parent_class_id = analyzer
                            .codebase
                            .get_class(resolved_class_id)
                            .and_then(|resolved_info| resolved_info.parent_class);
                        let localized_return_type = localize_special_class_type_union(
                            &resolved_return_type,
                            resolved_class_id,
                            resolved_class_id,
                            parent_class_id,
                        );

                        combined_return_type = Some(if let Some(existing) = combined_return_type {
                            combine_union_types(&existing, &localized_return_type, false)
                        } else {
                            localized_return_type
                        });

                        if !method_info.is_static && !allow_non_static_via_magic {
                            if emitted_invalid_static_invocation {
                                continue;
                            }

                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidStaticInvocation,
                                format!(
                                    "Cannot call non-static method {}::{} statically",
                                    analyzer.interner.lookup(resolved_class_id),
                                    method_name
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                            emitted_invalid_static_invocation = true;
                        }
                    }
                }

                if !intersection_found_method
                    && !emitted_undefined_method
                    && !is_method_guarded_by_exists(context, analyzer, method_name)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedMethod,
                        format!("Method {} does not exist", method_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                    emitted_undefined_method = true;
                }
            }
            TAtomic::TClassString { .. } => {
                // `class-string` can represent a valid runtime class, but may not
                // have enough information to resolve a concrete method target here.
                continue;
            }
            TAtomic::TMixed => {
                if emitted_mixed_method_call {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedMethodCall,
                    "Cannot call method on an unknown class",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_mixed_method_call = true;
            }
            _ => {
                if emitted_undefined_class {
                    continue;
                }

                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    format!(
                        "Type {} cannot be called as a class",
                        atomic.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                emitted_undefined_class = true;
            }
        }
    }

    if static_call.argument_list.arguments.is_empty() && emitted_invalid_string_class {
        // Preserve Psalm behavior where string static calls degrade result to mixed,
        // which can trigger downstream MixedAssignment.
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedAssignment,
            "Unable to determine return type of dynamically-invoked static method",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    combined_return_type
}

fn collect_dynamic_static_call_target_atomics(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    output: &mut Vec<TAtomic>,
    from_class_string_context: bool,
) {
    match atomic {
        TAtomic::TTemplateParam { as_type, .. } => {
            if from_class_string_context
                && (as_type.is_mixed()
                    || as_type.types.iter().all(|nested| {
                        matches!(
                            nested,
                            TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
                        )
                    }))
            {
                push_unique_dynamic_static_target(output, TAtomic::TClassString { as_type: None });
                return;
            }

            for nested in &as_type.types {
                collect_dynamic_static_call_target_atomics(
                    analyzer,
                    nested,
                    output,
                    from_class_string_context,
                );
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            if from_class_string_context
                && matches!(
                    as_type.as_ref(),
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
                )
            {
                push_unique_dynamic_static_target(output, TAtomic::TClassString { as_type: None });
                return;
            }

            collect_dynamic_static_call_target_atomics(
                analyzer,
                as_type.as_ref(),
                output,
                from_class_string_context,
            );
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            collect_dynamic_static_call_target_atomics(analyzer, as_type.as_ref(), output, true);
        }
        TAtomic::TLiteralClassString { name } => {
            push_unique_dynamic_static_target(
                output,
                TAtomic::TNamedObject {
                    name: analyzer.interner.intern(name.trim_start_matches('\\')),
                    type_params: None,
                },
            );
        }
        TAtomic::TObjectIntersection { types } => {
            if from_class_string_context {
                push_unique_dynamic_static_target(output, atomic.clone());
            } else {
                let start_len = output.len();
                for nested in types {
                    collect_dynamic_static_call_target_atomics(
                        analyzer,
                        nested,
                        output,
                        from_class_string_context,
                    );
                }

                if output.len() == start_len {
                    push_unique_dynamic_static_target(output, atomic.clone());
                }
            }
        }
        _ => push_unique_dynamic_static_target(output, atomic.clone()),
    }
}

fn push_unique_dynamic_static_target(output: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !output.contains(&atomic) {
        output.push(atomic);
    }
}

fn is_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    if function_info.is_pure || function_info.is_mutation_free {
        return true;
    }

    if function_info.is_static {
        return false;
    }

    if let Some(class_id) = function_info.declaring_class {
        return analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| class_info.is_immutable);
    }

    false
}

fn localize_special_class_type_union(
    union: &TUnion,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let localized_atomic = localize_special_class_type_atomic(
            atomic,
            self_class_id,
            static_class_id,
            parent_class_id,
        );

        if !localized.contains(&localized_atomic) {
            localized.push(localized_atomic);
        }
    }

    let mut localized_union = union.clone();
    localized_union.types = localized;
    localized_union.is_nullable = localized_union.types.iter().any(|t| t.is_nullable());
    localized_union.is_falsable = localized_union.types.iter().any(|t| t.is_falsable());
    localized_union
}

fn localize_special_class_type_atomic(
    atomic: &TAtomic,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            let localized_name = if *name == StrId::SELF {
                self_class_id
            } else if *name == StrId::STATIC {
                static_class_id
            } else if *name == StrId::PARENT {
                parent_class_id.unwrap_or(StrId::PARENT)
            } else {
                *name
            };

            TAtomic::TNamedObject {
                name: localized_name,
                type_params: type_params.as_ref().map(|params| {
                    params
                        .iter()
                        .map(|param| {
                            localize_special_class_type_union(
                                param,
                                self_class_id,
                                static_class_id,
                                parent_class_id,
                            )
                        })
                        .collect()
                }),
            }
        }
        TAtomic::TObjectIntersection { types } => {
            let mut localized = Vec::with_capacity(types.len());
            for nested in types {
                let localized_nested = localize_special_class_type_atomic(
                    nested,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                );
                if !localized.contains(&localized_nested) {
                    localized.push(localized_nested);
                }
            }

            TAtomic::TObjectIntersection { types: localized }
        }
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: localize_special_class_type_union(
                            &param.param_type,
                            self_class_id,
                            static_class_id,
                            parent_class_id,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(localize_special_class_type_union(
                    return_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: localize_special_class_type_union(
                            &param.param_type,
                            self_class_id,
                            static_class_id,
                            parent_class_id,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(localize_special_class_type_union(
                    return_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_type_union(
                as_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParamClass {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_type_atomic(
                as_type,
                self_class_id,
                static_class_id,
                parent_class_id,
            )),
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|as_type| {
                Box::new(localize_special_class_type_atomic(
                    as_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ))
            }),
        },
        _ => atomic.clone(),
    }
}

fn infer_closure_from_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let callback_pos = *arg_positions.first()?;
    let callback_type = analysis_data.get_expr_type(callback_pos)?;
    let mut closure_types = Vec::new();

    for atomic in &callback_type.types {
        collect_typed_closure_from_callable_atomic(analyzer, atomic, &mut closure_types);
    }

    if closure_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(closure_types))
    }
}

fn collect_typed_closure_from_callable_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    closure_types: &mut Vec<TAtomic>,
) {
    match atomic {
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        }
        | TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => {
            closure_types.push(TAtomic::TClosure {
                params: params.clone(),
                return_type: return_type.clone(),
                is_pure: *is_pure,
            });
        }
        TAtomic::TNamedObject { name, .. } => {
            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                return;
            };
            let Some(invoke_method) = class_info.methods.get(&StrId::INVOKE) else {
                return;
            };

            if invoke_method.visibility != Visibility::Public {
                return;
            }

            closure_types.push(functionlike_to_typed_closure(invoke_method));
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested_atomic in &as_type.types {
                collect_typed_closure_from_callable_atomic(analyzer, nested_atomic, closure_types);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                collect_typed_closure_from_callable_atomic(analyzer, nested_atomic, closure_types);
            }
        }
        _ => {}
    }
}

fn functionlike_to_typed_closure(function_info: &pzoom_code_info::FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| pzoom_code_info::FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TClosure {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure),
    }
}

fn get_inherited_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }

    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();
    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        if candidate_method_info.return_type.is_none() {
            continue;
        }

        let mut candidate_defaults = template_defaults.clone();
        for (template_name, template_default) in
            function_call_analyzer::get_class_template_defaults(candidate_class_info)
        {
            candidate_defaults
                .entry(template_name)
                .or_insert(template_default);
        }
        for (template_name, template_default) in
            function_call_analyzer::get_template_defaults(candidate_method_info)
        {
            candidate_defaults
                .entry(template_name)
                .or_insert(template_default);
        }

        let mut candidate_replacements = template_replacements.clone();
        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            function_call_analyzer::overlay_template_replacements(
                &mut candidate_replacements,
                candidate_template_map.clone(),
            );
        }

        let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            candidate_method_info,
            &candidate_defaults,
            &candidate_replacements,
            arg_count,
        )
        .unwrap_or_else(TUnion::mixed);

        return Some(resolved_return_type);
    }

    None
}

fn verify_method_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_pos: Pos,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
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
            .unwrap_or(call_pos);
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
                callable_validation::verify_unpacked_argument(
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
            if let Some(param_type) = param.get_type() {
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

                effective_param.param_type = Some(localize_special_class_type_union(
                    &replaced_param_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ));
            }

            callable_validation::verify_argument_type(
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
