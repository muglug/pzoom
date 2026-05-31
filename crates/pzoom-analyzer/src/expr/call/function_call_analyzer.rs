//! Function call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template;

pub(crate) use super::callable_validation::{
    analyze_arguments_with_callable_context, callable_union_is_pure,
    has_known_literal_function_target, infer_array_map_callable_return_type,
    infer_invokable_object_return_type, maybe_check_builtin_callable_arity,
    resolve_callable_union_for_template_inference, union_contains_non_pure_callable,
    validate_direct_callable_invocation, widen_literal_scalar_union_for_callable,
};
pub(crate) use super::function_call_return_type_fetcher::resolve_functionlike_return_type;
pub(crate) use super::arguments_analyzer::{
    apply_param_out_types, is_array_filter_function_name, predeclare_by_ref_argument_vars,
    verify_arguments,
};
pub(crate) use super::function_call_assertion_analyzer::{
    apply_assert_builtin_assertions, apply_post_call_assertions,
    emit_non_mutation_free_magic_property_assertion_issues, narrow_union_to_truthy,
};
use super::{
    argument_analyzer, array_multisort_analyzer, callable_validation,
    function_call_return_type_fetcher, named_function_call_handler,
};
use crate::template::TemplateMap;

/// Analyze a function call expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the callee expression to get the function name
    let callee_pos =
        expression_analyzer::analyze(analyzer, func_call.function, analysis_data, context);

    // Try to get the function name and whether it's fully qualified.
    // We need this before analyzing arguments to predeclare by-ref out variables.
    let (func_name, is_fq, name_offset) = get_function_name(func_call.function);
    let pre_resolved_func_info =
        func_name.and_then(|name| resolve_function(analyzer, name, is_fq, name_offset, context));
    predeclare_by_ref_argument_vars(
        analyzer,
        func_name,
        pre_resolved_func_info,
        &func_call.argument_list.arguments,
        context,
    );

    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();

    // `array_multisort` has a dynamic parameter list that the generic argument
    // flow cannot validate (see ArrayMultisortParamsProvider). Handle it here and
    // return early so the generic by-reference / argument checks are skipped.
    if func_name.is_some_and(array_multisort_analyzer::is_array_multisort)
        && pre_resolved_func_info.is_some()
    {
        array_multisort_analyzer::analyze(
            analyzer,
            &args,
            &arg_positions,
            analysis_data,
            context,
        );
        analysis_data.set_expr_type(pos, TUnion::bool());
        return;
    }

    if let Some(func_info) = pre_resolved_func_info {
        analyze_arguments_with_callable_context(
            analyzer,
            &args,
            &arg_positions,
            &func_info.params,
            &get_template_defaults(func_info),
            analysis_data,
            context,
        );
    } else {
        for arg in &args {
            argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        }
    }

    // Try to look up function return type
    if let Some(name) = func_name {
        if let Some(return_type) = named_function_call_handler::handle(
            analyzer,
            name,
            func_call,
            &arg_positions,
            pos,
            analysis_data,
            context,
        ) {
            let functionlike_id = pre_resolved_func_info
                .map(|info| FunctionLikeIdentifier::Function(info.name))
                .unwrap_or_else(|| {
                    FunctionLikeIdentifier::Function(analyzer.interner.intern(name))
                });
            let return_type = add_function_call_dataflow(
                analyzer,
                analysis_data,
                functionlike_id,
                pos,
                &arg_positions,
                return_type,
            );
            analysis_data.set_expr_type(pos, return_type);
            return;
        }

        // Resolve the function name considering namespace context
        let func_info = pre_resolved_func_info;
        if is_forbidden_function_call(analyzer, name, func_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ForbiddenCode,
                format!("Cannot use forbidden function {}", name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            analysis_data.set_expr_type(pos, TUnion::mixed());
            return;
        }

        if let Some(func_info) = func_info {
            let is_class_alias_call = func_info.name == StrId::CLASS_ALIAS;
            apply_function_defined_constants_side_effects(func_info, context);
            let template_defaults = get_template_defaults(func_info);
            let template_replacements = infer_function_template_replacements(
                analyzer,
                func_call,
                &arg_positions,
                func_info,
                &template_defaults,
                analysis_data,
                context,
            );

            record_named_function_callsite_argument_types(
                func_info.name,
                &arg_positions,
                analysis_data,
            );

            // Check for deprecated functions
            let is_stub_function = analyzer
                .codebase
                .files
                .get(&func_info.file_path)
                .is_some_and(|file_info| file_info.is_stub);

            if func_info.is_deprecated && !is_stub_function {
                let (line, col) = analyzer.get_line_column(pos.0);
                let message = func_info
                    .deprecation_message
                    .as_ref()
                    .map(|m| format!("Function {} is deprecated: {}", name, m))
                    .unwrap_or_else(|| format!("Function {} is deprecated", name));
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedFunction,
                    message,
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if !can_access_internal(analyzer, &func_info.internal, Some(context)) {
                let (line, col) = analyzer.get_line_column(pos.0);
                let scope_phrase = format_internal_scope_phrase(analyzer, &func_info.internal);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InternalMethod,
                    format!("Function {} is internal to {}", name, scope_phrase),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Verify argument types against function parameters
            if !is_class_alias_call {
                verify_arguments(
                    analyzer,
                    func_call,
                    &arg_positions,
                    func_info,
                    name,
                    analysis_data,
                    context,
                    &template_defaults,
                    &template_replacements,
                );
            }

            let fetched_return_type = function_call_return_type_fetcher::fetch(
                analyzer,
                name,
                Some(func_info),
                &args,
                &arg_positions,
                analysis_data,
                context,
                Some(&template_defaults),
                Some(&template_replacements),
            );

            apply_param_out_types(
                analyzer,
                func_info.name,
                &args,
                &arg_positions,
                &func_info.params,
                context,
                &template_defaults,
                &template_replacements,
            );

            apply_assert_builtin_assertions(
                analyzer,
                func_info.name,
                func_call,
                analysis_data,
                context,
            );
            apply_post_call_assertions(
                analyzer,
                func_call,
                func_info,
                context,
                &template_defaults,
                &template_replacements,
                analysis_data,
            );
            emit_non_mutation_free_magic_property_assertion_issues(
                analyzer,
                func_info,
                func_call,
                analysis_data,
            );

            if enforce_mutation_free
                && !function_call_is_mutation_free(
                    analyzer,
                    func_info,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                )
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureFunctionCall,
                    format!(
                        "Cannot call an impure function {} from a mutation-free context",
                        name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if let Some(resolved_return_type) = fetched_return_type {
                let resolved_return_type = add_function_call_dataflow(
                    analyzer,
                    analysis_data,
                    FunctionLikeIdentifier::Function(func_info.name),
                    pos,
                    &arg_positions,
                    resolved_return_type,
                );
                analysis_data.set_expr_type(pos, resolved_return_type);
                return;
            }
        } else {
            // Function not found in codebase
            // Don't emit error for language constructs that look like functions
            if !is_language_construct(name)
                && !is_function_guarded_by_function_exists(context, analyzer, name)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedFunction,
                    format!("Function {} is not defined", name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    } else {
        if let Some(callee_type) = analysis_data.get_expr_type(callee_pos) {
            if enforce_mutation_free
                && (callee_type.is_mixed() || union_contains_non_pure_callable(&callee_type))
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureFunctionCall,
                    "Potentially impure callable invocation in mutation-free context",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if callee_type.is_single() {
                if let Some(TAtomic::TLiteralString { value }) = callee_type.get_single() {
                    if value.is_empty() {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidFunctionCall,
                            "Cannot call empty string as a function",
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
            }

            validate_direct_callable_invocation(
                analyzer,
                &callee_type,
                &args,
                &arg_positions,
                analysis_data,
                context,
                pos,
            );

            if let Some(return_type) = callable_validation::infer_callee_return_type(&callee_type) {
                analysis_data.set_expr_type(pos, return_type);
                return;
            }

            if let Some(return_type) = infer_invokable_object_return_type(
                analyzer,
                &callee_type,
                &args,
                &arg_positions,
                analysis_data,
                context,
            ) {
                analysis_data.set_expr_type(pos, return_type);
                return;
            }

            if callee_type.has_string()
                && !callable_validation::union_has_callable(&callee_type)
                && !has_known_literal_function_target(analyzer, &callee_type, context)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedAssignment,
                    "Unable to determine return type of dynamically-invoked function",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        } else if enforce_mutation_free {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ImpureFunctionCall,
                "Potentially impure callable invocation in mutation-free context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

fn add_function_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    functionlike_id: FunctionLikeIdentifier,
    pos: Pos,
    arg_positions: &[Pos],
    mut return_type: TUnion,
) -> TUnion {
    let call_node =
        DataFlowNode::get_for_call(functionlike_id, make_data_flow_node_position(analyzer, pos));
    analysis_data.data_flow_graph.add_node(call_node.clone());

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

/// Verify argument types against function parameter types.
pub(crate) fn get_template_defaults(
    func_info: &pzoom_code_info::FunctionLikeInfo,
) -> TemplateMap {
    let mut template_defaults = TemplateMap::new();

    for template_type in &func_info.template_types {
        template_defaults.insert(
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    template_defaults
}

pub(crate) fn get_class_template_defaults(
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> TemplateMap {
    let mut template_defaults = TemplateMap::new();

    for template_type in &class_info.template_types {
        template_defaults.insert(
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    template_defaults
}

pub(crate) use super::class_template_param_collector::infer_class_template_replacements_from_type_params;

pub(crate) fn infer_class_template_replacements_from_extended_params(
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> TemplateMap {
    let mut template_replacements = TemplateMap::new();

    for (ancestor_class, template_map) in &class_info.template_extended_params {
        for (template_name, replacement) in template_map {
            template_replacements.insert_combined(
                *template_name,
                *ancestor_class,
                replacement.clone(),
            );
        }
    }

    template_replacements
}

pub(crate) fn overlay_template_replacements(target: &mut TemplateMap, incoming: TemplateMap) {
    target.extend_overlay(incoming);
}

fn infer_function_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &TemplateMap,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> TemplateMap {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    infer_template_replacements_from_args(
        analyzer,
        &args,
        arg_positions,
        &func_info.params,
        template_defaults,
        analysis_data,
        context,
    )
}

pub(crate) fn infer_template_replacements_from_args(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    template_defaults: &TemplateMap,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> TemplateMap {
    let mut template_replacements = TemplateMap::new();
    if template_defaults.is_empty() {
        return template_replacements;
    }

    for (idx, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            continue;
        }

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        let Some(arg_type) = analysis_data.get_expr_type(arg_pos) else {
            continue;
        };

        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };

        let Some(param_type) = param.and_then(|p| p.get_type()) else {
            continue;
        };

        let arg_type = if callable_validation::union_has_callable(param_type)
            && !callable_validation::union_has_callable(&arg_type)
        {
            resolve_callable_union_for_template_inference(analyzer, &arg_type, context)
                .unwrap_or_else(|| (*arg_type).clone())
        } else {
            (*arg_type).clone()
        };

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            &arg_type,
            template_defaults,
            &mut template_replacements,
        );
    }

    // Infer template replacements from omitted optional args that have known defaults.
    // This is important for conditional template behavior with default literal values.
    for param in params.iter().skip(args.len()) {
        let Some(param_type) = param.get_type() else {
            continue;
        };
        let Some(default_type) = &param.default_type else {
            continue;
        };

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            default_type,
            template_defaults,
            &mut template_replacements,
        );
    }

    template_replacements
}

pub(crate) fn replace_templates_in_union(
    union: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TUnion {
    let standin_replaced = crate::template::standin_type_replacer::substitute_templates_in_union(
        union,
        template_replacements,
        template_defaults,
    );

    template::inferred_type_replacer::replace(
        &standin_replaced,
        template_replacements,
        template_defaults,
    )
}

#[derive(Clone)]
pub(crate) struct ArrayLikeInfo {
    pub(crate) key_type: TUnion,
    pub(crate) value_type: TUnion,
    pub(crate) is_list: bool,
    pub(crate) is_non_empty: bool,
}

pub(crate) fn get_builtin_type_check_atomic(function_name: &str) -> Option<TAtomic> {
    let normalized_name = function_name.to_ascii_lowercase();

    Some(match normalized_name.as_str() {
        "is_string" => TAtomic::TString,
        "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
        "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
        "is_bool" => TAtomic::TBool,
        "is_array" => TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        },
        "is_object" => TAtomic::TObject,
        "is_null" => TAtomic::TNull,
        "is_numeric" => TAtomic::TNumeric,
        "is_resource" => TAtomic::TResource,
        "is_scalar" => TAtomic::TScalar,
        "is_iterable" => TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        },
        _ => return None,
    })
}

pub(crate) fn infer_builtin_type_check_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    asserted_atomic: TAtomic,
    arg_has_var_key: bool,
) -> Option<TUnion> {
    // A reconcilable-lvalue argument is reconciled (and refined) through the
    // assertion machinery, which emits any redundancy/impossibility itself —
    // narrowing the return type here would clobber that refinement. Only decide the
    // result for arguments with no reconcilable key.
    if arg_has_var_key {
        return Some(TUnion::bool());
    }

    let Some(arg_type) = arg_positions
        .first()
        .and_then(|pos| analysis_data.get_expr_type(*pos))
        .map(|t| (*t).clone())
    else {
        return Some(TUnion::bool());
    };

    // Only decide when the argument type is concrete: mixed/never and template
    // parameters keep the result a plain `bool`.
    if arg_type.is_mixed()
        || arg_type.is_nothing()
        || arg_type.from_calculation
        || arg_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
    {
        return Some(TUnion::bool());
    }

    let asserted_union = TUnion::new(asserted_atomic.clone());
    let mut comparison_result =
        crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
    let always_matches = crate::type_comparator::union_type_comparator::is_contained_by(
        analyzer.codebase,
        &arg_type,
        &asserted_union,
        false,
        false,
        &mut comparison_result,
    );

    let mut result = if always_matches {
        // Every member of the argument type already satisfies the check — always true.
        TUnion::new(TAtomic::TTrue)
    } else if assertion_reconciler::intersect_union_with_atomic(&arg_type, &asserted_atomic, analyzer)
        .is_none()
    {
        // No member can be of the asserted type — always false.
        TUnion::new(TAtomic::TFalse)
    } else {
        // Otherwise the runtime check genuinely discriminates.
        return Some(TUnion::bool());
    };
    result.from_docblock = arg_type.from_docblock;
    Some(result)
}

pub(crate) fn infer_string_transform_return_type(subject_type: &TUnion) -> Option<TUnion> {
    if subject_type.is_mixed() {
        return Some(TUnion::mixed());
    }

    let mut result_types = Vec::new();

    if union_contains_stringish(subject_type) {
        result_types.push(TAtomic::TString);
    }

    if let Some(array_info) = extract_array_like_info_from_union(subject_type) {
        let key_type = Box::new(normalize_array_key_union(&array_info.key_type));
        let value_type = Box::new(TUnion::string());
        let array_atomic = if array_info.is_list {
            if array_info.is_non_empty {
                TAtomic::TNonEmptyList { value_type }
            } else {
                TAtomic::TList { value_type }
            }
        } else if array_info.is_non_empty {
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            }
        } else {
            TAtomic::TArray {
                key_type,
                value_type,
            }
        };

        if !result_types.contains(&array_atomic) {
            result_types.push(array_atomic);
        }
    }

    if result_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(result_types))
    }
}

pub(crate) fn is_default_array_filter_callback(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> bool {
    if args.len() < 2 || arg_positions.len() < 2 {
        return true;
    }

    analysis_data
        .get_expr_type(arg_positions[1])
        .is_some_and(|callback_type| callback_type.is_null())
}

pub(crate) fn infer_array_filter_return_atomic(
    atomic: &TAtomic,
    callback_is_default: bool,
) -> Option<TAtomic> {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        } => Some(TAtomic::TArray {
            key_type: key_type.clone(),
            value_type: Box::new(if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            }),
        }),
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => {
            let filtered_value = if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            };

            if callback_is_default && filtered_value.is_nothing() {
                Some(TAtomic::TArray {
                    key_type: key_type.clone(),
                    value_type: Box::new(filtered_value),
                })
            } else {
                Some(TAtomic::TNonEmptyArray {
                    key_type: key_type.clone(),
                    value_type: Box::new(filtered_value),
                })
            }
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            let filtered_value = if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            };

            Some(TAtomic::TArray {
                key_type: Box::new(TUnion::int()),
                value_type: Box::new(filtered_value),
            })
        }
        TAtomic::TKeyedArray {
            properties,
            sealed,
            fallback_key_type,
            fallback_value_type,
            is_list,
        } => {
            let had_one = properties.len() == 1;
            let mut next_properties = rustc_hash::FxHashMap::default();

            for (key, property_type) in properties {
                if callback_is_default {
                    if property_type.is_always_falsy() {
                        continue;
                    }

                    let narrowed_type = narrow_union_to_truthy(property_type);

                    if property_type.is_always_truthy() {
                        next_properties.insert(key.clone(), narrowed_type);
                    } else {
                        next_properties.insert(
                            key.clone(),
                            mark_union_as_possibly_undefined(&narrowed_type),
                        );
                    }
                } else {
                    next_properties
                        .insert(key.clone(), mark_union_as_possibly_undefined(property_type));
                }
            }

            let next_fallback_value = fallback_value_type.as_ref().map(|fallback| {
                Box::new(if callback_is_default {
                    narrow_union_to_truthy(fallback)
                } else {
                    (**fallback).clone()
                })
            });

            // Mirror Psalm's ArrayFilterReturnTypeProvider: list-ness is only
            // preserved when the original keyed array had exactly one property
            // (so removing it can't leave a gap in the integer key sequence).
            let next_is_list = *is_list && had_one;

            Some(TAtomic::TKeyedArray {
                properties: next_properties,
                is_list: next_is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.clone(),
                fallback_value_type: next_fallback_value,
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut inferred = Vec::new();
            for nested_atomic in &as_type.types {
                let Some(filtered_atomic) =
                    infer_array_filter_return_atomic(nested_atomic, callback_is_default)
                else {
                    continue;
                };

                if !inferred.contains(&filtered_atomic) {
                    inferred.push(filtered_atomic);
                }
            }

            if inferred.is_empty() {
                None
            } else {
                Some(TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::from_types(inferred)),
                })
            }
        }
        _ => None,
    }
}

fn mark_union_as_possibly_undefined(union: &TUnion) -> TUnion {
    if union.possibly_undefined {
        return union.clone();
    }

    let mut possibly_undefined = union.clone();
    possibly_undefined.possibly_undefined = true;
    possibly_undefined
}

pub(crate) fn extract_array_like_info_from_union(union: &TUnion) -> Option<ArrayLikeInfo> {
    let mut combined: Option<ArrayLikeInfo> = None;

    for atomic in &union.types {
        let Some(info) = extract_array_like_info_from_atomic(atomic) else {
            continue;
        };

        combined = Some(if let Some(existing) = combined {
            combine_array_like_info(existing, info)
        } else {
            info
        });
    }

    combined
}

fn union_contains_stringish(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
        )
    })
}

fn extract_array_like_info_from_atomic(atomic: &TAtomic) -> Option<ArrayLikeInfo> {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: false,
        }),
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: true,
        }),
        TAtomic::TList { value_type } => Some(ArrayLikeInfo {
            key_type: TUnion::int(),
            value_type: (**value_type).clone(),
            is_list: true,
            is_non_empty: false,
        }),
        TAtomic::TNonEmptyList { value_type } => Some(ArrayLikeInfo {
            key_type: TUnion::int(),
            value_type: (**value_type).clone(),
            is_list: true,
            is_non_empty: true,
        }),
        TAtomic::TKeyedArray {
            properties,
            is_list,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            let key_type = extract_keyed_array_key_type(properties, fallback_key_type.as_deref());
            let value_type = crate::template::standin_type_replacer::extract_keyed_array_value_type(
                properties,
                fallback_value_type.as_deref(),
            );

            Some(ArrayLikeInfo {
                key_type,
                value_type,
                is_list: *is_list,
                is_non_empty: !properties.is_empty(),
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => extract_array_like_info_from_union(as_type),
        _ => None,
    }
}

pub(crate) fn extract_iterable_like_info_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> Option<ArrayLikeInfo> {
    let mut combined: Option<ArrayLikeInfo> = None;

    for atomic in &union.types {
        let Some(info) = extract_iterable_like_info_from_atomic(analyzer, atomic) else {
            continue;
        };

        combined = Some(if let Some(existing) = combined {
            combine_array_like_info(existing, info)
        } else {
            info
        });
    }

    combined
}

fn extract_iterable_like_info_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<ArrayLikeInfo> {
    if let Some(array_info) = extract_array_like_info_from_atomic(atomic) {
        return Some(array_info);
    }

    match atomic {
        TAtomic::TIterable {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: false,
        }),
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
            if !named_object_is_traversable(analyzer, *name) {
                return None;
            }

            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some(ArrayLikeInfo {
                key_type,
                value_type,
                is_list: false,
                is_non_empty: false,
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            extract_iterable_like_info_from_union(analyzer, as_type)
        }
        TAtomic::TObjectIntersection { types } => {
            let mut combined: Option<ArrayLikeInfo> = None;
            for intersection_atomic in types {
                let Some(info) =
                    extract_iterable_like_info_from_atomic(analyzer, intersection_atomic)
                else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_array_like_info(existing, info)
                } else {
                    info
                });
            }

            combined
        }
        _ => None,
    }
}

