//! Return statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::r#return::Return;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{DataFlowNode, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expr::call::{callable_validation, function_call_analyzer};
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::type_comparator;
use crate::template::TemplateMap;

/// Analyze a return statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    ret: &Return<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let yield_count_before = analysis_data.inferred_yield_types.len();
    let mut return_type = TUnion::void();

    if let Some(value) = ret.value.as_ref() {
        let inserted_expected_callable_offset =
            if let Some(expected_return_type) = analyzer.get_expected_return_type() {
                get_closure_like_expression_offset(value).and_then(|closure_offset| {
                    if callable_validation::union_has_callable(expected_return_type) {
                        context
                            .expected_callable_arg_types
                            .insert(closure_offset, expected_return_type.clone());
                        Some(closure_offset)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

        let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        if let Some(closure_offset) = inserted_expected_callable_offset {
            context.expected_callable_arg_types.remove(&closure_offset);
        }

        return_type = analysis_data
            .get_expr_type(value_pos)
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed);

        if let Some(inline_annotation) =
            get_inline_return_annotation_type(analyzer, value, analysis_data.current_stmt_start)
        {
            analysis_data.set_expr_type(value_pos, inline_annotation.clone());
            return_type = inline_annotation;
        }
    }

    let return_expression_uses_yield =
        analysis_data.inferred_yield_types.len() > yield_count_before;

    if analyzer
        .function_info
        .is_some_and(|function_info| function_info.returns_by_ref)
        && ret.value.is_some()
        && !is_reference_returnable_expression(ret.value.as_ref().unwrap())
        && let Some(value) = ret.value.as_ref()
    {
        let span = value.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::NonVariableReferenceReturn,
            "Only variable references should be returned by reference",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    // Check against expected return type
    if let Some(expected_type) = analyzer.get_expected_return_type() {
        // A conditional return type's branch is unknown inside the body, so the body
        // may legitimately return either branch — compare against the union of both
        // (Psalm expands TConditional when used as a concrete type).
        let mut expanded_expected_type = expected_type.clone();
        // NB: `self`/`static` are resolved later by the call analyzers
        // (`localize_special_class_type_*`); resolving them here too would
        // double-resolve (see tests/inference/Class/preventDoubleStaticResolution1).
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            &mut expanded_expected_type,
            &crate::type_expander::TypeExpansionOptions { evaluate_conditional_types: true, ..Default::default() },
        );
        let expected_type = &expanded_expected_type;
        if let Some(return_expr) = ret.value.as_ref() {
            if let Some(special_type_name) =
                infer_explicit_special_return_type_name(analyzer, return_expr)
            {
                if union_contains_special_class_name(expected_type, special_type_name) {
                    return_type = rewrite_declaring_class_named_object_to_special(
                        &return_type,
                        analyzer.get_declaring_class(),
                        special_type_name,
                    );
                }
            }
        }

        let has_return_value = ret.value.is_some();

        // Check if we're returning a value from a never function
        if has_return_value && expected_type.is_nothing() {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    "Cannot return a value from a function with never return type",
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
        // Check if we're returning a value from a void function
        else if has_return_value && expected_type.is_void() {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    format!(
                        "No return values are expected for this function, but {} was returned",
                        return_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
        // Check type compatibility for non-void/never functions
        else if has_return_value && !expected_type.is_mixed() && !expected_type.is_void() {
            // A function's own template parameters are abstract inside its body
            // and are rigid: only a value derived from the same template parameter
            // satisfies a `@return T`. Returning a concrete value (even one within
            // the template's `as` bound) is an InvalidReturnStatement, matching
            // Psalm's ReturnTypeAnalyzer.
            if expected_is_rigid_template(analyzer, expected_type)
                && return_violates_rigid_template(&return_type, expected_type)
            {
                if let Some(start) = analysis_data.current_stmt_start {
                    let (line, col) = analyzer.get_line_column(start);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidReturnStatement,
                        format!(
                            "The type {} does not match the declared return type {}",
                            return_type.get_id(Some(analyzer.interner)),
                            expected_type.get_id(Some(analyzer.interner))
                        ),
                        analyzer.file_path,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        line,
                        col,
                    ));
                }
            } else {
            let expected_type = resolve_expected_return_type_templates(analyzer, expected_type);
            // In a generator function, a `return X` provides the Generator's TReturn
            // (its 4th type parameter), not the Generator type itself. For other
            // generator-like declared types (Iterator/Traversable/iterable) the
            // returned value is discarded, so any type is accepted.
            let is_generator = analysis_data.current_function_is_generator
                && expected_type_allows_generator_void_return(&expected_type, analyzer.interner);
            let comparison_expected_type = if is_generator {
                get_generator_return_type(&expected_type, analyzer.interner)
                    .unwrap_or_else(TUnion::mixed)
            } else if return_expression_uses_yield {
                get_generator_return_type(&expected_type, analyzer.interner)
                    .unwrap_or_else(|| expected_type.clone())
            } else {
                expected_type
            };

            // An object with a `__toString` method returned where a string is
            // expected is implicitly cast to string. Report ImplicitToStringCast
            // (matching Psalm) and accept the return rather than flagging a mismatch.
            if let Some(casted) = union_cast_stringable_to_string(analyzer, &return_type)
                && is_contained_without_coercion(
                    &casted,
                    &comparison_expected_type,
                    analyzer.codebase,
                    casted.ignore_nullable_issues,
                    casted.ignore_falsable_issues,
                )
            {
                if let Some(start) = analysis_data.current_stmt_start {
                    let (line, col) = analyzer.get_line_column(start);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ImplicitToStringCast,
                        format!(
                            "Object with a __toString method is implicitly converted to \
                             the declared return type {}",
                            comparison_expected_type.get_id(Some(analyzer.interner))
                        ),
                        analyzer.file_path,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        line,
                        col,
                    ));
                }
            }
            // Skip mixed return validation for now - without docblock parsing,
            // we get too many false positives from untyped parameters
            else if return_type.is_mixed()
                || return_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TNonEmptyMixed))
            {
                if let Some(concrete_return_type) = strip_mixed_types(&return_type)
                    && !is_contained_without_coercion(
                        &concrete_return_type,
                        &comparison_expected_type,
                        analyzer.codebase,
                        concrete_return_type.ignore_nullable_issues,
                        concrete_return_type.ignore_falsable_issues,
                    )
                {
                    if let Some(start) = analysis_data.current_stmt_start {
                        let (line, col) = analyzer.get_line_column(start);
                        let issue_kind = if concrete_return_type.is_nullable
                            && !comparison_expected_type.is_nullable
                            && !concrete_return_type.ignore_nullable_issues
                        {
                            IssueKind::NullableReturnStatement
                        } else if should_emit_falsable_return_statement(
                            &concrete_return_type,
                            &comparison_expected_type,
                        ) {
                            IssueKind::FalsableReturnStatement
                        } else {
                            // Mirror Psalm's ReturnAnalyzer: when the inferred type is not
                            // contained but the comparison coerced (the inferred type is a
                            // wider/less-specific version of the declared type) emit
                            // LessSpecificReturnStatement, otherwise InvalidReturnStatement.
                            let mut comparison_result =
                                type_comparator::TypeComparisonResult::new();
                            type_comparator::union_type_comparator::is_contained_by(
                                analyzer.codebase,
                                &concrete_return_type,
                                &comparison_expected_type,
                                concrete_return_type.ignore_nullable_issues,
                                concrete_return_type.ignore_falsable_issues,
                                &mut comparison_result,
                            );
                            if comparison_result.type_coerced.unwrap_or(false) {
                                IssueKind::LessSpecificReturnStatement
                            } else {
                                IssueKind::InvalidReturnStatement
                            }
                        };

                        let actual_type_id = concrete_return_type.get_id(Some(analyzer.interner));
                        let expected_type_id =
                            comparison_expected_type.get_id(Some(analyzer.interner));

                        emit_unknown_class_string_return_literals(
                            analyzer,
                            analysis_data,
                            start,
                            analysis_data.current_stmt_end.unwrap_or(start),
                            &concrete_return_type,
                            &comparison_expected_type,
                        );

                        analysis_data.add_issue(Issue::new(
                            issue_kind,
                            format!(
                                "The type {} does not match the declared return type {}",
                                actual_type_id, expected_type_id
                            ),
                            analyzer.file_path,
                            start,
                            analysis_data.current_stmt_end.unwrap_or(start),
                            line,
                            col,
                        ));
                    }
                } else if ret.value.as_ref().is_some_and(|value| {
                    should_emit_mixed_return_statement(value, context, analyzer)
                }) && let Some(start) = analysis_data.current_stmt_start
                {
                    let (line, col) = analyzer.get_line_column(start);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedReturnStatement,
                        "Could not infer a return type due to mixed return values",
                        analyzer.file_path,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        line,
                        col,
                    ));
                }
            } else {
                let mut comparison_result = type_comparator::TypeComparisonResult::new();
                let is_contained = type_comparator::union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &return_type,
                    &comparison_expected_type,
                    return_type.ignore_nullable_issues,
                    return_type.ignore_falsable_issues,
                    &mut comparison_result,
                );

                if !(is_contained && !comparison_result.type_coerced.unwrap_or(false))
                    && let Some(start) = analysis_data.current_stmt_start
                {
                    let (line, col) = analyzer.get_line_column(start);
                    // Determine the specific issue kind
                    let issue_kind = if return_type.is_nullable
                        && !comparison_expected_type.is_nullable
                        && !return_type.ignore_nullable_issues
                    {
                        IssueKind::NullableReturnStatement
                    } else if should_emit_falsable_return_statement(
                        &return_type,
                        &comparison_expected_type,
                    ) {
                        IssueKind::FalsableReturnStatement
                    } else if comparison_result.type_coerced.unwrap_or(false) {
                        // Mirror Psalm's ReturnAnalyzer: a coerced comparison means the
                        // inferred type is a wider/less-specific version of the declared
                        // type. Coercion from mixed is reported as MixedReturnTypeCoercion;
                        // any other coercion is a LessSpecificReturnStatement.
                        if union_contains_mixed_deep(&return_type) {
                            IssueKind::MixedReturnTypeCoercion
                        } else {
                            IssueKind::LessSpecificReturnStatement
                        }
                    } else {
                        IssueKind::InvalidReturnStatement
                    };

                    let actual_type_id = return_type.get_id(Some(analyzer.interner));
                    let expected_type_id = comparison_expected_type.get_id(Some(analyzer.interner));

                    emit_unknown_class_string_return_literals(
                        analyzer,
                        analysis_data,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        &return_type,
                        &comparison_expected_type,
                    );

                    analysis_data.add_issue(Issue::new(
                        issue_kind,
                        format!(
                            "The type {} does not match the declared return type {}",
                            actual_type_id, expected_type_id
                        ),
                        analyzer.file_path,
                        start,
                        analysis_data.current_stmt_end.unwrap_or(start),
                        line,
                        col,
                    ));
                }
            }
            }
        }
        // Check if we're not returning a value when one is expected
        else if !has_return_value
            && !expected_type.is_void()
            && !expected_type.is_mixed()
            && !(analysis_data.current_function_is_generator
                && expected_type_allows_generator_void_return(expected_type, analyzer.interner))
        {
            if let Some(start) = analysis_data.current_stmt_start {
                let (line, col) = analyzer.get_line_column(start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidReturnStatement,
                    format!(
                        "Empty return statement not expected, function should return {}",
                        expected_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    start,
                    analysis_data.current_stmt_end.unwrap_or(start),
                    line,
                    col,
                ));
            }
        }
    }

    let return_span = ret.span();
    let return_node = DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(
        analyzer,
        (return_span.start.offset, return_span.end.offset),
    ));
    analysis_data.data_flow_graph.add_node(return_node.clone());
    add_default_dataflow_paths(
        &mut analysis_data.data_flow_graph,
        &return_type.parent_nodes,
        &return_node,
    );

    // Record the return type for later comparison
    analysis_data.add_return_type(return_type);

    // Mark that control flow has exited
    context.has_returned = true;

    Ok(())
}

