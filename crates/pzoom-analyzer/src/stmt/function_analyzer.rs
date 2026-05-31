//! Function declaration analyzer.
//!
//! Analyzes function bodies with proper return type context.

use mago_span::HasSpan;
use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::{DataFlowNode, Issue, IssueKind, TAtomic, TUnion, VarId, VariableSourceKind};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::attribute_analyzer;
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a function declaration.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func: &Function<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_with_namespace(analyzer, func, None, analysis_data, context)
}

/// Analyze a function declaration with a namespace context.
pub fn analyze_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    func: &Function<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the function name - use FQN if in a namespace
    let func_name = func.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, func_name)
    } else {
        func_name.to_string()
    };

    // Look up the function info from the codebase
    let func_name_id = analyzer.interner.intern(&fqn);
    let function_info = analyzer.codebase.get_function(func_name_id);

    attribute_analyzer::analyze_function_attributes(analyzer, func, context, analysis_data);
    if let Some(info) = function_info {
        emit_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_types(analyzer, info, analysis_data);
        check_param_class_casing(analyzer, info, analysis_data);
        check_invalid_param_defaults(analyzer, info, &fqn, analysis_data);
        check_template_param_bounds(analyzer, info, analysis_data);
    }

    // Create a new analyzer with the function context
    let func_analyzer = analyzer.for_nested_function(function_info);

    // Create a new context for the function body, preserving namespace
    let mut func_context = BlockContext::new();
    func_context.namespace = context.namespace;
    let no_named_arguments = function_info.is_some_and(|info| info.no_named_arguments);

    // Add parameters to context
    for (param_index, param) in func.parameter_list.parameters.iter().enumerate() {
        let param_name = param.variable.name;
        let param_name_id = analyzer.interner.intern(param_name);

        // Get parameter info from function info
        let param_info =
            function_info.and_then(|info| info.params.iter().find(|p| p.name == param_name_id));

        // Get parameter type - for variadic params, wrap in array type
        let mut param_type = if let Some(info) = param_info {
            let mut base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);
            if let Some(signature_type) = &info.signature_type {
                if !info.has_docblock_type {
                    base_type.from_docblock = false;
                } else {
                    base_type.from_docblock =
                        should_preserve_docblock_param_origin(signature_type, &base_type);
                }
            }
            if info.is_variadic {
                // Match Psalm: variadics accept named args unless explicitly disabled.
                if no_named_arguments {
                    TUnion::new(TAtomic::TList {
                        value_type: Box::new(base_type),
                    })
                } else {
                    TUnion::new(TAtomic::TArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(base_type),
                    })
                }
            } else {
                base_type
            }
        } else {
            TUnion::mixed()
        };

        // Call-site argument types are used to narrow a parameter's declared
        // type inside the body, but a templated parameter (e.g. `Collection<T>`)
        // must stay abstract: `T` is universally quantified over every caller, so
        // narrowing it to a single observed call site would be unsound. Psalm
        // analyses such bodies with the abstract template parameter.
        if !crate::type_comparator::generic_type_comparator::union_has_template(&param_type)
            && let Some(callsite_type) =
                analysis_data.get_function_argument_callsite_type(func_name_id, param_index)
        {
            param_type =
                assertion_reconciler::intersect_union_with_union(&param_type, callsite_type)
                    .unwrap_or_else(|| callsite_type.clone());
        }

        let param_span = param.variable.span();
        let parent_node = DataFlowNode::get_for_variable_source(
            VariableSourceKind::NonPrivateParam,
            VarId(param_name_id),
            make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            function_info.is_some_and(|info| info.is_pure),
            !param_type.parent_nodes.is_empty(),
            false,
            false,
            false,
        );
        analysis_data.data_flow_graph.add_node(parent_node.clone());
        param_type.parent_nodes = vec![parent_node];

        func_context.set_var_type(param_name_id, param_type.clone());
        if let Some(alt_param_name_id) = get_alternate_param_var_id(analyzer, param_name)
            && alt_param_name_id != param_name_id
        {
            func_context.set_var_type(alt_param_name_id, param_type.clone());
        }
    }

    // Analyze parameter default expressions in function scope.
    for param in func.parameter_list.parameters.iter() {
        let Some(default_value) = &param.default_value else {
            continue;
        };
        let _ = expression_analyzer::analyze(
            &func_analyzer,
            &default_value.value,
            analysis_data,
            &mut func_context,
        );
    }

    // Analyze the function body
    let yield_types_start = analysis_data.inferred_yield_types.len();
    let return_types_start = analysis_data.inferred_return_types.len();
    let prev_is_generator = analysis_data.current_function_is_generator;
    analysis_data.current_function_is_generator =
        stmt_analyzer::body_contains_yield(func.body.statements.as_slice());
    stmt_analyzer::analyze_stmts(
        &func_analyzer,
        func.body.statements.as_slice(),
        analysis_data,
        &mut func_context,
    )?;
    analysis_data.current_function_is_generator = prev_is_generator;
    let has_yield = analysis_data.inferred_yield_types.len() > yield_types_start;

    if let Some(info) = function_info {
        emit_invalid_by_ref_param_out_types(&func_analyzer, info, &func_context, analysis_data);
        maybe_emit_missing_return_issue(
            &func_analyzer,
            info,
            &func_context,
            analysis_data,
            func.span().start.offset as u32,
            &fqn,
            has_yield,
            !func.body.statements.is_empty(),
        );
        maybe_emit_missing_return_type_issue(
            &func_analyzer,
            info,
            analysis_data,
            return_types_start,
            &fqn,
        );
    }

    Ok(())
}