fn combine_array_like_info(left: ArrayLikeInfo, right: ArrayLikeInfo) -> ArrayLikeInfo {
    ArrayLikeInfo {
        key_type: combine_union_types(&left.key_type, &right.key_type, false),
        value_type: combine_union_types(&left.value_type, &right.value_type, false),
        is_list: left.is_list && right.is_list,
        is_non_empty: left.is_non_empty && right.is_non_empty,
    }
}

fn extract_keyed_array_key_type(
    properties: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, TUnion>,
    fallback_key_type: Option<&TUnion>,
) -> TUnion {
    let mut key_type = fallback_key_type.cloned().unwrap_or_else(TUnion::nothing);

    for key in properties.keys() {
        let literal_key_type = match key {
            pzoom_code_info::ArrayKey::Int(value) => {
                TUnion::new(TAtomic::TLiteralInt { value: *value })
            }
            pzoom_code_info::ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                value: value.clone(),
            }),
        };

        key_type = if key_type.is_nothing() {
            literal_key_type
        } else {
            combine_union_types(&key_type, &literal_key_type, false)
        };
    }

    if key_type.is_nothing() {
        TUnion::array_key()
    } else {
        key_type
    }
}

pub(crate) fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
    if name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
    {
        return true;
    }

    analyzer.codebase.get_class(name).is_some_and(|class_info| {
        class_info.interfaces.contains(&StrId::TRAVERSABLE)
            || class_info
                .all_parent_interfaces
                .iter()
                .any(|interface| *interface == StrId::TRAVERSABLE)
    })
}