/// Returns true if the declared return type consists solely of template
/// parameters defined by the current function or its declaring class. Such a
/// type is "rigid": only a value of the same template parameter satisfies it.
fn expected_is_rigid_template(analyzer: &StatementsAnalyzer<'_>, expected_type: &TUnion) -> bool {
    if expected_type.types.is_empty() {
        return false;
    }

    expected_type.types.iter().all(|atomic| {
        if let TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        } = atomic
        {
            template_is_defined_locally(analyzer, *name, *defining_entity)
        } else {
            false
        }
    })
}

fn template_is_defined_locally(
    analyzer: &StatementsAnalyzer<'_>,
    name: StrId,
    defining_entity: StrId,
) -> bool {
    if let Some(function_info) = analyzer.function_info
        && function_info.name == defining_entity
        && function_info.template_types.iter().any(|t| t.name == name)
    {
        return true;
    }

    if let Some(declaring_class) = analyzer.get_declaring_class()
        && declaring_class == defining_entity
        && let Some(class_info) = analyzer.codebase.get_class(declaring_class)
    {
        return class_info.template_types.iter().any(|t| t.name == name);
    }

    false
}

/// Returns true if `return_type` does not satisfy a rigid template return type:
/// it carries no matching template parameter and is a fully concrete, non-null
/// type (so it is not the template value itself).
fn return_violates_rigid_template(return_type: &TUnion, expected_type: &TUnion) -> bool {
    if return_type.types.is_empty()
        || return_type.is_mixed()
        || return_type.is_nullable
        || return_type.is_nothing()
    {
        return false;
    }

    let expected_names: Vec<StrId> = expected_type
        .types
        .iter()
        .filter_map(|atomic| match atomic {
            TAtomic::TTemplateParam { name, .. } => Some(*name),
            _ => None,
        })
        .collect();

    // If the returned type is derived from one of the expected template
    // parameters (directly, or via an intersection such as `T&object` produced
    // by `is_object()` narrowing), it is acceptable.
    let carries_matching_template = return_type
        .types
        .iter()
        .any(|atomic| atomic_carries_template(atomic, &expected_names));

    !carries_matching_template
}