fn maybe_emit_missing_return_type_issue(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    return_types_start: usize,
    function_name: &str,
) {
    if function_info.signature_return_type.is_some() || function_info.return_type.is_some() {
        return;
    }

    let has_invalid_or_missing_return_docblock =
        function_info.docblock_issues.iter().any(|issue| {
            issue
                .message
                .eq_ignore_ascii_case("Missing return docblock type")
                || issue
                    .message
                    .eq_ignore_ascii_case("Invalid return docblock type")
        });

    if has_invalid_or_missing_return_docblock {
        let (line, col) = analyzer.get_line_column(function_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::MissingReturnType,
            format!("Function {} does not have a return type", function_name),
            analyzer.file_path,
            function_info.start_offset,
            function_info.end_offset,
            line,
            col,
        ));
        return;
    }

    // Empty inferred set yields `void`, which the guard below treats as "no
    // missing-return-type issue", matching the previous early return.
    let inferred_return_type = analysis_data.combine_inferred_return_types(return_types_start);

    if inferred_return_type.is_void()
        || inferred_return_type.is_nothing()
        || inferred_return_type.is_mixed()
        || inferred_return_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TNonEmptyMixed))
    {
        return;
    }

    let (line, col) = analyzer.get_line_column(function_info.start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::MissingReturnType,
        format!("Function {} does not have a return type", function_name),
        analyzer.file_path,
        function_info.start_offset,
        function_info.end_offset,
        line,
        col,
    ));
}

fn maybe_emit_missing_return_issue(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    issue_offset: u32,
    function_name: &str,
    has_yield: bool,
    has_statements: bool,
) {
    if analyzer
        .codebase
        .files
        .get(&analyzer.file_path)
        .is_some_and(|file_info| file_info.is_stub)
    {
        return;
    }

    let Some(expected_return_type) = function_info.get_return_type() else {
        return;
    };
    // Expand any conditional return type to the union of its branches before deciding
    // whether a return value is required (a void branch makes it optional).
    let mut expected_return_type = expected_return_type.clone();
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut expected_return_type,
        &crate::type_expander::TypeExpansionOptions { evaluate_conditional_types: true, ..Default::default() },
    );
    let expected_return_type = &expected_return_type;

    if context.has_returned
        || has_yield
        || !has_statements
        || expected_return_type.is_nullable
        || expected_return_type.is_void()
        || expected_return_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TVoid))
        || expected_return_type.is_mixed()
        || expected_return_type.is_nothing()
    {
        return;
    }

    let issue_kind = if function_info.signature_return_type.is_some() {
        IssueKind::InvalidReturnType
    } else {
        IssueKind::InvalidNullableReturnType
    };

    let (line, col) = analyzer.get_line_column(issue_offset);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!(
            "Not all code paths of {} end in a return statement, expected {}",
            function_name,
            expected_return_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        issue_offset,
        issue_offset + 1,
        line,
        col,
    ));
}