pub(crate) fn normalize_array_key_union(key_type: &TUnion) -> TUnion {
    if key_type.is_nothing() {
        return TUnion::nothing();
    }

    assertion_reconciler::intersect_union_with_union(key_type, &TUnion::array_key())
        .unwrap_or_else(TUnion::array_key)
}

pub(crate) fn get_literal_bool_from_union(union: &TUnion) -> Option<bool> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn is_forbidden_function_call(
    analyzer: &StatementsAnalyzer<'_>,
    called_name: &str,
    resolved_function: Option<&pzoom_code_info::FunctionLikeInfo>,
) -> bool {
    if analyzer.config.forbidden_functions.is_empty() {
        return false;
    }

    if let Some(function_info) = resolved_function {
        let resolved_name = analyzer.interner.lookup(function_info.name);
        return is_forbidden_function_name(analyzer, resolved_name.as_ref());
    }

    let normalized = called_name.strip_prefix('\\').unwrap_or(called_name);
    if normalized.contains('\\') {
        return false;
    }

    is_forbidden_function_name(analyzer, normalized)
}

fn is_forbidden_function_name(analyzer: &StatementsAnalyzer<'_>, function_name: &str) -> bool {
    let normalized = function_name.strip_prefix('\\').unwrap_or(function_name);
    analyzer.config.forbidden_functions.iter().any(|forbidden| {
        forbidden
            .strip_prefix('\\')
            .unwrap_or(forbidden.as_str())
            .eq_ignore_ascii_case(normalized)
    })
}

