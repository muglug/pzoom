//! Atomic static-call analyzer.
//!
//! Resolves and checks a static method call against a single (named or
//! dynamically-typed) class type: method resolution up the hierarchy, magic
//! `__callStatic`, visibility, template context, argument verification, and
//! return-type inference. Mirrors Psalm's `AtomicStaticCallAnalyzer` and
//! Hakana's `atomic_static_call_analyzer`.

use crate::type_expander::localize_special_class_type_union;
use mago_span::HasSpan;
use mago_syntax::ast::ast::call::StaticMethodCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;


use super::static_call_analyzer::*;
use super::existing_atomic_static_call_analyzer::*;

pub(crate) fn add_static_call_dataflow(
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

pub(crate) fn get_called_class_type_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    resolved_class_id: StrId,
) -> StrId {
    // For `self::`, `static::`, and `parent::` calls, late static binding keeps the
    // `static` type bound to the *current* class context, so a `static` return type
    // resolves to the current class (not the defining/parent class).
    let current_static = analyzer.get_declaring_class().unwrap_or(StrId::STATIC);
    match expr.unparenthesized() {
        Expression::Self_(_) => current_static,
        Expression::Static(_) => current_static,
        Expression::Parent(_) => current_static,
        Expression::Identifier(id) => {
            let value = id.value();
            if value.eq_ignore_ascii_case("self")
                || value.eq_ignore_ascii_case("static")
                || value.eq_ignore_ascii_case("parent")
            {
                current_static
            } else {
                let span = id.span();
                let source_value = analyzer
                    .get_source_substring(span.start.offset as usize, span.end.offset as usize)
                    .trim();
                if source_value.eq_ignore_ascii_case("self")
                    || source_value.eq_ignore_ascii_case("static")
                    || source_value.eq_ignore_ascii_case("parent")
                {
                    current_static
                } else {
                    resolved_class_id
                }
            }
        }
        _ => resolved_class_id,
    }
}

pub(crate) fn handle_dynamic_static_call(
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
                    let localized_return_type = localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
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
                        let localized_return_type = localize_special_class_type_union(analyzer.codebase, analyzer.interner, 
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

pub(crate) fn collect_dynamic_static_call_target_atomics(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    output: &mut Vec<TAtomic>,
    from_class_string_context: bool,
) {
    // A dependent `get_class($x)` value is a class-string; resolve the static-call
    // target from its class-string equivalent.
    if let Some(equiv) = atomic.dependent_string_equivalent() {
        collect_dynamic_static_call_target_atomics(
            analyzer,
            &equiv,
            output,
            from_class_string_context,
        );
        return;
    }

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
                is_static: false, remapped_params: false },
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

pub(crate) fn push_unique_dynamic_static_target(output: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !output.contains(&atomic) {
        output.push(atomic);
    }
}

pub(crate) fn infer_closure_from_callable_return_type(
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

pub(crate) fn collect_typed_closure_from_callable_atomic(
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

pub(crate) fn functionlike_to_typed_closure(function_info: &pzoom_code_info::FunctionLikeInfo) -> TAtomic {
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