/// Validates that the type arguments of a generic type used in the signature
/// satisfy their template parameters' bounds, including dependent bounds such as
/// `@template B of AType<T>` where an earlier argument is substituted into the
/// bound before comparison. Mirrors Psalm's `InvalidTemplateParam`.
fn check_template_param_bounds(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &function_info.params {
        if let Some(param_type) = param.get_type() {
            for atomic in &param_type.types {
                check_atomic_template_bounds(analyzer, atomic, param.start_offset, analysis_data);
            }
        }
    }

    if let Some(return_type) = &function_info.return_type {
        let offset = function_info.start_offset;
        for atomic in &return_type.types {
            check_atomic_template_bounds(analyzer, atomic, offset, analysis_data);
        }
    }
}

fn check_atomic_template_bounds(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let TAtomic::TNamedObject {
        name,
        type_params: Some(type_params),
        ..
    } = atomic
    else {
        return;
    };

    if let Some(class_info) = analyzer.codebase.get_class(*name) {
        // Map each template parameter name to the supplied argument so dependent
        // bounds (`B of AType<T>`) can be resolved.
        let mut substitutions = crate::template::TemplateMap::new();
        for (index, template_type) in class_info.template_types.iter().enumerate() {
            if let Some(type_param) = type_params.get(index) {
                substitutions.insert(
                    template_type.name,
                    template_type.defining_entity,
                    type_param.clone(),
                );
            }
        }

        let empty_defaults = crate::template::TemplateMap::new();
        for (index, template_type) in class_info.template_types.iter().enumerate() {
            let Some(type_param) = type_params.get(index) else {
                continue;
            };
            if template_type.as_type.is_mixed() {
                continue;
            }

            let effective_bound = crate::expr::call::function_call_analyzer::replace_templates_in_union(
                &template_type.as_type,
                &substitutions,
                &empty_defaults,
            );
            if effective_bound.is_mixed() {
                continue;
            }

            let mut comparison_result = TypeComparisonResult::new();
            let is_contained = union_type_comparator::is_contained_by(
                analyzer.codebase,
                type_param,
                &effective_bound,
                false,
                false,
                &mut comparison_result,
            );

            if !is_contained && !comparison_result.type_coerced.unwrap_or(false) {
                let (line, col) = analyzer.get_line_column(offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidTemplateParam,
                    format!(
                        "Type {} is not within the bound {} of template param {} of {}",
                        type_param.get_id(Some(analyzer.interner)),
                        effective_bound.get_id(Some(analyzer.interner)),
                        analyzer.interner.lookup(template_type.name),
                        analyzer.interner.lookup(*name),
                    ),
                    analyzer.file_path,
                    offset,
                    offset.saturating_add(1),
                    line,
                    col,
                ));
            }
        }
    }

    // Recurse into the type arguments, which may themselves be generic.
    for type_param in type_params {
        for inner in &type_param.types {
            check_atomic_template_bounds(analyzer, inner, offset, analysis_data);
        }
    }
}

fn check_param_class_casing(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted: FxHashSet<(u32, StrId)> = FxHashSet::default();

    for param in &function_info.params {
        let Some(param_type) = param.signature_type.as_ref().or(param.param_type.as_ref()) else {
            continue;
        };

        for atomic in &param_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };

            if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                continue;
            }

            if let Some(class_info) = analyzer.codebase.get_class(*name) {
                if class_info.is_deprecated && emitted.insert((param.start_offset, *name)) {
                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(param.start_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedClass,
                        format!("{} is marked deprecated", class_name),
                        analyzer.file_path,
                        param.start_offset,
                        param.start_offset + 1,
                        line,
                        col,
                    ));
                }
                continue;
            }

            let requested = analyzer.interner.lookup(*name);
            let Some(actual_id) = find_class_case_insensitive(analyzer, requested.as_ref()) else {
                continue;
            };

            if !emitted.insert((param.start_offset, actual_id)) {
                continue;
            }

            let actual = analyzer.interner.lookup(actual_id);
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidClass,
                format!(
                    "Class {} has incorrect casing, expected {}",
                    requested, actual
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset + 1,
                line,
                col,
            ));
        }
    }
}