fn apply_function_defined_constants_side_effects(
    function_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
) {
    for (const_id, const_type) in &function_info.defined_constants {
        context
            .defined_constants
            .insert(*const_id, const_type.clone());
    }
}

/// Resolve a function by name, considering namespace context.
///
/// PHP function resolution:
/// 1. If fully qualified (starts with \), use it directly
/// 2. If unqualified, first try current_namespace\function_name
/// 3. Fall back to global namespace
pub(crate) fn resolve_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    is_fully_qualified: bool,
    name_offset: Option<u32>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    if let Some(offset) = name_offset
        && let Some(resolved_name) = analyzer.get_resolved_name(offset)
    {
        if let Some(func_info) = analyzer.codebase.get_function(resolved_name) {
            return Some(func_info);
        }

        let resolved_name = analyzer.interner.lookup(resolved_name);
        if let Some(func_info) =
            get_function_case_insensitive(analyzer, resolved_name.trim_start_matches('\\'))
        {
            return Some(func_info);
        }
    }

    if is_fully_qualified {
        // Strip leading backslash and look up directly
        let clean_name = name.strip_prefix('\\').unwrap_or(name);
        let func_id = analyzer.interner.intern(clean_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }

        return get_function_case_insensitive(analyzer, clean_name);
    }

    // Try namespace-qualified lookup first
    if let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, name);
        let func_id = analyzer.interner.intern(&qualified_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }

        if let Some(func_info) = get_function_case_insensitive(analyzer, &qualified_name) {
            return Some(func_info);
        }
    }

    // Fall back to global namespace
    let func_id = analyzer.interner.intern(name);
    if let Some(func_info) = analyzer.codebase.get_function(func_id) {
        return Some(func_info);
    }

    get_function_case_insensitive(analyzer, name)
}