fn atomic_carries_template(atomic: &TAtomic, expected_names: &[StrId]) -> bool {
    match atomic {
        TAtomic::TTemplateParam { name, .. } => expected_names.contains(name),
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|inner| atomic_carries_template(inner, expected_names)),
        // A `static` (late-static-bound) return inside a `@return T` function can
        // only originate from a template-typed receiver, so its runtime type is
        // the template parameter itself — treat it as template-derived rather
        // than a concrete mismatch.
        TAtomic::TNamedObject {
            name, is_static, ..
        } => *name == StrId::STATIC || *is_static,
        _ => false,
    }
}

fn resolve_expected_return_type_templates(
    analyzer: &StatementsAnalyzer<'_>,
    expected_type: &TUnion,
) -> TUnion {
    let Some(function_info) = analyzer.function_info else {
        return expected_type.clone();
    };

    let mut template_defaults = TemplateMap::new();
    template_defaults.extend_overlay(function_call_analyzer::get_template_defaults(function_info));

    if let Some(declaring_class) = function_info.declaring_class
        && let Some(class_info) = analyzer.codebase.get_class(declaring_class)
    {
        template_defaults.extend_overlay(function_call_analyzer::get_class_template_defaults(
            class_info,
        ));
    }

    if template_defaults.is_empty() {
        expected_type.clone()
    } else {
        function_call_analyzer::replace_templates_in_union(
            expected_type,
            &TemplateMap::new(),
            &template_defaults,
        )
    }
}

