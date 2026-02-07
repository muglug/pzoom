//! New (object instantiation) analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::instantiation::Instantiation;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

use super::{argument_analyzer, arguments_analyzer, callable_validation, function_call_analyzer};

/// Analyze a new expression (object instantiation).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    instantiation: &Instantiation<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let class_pos =
        expression_analyzer::analyze(analyzer, instantiation.class, analysis_data, context);
    let mut class_expr_type = analysis_data.get_expr_type(class_pos).map(|t| (*t).clone());

    // Try to get the resolved class ID
    let requested_class_id = get_resolved_class_id(analyzer, instantiation.class).map(|class_id| {
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .unwrap_or(class_id)
    });
    let is_static_class_reference =
        matches!(instantiation.class.unparenthesized(), Expression::Static(_));
    let mut concrete_class_id = requested_class_id;
    let mut class_case_mismatch = false;
    if let Some(class_id) = concrete_class_id
        && analyzer.codebase.get_class(class_id).is_none()
        && let Some(actual_class_id) =
            find_class_case_insensitive(analyzer, analyzer.interner.lookup(class_id).as_ref())
    {
        class_case_mismatch = actual_class_id != class_id;
        concrete_class_id = Some(actual_class_id);
    }

    // Dynamic class expressions may bypass normal variable-fetch typing. Prefer the
    // scoped variable type when available.
    if class_expr_type.as_ref().map_or(true, |t| t.is_mixed()) {
        if let Some(var_id) = get_dynamic_class_var_id(analyzer, instantiation.class) {
            if let Some(var_type) = context.get_var_type(var_id) {
                class_expr_type = Some(var_type.clone());
            }
        }
    }

    if concrete_class_id.is_none() {
        concrete_class_id = infer_concrete_class_id_from_class_expr_type(
            analyzer,
            class_expr_type.as_ref(),
            context,
        );
    }

    let classlike_name = concrete_class_id.map(|id| analyzer.interner.lookup(id));
    let class_is_known =
        concrete_class_id.is_some_and(|class_id| analyzer.codebase.get_class(class_id).is_some());
    let suppress_arg_undefined_checks = !class_is_known;

    if let Some(var_id) = get_dynamic_class_var_id(analyzer, instantiation.class) {
        if let Some(var_type) = context.get_var_type(var_id) {
            if !is_dynamic_instantiable_union(var_type) && !var_type.is_mixed() {
                let issue_kind = if var_type.has_object() {
                    IssueKind::MixedMethodCall
                } else {
                    IssueKind::UndefinedClass
                };
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    issue_kind,
                    format!(
                        "Type {} cannot be called as a class",
                        var_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TObject));
                return;
            }

            if analyzer.function_info.is_none()
                && context.is_assigned(var_id)
                && var_type.is_mixed()
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    "Type mixed cannot be called as a class",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TObject));
                return;
            }
        }
    }

    // Analyze constructor arguments and collect positions.
    let mut arg_positions = Vec::new();
    let previous_check_variables = context.check_variables;
    if suppress_arg_undefined_checks {
        context.check_variables = false;
    }
    if let Some(ref args) = instantiation.argument_list {
        for arg in args.arguments.iter() {
            let arg_span = arg.span();
            let arg_pos = (arg_span.start.offset, arg_span.end.offset);

            if is_closure_like_argument(arg) {
                arg_positions.push(arg_pos);
                continue;
            }

            let arg_pos = argument_analyzer::analyze(analyzer, arg, analysis_data, context);
            arg_positions.push(arg_pos);
        }
    }
    context.check_variables = previous_check_variables;

    // Create the result type
    if let (Some(concrete_class_id), Some(class_name)) = (concrete_class_id, classlike_name) {
        let mut inferred_type_params = None;

        // Check if the class exists
        if let Some(class_info) = analyzer.codebase.get_class(concrete_class_id) {
            if class_case_mismatch {
                if let Some(requested_class_id) = requested_class_id {
                    let requested = analyzer.interner.lookup(requested_class_id);
                    let actual = analyzer.interner.lookup(concrete_class_id);
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidClass,
                        format!(
                            "Class {} has incorrect casing, expected {}",
                            requested, actual
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }

            if class_info.kind == ClassLikeKind::Enum {
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
            } else if class_info.kind == ClassLikeKind::Interface {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InterfaceInstantiation,
                    format!("Cannot instantiate interface {}", class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            } else if class_info.is_abstract && !is_static_class_reference {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::AbstractInstantiation,
                    format!("Cannot instantiate abstract class {}", class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if class_info.is_deprecated
                && analyzer
                    .get_declaring_class()
                    .is_none_or(|declaring_class| declaring_class != concrete_class_id)
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

            if let Some(construct_info) = class_info.methods.get(&StrId::CONSTRUCT) {
                analyze_pending_closure_args_for_constructor(
                    analyzer,
                    instantiation,
                    &arg_positions,
                    class_info,
                    construct_info,
                    analysis_data,
                    context,
                );

                verify_constructor_arguments(
                    analyzer,
                    class_info,
                    construct_info,
                    instantiation,
                    &arg_positions,
                    analysis_data,
                    context,
                    pos,
                );

                let constructor_visibility_scope_class_id =
                    get_method_visibility_scope_class_id(class_info, construct_info);

                match construct_info.visibility {
                    Visibility::Public => {}
                    Visibility::Private => {
                        let is_same_class =
                            analyzer.get_declaring_class().is_some_and(|calling_class| {
                                calling_class == constructor_visibility_scope_class_id
                            });

                        if !is_same_class {
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InaccessibleMethod,
                                format!(
                                    "Cannot access private method {}::__construct",
                                    analyzer
                                        .interner
                                        .lookup(constructor_visibility_scope_class_id)
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
                                    constructor_visibility_scope_class_id,
                                )
                            });

                        if !can_access {
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InaccessibleMethod,
                                format!(
                                    "Cannot access protected method {}::__construct",
                                    analyzer
                                        .interner
                                        .lookup(constructor_visibility_scope_class_id)
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

                if construct_info.is_deprecated {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedMethod,
                        format!("Method {}::__construct is deprecated", class_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if !can_access_internal(analyzer, &construct_info.internal, Some(context)) {
                    let scope_phrase =
                        format_internal_scope_phrase(analyzer, &construct_info.internal);
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InternalMethod,
                        format!(
                            "The method {}::__construct is internal to {}",
                            class_name, scope_phrase
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            } else {
                analyze_pending_closure_args_without_context(
                    analyzer,
                    instantiation,
                    &arg_positions,
                    analysis_data,
                    context,
                );

                if let Some((constructor_class_id, inherited_constructor)) =
                    find_inherited_constructor(analyzer, class_info)
                {
                    let constructor_visibility_scope_class_id = analyzer
                        .codebase
                        .get_class(constructor_class_id)
                        .map(|constructor_class_info| {
                            get_method_visibility_scope_class_id(
                                constructor_class_info,
                                &inherited_constructor,
                            )
                        })
                        .unwrap_or(constructor_class_id);

                    match inherited_constructor.visibility {
                        Visibility::Public => {}
                        Visibility::Private => {
                            let is_same_class =
                                analyzer.get_declaring_class().is_some_and(|calling_class| {
                                    calling_class == constructor_visibility_scope_class_id
                                });

                            if !is_same_class {
                                let (line, col) = analyzer.get_line_column(pos.0);
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::InaccessibleMethod,
                                    format!(
                                        "Cannot access private method {}::__construct",
                                        analyzer
                                            .interner
                                            .lookup(constructor_visibility_scope_class_id)
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
                                        constructor_visibility_scope_class_id,
                                    )
                                });

                            if !can_access {
                                let (line, col) = analyzer.get_line_column(pos.0);
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::InaccessibleMethod,
                                    format!(
                                        "Cannot access protected method {}::__construct",
                                        analyzer
                                            .interner
                                            .lookup(constructor_visibility_scope_class_id)
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
                } else if let Some(argument_list) = &instantiation.argument_list {
                    let args_count = argument_list.arguments.len();
                    if args_count > 0 {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::TooManyArguments,
                            format!(
                                "Too many arguments to constructor {}::__construct, 0 expected, {} provided",
                                class_name, args_count
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

            inferred_type_params = infer_constructor_template_params(
                analyzer,
                class_info,
                instantiation,
                &arg_positions,
                analysis_data,
                context,
            );
        } else {
            analyze_pending_closure_args_without_context(
                analyzer,
                instantiation,
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
        }

        let has_undefined_arg_issue = arg_positions.iter().any(|(start, end)| {
            analysis_data.issues.iter().any(|issue| {
                issue.start_offset >= *start
                    && issue.start_offset <= *end
                    && matches!(
                        issue.kind,
                        IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
                    )
            })
        });

        if !class_is_known || has_undefined_arg_issue {
            emit_unknown_class_constructor_arg_issues(
                analyzer,
                instantiation,
                context,
                analysis_data,
            );
        }

        let result_class_id =
            get_instantiated_type_name_id(analyzer, instantiation.class, concrete_class_id);

        let result_type = TUnion::new(TAtomic::TNamedObject {
            name: result_class_id,
            type_params: inferred_type_params,
        });
        let result_type = add_instantiation_dataflow(
            analyzer,
            analysis_data,
            result_class_id,
            class_expr_type.as_ref(),
            &arg_positions,
            pos,
            result_type,
        );
        analysis_data.set_expr_type(pos, result_type);
        return;
    }

    let has_undefined_arg_issue = arg_positions.iter().any(|(start, end)| {
        analysis_data.issues.iter().any(|issue| {
            issue.start_offset >= *start
                && issue.start_offset <= *end
                && matches!(
                    issue.kind,
                    IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
                )
        })
    });

    if !class_is_known || has_undefined_arg_issue {
        emit_unknown_class_constructor_arg_issues(analyzer, instantiation, context, analysis_data);
    }

    analyze_pending_closure_args_without_context(
        analyzer,
        instantiation,
        &arg_positions,
        analysis_data,
        context,
    );

    if let Some(class_expr_type) = class_expr_type.as_ref() {
        emit_definite_dynamic_instantiation_issues(analyzer, class_expr_type, pos, analysis_data);

        if let Some(dynamic_type) = infer_dynamic_instantiation_type(analyzer, class_expr_type) {
            if analysis_data.current_stmt_start == Some(pos.0)
                && dynamic_type_requires_mixed_constructor_issue(&dynamic_type)
            {
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
            }
            analysis_data.set_expr_type(pos, dynamic_type);
            return;
        }

        emit_dynamic_instantiation_issues(analyzer, class_expr_type, pos, analysis_data);

        if analysis_data.current_stmt_start == Some(pos.0)
            && union_has_unresolved_class_string_target(class_expr_type)
        {
            let already_emitted_mixed_method_call = analysis_data.issues.iter().any(|issue| {
                issue.kind == IssueKind::MixedMethodCall && issue.start_offset == pos.0
            });

            if !already_emitted_mixed_method_call {
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
            }
        }
    }

    let class_expr_is_dynamic_instantiable = class_expr_type
        .as_ref()
        .is_some_and(is_dynamic_instantiable_union);

    if !class_is_known && !class_expr_is_dynamic_instantiable {
        let already_emitted_undefined_class = analysis_data
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::UndefinedClass && issue.start_offset == pos.0);

        if !already_emitted_undefined_class {
            let class_expr_id = class_expr_type
                .as_ref()
                .map(|t| t.get_id(Some(analyzer.interner)))
                .unwrap_or_else(|| "mixed".to_string());
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!("Type {} cannot be called as a class", class_expr_id),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Fall back to generic object
    analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TObject));
}

fn add_instantiation_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    class_id: StrId,
    class_expr_type: Option<&TUnion>,
    arg_positions: &[Pos],
    pos: Pos,
    mut result_type: TUnion,
) -> TUnion {
    let call_node = DataFlowNode::get_for_call(
        FunctionLikeIdentifier::Method(class_id, StrId::CONSTRUCT),
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
        &mut result_type.parent_nodes,
        vec![call_node],
    );
    result_type
}

fn emit_definite_dynamic_instantiation_issues(
    analyzer: &StatementsAnalyzer<'_>,
    class_expr_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut literal_class_ids = Vec::new();
    for atomic in &class_expr_type.types {
        let TAtomic::TLiteralClassString { name } = atomic else {
            return;
        };

        literal_class_ids.push(analyzer.interner.intern(name.trim_start_matches('\\')));
    }

    if literal_class_ids.is_empty() {
        return;
    }

    let mut seen = FxHashSet::default();
    for class_id in literal_class_ids {
        if !seen.insert(class_id) {
            continue;
        }

        let Some(class_info) = analyzer.codebase.get_class(class_id) else {
            continue;
        };

        let class_name = analyzer.interner.lookup(class_id);
        let (line, col) = analyzer.get_line_column(pos.0);

        if class_info.kind == ClassLikeKind::Enum {
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        } else if class_info.kind == ClassLikeKind::Interface {
            analysis_data.add_issue(Issue::new(
                IssueKind::InterfaceInstantiation,
                format!("Cannot instantiate interface {}", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        } else if class_info.is_abstract {
            analysis_data.add_issue(Issue::new(
                IssueKind::AbstractInstantiation,
                format!("Cannot instantiate abstract class {}", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer
                .get_resolved_name(offset)
                .or_else(|| Some(analyzer.interner.intern(id.value())))
        }
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }
}

fn find_class_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
) -> Option<StrId> {
    analyzer
        .codebase
        .classlike_infos
        .keys()
        .copied()
        .find(|class_id| {
            analyzer
                .interner
                .lookup(*class_id)
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(class_name.trim_start_matches('\\'))
        })
}

fn infer_concrete_class_id_from_class_expr_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_expr_type: Option<&TUnion>,
    context: &BlockContext,
) -> Option<StrId> {
    let class_expr_type = class_expr_type?;
    let atomic = class_expr_type.get_single()?;

    let raw_class_id = match atomic {
        TAtomic::TLiteralClassString { name } => Some(analyzer.interner.intern(name)),
        TAtomic::TClassString {
            as_type: Some(as_type),
        }
        | TAtomic::TTemplateParamClass { as_type, .. } => match as_type.as_ref() {
            TAtomic::TNamedObject { name, .. } if *name != StrId::STATIC => Some(*name),
            _ => None,
        },
        TAtomic::TTemplateParam { as_type, .. } => match as_type.get_single() {
            Some(TAtomic::TNamedObject { name, .. }) if *name != StrId::STATIC => Some(*name),
            _ => None,
        },
        _ => None,
    }?;

    let class_id = context
        .class_aliases
        .get(&raw_class_id)
        .copied()
        .unwrap_or(raw_class_id);

    analyzer
        .codebase
        .get_class(class_id)
        .map(|_| class_id)
        .or_else(|| {
            find_class_case_insensitive(analyzer, analyzer.interner.lookup(class_id).as_ref())
        })
}

fn get_instantiated_type_name_id(
    _analyzer: &StatementsAnalyzer<'_>,
    _expr: &Expression<'_>,
    concrete_class_id: StrId,
) -> StrId {
    concrete_class_id
}

fn is_closure_like_argument(arg: &mago_syntax::ast::ast::argument::Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

fn get_closure_like_argument_offset(
    arg: &mago_syntax::ast::ast::argument::Argument<'_>,
) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn analyze_pending_closure_args_for_constructor(
    analyzer: &StatementsAnalyzer<'_>,
    instantiation: &Instantiation<'_>,
    arg_positions: &[Pos],
    class_info: &pzoom_code_info::ClassLikeInfo,
    construct_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(argument_list) = instantiation.argument_list.as_ref() else {
        return;
    };

    let args: Vec<_> = argument_list.arguments.iter().collect();
    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    template_defaults.extend(function_call_analyzer::get_template_defaults(
        construct_info,
    ));

    let template_replacements = function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        &args,
        arg_positions,
        &construct_info.params,
        &template_defaults,
        analysis_data,
        context,
    );

    for (idx, arg) in args.iter().enumerate() {
        let Some(closure_offset) = get_closure_like_argument_offset(arg) else {
            continue;
        };

        let arg_span = arg.span();
        let arg_pos = arg_positions
            .get(idx)
            .copied()
            .unwrap_or((arg_span.start.offset, arg_span.end.offset));
        if analysis_data.get_expr_type(arg_pos).is_some() {
            continue;
        }

        let param = if idx < construct_info.params.len() {
            Some(&construct_info.params[idx])
        } else {
            construct_info
                .params
                .last()
                .filter(|param| param.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    param_type,
                    &template_replacements,
                    &template_defaults,
                )
            }
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

fn analyze_pending_closure_args_without_context(
    analyzer: &StatementsAnalyzer<'_>,
    instantiation: &Instantiation<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(argument_list) = instantiation.argument_list.as_ref() else {
        return;
    };

    for (idx, arg) in argument_list.arguments.iter().enumerate() {
        if !is_closure_like_argument(arg) {
            continue;
        }

        let arg_span = arg.span();
        let arg_pos = arg_positions
            .get(idx)
            .copied()
            .unwrap_or((arg_span.start.offset, arg_span.end.offset));
        if analysis_data.get_expr_type(arg_pos).is_some() {
            continue;
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }
}

fn verify_constructor_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    construct_info: &pzoom_code_info::FunctionLikeInfo,
    instantiation: &Instantiation<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    pos: Pos,
) {
    let args: Vec<_> = instantiation
        .argument_list
        .as_ref()
        .map(|argument_list| argument_list.arguments.iter().collect())
        .unwrap_or_default();

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = construct_info
        .params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to constructor {}::__construct, {} expected, {} provided",
                analyzer.interner.lookup(class_info.name),
                required_params,
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = construct_info
        .params
        .last()
        .is_some_and(|param| param.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > construct_info.params.len() {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to constructor {}::__construct, {} expected, {} provided",
                analyzer.interner.lookup(class_info.name),
                construct_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    template_defaults.extend(function_call_analyzer::get_template_defaults(
        construct_info,
    ));
    let template_replacements = function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        &args,
        arg_positions,
        &construct_info.params,
        &template_defaults,
        analysis_data,
        context,
    );

    let callable_name = format!("{}::__construct", analyzer.interner.lookup(class_info.name));
    let arg_param_indices = arguments_analyzer::check_arguments_match(
        analyzer,
        &args,
        arg_positions,
        construct_info,
        &callable_name,
        analysis_data,
        context,
        Some(&template_defaults),
        Some(&template_replacements),
        pos,
        false,
        false,
    );

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
                    construct_info.no_named_arguments,
                    analysis_data,
                );
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| construct_info.params.get(mapped_index))
            .or_else(|| {
                construct_info
                    .params
                    .last()
                    .filter(|param| param.is_variadic)
            });
        let Some(param) = param else {
            continue;
        };

        let Some(arg_type) =
            arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
        else {
            continue;
        };

        let mut effective_param = param.clone();
        if !template_defaults.is_empty() || !template_replacements.is_empty() {
            if let Some(param_type) = param.get_type() {
                effective_param.param_type =
                    Some(function_call_analyzer::replace_templates_in_union(
                        param_type,
                        &template_replacements,
                        &template_defaults,
                    ));
            }
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

        if effective_param.by_ref
            && let Expression::Variable(Variable::Direct(direct)) = arg.value().unparenthesized()
        {
            let var_id = analyzer.interner.intern(direct.name);
            if let Some(constraint_type) = effective_param.get_type()
                && !constraint_type.is_mixed()
                && var_id != StrId::THIS_VAR
            {
                context.add_reference_constraint(var_id, constraint_type.clone());
                context.set_var_type(var_id, constraint_type.clone());
            }
        }
    }
}

fn infer_constructor_template_params(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    instantiation: &Instantiation<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> Option<Vec<TUnion>> {
    if class_info.template_types.is_empty() {
        return None;
    }

    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);

    if let (Some(argument_list), Some(construct_info)) = (
        instantiation.argument_list.as_ref(),
        class_info.methods.get(&StrId::CONSTRUCT),
    ) {
        template_defaults.extend(function_call_analyzer::get_template_defaults(
            construct_info,
        ));
        let args: Vec<_> = argument_list.arguments.iter().collect();
        let arg_template_replacements =
            function_call_analyzer::infer_template_replacements_from_args(
                analyzer,
                &args,
                arg_positions,
                &construct_info.params,
                &template_defaults,
                analysis_data,
                context,
            );
        function_call_analyzer::overlay_template_replacements(
            &mut template_replacements,
            arg_template_replacements,
        );

        let inferred_class_template_replacements =
            infer_class_template_replacements_from_constructor_templates(
                class_info,
                &template_replacements,
            );
        function_call_analyzer::overlay_template_replacements(
            &mut template_replacements,
            inferred_class_template_replacements,
        );
    }

    Some(
        class_info
            .template_types
            .iter()
            .map(|template_type| {
                template_replacements
                    .get(&template_type.name)
                    .cloned()
                    .unwrap_or_else(|| template_type.as_type.clone())
            })
            .collect(),
    )
}

pub(crate) fn infer_class_template_replacements_from_constructor_templates(
    class_info: &pzoom_code_info::ClassLikeInfo,
    template_replacements: &rustc_hash::FxHashMap<StrId, TUnion>,
) -> rustc_hash::FxHashMap<StrId, TUnion> {
    let class_template_names: rustc_hash::FxHashSet<StrId> = class_info
        .template_types
        .iter()
        .map(|template_type| template_type.name)
        .collect();

    let mut propagated_replacements = template_replacements.clone();

    loop {
        let mut changed = false;

        for template_map in class_info.template_extended_params.values() {
            for (ancestor_template, mapped_type) in template_map {
                let Some(ancestor_replacement) =
                    propagated_replacements.get(ancestor_template).cloned()
                else {
                    continue;
                };

                for mapped_atomic in &mapped_type.types {
                    let mapped_template = match mapped_atomic {
                        TAtomic::TTemplateParam { name, .. } => Some(*name),
                        TAtomic::TTemplateParamClass { name, .. } => Some(*name),
                        _ => None,
                    };

                    let Some(mapped_template) = mapped_template else {
                        continue;
                    };

                    let should_propagate = propagated_replacements
                        .get(&mapped_template)
                        .is_none_or(is_template_placeholder_union);

                    if !should_propagate {
                        continue;
                    }

                    if propagated_replacements
                        .get(&mapped_template)
                        .is_none_or(|existing| existing != &ancestor_replacement)
                    {
                        propagated_replacements
                            .insert(mapped_template, ancestor_replacement.clone());
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    let mut inferred_replacements = rustc_hash::FxHashMap::default();
    for template_name in class_template_names {
        if let Some(replacement) = propagated_replacements.get(&template_name) {
            inferred_replacements.insert(template_name, replacement.clone());
        }
    }

    inferred_replacements
}

fn is_template_placeholder_union(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
            )
        })
}

fn infer_dynamic_instantiation_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_expr_type: &TUnion,
) -> Option<TUnion> {
    let mut inferred_types = Vec::new();

    for atomic in &class_expr_type.types {
        collect_instantiable_atomic(analyzer, atomic, &mut inferred_types);
    }

    if inferred_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(inferred_types))
    }
}

fn emit_dynamic_instantiation_issues(
    analyzer: &StatementsAnalyzer<'_>,
    class_expr_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted = false;

    for atomic in &class_expr_type.types {
        match atomic {
            TAtomic::TNamedObject { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. } => {}
            TAtomic::TMixed => {}
            _ => {
                if emitted {
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
                emitted = true;
            }
        }
    }
}

fn emit_unknown_class_constructor_arg_issues(
    analyzer: &StatementsAnalyzer<'_>,
    instantiation: &Instantiation<'_>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(args) = instantiation.argument_list.as_ref() else {
        return;
    };

    for arg in args.arguments.iter() {
        let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(variable)) =
            arg.value().unparenthesized()
        else {
            continue;
        };

        let var_id = analyzer.interner.intern(variable.name);
        if context.get_var_type(var_id).is_some() {
            continue;
        }

        let span = arg.span();
        analysis_data.issues.retain(|issue| {
            !(issue.start_offset >= span.start.offset
                && issue.start_offset <= span.end.offset
                && matches!(
                    issue.kind,
                    IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
                ))
        });

        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedVariable,
            format!(
                "Undefined variable ${}",
                variable.name.trim_start_matches('$')
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }
}

fn get_method_visibility_scope_class_id(
    class_info: &pzoom_code_info::ClassLikeInfo,
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

fn find_inherited_constructor(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> Option<(StrId, pzoom_code_info::FunctionLikeInfo)> {
    let mut current_parent = class_info.parent_class;

    while let Some(parent_class_id) = current_parent {
        let parent_class_info = analyzer.codebase.get_class(parent_class_id)?;

        if let Some(parent_constructor) = parent_class_info.methods.get(&StrId::CONSTRUCT) {
            return Some((parent_class_id, parent_constructor.clone()));
        }

        current_parent = parent_class_info.parent_class;
    }

    None
}

fn is_dynamic_instantiable_union(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(is_dynamic_instantiable_atomic)
}

fn is_dynamic_instantiable_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TNamedObject { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. }
    )
}

fn get_dynamic_class_var_id(
    analyzer: &StatementsAnalyzer<'_>,
    class_expr: &Expression<'_>,
) -> Option<StrId> {
    match class_expr.unparenthesized() {
        Expression::Variable(Variable::Direct(variable)) => {
            Some(analyzer.interner.intern(variable.name))
        }
        Expression::Identifier(identifier) if identifier.value().starts_with('$') => {
            Some(analyzer.interner.intern(identifier.value()))
        }
        _ => None,
    }
}

fn collect_instantiable_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    inferred_types: &mut Vec<TAtomic>,
) {
    match atomic {
        TAtomic::TNamedObject { .. } => push_unique_atomic(inferred_types, atomic.clone()),
        TAtomic::TLiteralClassString { name } => push_unique_atomic(
            inferred_types,
            TAtomic::TNamedObject {
                name: analyzer.interner.intern(name.trim_start_matches('\\')),
                type_params: None,
            },
        ),
        TAtomic::TClassString {
            as_type: Some(as_type),
        }
        | TAtomic::TTemplateParamClass { as_type, .. } => {
            collect_instantiable_atomic(analyzer, as_type, inferred_types);
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for bound_atomic in &as_type.types {
                collect_instantiable_atomic(analyzer, bound_atomic, inferred_types);
            }
        }
        TAtomic::TObject => push_unique_atomic(inferred_types, TAtomic::TObject),
        TAtomic::TObjectIntersection { .. } => push_unique_atomic(inferred_types, atomic.clone()),
        _ => {}
    }
}

fn push_unique_atomic(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !target.contains(&atomic) {
        target.push(atomic);
    }
}

fn dynamic_type_requires_mixed_constructor_issue(dynamic_type: &TUnion) -> bool {
    dynamic_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TObjectIntersection { types }
                if types.iter().any(|inner| matches!(inner, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
        )
    })
}

fn union_has_unresolved_class_string_target(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| match atomic {
        TAtomic::TClassString { as_type: None } => true,
        TAtomic::TTemplateParamClass { as_type, .. } => {
            matches!(
                as_type.as_ref(),
                TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
            )
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            as_type.is_mixed()
                || as_type.types.iter().all(|nested| {
                    matches!(
                        nested,
                        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject
                    )
                })
        }
        _ => false,
    })
}
