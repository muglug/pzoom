//! Return statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::r#return::Return;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::{DataFlowNode, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expr::call::callable_validation;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::type_comparator;
use std::rc::Rc;

/// Analyze a return statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    ret: &Return<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let yield_count_before = analysis_data.inferred_yield_types.len();
    let mut return_type = TUnion::void();

    // Psalm's ReturnAnalyzer assigns *named* statement-level `@var` comments
    // into the context before analyzing the returned expression (the unnamed
    // form overrides the expression type below); narrowing inside the
    // expression then proceeds from the commented types.
    if let Some(stmt_start) = analysis_data.current_stmt_start
        && let Some(annotations) = analyzer.get_inline_var_annotations(stmt_start)
    {
        let annotations = annotations.clone();
        for annotation in &annotations {
            let Some(var_name) = annotation.var_name else {
                continue;
            };
            let var_id = pzoom_code_info::VarName::new(analyzer.interner.lookup(var_name));
            let mut annotation_type = annotation.var_type.clone();
            if let Some(existing) = context.get_var_type(&var_id) {
                annotation_type.parent_nodes = existing.parent_nodes.clone();
            }
            context.set_var_type(var_id, annotation_type);
        }
    }

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

        let was_inside_return = context.inside_return;
        context.inside_return = true;
        let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        context.inside_return = was_inside_return;
        if let Some(closure_offset) = inserted_expected_callable_offset {
            context.expected_callable_arg_types.remove(&closure_offset);
        }

        return_type = analysis_data
            .expr_types.get(&value_pos).cloned()
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed);

        if let Some(inline_annotation) =
            get_inline_return_annotation_type(analyzer, value, analysis_data.current_stmt_start)
        {
            analysis_data.expr_types.insert(value_pos, Rc::new(inline_annotation.clone()));
            return_type = inline_annotation;
        }

        // Psalm's ReturnAnalyzer: a `never`-typed return expression means every
        // possible type for it was invalidated — likely dead code.
        if !return_type.types.is_empty() && return_type.is_nothing() {
            let span = value.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::NoValue,
                "All possible types for this return were invalidated - This may be dead code",
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }

        // Psalm's ReturnAnalyzer converts a void-typed return expression to
        // null (`return voidCall();` yields null) instead of reporting a
        // mismatch against the declared type.
        if return_type.is_void() {
            return_type = TUnion::new(pzoom_code_info::TAtomic::TNull);
        }

        // Psalm's ReturnAnalyzer: a value returned from a constructor reports
        // InvalidReturnStatement ("No return values are expected").
        if !analyzer.inside_closure
            && analyzer
                .function_info
                .is_some_and(|function_info| function_info.name == StrId::CONSTRUCT)
            && let Some(declaring_class) = analyzer.get_declaring_class()
        {
            let span = value.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidReturnStatement,
                format!(
                    "No return values are expected for {}::__construct",
                    analyzer.interner.lookup(declaring_class)
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
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
        let declaring_class = analyzer.get_declaring_class();
        crate::type_expander::bind_properties_of_self_names(
            &mut expanded_expected_type,
            declaring_class,
            declaring_class
                .and_then(|class_id| analyzer.codebase.get_class(class_id))
                .and_then(|class_info| class_info.parent_class),
        );
        // NB: `self`/`static` are resolved later by the call analyzers
        // (`localize_special_class_type_*`); resolving them here too would
        // double-resolve (see tests/inference/Class/preventDoubleStaticResolution1).
        // The one exception is a *final* enclosing class: call-site expansion
        // binds `static` firmly there (Psalm's $final flag), so the declared
        // type must too or the two sides disagree.
        let final_declaring_class = analyzer.get_declaring_class().filter(|class_id| {
            analyzer
                .codebase
                .get_class(*class_id)
                .is_some_and(|class_info| class_info.is_final)
        });
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            &mut expanded_expected_type,
            &crate::type_expander::TypeExpansionOptions {
                evaluate_conditional_types: true,
                self_class: final_declaring_class,
                static_class_type: match final_declaring_class {
                    Some(class_id) => crate::type_expander::StaticClassType::Name(class_id),
                    None => crate::type_expander::StaticClassType::None,
                },
                function_is_final: final_declaring_class.is_some(),
                ..Default::default()
            },
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
        // Issue positions target the returned expression, not the whole
        // return statement (Psalm's `new CodeLocation($source, $stmt->expr)`).
        let value_span: Option<(u32, u32)> = ret.value.as_ref().map(|value| {
            let span = value.span();
            (span.start.offset, span.end.offset)
        });
        let value_issue_pos = |analysis_data: &FunctionAnalysisData| -> Option<(u32, u32)> {
            let stmt_start = analysis_data.current_stmt_start?;
            Some(value_span.unwrap_or((
                stmt_start,
                analysis_data.current_stmt_end.unwrap_or(stmt_start),
            )))
        };

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
        // A possibly-undefined return value never satisfies the declared type
        // (Psalm's UnionTypeComparator rejects possibly-undefined inputs
        // outright; pzoom applies the gate at the return site until shape
        // optional-property comparisons are calibrated for it).
        else if has_return_value
            && return_type.possibly_undefined
            && !expected_type.is_mixed()
            && !expected_type.is_void()
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
        }
        // Check type compatibility for non-void/never functions
        else if has_return_value && !expected_type.is_mixed() && !expected_type.is_void() {
            // Psalm/Hakana compare against the declared return type as-is:
            // the function's own template params stay rigid in the
            // container (they are the *caller's* choice), and a type
            // variable in the inferred type records the container as an
            // upper bound for the end-of-function reconcile.
            let expected_type = expected_type.clone();
            // In a generator function, a `return X` provides the Generator's TReturn
            // (its 4th type parameter), not the Generator type itself. For other
            // generator-like declared types (Iterator/Traversable/iterable) the
            // returned value is discarded, so any type is accepted.
            let is_generator = analysis_data.current_function_is_generator
                && expected_type_allows_generator_void_return(
                    &expected_type,
                    analyzer.interner,
                );
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
                if let Some((start, end)) = value_issue_pos(analysis_data) {
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
                        end,
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
                // Psalm: a fully-mixed return against a declared return type
                // reports MixedReturnStatement with the value's dataflow
                // origin. Gated on report_unused until pzoom's
                // mixed-inference parity catches up.
                if analyzer.config.report_unused
                    && return_type.is_mixed()
                    && let Some(value_expr) = ret.value.as_ref()
                {
                    let span = value_expr.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    let origin_secondary = crate::data_flow::mixed_origin_secondary(
                        analyzer,
                        analysis_data,
                        &return_type,
                        span.start.offset,
                    );
                    analysis_data.add_issue(
                        Issue::new(
                            IssueKind::MixedReturnStatement,
                            "Could not infer a return type",
                            analyzer.file_path,
                            span.start.offset,
                            span.end.offset,
                            line,
                            col,
                        )
                        .with_secondary_opt(origin_secondary),
                    );
                }

                // Psalm reports "Possibly-mixed return value" for a partly-mixed
                // return (hasMixed but not isMixed) before the containment
                // check continues, regardless of the concrete part's fit. A
                // mixed promoted from a template's defaulted bound is not a
                // real TMixed in Psalm (hasMixed is false there).
                if !return_type.from_template_default
                    && strip_mixed_types(&return_type).is_some()
                    && let Some((start, end)) = value_issue_pos(analysis_data)
                {
                    let (line, col) = analyzer.get_line_column(start);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedReturnStatement,
                        "Possibly-mixed return value",
                        analyzer.file_path,
                        start,
                        end,
                        line,
                        col,
                    ));
                }

                if let Some(concrete_return_type) = strip_mixed_types(&return_type) {
                    // Psalm's ReturnAnalyzer containment ignores null and
                    // false outright; NullableReturnStatement and
                    // FalsableReturnStatement are independent follow-up
                    // checks below, not alternative kinds of this mismatch.
                    let contained = is_contained_without_coercion(
                        &concrete_return_type,
                        &comparison_expected_type,
                        analyzer.codebase,
                        true,
                        true,
                    );
                    if !contained && let Some((start, end)) = value_issue_pos(analysis_data) {
                        let (line, col) = analyzer.get_line_column(start);
                        // When the comparison coerced (the inferred type is a
                        // wider/less-specific version of the declared type) emit
                        // LessSpecificReturnStatement, otherwise InvalidReturnStatement.
                        let mut comparison_result =
                            type_comparator::TypeComparisonResult::new();
                        type_comparator::union_type_comparator::is_contained_by(
                            analyzer.codebase,
                            &concrete_return_type,
                            &comparison_expected_type,
                            true,
                            true,
                            &mut comparison_result,
                        );
                        let issue_kind = if comparison_result.type_coerced.unwrap_or(false) {
                            IssueKind::LessSpecificReturnStatement
                        } else {
                            IssueKind::InvalidReturnStatement
                        };

                        let actual_type_id =
                            concrete_return_type.get_id(Some(analyzer.interner));
                        let expected_type_id =
                            comparison_expected_type.get_id(Some(analyzer.interner));

                        emit_unknown_class_string_return_literals(
                            analyzer,
                            analysis_data,
                            start,
                            end,
                            &concrete_return_type,
                            &comparison_expected_type,
                        );

                        analysis_data.add_issue(
                            Issue::new(
                                issue_kind,
                                format!(
                                    "The type {} does not match the declared return type {}",
                                    actual_type_id, expected_type_id
                                ),
                                analyzer.file_path,
                                start,
                                end,
                                line,
                                col,
                            )
                            .with_secondary_opt(
                                return_declaration_secondary(
                                    analyzer,
                                    issue_kind,
                                    &expected_type_id,
                                ),
                            ),
                        );
                    }

                    if !concrete_return_type.ignore_nullable_issues
                        && concrete_return_type.is_nullable()
                        && !comparison_expected_type.is_nullable()
                        && !union_has_template(&comparison_expected_type)
                        && let Some((start, end)) = value_issue_pos(analysis_data)
                    {
                        let (line, col) = analyzer.get_line_column(start);
                        let expected_type_id =
                            comparison_expected_type.get_id(Some(analyzer.interner));
                        analysis_data.add_issue(
                            Issue::new(
                                IssueKind::NullableReturnStatement,
                                format!(
                                    "The type {} does not match the declared return type {}",
                                    concrete_return_type.get_id(Some(analyzer.interner)),
                                    expected_type_id
                                ),
                                analyzer.file_path,
                                start,
                                end,
                                line,
                                col,
                            )
                            .with_secondary_opt(return_declaration_secondary(
                                analyzer,
                                IssueKind::NullableReturnStatement,
                                &expected_type_id,
                            )),
                        );
                    }

                    if should_emit_falsable_return_statement(
                        &concrete_return_type,
                        &comparison_expected_type,
                    ) && let Some((start, end)) = value_issue_pos(analysis_data)
                    {
                        let (line, col) = analyzer.get_line_column(start);
                        let expected_type_id =
                            comparison_expected_type.get_id(Some(analyzer.interner));
                        analysis_data.add_issue(
                            Issue::new(
                                IssueKind::FalsableReturnStatement,
                                format!(
                                    "The type {} does not match the declared return type {}",
                                    concrete_return_type.get_id(Some(analyzer.interner)),
                                    expected_type_id
                                ),
                                analyzer.file_path,
                                start,
                                end,
                                line,
                                col,
                            )
                            .with_secondary_opt(return_declaration_secondary(
                                analyzer,
                                IssueKind::FalsableReturnStatement,
                                &expected_type_id,
                            )),
                        );
                    }
                }

                if strip_mixed_types(&return_type).is_none_or(|concrete| {
                    is_contained_without_coercion(
                        &concrete,
                        &comparison_expected_type,
                        analyzer.codebase,
                        true,
                        true,
                    )
                }) && ret.value.as_ref().is_some_and(|value| {
                    should_emit_mixed_return_statement(value, context)
                }) && let Some((start, end)) = value_issue_pos(analysis_data)
                {
                    let (line, col) = analyzer.get_line_column(start);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedReturnStatement,
                        "Could not infer a return type due to mixed return values",
                        analyzer.file_path,
                        start,
                        end,
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

                if is_contained {
                    // Hakana's return analyzer: transfer recorded type-variable
                    // bounds, upgrading plain upper bounds to equality bounds
                    // ("bit of a hack but this ensures that we add strict
                    // checks").
                    let bound_pos = analysis_data
                        .current_stmt_start
                        .zip(analysis_data.current_stmt_end)
                        .map(|(start, end)| {
                            crate::template::bound_location(analyzer, (start, end))
                        });
                    let mut upper_bounds =
                        std::mem::take(&mut comparison_result.type_variable_upper_bounds);
                    for (_, bound) in upper_bounds.iter_mut() {
                        if bound.equality_bound_classlike.is_none() {
                            bound.equality_bound_classlike = Some(pzoom_str::StrId::EMPTY);
                        }
                    }
                    crate::template::record_type_variable_bounds(
                        analysis_data,
                        std::mem::take(&mut comparison_result.type_variable_lower_bounds),
                        upper_bounds,
                        bound_pos,
                    );
                }

                // Psalm's containment check ignores nullability (the
                // nullable mismatch is a separate NullableReturnStatement
                // check, exempt for templated declared types): returning
                // null against `@return T` reports nothing.
                let null_against_template = return_type.is_nullable()
                    && !comparison_expected_type.is_nullable()
                    && union_has_template(&comparison_expected_type)
                    && type_comparator::union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &return_type,
                        &comparison_expected_type,
                        true,
                        return_type.ignore_falsable_issues,
                        &mut type_comparator::TypeComparisonResult::new(),
                    );

                if !(is_contained && !comparison_result.type_coerced.unwrap_or(false))
                    && !null_against_template
                    && let Some((start, end)) = value_issue_pos(analysis_data)
                {
                    let (line, col) = analyzer.get_line_column(start);
                    // Determine the specific issue kind
                    let issue_kind = if return_type.is_nullable()
                        && !comparison_expected_type.is_nullable()
                        && !union_has_template(&comparison_expected_type)
                        && !return_type.ignore_nullable_issues
                    {
                        IssueKind::NullableReturnStatement
                    } else if should_emit_falsable_return_statement(
                        &return_type,
                        &comparison_expected_type,
                    ) && type_comparator::union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &return_type,
                        &comparison_expected_type,
                        true,
                        true,
                        &mut type_comparator::TypeComparisonResult::new(),
                    ) {
                        // Psalm only reports the falsable variant when the
                        // false branch is the SOLE mismatch; `B1|false` vs `A1`
                        // is a plain InvalidReturnStatement.
                        IssueKind::FalsableReturnStatement
                    } else if comparison_result.type_coerced.unwrap_or(false) {
                        // Mirror Psalm's ReturnAnalyzer: a coerced comparison means the
                        // inferred type is a wider/less-specific version of the declared
                        // type. A coercion from mixed reports MixedReturnTypeCoercion
                        // (unless the mixed came from a template's as-mixed bound);
                        // any other coercion is a LessSpecificReturnStatement.
                        if comparison_result.type_coerced_from_mixed.unwrap_or(false)
                            && !comparison_result
                                .type_coerced_from_as_mixed
                                .unwrap_or(false)
                        {
                            IssueKind::MixedReturnTypeCoercion
                        } else {
                            IssueKind::LessSpecificReturnStatement
                        }
                    } else {
                        IssueKind::InvalidReturnStatement
                    };

                    let actual_type_id = return_type.get_id(Some(analyzer.interner));
                    let expected_type_id =
                        comparison_expected_type.get_id(Some(analyzer.interner));

                    emit_unknown_class_string_return_literals(
                        analyzer,
                        analysis_data,
                        start,
                        end,
                        &return_type,
                        &comparison_expected_type,
                    );

                    analysis_data.add_issue(
                        Issue::new(
                            issue_kind,
                            format!(
                                "The type {} does not match the declared return type {}",
                                actual_type_id, expected_type_id
                            ),
                            analyzer.file_path,
                            start,
                            end,
                            line,
                            col,
                        )
                        .with_secondary_opt(
                            return_declaration_secondary(
                                analyzer,
                                issue_kind,
                                &expected_type_id,
                            ),
                        ),
                    );
                }
            }
        }
        // Check if we're not returning a value when one is expected        // Check if we're not returning a value when one is expected
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

    // Hakana `return_analyzer::handle_inout_at_return` (Hack inout ≈ PHP
    // by-ref): by-ref param values flow out of the function at every return.
    handle_byref_at_return(analyzer, analysis_data, context);

    // Hakana `return_analyzer::handle_dataflow`. Function-body branch: the
    // returned expression's dataflow terminates in an unlabelled sink at the
    // returned expression's position. Whole-program (taint) branch: a `Return`
    // node links to the function's `CallTo` node, plus parent-classlike
    // return nodes so overridden methods taint their ancestors' call sites.
    if let Some(value) = ret.value.as_ref() {
        let value_span = value.span();
        let value_pos = make_data_flow_node_position(
            analyzer,
            (value_span.start.offset, value_span.end.offset),
        );

        if let pzoom_code_info::GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind {
            handle_whole_program_return_dataflow(analyzer, analysis_data, &return_type, value_pos);
        } else {
            let return_node = DataFlowNode::get_for_unlabelled_sink(value_pos);
            add_default_dataflow_paths(
                &mut analysis_data.data_flow_graph,
                &return_type.parent_nodes,
                &return_node,
            );
            analysis_data.data_flow_graph.add_node(return_node);
        }
    }

    // Record the return type for later comparison
    analysis_data.inferred_return_types.push(return_type);

    // Mark that control flow has exited
    context.has_returned = true;

    Ok(())
}