fn unions_are_array_like(left: &TUnion, right: &TUnion) -> bool {
    is_array_like_union(left) && is_array_like_union(right)
}

fn strip_mixed_types(union: &TUnion) -> Option<TUnion> {
    let filtered: Vec<_> = union
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        .cloned()
        .collect();

    if filtered.is_empty() {
        None
    } else {
        let mut stripped = TUnion::from_types(filtered);
        stripped.from_docblock = union.from_docblock;
        stripped.ignore_nullable_issues = union.ignore_nullable_issues;
        stripped.ignore_falsable_issues = union.ignore_falsable_issues;
        Some(stripped)
    }
}

fn is_contained_without_coercion(
    input_type: &TUnion,
    container_type: &TUnion,
    codebase: &pzoom_code_info::CodebaseInfo,
    ignore_null: bool,
    ignore_false: bool,
) -> bool {
    let mut comparison_result = type_comparator::TypeComparisonResult::new();
    let is_contained = type_comparator::union_type_comparator::is_contained_by(
        codebase,
        input_type,
        container_type,
        ignore_null,
        ignore_false,
        &mut comparison_result,
    );

    is_contained && !comparison_result.type_coerced.unwrap_or(false)
}

fn union_contains_class_string_like(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_class_string_like)
}

fn atomic_contains_class_string_like(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => union_contains_class_string_like(as_type),
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_contains_class_string_like)
        }
        _ => false,
    }
}