fn get_function_case_insensitive<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    function_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    analyzer
        .codebase
        .functionlike_infos
        .iter()
        .find_map(|(func_id, func_info)| {
            analyzer
                .interner
                .lookup(*func_id)
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(function_name.trim_start_matches('\\'))
                .then_some(func_info)
        })
}

/// Extract the function name from a function call expression.
/// Returns (name, is_fully_qualified, identifier_offset).
fn get_function_name<'a>(expr: &'a Expression<'a>) -> (Option<&'a str>, bool, Option<u32>) {
    match expr.unparenthesized() {
        Expression::Identifier(id) => (
            Some(id.value()),
            id.is_fully_qualified(),
            Some(id.span().start.offset),
        ),
        _ => (None, false, None),
    }
}

fn function_call_is_mutation_free(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> bool {
    if function_is_mutation_free(function_info) {
        return true;
    }

    // `assert()` has no side effects of its own (the asserted expression is analyzed
    // separately), so it is mutation-free. Psalm treats it as pure.
    if function_info.name == StrId::ASSERT {
        return true;
    }

    if is_array_map_function_name(function_info.name) {
        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 0);
    }

    if is_array_filter_function_name(function_info.name) {
        if args.len() < 2 {
            return true;
        }

        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 1);
    }

    // The user-comparator sort functions only mutate their by-reference array
    // argument (a local), so they are mutation-free when the comparator is pure -
    // matching Psalm.
    if matches!(
        function_info.name,
        StrId::USORT | StrId::UASORT | StrId::UKSORT
    ) {
        if args.len() < 2 {
            return true;
        }
        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 1);
    }

    false
}