fn emit_docblock_issues(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for issue in &function_info.docblock_issues {
        let (line, col) = analyzer.get_line_column(issue.start_offset);
        let issue_kind = if issue.message.eq_ignore_ascii_case("Missing docblock type") {
            IssueKind::MissingDocblockType
        } else if issue
            .message
            .eq_ignore_ascii_case("Possibly invalid docblock tag")
        {
            IssueKind::PossiblyInvalidDocblockTag
        } else if issue
            .message
            .eq_ignore_ascii_case("Undefined docblock class")
        {
            IssueKind::UndefinedDocblockClass
        } else if issue.message.eq_ignore_ascii_case("Undefined constant") {
            IssueKind::UndefinedConstant
        } else {
            IssueKind::InvalidDocblock
        };
        analysis_data.add_issue(Issue::new(
            issue_kind,
            issue.message.clone(),
            analyzer.file_path,
            issue.start_offset,
            issue.end_offset,
            line,
            col,
        ));
    }
}

fn check_undefined_docblock_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted_classes = FxHashSet::default();
    let mut emitted_constants = FxHashSet::default();

    for param in &function_info.params {
        if !param.has_docblock_type {
            continue;
        }

        let Some(param_type) = param.get_type() else {
            continue;
        };

        inspect_union_for_docblock_refs(
            analyzer,
            function_info,
            param_type,
            &mut emitted_classes,
            &mut emitted_constants,
            analysis_data,
        );
    }

    if let Some(return_type) = function_info.get_return_type() {
        let matches_signature = function_info
            .signature_return_type
            .as_ref()
            .is_some_and(|signature_return_type| signature_return_type == return_type);

        if return_type.from_docblock || !matches_signature {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                return_type,
                &mut emitted_classes,
                &mut emitted_constants,
                analysis_data,
            );
        }
    }
}

fn inspect_union_for_docblock_refs(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    union: &TUnion,
    emitted_classes: &mut FxHashSet<StrId>,
    emitted_constants: &mut FxHashSet<(StrId, StrId)>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for atomic in &union.types {
        inspect_atomic_for_docblock_refs(
            analyzer,
            function_info,
            atomic,
            emitted_classes,
            emitted_constants,
            analysis_data,
        );
    }
}