fn union_array_like_value_type(union: &TUnion) -> Option<TUnion> {
    if union.types.len() != 1 {
        return None;
    }

    let atomic = union.types.first()?;
    match atomic {
        TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
            Some((**value_type).clone())
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            Some((**value_type).clone())
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut combined: Option<TUnion> = None;
            for property_type in properties.values() {
                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, property_type, false)
                } else {
                    property_type.clone()
                });
            }

            if let Some(fallback_value_type) = fallback_value_type {
                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, fallback_value_type, false)
                } else {
                    (**fallback_value_type).clone()
                });
            }

            combined
        }
        _ => None,
    }
}

fn emit_unknown_class_string_return_literals(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_start: u32,
    issue_end: u32,
    return_type: &TUnion,
    expected_type: &TUnion,
) {
    if !union_contains_class_string_like(expected_type)
        && union_array_like_value_type(expected_type)
            .is_none_or(|expected_value| !union_contains_class_string_like(&expected_value))
    {
        return;
    }

    let mut unknown_classes = FxHashSet::default();
    collect_unknown_class_string_literals_from_union(analyzer, return_type, &mut unknown_classes);

    if unknown_classes.is_empty() {
        return;
    }

    let (line, col) = analyzer.get_line_column(issue_start);
    let mut unknown_classes = unknown_classes.into_iter().collect::<Vec<_>>();
    unknown_classes.sort();

    for class_name in unknown_classes {
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedClass,
            format!("Class {} does not exist", class_name),
            analyzer.file_path,
            issue_start,
            issue_end,
            line,
            col,
        ));
    }
}

fn collect_unknown_class_string_literals_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
    unknown_classes: &mut FxHashSet<String>,
) {
    for atomic in &union.types {
        match atomic {
            TAtomic::TLiteralString { value } => {
                if value.is_empty()
                    || value == pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE
                {
                    continue;
                }

                if analyzer.codebase.resolve_classlike_name(value).is_none() {
                    unknown_classes.insert(value.clone());
                }
            }
            TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                collect_unknown_class_string_literals_from_union(
                    analyzer,
                    value_type,
                    unknown_classes,
                );
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                collect_unknown_class_string_literals_from_union(
                    analyzer,
                    value_type,
                    unknown_classes,
                );
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                for property_type in properties.values() {
                    collect_unknown_class_string_literals_from_union(
                        analyzer,
                        property_type,
                        unknown_classes,
                    );
                }

                if let Some(fallback_value_type) = fallback_value_type {
                    collect_unknown_class_string_literals_from_union(
                        analyzer,
                        fallback_value_type,
                        unknown_classes,
                    );
                }
            }
            _ => {}
        }
    }
}

fn union_contains_explicit_false(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_explicit_false)
}

fn union_contains_mixed_deep(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_mixed_deep)
}

fn atomic_contains_mixed_deep(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => true,
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => union_contains_mixed_deep(key_type) || union_contains_mixed_deep(value_type),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_contains_mixed_deep(value_type)
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => union_contains_mixed_deep(key_type) || union_contains_mixed_deep(value_type),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            properties.values().any(union_contains_mixed_deep)
                || fallback_key_type
                    .as_ref()
                    .is_some_and(|key_type| union_contains_mixed_deep(key_type))
                || fallback_value_type
                    .as_ref()
                    .is_some_and(|value_type| union_contains_mixed_deep(value_type))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_mixed_deep(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_mixed_deep),
        _ => false,
    }
}