pub(crate) fn callback_argument_is_pure(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
    callback_index: usize,
) -> bool {
    let Some(_callback_arg) = args.get(callback_index) else {
        return true;
    };
    let Some(callback_pos) = arg_positions.get(callback_index).copied() else {
        return true;
    };
    let Some(callback_type) = analysis_data.get_expr_type(callback_pos) else {
        return false;
    };

    let resolved = resolve_callable_union_for_template_inference(analyzer, &callback_type, context)
        .unwrap_or_else(|| (*callback_type).clone());

    let mut saw_non_null_candidate = false;
    let mut saw_callable_candidate = false;

    for atomic in &resolved.types {
        match atomic {
            TAtomic::TNull => {}
            TAtomic::TCallable { is_pure, .. } | TAtomic::TClosure { is_pure, .. } => {
                saw_non_null_candidate = true;
                saw_callable_candidate = true;
                if !matches!(is_pure, Some(true)) {
                    return false;
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                saw_non_null_candidate = true;
                if !callable_union_is_pure(as_type) {
                    return false;
                }
                saw_callable_candidate = true;
            }
            _ => {
                saw_non_null_candidate = true;
                return false;
            }
        }
    }

    !saw_non_null_candidate || saw_callable_candidate
}

fn function_is_mutation_free(function_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    function_info.is_pure || function_info.is_mutation_free
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

fn record_named_function_callsite_argument_types(
    function_id: StrId,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    for (index, arg_pos) in arg_positions.iter().enumerate() {
        let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) else {
            continue;
        };

        analysis_data.record_function_argument_callsite_type(
            function_id,
            index,
            (*arg_type).clone(),
        );
    }
}

/// Check if a name is a PHP language construct (not a real function).
///
/// These are special syntax that look like function calls but are actually
/// language constructs handled by the parser/compiler. They won't be in stubs.
fn is_language_construct(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        // Output constructs
        "echo"
            | "print"
            // Program termination
            | "die"
            | "exit"
            // Variable inspection (actually functions, but often not in stubs)
            | "isset"
            | "unset"
            | "empty"
            // Include/require (handled separately but can appear as function-like)
            | "include"
            | "include_once"
            | "require"
            | "require_once"
            // Evaluation
            | "eval"
            // List assignment
            | "list"
            // Array literal (not really a function)
            | "array"
    )
}

pub(crate) fn is_array_map_function_name(function_id: StrId) -> bool {
    function_id == StrId::ARRAY_MAP
}

fn is_function_guarded_by_function_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
) -> bool {
    let key = function_exists_assertion_key(function_name);
    let key_id = analyzer.interner.intern(&key);

    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

fn function_exists_assertion_key(function_name: &str) -> String {
    format!(
        "@function_exists({})",
        function_name.trim_start_matches('\\').to_ascii_lowercase()
    )
}