/// Hakana `return_analyzer::handle_dataflow`, whole-program branch: the
/// return expression's parents flow into a `Return` node, which flows into
/// the function-like's `CallTo` node (with the storage's added/removed
/// taints), which flows into every parent classlike's `CallTo` node for the
/// same method.
fn handle_whole_program_return_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    return_type: &TUnion,
    value_pos: pzoom_code_info::DataFlowNodePosition,
) {
    use pzoom_code_info::data_flow::node::FunctionLikeIdentifier;

    if !return_type.has_taintable_value() {
        return;
    }

    let Some(function_info) = analyzer.function_info else {
        return;
    };

    let functionlike_id = match function_info.declaring_class {
        Some(classlike_name) => FunctionLikeIdentifier::Method(classlike_name, function_info.name),
        None => FunctionLikeIdentifier::Function(function_info.name),
    };

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    let return_node = DataFlowNode::get_for_return_expr(value_pos);

    for parent_node in &return_type.parent_nodes {
        data_flow_graph.add_path(
            &parent_node.id,
            &return_node.id,
            pzoom_code_info::PathKind::Default,
            function_info.taints.added_taints.clone(),
            function_info.taints.removed_taints.clone(),
        );
    }

    // Psalm positions the method-return node at the native return-type hint
    // when present, else at the function-like's declaration.
    let method_node_pos = function_info
        .return_type_location
        .map(|(start, end)| make_data_flow_node_position(analyzer, (start, end)))
        .unwrap_or_else(|| {
            make_data_flow_node_position(
                analyzer,
                (function_info.start_offset, function_info.start_offset),
            )
        });
    let method_node =
        DataFlowNode::get_for_method_return(&functionlike_id, Some(method_node_pos), None);

    data_flow_graph.add_path(
        &return_node.id,
        &method_node.id,
        pzoom_code_info::PathKind::Default,
        vec![],
        vec![],
    );

    if let FunctionLikeIdentifier::Method(classlike_name, method_name) = functionlike_id
        && method_name != pzoom_str::StrId::CONSTRUCT
        && let Some(classlike_info) = analyzer.codebase.get_class(classlike_name)
    {
        let mut all_parents = classlike_info.all_parent_classes.clone();
        all_parents.extend(classlike_info.all_parent_interfaces.iter().copied());
        all_parents.sort_unstable();
        all_parents.dedup();

        for parent_classlike in all_parents {
            let parent_declares_method = analyzer
                .codebase
                .get_class(parent_classlike)
                .is_some_and(|parent_info| parent_info.methods.contains_key(&method_name));
            if parent_declares_method {
                let new_sink = DataFlowNode::get_for_method_return(
                    &FunctionLikeIdentifier::Method(parent_classlike, method_name),
                    None,
                    None,
                );

                data_flow_graph.add_node(new_sink.clone());
                data_flow_graph.add_path(
                    &method_node.id,
                    &new_sink.id,
                    pzoom_code_info::PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }
    }

    data_flow_graph.add_node(method_node);
    data_flow_graph.add_node(return_node);
}

/// Port of Hakana `return_analyzer::handle_inout_at_return` (Hack inout ≈ PHP
/// by-ref): at a return point the current value of every by-ref parameter
/// flows into an unlabelled variable-use sink, marking the value as consumed
/// by the caller. (Hakana's whole-program branch uses a `FunctionLikeOut` node
/// instead — taint-graph only, skipped. pzoom's `ParamInfo` stores only the
/// param's start offset, so that is used for the sink position.)
pub(crate) fn handle_byref_at_return(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Some(functionlike_storage) = analyzer.function_info else {
        return;
    };

    for param in &functionlike_storage.params {
        if !param.by_ref {
            continue;
        }

        let Some(parent_nodes) = context
            .get_var_type(&analyzer.interner.lookup(param.name))
            .map(|context_type| context_type.parent_nodes.clone())
        else {
            continue;
        };

        // An untouched by-ref param (its only parent is its own Param node)
        // flows nothing back to the caller — Psalm still reports it as
        // UnusedParam (unusedPassByReference). Only written params escape.
        if parent_nodes
            .iter()
            .all(|node| matches!(node.id, pzoom_code_info::DataFlowNodeId::Param(..)))
        {
            continue;
        }

        let new_parent_node = DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(
            analyzer,
            (param.start_offset, param.start_offset),
        ));

        analysis_data
            .data_flow_graph
            .add_node(new_parent_node.clone());

        for parent_node in &parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &new_parent_node.id,
                pzoom_code_info::PathKind::Default,
                vec![],
                vec![],
            );
        }
    }
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
            crate::class_casing::undefined_class_message(analyzer, &class_name),
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
            let mut new_properties = (**properties).clone();
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
                        properties: std::sync::Arc::new(new_properties),
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
    // Psalm ReturnAnalyzer: !ignore_falsable && inferred->isFalsable()
    // && !declared->isFalsable() && (!declared->hasBool() || declared->isTrue())
    // && !declared->hasScalar() — hasScalar is the literal `scalar` type only.
    if return_type.ignore_falsable_issues
        || !return_type.is_falsable()
        || expected_type.is_falsable()
    {
        return false;
    }

    if expected_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TScalar))
    {
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

pub(crate) fn get_inline_return_annotation_type(
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
            ..
        } = atomic
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
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
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
                is_static: false,
                remapped_params: false,
            }
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
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
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
) -> bool {
    if let Expression::Variable(Variable::Direct(direct)) = expr.unparenthesized() {
        let var_id = VarName::new(direct.name);
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

/// The declared return type's location with an explanatory message — shown as
/// a secondary location under return-statement mismatch issues (e.g.
/// "Return type declared as non-nullable `string` here").
fn return_declaration_secondary(
    analyzer: &StatementsAnalyzer<'_>,
    issue_kind: IssueKind,
    expected_type_id: &str,
) -> Option<pzoom_code_info::SecondaryLocation> {
    let (start, end) = analyzer.function_info?.return_type_location?;
    let (line, col) = analyzer.get_line_column(start);
    let message = match issue_kind {
        IssueKind::NullableReturnStatement => {
            format!(
                "Return type declared as non-nullable {} here",
                expected_type_id
            )
        }
        IssueKind::FalsableReturnStatement => {
            format!(
                "Return type declared as non-falsable {} here",
                expected_type_id
            )
        }
        _ => return None,
    };
    Some(pzoom_code_info::SecondaryLocation::new(
        pzoom_code_info::code_location::CodeLocation::new(
            analyzer.file_path,
            start,
            end,
            line,
            col,
        ),
        message,
    ))
}

/// Psalm's `Union::hasTemplate()` as used by the NullableReturnStatement
/// exemption: a declared return type mentioning a template parameter does
/// not report nullable returns (ReturnAnalyzer's `!hasTemplate()` guard).
fn union_has_template(union: &pzoom_code_info::TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, pzoom_code_info::TAtomic::TTemplateParam { .. }))
}