/// If the union contains any object with a `__toString` method (possibly nested in
/// array value/key types), return a copy with those objects replaced by `string`.
/// Returns None if no such object is present, so callers can tell whether an implicit
/// to-string cast actually applies.
fn union_cast_stringable_to_string(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> Option<TUnion> {
    let mut changed = false;
    let mut atomics = Vec::with_capacity(union.types.len());
    for atomic in &union.types {
        let (new_atomic, atomic_changed) = atomic_cast_stringable_to_string(analyzer, atomic);
        changed |= atomic_changed;
        atomics.push(new_atomic);
    }

    if !changed {
        return None;
    }

    let mut result = union.clone();
    result.types = atomics;
    Some(result)
}

fn atomic_cast_stringable_to_string(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> (TAtomic, bool) {
    match atomic {
        TAtomic::TNamedObject { name, .. }
            if analyzer
                .codebase
                .get_class(*name)
                .is_some_and(|class_info| class_info.methods.contains_key(&StrId::TO_STRING)) =>
        {
            (TAtomic::TString, true)
        }
        TAtomic::TList { value_type } => {
            if let Some(new_value) = union_cast_stringable_to_string(analyzer, value_type) {
                (
                    TAtomic::TList {
                        value_type: Box::new(new_value),
                    },
                    true,
                )
            } else {
                (atomic.clone(), false)
            }
        }
        TAtomic::TNonEmptyList { value_type } => {
            if let Some(new_value) = union_cast_stringable_to_string(analyzer, value_type) {
                (
                    TAtomic::TNonEmptyList {
                        value_type: Box::new(new_value),
                    },
                    true,
                )
            } else {
                (atomic.clone(), false)
            }
        }
        TAtomic::TArray {
            key_type,
            value_type,
        } => {
            if let Some(new_value) = union_cast_stringable_to_string(analyzer, value_type) {
                (
                    TAtomic::TArray {
                        key_type: key_type.clone(),
                        value_type: Box::new(new_value),
                    },
                    true,
                )
            } else {
                (atomic.clone(), false)
            }
        }
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => {
            if let Some(new_value) = union_cast_stringable_to_string(analyzer, value_type) {
                (
                    TAtomic::TNonEmptyArray {
                        key_type: key_type.clone(),
                        value_type: Box::new(new_value),
                    },
                    true,
                )
            } else {
                (atomic.clone(), false)
            }
        }
        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            let mut changed = false;
            let mut new_properties = properties.clone();
            for value in new_properties.values_mut() {
                if let Some(new_value) = union_cast_stringable_to_string(analyzer, value) {
                    *value = new_value;
                    changed = true;
                }
            }
            let new_fallback_value = fallback_value_type.as_ref().map(|fv| {
                match union_cast_stringable_to_string(analyzer, fv) {
                    Some(new_value) => {
                        changed = true;
                        Box::new(new_value)
                    }
                    None => fv.clone(),
                }
            });
            if changed {
                (
                    TAtomic::TKeyedArray {
                        properties: new_properties,
                        is_list: *is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: new_fallback_value,
                    },
                    true,
                )
            } else {
                (atomic.clone(), false)
            }
        }
        _ => (atomic.clone(), false),
    }
}

fn should_emit_falsable_return_statement(return_type: &TUnion, expected_type: &TUnion) -> bool {
    if return_type.ignore_falsable_issues
        || unions_are_array_like(return_type, expected_type)
        || !union_contains_explicit_false(return_type)
        || union_contains_explicit_false(expected_type)
    {
        return false;
    }

    if union_has_scalar(expected_type) {
        return false;
    }

    !union_has_boolish(expected_type) || union_is_true_only(expected_type)
}

fn union_has_boolish(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| atomic_contains_boolish(atomic))
}

fn union_is_true_only(union: &TUnion) -> bool {
    union.types.len() == 1 && matches!(union.types.first(), Some(TAtomic::TTrue))
}

fn atomic_contains_boolish(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => true,
        TAtomic::TTemplateParam { as_type, .. } => union_has_boolish(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_contains_boolish(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_boolish),
        _ => false,
    }
}

fn union_has_scalar(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| atomic_contains_scalar(atomic))
}