fn inspect_atomic_for_docblock_refs(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    atomic: &TAtomic,
    emitted_classes: &mut FxHashSet<StrId>,
    emitted_constants: &mut FxHashSet<(StrId, StrId)>,
    analysis_data: &mut FunctionAnalysisData,
) {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
            if *name == StrId::PZOOM_INDEXED_ACCESS {
                if let Some(type_params) = type_params {
                    for type_param in type_params {
                        inspect_union_for_docblock_refs(
                            analyzer,
                            function_info,
                            type_param,
                            emitted_classes,
                            emitted_constants,
                            analysis_data,
                        );
                    }
                }
                return;
            }

            let raw_name = analyzer.interner.lookup(*name);
            let raw_name = raw_name.as_ref().trim();

            if let Some((class_part, constant_part)) = raw_name.split_once("::") {
                let class_name = class_part.trim().trim_start_matches('\\');
                let class_id = analyzer.interner.intern(class_name);
                let Some(actual_class_id) =
                    resolve_docblock_class_id(analyzer, function_info, class_id)
                else {
                    emit_undefined_docblock_class_issue(
                        analyzer,
                        function_info,
                        class_id,
                        emitted_classes,
                        analysis_data,
                    );
                    return;
                };

                let constant_name = constant_part.trim();
                if !constant_name.eq_ignore_ascii_case("class")
                    && !docblock_class_constant_exists(analyzer, actual_class_id, constant_name)
                {
                    let constant_id = analyzer.interner.intern(constant_name);
                    if emitted_constants.insert((actual_class_id, constant_id)) {
                        let (line, col) = analyzer.get_line_column(function_info.start_offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::UndefinedConstant,
                            format!(
                                "Constant {}::{} is not defined",
                                analyzer.interner.lookup(actual_class_id),
                                constant_name
                            ),
                            analyzer.file_path,
                            function_info.start_offset,
                            function_info.end_offset,
                            line,
                            col,
                        ));
                    }
                }
            } else {
                let class_name = raw_name.trim_start_matches('\\');
                let class_id = analyzer.interner.intern(class_name);
                if resolve_docblock_class_id(analyzer, function_info, class_id).is_none() {
                    emit_undefined_docblock_class_issue(
                        analyzer,
                        function_info,
                        class_id,
                        emitted_classes,
                        analysis_data,
                    );
                }
            }

            if let Some(type_params) = type_params {
                for type_param in type_params {
                    inspect_union_for_docblock_refs(
                        analyzer,
                        function_info,
                        type_param,
                        emitted_classes,
                        emitted_constants,
                        analysis_data,
                    );
                }
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => inspect_union_for_docblock_refs(
            analyzer,
            function_info,
            as_type,
            emitted_classes,
            emitted_constants,
            analysis_data,
        ),
        TAtomic::TTemplateParamClass { as_type, .. } => inspect_atomic_for_docblock_refs(
            analyzer,
            function_info,
            as_type,
            emitted_classes,
            emitted_constants,
            analysis_data,
        ),
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                inspect_atomic_for_docblock_refs(
                    analyzer,
                    function_info,
                    nested_atomic,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        TAtomic::TClassString { as_type } => {
            if let Some(as_type) = as_type {
                inspect_atomic_for_docblock_refs(
                    analyzer,
                    function_info,
                    as_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                key_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                value_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                value_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for property_type in properties.values() {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    property_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }

            if let Some(fallback_key_type) = fallback_key_type {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    fallback_key_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }

            if let Some(fallback_value_type) = fallback_value_type {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    fallback_value_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        _ => {}
    }
}

fn emit_undefined_docblock_class_issue(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    class_id: StrId,
    emitted_classes: &mut FxHashSet<StrId>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !emitted_classes.insert(class_id) {
        return;
    }

    let (line, col) = analyzer.get_line_column(function_info.start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedDocblockClass,
        format!(
            "Docblock class {} does not exist",
            analyzer.interner.lookup(class_id)
        ),
        analyzer.file_path,
        function_info.start_offset,
        function_info.end_offset,
        line,
        col,
    ));
}

fn resolve_docblock_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    class_id: StrId,
) -> Option<StrId> {
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let class_name = analyzer.interner.lookup(class_id);
    if let Some(actual_id) = find_class_case_insensitive(analyzer, class_name.as_ref()) {
        return Some(actual_id);
    }

    let class_name = class_name.as_ref();
    if class_name.contains('\\') {
        return None;
    }

    let function_name = analyzer.interner.lookup(function_info.name);
    let namespace = function_name.rsplit_once('\\').map(|(ns, _)| ns)?;
    let namespaced_candidate = analyzer
        .interner
        .intern(&format!("{namespace}\\{class_name}"));

    if analyzer.codebase.get_class(namespaced_candidate).is_some() {
        return Some(namespaced_candidate);
    }

    let candidate_name = analyzer.interner.lookup(namespaced_candidate);
    find_class_case_insensitive(analyzer, candidate_name.as_ref())
}

fn docblock_class_constant_exists(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    constant_name: &str,
) -> bool {
    let mut to_visit = vec![class_id];
    let mut visited = FxHashSet::default();

    while let Some(current_class_id) = to_visit.pop() {
        if !visited.insert(current_class_id) {
            continue;
        }

        let Some(class_info) = analyzer.codebase.get_class(current_class_id) else {
            continue;
        };

        if constant_name.ends_with('*') {
            let prefix = constant_name.trim_end_matches('*');
            if class_info.constants.keys().any(|const_id| {
                analyzer
                    .interner
                    .lookup(*const_id)
                    .as_ref()
                    .starts_with(prefix)
            }) {
                return true;
            }
        } else {
            let constant_id = analyzer.interner.intern(constant_name);
            if class_info.constants.contains_key(&constant_id) {
                return true;
            }
        }

        if let Some(parent_class_id) = class_info.parent_class {
            to_visit.push(parent_class_id);
        }

        to_visit.extend(class_info.interfaces.iter().copied());
        to_visit.extend(class_info.all_parent_interfaces.iter().copied());
    }

    false
}

fn check_invalid_param_defaults(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    function_name: &str,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (idx, param) in function_info.params.iter().enumerate() {
        let Some(default_type) = param.default_type.as_ref() else {
            continue;
        };
        let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) else {
            continue;
        };
        let default_check_param_type = if default_type.is_null() {
            param
                .signature_type
                .as_ref()
                .filter(|signature_type| signature_type.is_nullable || signature_type.is_null())
                .unwrap_or(param_type)
        } else {
            param_type
        };

        if union_has_callable_like(default_check_param_type) && !default_type.is_null() {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type for callable argument {} of {} can only be null, {} specified",
                    idx + 1,
                    function_name,
                    default_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset.saturating_add(1),
                line,
                col,
            ));
            continue;
        }

        if default_type.is_mixed() {
            continue;
        }

        if is_empty_array_default_for_array_like_param(default_type, default_check_param_type) {
            continue;
        }

        let mut comparison_result = TypeComparisonResult::new();
        let default_is_valid = union_type_comparator::is_contained_by(
            analyzer.codebase,
            default_type,
            default_check_param_type,
            false,
            false,
            &mut comparison_result,
        );

        if !default_is_valid {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type {} for argument {} of {} does not match the given type {}",
                    default_type.get_id(Some(analyzer.interner)),
                    idx + 1,
                    function_name,
                    default_check_param_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn emit_invalid_by_ref_param_out_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &function_info.params {
        if !param.by_ref || param.is_variadic {
            continue;
        }

        let Some(actual_type) = context.get_var_type(param.name) else {
            continue;
        };

        let expected_type = param
            .param_out_type
            .as_ref()
            .or(param.get_type())
            .or(param.signature_type.as_ref());
        let Some(expected_type) = expected_type else {
            continue;
        };

        if actual_type.is_mixed() {
            continue;
        }

        let mut comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            actual_type,
            expected_type,
            false,
            false,
            &mut comparison,
        ) {
            continue;
        }

        // Avoid false positives when analysis widened the observed by-ref local
        // type but it still includes all values allowed by the declared out type.
        let mut reverse_comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            expected_type,
            actual_type,
            false,
            false,
            &mut reverse_comparison,
        ) {
            if !(union_is_list_only(expected_type) && union_has_non_list_array_like(actual_type)) {
                continue;
            }
        }

        let (line, col) = analyzer.get_line_column(param.start_offset);
        let param_name = analyzer.interner.lookup(param.name);
        analysis_data.add_issue(Issue::new(
            IssueKind::ReferenceConstraintViolation,
            format!(
                "Variable {} is limited to values of type {} because it is passed by reference, {} type found. Use @param-out to specify a different output type",
                param_name,
                expected_type.get_id(Some(analyzer.interner)),
                actual_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            param.start_offset,
            param.start_offset.saturating_add(1),
            line,
            col,
        ));
    }
}