fn atomic_contains_scalar(atomic: &TAtomic) -> bool {
    match atomic {
        // Boolean types (`bool`/`true`/`false`) are intentionally excluded: Psalm's
        // falsable-return guard uses `hasScalar()` (TScalar-family, not bool) and a
        // separate `isTrue()` exception, so a `true`/`bool` expected return still
        // reaches the boolish check below (which yields FalsableReturnStatement for
        // an expected `true`).
        TAtomic::TScalar
        | TAtomic::TNumeric
        | TAtomic::TArrayKey
        | TAtomic::TInt
        | TAtomic::TFloat
        | TAtomic::TString
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TClassString { .. }
        | TAtomic::TIntRange { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => union_has_scalar(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_contains_scalar(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_scalar),
        _ => false,
    }
}

fn atomic_contains_explicit_false(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TFalse => true,
        TAtomic::TTemplateParam { as_type, .. } => union_contains_explicit_false(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_contains_explicit_false(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_explicit_false),
        _ => false,
    }
}

fn is_array_like_union(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TIterable { .. } => true,
            TAtomic::TTemplateParam { as_type, .. } => is_array_like_union(as_type),
            _ => false,
        })
}

pub(crate) fn is_reference_returnable_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Variable(_)
            | Expression::Access(Access::Property(_))
            | Expression::Access(Access::StaticProperty(_))
    )
}

fn get_closure_like_expression_offset(expr: &Expression<'_>) -> Option<u32> {
    match expr.unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn get_inline_return_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    stmt_start: Option<u32>,
) -> Option<TUnion> {
    let direct_var_id = match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            Some(analyzer.interner.intern(direct.name))
        }
        _ => None,
    };

    let mut candidate_offsets = Vec::new();
    candidate_offsets.push(expr.start_offset() as u32);
    if let Some(stmt_start) = stmt_start {
        if stmt_start != expr.start_offset() as u32 {
            candidate_offsets.push(stmt_start);
        }
    }

    for offset in candidate_offsets {
        let Some(annotations) = analyzer.get_inline_var_annotations(offset) else {
            continue;
        };

        let mut unnamed_match = None;
        for annotation in annotations {
            match annotation.var_name {
                Some(name) if Some(name) == direct_var_id => {
                    return Some(annotation.var_type.clone());
                }
                None if unnamed_match.is_none() => {
                    unnamed_match = Some(annotation.var_type.clone())
                }
                _ => {}
            }
        }

        if unnamed_match.is_some() {
            return unnamed_match;
        }
    }

    None
}

fn get_generator_return_type(
    expected_type: &TUnion,
    interner: &pzoom_str::Interner,
) -> Option<TUnion> {
    let mut combined_return_type = None;

    for atomic in &expected_type.types {
        let TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
        .. } = atomic
        else {
            continue;
        };

        let class_name = interner.lookup(*name);
        if !class_name
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("Generator")
        {
            continue;
        }

        let generator_return_type = type_params.get(3).cloned().unwrap_or_else(TUnion::mixed);
        combined_return_type = Some(if let Some(existing) = combined_return_type {
            combine_union_types(&existing, &generator_return_type, false)
        } else {
            generator_return_type
        });
    }

    combined_return_type
}

fn expected_type_allows_generator_void_return(
    expected_type: &TUnion,
    interner: &pzoom_str::Interner,
) -> bool {
    expected_type.types.iter().any(|atomic| match atomic {
        TAtomic::TIterable { .. } => true,
        TAtomic::TNamedObject { name, .. } => {
            matches!(
                interner
                    .lookup(*name)
                    .trim_start_matches('\\')
                    .to_ascii_lowercase()
                    .as_str(),
                "generator" | "traversable" | "iterator" | "iteratoraggregate"
            )
        }
        _ => false,
    })
}

fn infer_explicit_special_return_type_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct))
            if direct.name.eq_ignore_ascii_case("this")
                || direct.name.eq_ignore_ascii_case("$this") =>
        {
            Some(StrId::STATIC)
        }
        Expression::Clone(clone_expr) => {
            infer_explicit_special_return_type_name(analyzer, clone_expr.object.unparenthesized())
        }
        Expression::Instantiation(instantiation) => {
            infer_special_class_type_name(analyzer, instantiation.class)
        }
        Expression::Call(call) => {
            if let Call::StaticMethod(static_call) = call {
                infer_special_class_type_name(analyzer, static_call.class)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn infer_special_class_type_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Self_(_) => Some(StrId::SELF),
        Expression::Static(_) => Some(StrId::STATIC),
        Expression::Parent(_) => Some(StrId::PARENT),
        Expression::Identifier(id) => {
            let value = id.value();
            if value.eq_ignore_ascii_case("self") {
                Some(StrId::SELF)
            } else if value.eq_ignore_ascii_case("static") {
                Some(StrId::STATIC)
            } else if value.eq_ignore_ascii_case("parent") {
                Some(StrId::PARENT)
            } else {
                let span = id.span();
                let source_value = analyzer
                    .get_source_substring(span.start.offset as usize, span.end.offset as usize)
                    .trim();
                if source_value.eq_ignore_ascii_case("self") {
                    Some(StrId::SELF)
                } else if source_value.eq_ignore_ascii_case("static") {
                    Some(StrId::STATIC)
                } else if source_value.eq_ignore_ascii_case("parent") {
                    Some(StrId::PARENT)
                } else {
                    None
                }
            }
        }
        _ => None,
    }
}

fn rewrite_declaring_class_named_object_to_special(
    union: &TUnion,
    declaring_class: Option<StrId>,
    special_name: StrId,
) -> TUnion {
    let Some(declaring_class) = declaring_class else {
        return union.clone();
    };

    let mut rewritten = Vec::with_capacity(union.types.len());
    for atomic in &union.types {
        let rewritten_atomic =
            rewrite_declaring_class_atomic_to_special(atomic, declaring_class, special_name);
        if !rewritten.contains(&rewritten_atomic) {
            rewritten.push(rewritten_atomic);
        }
    }

    TUnion::from_types(rewritten)
}

fn rewrite_declaring_class_atomic_to_special(
    atomic: &TAtomic,
    declaring_class: StrId,
    special_name: StrId,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
            let rewritten_name = if *name == declaring_class
                || matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT)
            {
                special_name
            } else {
                *name
            };

            TAtomic::TNamedObject {
                name: rewritten_name,
                type_params: type_params.as_ref().map(|params| {
                    params
                        .iter()
                        .map(|param| {
                            rewrite_declaring_class_named_object_to_special(
                                param,
                                Some(declaring_class),
                                special_name,
                            )
                        })
                        .collect()
                }),
            is_static: false, remapped_params: false }
        }
        TAtomic::TObjectIntersection { types } => {
            let mut rewritten = Vec::with_capacity(types.len());
            for nested in types {
                let rewritten_nested = rewrite_declaring_class_atomic_to_special(
                    nested,
                    declaring_class,
                    special_name,
                );
                if !rewritten.contains(&rewritten_nested) {
                    rewritten.push(rewritten_nested);
                }
            }
            TAtomic::TObjectIntersection { types: rewritten }
        }
        _ => atomic.clone(),
    }
}

fn union_contains_special_class_name(union: &TUnion, special_name: StrId) -> bool {
    union
        .types
        .iter()
        .any(|atomic| atomic_contains_named_class(atomic, special_name))
}

fn atomic_contains_named_class(atomic: &TAtomic, class_name: StrId) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
            if *name == class_name {
                return true;
            }

            type_params.as_ref().is_some_and(|params| {
                params
                    .iter()
                    .any(|param| union_contains_special_class_name(param, class_name))
            })
        }
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|nested| atomic_contains_named_class(nested, class_name)),
        _ => false,
    }
}

fn should_emit_mixed_return_statement(
    expr: &Expression<'_>,
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    if let Expression::Variable(Variable::Direct(direct)) = expr.unparenthesized() {
        let var_id = analyzer.interner.intern(direct.name);
        if context.static_var_ids.contains(&var_id) {
            return true;
        }
    }

    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return false;
    };

    let Expression::Identifier(identifier) = function_call.function.unparenthesized() else {
        return false;
    };

    identifier
        .value()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("array_pop")
}