fn union_has_callable_like(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
}

fn union_is_list_only(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TList { .. }
                    | TAtomic::TNonEmptyList { .. }
                    | TAtomic::TKeyedArray { is_list: true, .. }
            )
        })
}

fn union_has_non_list_array_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TKeyedArray { is_list: false, .. }
        )
    })
}

fn is_empty_array_default_for_array_like_param(default_type: &TUnion, param_type: &TUnion) -> bool {
    if !is_empty_array_type(default_type) {
        return false;
    }

    param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TKeyedArray { .. }
                | TAtomic::TIterable { .. }
        )
    })
}

fn is_empty_array_type(union: &TUnion) -> bool {
    let Some(single) = union.get_single() else {
        return false;
    };

    match single {
        TAtomic::TArray {
            key_type,
            value_type,
        } => key_type.is_nothing() && value_type.is_nothing(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => properties.is_empty() && fallback_key_type.is_none() && fallback_value_type.is_none(),
        _ => false,
    }
}

fn should_preserve_docblock_param_origin(signature_type: &TUnion, effective_type: &TUnion) -> bool {
    if signature_type == effective_type {
        return false;
    }

    if signature_type.is_nullable && !effective_type.is_nullable {
        return true;
    }

    if signature_type.is_falsable && !effective_type.is_falsable {
        return true;
    }

    let signature_maybe_truthy_and_falsy =
        !signature_type.is_always_truthy() && !signature_type.is_always_falsy();
    let effective_constant_truthiness =
        effective_type.is_always_truthy() || effective_type.is_always_falsy();

    signature_maybe_truthy_and_falsy && effective_constant_truthiness
}

fn get_alternate_param_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    if var_name.is_empty() {
        return None;
    }

    if let Some(stripped) = var_name.strip_prefix('$') {
        Some(analyzer.interner.intern(stripped))
    } else {
        Some(analyzer.interner.intern(&format!("${}", var_name)))
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
                .as_ref()
                .eq_ignore_ascii_case(class_name)
        })
}
