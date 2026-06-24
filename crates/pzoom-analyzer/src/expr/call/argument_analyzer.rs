//! Single argument analyzer.

use mago_syntax::cst::cst::argument::Argument;

use pzoom_code_info::{
    DataFlowNode, DataFlowNodeId, DataFlowNodeKind, FunctionLikeIdentifier, GraphKind, Issue,
    IssueKind, PathKind, TAtomic, TUnion,
};

use super::callable_validation::*;
use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use mago_syntax::cst::cst::expression::Expression;
use pzoom_code_info::combine_union_types;
use pzoom_code_info::functionlike_info::ParamInfo;

/// Analyze a single function/method argument.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    argument: &Argument<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    // Hakana's `arguments_analyzer` analyzes argument expressions with
    // `inside_general_use` set (Psalm marks every 'arg' edge as a use), so a
    // variable passed to a call counts as used.
    let was_inside_general_use = context.inside_general_use;
    let was_inside_call = context.inside_call;
    context.inside_general_use = true;
    context.inside_call = true;
    let arg_pos = expression_analyzer::analyze(analyzer, argument.value(), analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    context.inside_call = was_inside_call;

    // The argument value is consumed by the call: Psalm's 'arg' edges count
    // as uses unconditionally, so non-fetch arguments (e.g. `foo(++$a)`)
    // flow into a use sink too.
    if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
        && let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned()
        && !arg_type.parent_nodes.is_empty()
    {
        let arg_sink = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
            crate::data_flow::make_data_flow_node_position(analyzer, arg_pos),
        );
        for parent_node in &arg_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &arg_sink.id,
                pzoom_code_info::PathKind::Default,
                vec![],
                vec![],
            );
        }
        analysis_data.data_flow_graph.add_node(arg_sink);
    }

    // Check if this is a named argument
    if let Argument::Named(named) = argument {
        // Named arguments are handled differently in argument resolution
        // The name is available via named.name
        let _ = named.name;
    }

    // Check if this is a variadic/spread argument (...$arg)
    if argument.is_unpacked() {
        // Spread arguments should be arrays/iterables
        if let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned() {
            if arg_type.is_mixed() {
                // Unpacking a mixed value: Psalm/Hakana report MixedArgument rather
                // than silently treating mixed as a valid collection.
                if !analyzer.config.is_issue_suppressed("MixedArgument") {
                    let (line, col) = analyzer.get_line_column(arg_pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedArgument,
                        "Unpacking requires a collection type, but mixed was provided".to_string(),
                        analyzer.file_path,
                        arg_pos.0,
                        arg_pos.1,
                        line,
                        col,
                    ));
                }
            } else {
                let is_iterable = arg_type.types.iter().any(|t| {
                    t.is_array()
                        || matches!(
                            t,
                            TAtomic::TClassStringMap { .. } | TAtomic::TIterable { .. }
                        )
                        || is_traversable_object(analyzer, t)
                });

                if !is_iterable {
                    let (line, col) = analyzer.get_line_column(arg_pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidArgument,
                        "Spread operator requires an array or iterable".to_string(),
                        analyzer.file_path,
                        arg_pos.0,
                        arg_pos.1,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    arg_pos
}

fn is_traversable_object(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    let TAtomic::TNamedObject { name, .. } = atomic else {
        return false;
    };

    if *name == pzoom_str::StrId::TRAVERSABLE
        || *name == pzoom_str::StrId::ITERATOR
        || *name == pzoom_str::StrId::ITERATOR_AGGREGATE
        || *name == pzoom_str::StrId::GENERATOR
    {
        return true;
    }

    analyzer
        .codebase
        .get_class(*name)
        .is_some_and(|class_info| {
            class_info
                .all_parent_interfaces
                .iter()
                .any(|interface| *interface == pzoom_str::StrId::TRAVERSABLE)
                || class_info
                    .interfaces
                    .iter()
                    .any(|interface| *interface == pzoom_str::StrId::TRAVERSABLE)
        })
}

#[allow(clippy::too_many_arguments)]
pub fn verify_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_pos: Pos,
    arg_type: &TUnion,
    param: &ParamInfo,
    argument_offset: usize,
    callable_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_dataflow: Option<(FunctionLikeIdentifier, Pos, bool)>,
) {
    coerce_value_after_gatekeeper_argument(arg, arg_type, param, context, analysis_data);

    // Psalm ArgumentAnalyzer type-coverage: a verified argument counts as mixed
    // when its type is mixed (or could not be inferred), otherwise non-mixed.
    analysis_data.record_mixedness(context, arg_type.is_mixed());

    // Hakana `argument_analyzer::verify_type` attaches argument dataflow
    // (`add_dataflow`) for every verified argument, regardless of whether the
    // type check below succeeds.
    if let Some((functionlike_id, call_pos, specialize_taint)) = call_dataflow {
        add_dataflow(
            analyzer,
            &functionlike_id,
            argument_offset,
            arg_pos,
            arg_type,
            param,
            specialize_taint,
            context,
            analysis_data,
            call_pos,
        );

        // Psalm `ArgumentAnalyzer::processTaintedness`: a
        // `@psalm-assert-untainted $param` clears the argument variable's
        // dataflow after the call (`$input_type->setParentNodes([])` with
        // `$replace_input_type`) — `validateUserId($userId)` vouches for
        // `$userId` from here on. Queued like the gatekeeper coercion (the
        // verification chain holds the context immutably).
        if param.assert_untainted
            && matches!(
                analysis_data.data_flow_graph.kind,
                pzoom_code_info::GraphKind::WholeProgram(_)
            )
            && let Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(direct)) =
                arg.value().unparenthesized()
        {
            let mut untainted_type = arg_type.clone();
            untainted_type.parent_nodes = vec![];
            analysis_data
                .pending_gatekeeper_coercions
                .push((pzoom_code_info::VarName::new(pzoom_syntax::bytes_to_str(direct.name)), untainted_type));
        }
    }

    if param.by_ref {
        // Psalm's ArgumentsAnalyzer exempts extract() from the by-ref
        // variable requirement.
        if callable_name.eq_ignore_ascii_case("extract") {
        } else if !is_valid_by_ref_arg(analyzer, arg, context) {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidPassByReference,
                format!(
                    "Argument {} of {} is passed by reference, but the provided value is not a variable",
                    argument_offset + 1,
                    callable_name
                ),
            );
        } else {
            check_by_ref_property_mutability(analyzer, arg, arg_pos, analysis_data);
        }
    }

    // Psalm's `expect_variable` check: a single literal string passed to an
    // internal-stub `haystack` parameter is usually a swapped argument.
    // Constant fetches are exempt, as are path-like strings (more than two
    // directory separators) and long mostly-ascending strings (alphabets).
    if param.expect_variable
        && arg_type.types.len() == 1
        && let Some(TAtomic::TLiteralString { value }) = arg_type.types.first()
        && value != pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE
        && !matches!(
            arg.value().unparenthesized(),
            Expression::ConstantAccess(_)
                | Expression::MagicConstant(_)
                | Expression::Access(mago_syntax::cst::cst::access::Access::ClassConstant(_))
        )
    {
        let chars: Vec<char> = value.chars().collect();
        let mut gt_count = 0usize;
        let mut prev_ord = 0u32;
        for ch in &chars {
            let ord = *ch as u32;
            if ord > prev_ord {
                gt_count += 1;
            }
            prev_ord = ord;
        }

        if value.matches('/').count() <= 2
            && (chars.len() < 12 || (gt_count as f64 / chars.len() as f64) < 0.8)
        {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidLiteralArgument,
                format!(
                    "Argument {} of {} expects a non-literal value, but {} provided",
                    argument_offset + 1,
                    callable_name,
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        }
    }

    let Some(param_type) = param.get_type() else {
        return;
    };

    let param_accepts_null = param_type.is_nullable()
        || param_type.is_mixed()
        || param_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        || param
            .default_type
            .as_ref()
            .is_some_and(|default_type| default_type.is_nullable() || default_type.is_null());
    let param_accepts_null = param_accepts_null
        || (param.is_optional
            && argument_offset == 2
            && callable_name.eq_ignore_ascii_case("InvalidArgumentException::__construct"));
    let param_type = if param_accepts_null && !param_type.is_nullable() && !param_type.is_mixed() {
        combine_union_types(param_type, &TUnion::null(), false)
    } else {
        param_type.clone()
    };
    let param_type = normalize_class_constant_param_type(analyzer, &param_type, callable_name);
    let param_type = expand_bare_generic_param_type(analyzer, &param_type);

    if argument_offset == 0
        && (callable_name.eq_ignore_ascii_case("is_a")
            || callable_name.eq_ignore_ascii_case("is_subclass_of"))
        && (arg_type.has_object() || union_is_string_like(arg_type))
    {
        return;
    }

    let mut adjusted_arg_type = arg_type.clone();
    if adjusted_arg_type.ignore_falsable_issues {
        adjusted_arg_type
            .types
            .retain(|atomic| !matches!(atomic, TAtomic::TFalse));
    }
    let arg_type = &adjusted_arg_type;

    if param.has_docblock_type
        && param.signature_type.is_none()
        && looks_like_unresolved_conditional_docblock_type(
            &param_type.get_id(Some(analyzer.interner)),
        )
    {
        return;
    }

    if param.has_docblock_type {
        if let (Some(signature_type), Some(docblock_type)) =
            (&param.signature_type, &param.param_type)
        {
            let signature_has_callable = union_has_callable(signature_type);
            let docblock_has_callable = union_has_callable(docblock_type);

            let suppress_callable_string_mismatch =
                docblock_has_callable && union_is_string_like(signature_type);

            // A templated docblock type (`@param TCallback $cb` with
            // `TCallback as Closure():string` against a `Closure` hint) is
            // checked against the signature through its bound where the
            // function is declared, not per-argument — Psalm accepts it
            // (noCrashTemplatedClosure). After standin replacement the
            // docblock side may also be an unresolved type variable.
            let docblock_is_templated = docblock_type
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TTypeVariable { .. }))
                || crate::type_comparator::generic_type_comparator::union_has_template(
                    docblock_type,
                );

            if signature_has_callable != docblock_has_callable
                && !suppress_callable_string_mismatch
                && !docblock_is_templated
            {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::MismatchingDocblockParamType,
                    format!(
                        "Parameter {} of {} has mismatching docblock type {} and signature type {}",
                        argument_offset + 1,
                        callable_name,
                        docblock_type.get_id(Some(analyzer.interner)),
                        signature_type.get_id(Some(analyzer.interner))
                    ),
                );
            }

            if is_untyped_callable_union(signature_type)
                && has_typed_callable_signature_union(docblock_type)
                && is_untyped_callable_union(arg_type)
            {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::MixedArgumentTypeCoercion,
                    format!(
                        "Argument {} of {} expects {}, but parent type {} provided",
                        argument_offset + 1,
                        callable_name,
                        docblock_type.get_id(Some(analyzer.interner)),
                        arg_type.get_id(Some(analyzer.interner))
                    ),
                );
                return;
            }
        }
    }

    // === Psalm ArgumentAnalyzer::verifyType — core type check ===
    //
    // This mirrors the decision flow of Psalm's `ArgumentAnalyzer::verifyType`:
    // containment (ignoring null/false, which are checked separately) → type
    // coercion → implicit __toString cast → not-contained mismatch
    // (scalar / possibly-invalid / invalid) → null / false checks. pzoom-specific
    // adaptations (callable validation, class-string-undefined detection, template
    // gaps, the stringable-object fallback) are kept but slotted into the matching
    // position in the flow.
    let is_echo_or_print =
        callable_name.eq_ignore_ascii_case("echo") || callable_name.eq_ignore_ascii_case("print");

    // Psalm: a `mixed` parameter accepts anything.
    if param_type.is_mixed() {
        return;
    }

    let arg_has_mixed = arg_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed));

    // An undefined variable passed by-ref has no type in Psalm, which skips
    // verification for it — except that a non-nullable by-ref param reports
    // NullReference ("Not expecting null argument passed by reference").
    // Psalm gates this on `$constrain_type` = method call or NOT in the
    // callmap, so built-in (stubbed) global functions like pcntl_waitpid
    // never report it.
    if arg_type.from_undefined_by_ref && param.by_ref {
        let callee_is_builtin_function = !callable_name.contains("::")
            && analyzer
                .interner
                .find(&callable_name.to_lowercase())
                .and_then(|name_id| analyzer.codebase.get_function(name_id))
                .is_some_and(|function_info| {
                    analyzer
                        .codebase
                        .files
                        .get(&function_info.file_path)
                        .is_some_and(|file_info| file_info.is_stub)
                });
        if !callee_is_builtin_function
            && let Some(param_type) = param.get_type()
            && !param_type.is_nullable()
            && !param_type.is_mixed()
            && !param.is_optional
            && param.default_type.is_none()
        {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::NullReference,
                "Not expecting null argument passed by reference".to_string(),
            );
        }
        return;
    }

    // Psalm emits `MixedArgument` for any input containing mixed and, only for
    // a *fully* mixed input (`Union::isMixed` — every atomic is mixed), stops.
    // A `mixed|null` or `mixed|<type>` input is not fully mixed, so it
    // continues to the containment and null/false checks below.
    if arg_has_mixed {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::MixedArgument,
            format!(
                "Argument {} of {} cannot be {}, expecting {}",
                argument_offset + 1,
                callable_name,
                arg_type.get_id(Some(analyzer.interner)),
                param_type.get_id(Some(analyzer.interner))
            ),
        );

        if arg_type.is_only_mixed() {
            return;
        }
    }

    // Psalm ArgumentAnalyzer: a `never` input means every possible type for the
    // argument was invalidated — likely dead code — and skips all further checks.
    if !arg_type.types.is_empty() && arg_type.is_nothing() {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::NoValue,
            "All possible types for this argument were invalidated - This may be dead code"
                .to_string(),
        );
        return;
    }

    // Psalm replaces a single-callable parameter's string/array inputs with their
    // callable form and validates them; pzoom keeps this as a dedicated pass.
    if union_has_callable(&param_type) {
        if union_has_callable(arg_type) && union_has_untyped_mixed_callable(&param_type) {
            return;
        }

        // Callable signature validation needs concrete parameter shapes:
        // type variables nested in the expected callable resolve through
        // their accumulated lower bounds (`callable(`_0 >: Foo):void` checks
        // as `callable(Foo):void`).
        let param_type = resolve_nested_type_variables_in_callables(
            &param_type,
            &analysis_data.type_variable_bounds,
        );

        let prefer_invalid_argument_for_undefined = param
            .signature_type
            .as_ref()
            .is_some_and(union_is_string_like);

        match validate_callable_argument(
            analyzer,
            arg,
            arg_pos,
            arg_type,
            &param_type,
            argument_offset,
            callable_name,
            analysis_data,
            context,
            prefer_invalid_argument_for_undefined,
        ) {
            CallableValidationOutcome::Valid | CallableValidationOutcome::IssueEmitted => return,
            CallableValidationOutcome::NotApplicable => {}
        }
    }

    // pzoom template-model gap: an inherited method's own templates are not
    // carried into the inheriting class's storage, so the standin replacement
    // can leave a bare callee template in the param type (`TResult:I::method
    // as mixed` when calling it through `A`). Until inherited storages carry
    // template_types, such params accept any argument rather than rigidly
    // rejecting everything. Templates belonging to the *enclosing*
    // function-like (or its class) are exempt: they are the caller's choice
    // and the rigid comparator must see them (`$c->take("foo")` on
    // `Container<T:fn-enclosing>` is an InvalidArgument).
    if (is_unconstrained_template_union(&param_type)
        || is_likely_unresolved_template_named_object_union(analyzer, &param_type))
        && !param_mentions_enclosing_scope_template(analyzer, &param_type)
    {
        return;
    }

    // Psalm passes `ignore_null = true` and `ignore_false = !param_has_true`, then
    // checks null/false separately below.
    let param_has_true = param_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TTrue));

    let mut comparison_result = TypeComparisonResult::new();
    let mut type_match_found = union_type_comparator::is_contained_by(
        analyzer.codebase,
        arg_type,
        &param_type,
        true,
        !param_has_true,
        &mut comparison_result,
    );

    // pzoom adaptation: a literal class-string argument naming a class that does not
    // exist (Psalm reports this through `verifyExplicitParam`).
    let explicit_undefined_class_name = if callable_allows_unknown_runtime_class(callable_name) {
        None
    } else if expects_class_string_union(&param_type) {
        find_undefined_class_string_literal_in_argument(
            analyzer,
            arg.value().unparenthesized(),
            &param_type,
            context,
        )
    } else {
        None
    };

    // Psalm strict_types adjustment: outside of array inputs, a scalar mismatch is no
    // longer "scalar" (so it becomes a hard `InvalidArgument`) and an implicit
    // __toString cast no longer counts as a match.
    if file_uses_strict_types(analyzer) && !union_is_array_like(arg_type) {
        comparison_result.scalar_type_match_found = Some(false);
        if comparison_result.to_string_cast {
            comparison_result.to_string_cast = false;
            type_match_found = false;
        }
    }

    // pzoom adaptation: `in_array($needle, [Foo::class, ...])` with a templated
    // class-string needle is exempt regardless of whether the comparator deemed
    // the mismatch a coercion.
    if !type_match_found
        && callable_name.eq_ignore_ascii_case("in_array")
        && argument_offset == 0
        && union_has_template_class_string_argument(arg_type)
        && union_is_specific_class_string_set(&param_type)
    {
        return;
    }

    // Psalm: the input is a parent (less specific) type of the parameter.
    if comparison_result.type_coerced.unwrap_or(false) && !arg_has_mixed {
        let kind = if comparison_result.type_coerced_from_mixed.unwrap_or(false)
            && !comparison_result
                .type_coerced_from_as_mixed
                .unwrap_or(false)
        {
            IssueKind::MixedArgumentTypeCoercion
        } else {
            IssueKind::ArgumentTypeCoercion
        };

        // Mixed coercions carry the mixed value's dataflow origin (Psalm's
        // MixedIssueTrait origin) as a secondary location.
        let origin_secondary = if matches!(kind, IssueKind::MixedArgumentTypeCoercion) {
            crate::data_flow::mixed_origin_secondary(analyzer, analysis_data, arg_type, arg_pos.0)
        } else {
            None
        };
        let (issue_line, issue_col) = analyzer.get_line_column(arg_pos.0);
        analysis_data.add_issue(
            Issue::new(
                kind,
                format!(
                    "Argument {} of {} expects {}, but parent type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner)),
                ),
                analyzer.file_path,
                arg_pos.0,
                arg_pos.1,
                issue_line,
                issue_col,
            )
            .with_secondary_opt(origin_secondary),
        );

        if let Some(undefined_class_name) = explicit_undefined_class_name.as_ref() {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedClass,
                crate::class_casing::undefined_class_message(analyzer, &undefined_class_name),
            );
        }

        return;
    }

    // Psalm: an implicit __toString cast. pzoom additionally recognises stringable
    // objects that the comparator does not yet flag through `to_string_cast`.
    if comparison_result.to_string_cast
        || (!type_match_found
            && !file_uses_strict_types(analyzer)
            && param_allows_string_like(&param_type)
            && union_is_stringable_object(analyzer, arg_type))
    {
        if !is_echo_or_print {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::ImplicitToStringCast,
                format!(
                    "Argument {} of {} expects {}, object converted via __toString",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                ),
            );
        }
        return;
    }

    // Psalm: not contained and not coerced — choose the most specific mismatch issue.
    if !type_match_found && !comparison_result.type_coerced.unwrap_or(false) {
        // pzoom adaptation: aliased runtime types (e.g. `class-string` aliases) that
        // the comparator does not yet relate.
        if is_runtime_alias_union_contained(analyzer, arg_type, &param_type, context) {
            return;
        }

        // pzoom adaptation: `in_array($needle, [Foo::class, ...])` with a templated
        // class-string needle.
        if callable_name.eq_ignore_ascii_case("in_array")
            && argument_offset == 0
            && union_has_template_class_string_argument(arg_type)
            && union_is_specific_class_string_set(&param_type)
        {
            return;
        }

        // pzoom adaptation: a plain string passed where a class-string is expected is
        // a coercion (Psalm models this via the class-string comparator).
        if expects_class_string_union(&param_type) && has_plain_string_like_atomic(arg_type) {
            if accepts_unconstrained_class_string(&param_type) {
                return;
            }

            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::ArgumentTypeCoercion,
                format!(
                    "Argument {} of {} expects {}, but parent type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
            return;
        }

        // Psalm's verifyType compares with ignore_null (nulls report
        // separately against the param's nullability), so a shared null
        // member must not make an otherwise-invalid union merely "possibly
        // different" (its fixme inventory expects InvalidArgument there).
        let strip_null = |union: &TUnion| -> TUnion {
            let kept: Vec<TAtomic> = union
                .types
                .iter()
                .filter(|atomic| !matches!(atomic, TAtomic::TNull))
                .cloned()
                .collect();
            if kept.is_empty() || kept.len() == union.types.len() {
                union.clone()
            } else {
                let mut stripped = union.clone();
                stripped.types = kept;
                stripped
            }
        };
        let (identity_arg, identity_param) = if param_type.is_nullable() && arg_type.is_nullable() {
            (strip_null(arg_type), strip_null(&param_type))
        } else {
            (arg_type.clone(), param_type.clone())
        };
        let types_can_be_identical = union_type_comparator::can_be_contained_by(
            analyzer.codebase,
            &identity_arg,
            &identity_param,
        );
        let strict_types = file_uses_strict_types(analyzer);
        // pzoom proxy for Psalm's `!container_type_part->from_docblock`: a scalar
        // mismatch only escalates to `InvalidScalarArgument` for a native/signature
        // parameter type, not a docblock-only one.
        let should_emit_scalar_mismatch = param.signature_type.is_some()
            || (!param.has_docblock_type && param.param_type.is_some());

        if comparison_result.scalar_type_match_found == Some(true) {
            if !is_echo_or_print {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    if !should_emit_scalar_mismatch || strict_types {
                        IssueKind::InvalidArgument
                    } else {
                        IssueKind::InvalidScalarArgument
                    },
                    format!(
                        "Argument {} of {} expects {}, but {} provided",
                        argument_offset + 1,
                        callable_name,
                        param_type.get_id(Some(analyzer.interner)),
                        arg_type.get_id(Some(analyzer.interner))
                    ),
                );
            }
        } else if types_can_be_identical {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::PossiblyInvalidArgument,
                format!(
                    "Argument {} of {} expects {}, but possibly different type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        } else {
            let kind = if union_is_array_like(arg_type)
                && union_is_array_like(&param_type)
                && (expects_class_string_union(&param_type)
                    || (union_is_list_like(&param_type) && !union_is_list_like(arg_type)))
            {
                IssueKind::ArgumentTypeCoercion
            } else {
                IssueKind::InvalidArgument
            };
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                kind,
                format!(
                    "Argument {} of {} expects {}, but {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        }

        return;
    }

    // Matched (possibly after coercion). Report a literal class-string naming a class
    // that does not exist (Psalm `verifyExplicitParam`).
    if let Some(undefined_class_name) = explicit_undefined_class_name.as_ref() {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::UndefinedClass,
            crate::class_casing::undefined_class_message(analyzer, &undefined_class_name),
        );
    }

    // Psalm: null argument checks (the parameter does not accept null).
    if !param_type.is_nullable() && !is_echo_or_print {
        if arg_type.is_null() {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::NullArgument,
                format!(
                    "Argument {} of {} cannot be null, {} expected",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner))
                ),
            );
            return;
        }

        if arg_type.is_nullable() && !arg_type.ignore_nullable_issues {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::PossiblyNullArgument,
                format!(
                    "Argument {} of {} expects {}, but possibly different type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        }

        // Psalm's coerceValueAfterGatekeeperArgument: under strict_types a
        // call-map function would have thrown on null, so the variable is
        // non-null afterwards (the same holds for user functions in any
        // mode). `strtoupper($g)` with `?string $g` leaves `$g: string`.
        if arg_type.is_nullable()
            && !param.by_ref
            && analyzer.file_uses_strict_types
            && let Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(direct)) =
                arg.value().unparenthesized()
        {
            let mut narrowed = arg_type.clone();
            narrowed
                .types
                .retain(|atomic| !matches!(atomic, TAtomic::TNull));
            if !narrowed.types.is_empty() {
                analysis_data
                    .pending_gatekeeper_coercions
                    .push((pzoom_code_info::VarName::new(pzoom_syntax::bytes_to_str(direct.name)), narrowed));
            }
        }
    }

    // Psalm: false argument checks (the parameter does not accept false, and false is
    // not subsumed by an accepted bool/scalar type).
    let param_has_bool = param_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TBool));
    let param_has_scalar = param_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TScalar));

    if !param_type.is_falsable() && !param_has_bool && !param_has_scalar && !is_echo_or_print {
        let arg_is_false =
            arg_type.types.len() == 1 && matches!(arg_type.types.first(), Some(TAtomic::TFalse));

        if arg_is_false {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                format!(
                    "Argument {} of {} cannot be false, {} value expected",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner))
                ),
            );
            return;
        }

        if arg_type.is_falsable() && !arg_type.ignore_falsable_issues {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::PossiblyFalseArgument,
                format!(
                    "Argument {} of {} cannot be false, possibly {} value expected",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner))
                ),
            );
        }
    }

    // Hakana `verify_type`'s tail: transfer any type-variable bounds the
    // containment comparison recorded, stamped with the argument's position.
    let bound_pos = crate::template::bound_location(analyzer, arg_pos);
    crate::template::record_type_variable_bounds(
        analysis_data,
        comparison_result.type_variable_lower_bounds,
        comparison_result.type_variable_upper_bounds,
        Some(bound_pos),
    );
}

/// Port of Hakana `argument_analyzer::add_dataflow`: connect the argument
/// value's parent nodes to a `FunctionLikeArg` node for the callee.
///
/// In whole-program (taint) graphs the method-argument node becomes a taint
/// sink when the param carries sink kinds. Hakana sources these from
/// `get_argument_taints` (hardcoded) + `function_param.taint_sinks`
/// (attributes); pzoom's `param.sinks` carries the same data scanned from
/// Psalm's `InternalTaintSinkMap` port, stub `@psalm-taint-sink` docblocks
/// and user docblocks.
///
/// Deviations from Hakana (noted conservative defaults):
/// - The callee param's `name_location` is not stored in pzoom's `ParamInfo`,
///   so the `VariableUseSink` position falls back to the argument expression's
///   own position, the `Vertex` position is `None`, and an unspecialized
///   sink's position is `None` (the taint engine then reports at the
///   predecessor argument-value node, which is also where Psalm reports).
/// - `context.allow_taints` and comment-based removed taints
///   (`HAKANA_SECURITY_IGNORE`) are not ported.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    arg_pos: Pos,
    input_type: &TUnion,
    param: &ParamInfo,
    specialize_taint: bool,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    call_pos: Pos,
) {
    // Psalm `ArgumentAnalyzer::processTaintedness`: an int/float/bool-typed
    // argument value cannot carry taint (except sleep), expressed as removed
    // taints on the argument path. (Hakana instead skips non-taintable
    // arguments entirely, but that loses sleep taints flowing through int
    // params - Psalm's TaintForIntSleep.) An empty taint set kills the path
    // in the BFS.
    let arg_removed_taints = if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind
    {
        input_type.get_taints_to_remove()
    } else {
        vec![]
    };

    let function_call_node_pos = make_data_flow_node_position(analyzer, call_pos);

    let method_node = if let GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind {
        let arg_location = make_data_flow_node_position(analyzer, arg_pos);

        let id = if specialize_taint {
            DataFlowNodeId::SpecializedFunctionLikeArg(
                *functionlike_id,
                argument_offset as u8,
                function_call_node_pos.file_path,
                function_call_node_pos.start_offset,
            )
        } else {
            DataFlowNodeId::FunctionLikeArg(*functionlike_id, argument_offset as u8)
        };

        // Hakana `get_argument_taints`: builtin sink kinds are looked up at
        // call time by function-like name (data: Psalm's InternalTaintSinkMap),
        // merged with the param's docblock-scanned sinks.
        let mut sinks = param.sinks.clone();
        for taint in
            get_builtin_argument_taints(functionlike_id, argument_offset, analyzer.interner)
        {
            if !sinks.contains(&taint) {
                sinks.push(taint);
            }
        }

        let method_node = DataFlowNode {
            id,
            kind: if sinks.is_empty() {
                DataFlowNodeKind::Vertex {
                    pos: None,
                    is_specialized: specialize_taint,
                }
            } else {
                DataFlowNodeKind::TaintSink {
                    pos: if specialize_taint {
                        Some(arg_location)
                    } else {
                        None
                    },
                    types: sinks,
                }
            },
        };

        // Hakana: when the called method is declared on an ancestor, the
        // called class's argument node flows into the declaring method's
        // (unspecialized) node — `C::__construct#1 → A::__construct#1` — so
        // taints reach the body analyzed under the declaring class.
        if let FunctionLikeIdentifier::Method(classlike_name, method_name) = functionlike_id
            && let Some(declaring_class) = analyzer
                .codebase
                .get_class(*classlike_name)
                .and_then(|class_info| class_info.methods.get(method_name))
                .and_then(|method_info| method_info.declaring_class)
            && declaring_class != *classlike_name
        {
            let declaring_node = DataFlowNode::get_for_method_argument(
                &FunctionLikeIdentifier::Method(declaring_class, *method_name),
                argument_offset,
                Some(make_data_flow_node_position(analyzer, arg_pos)),
                None,
            );

            analysis_data
                .data_flow_graph
                .add_node(declaring_node.clone());
            analysis_data.data_flow_graph.add_path(
                &method_node.id,
                &declaring_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }

        // Hakana: a tainted argument to `Base::method` also flows into every
        // descendant classlike's override of that method.
        if let FunctionLikeIdentifier::Method(classlike_name, method_name) = functionlike_id
            && *method_name != pzoom_str::StrId::CONSTRUCT
            && let Some(dependent_classlikes) = analyzer
                .codebase
                .all_classlike_descendants
                .get(classlike_name)
        {
            let mut dependent_classlikes = dependent_classlikes.iter().collect::<Vec<_>>();
            dependent_classlikes.sort_unstable();

            for dependent_classlike in dependent_classlikes {
                // Hakana `declaring_method_exists`: only descendants that
                // declare (override) the method themselves get a node.
                let declares_method = analyzer
                    .codebase
                    .get_class(*dependent_classlike)
                    .is_some_and(|info| {
                        info.declaring_method_ids.get(method_name) == Some(dependent_classlike)
                    });
                if declares_method {
                    let new_sink = DataFlowNode::get_for_method_argument(
                        &FunctionLikeIdentifier::Method(*dependent_classlike, *method_name),
                        argument_offset,
                        None,
                        specialize_taint.then_some(function_call_node_pos),
                    );

                    analysis_data.data_flow_graph.add_node(new_sink.clone());
                    analysis_data.data_flow_graph.add_path(
                        &method_node.id,
                        &new_sink.id,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
            }
        }

        method_node
    } else {
        // specialize_call conservative default: true in the function-body
        // graph (Hakana sets it for nearly every function/method).
        let id = DataFlowNodeId::SpecializedFunctionLikeArg(
            *functionlike_id,
            argument_offset as u8,
            function_call_node_pos.file_path,
            function_call_node_pos.start_offset,
        );

        DataFlowNode {
            id,
            kind: if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
                && context.inside_general_use
            {
                DataFlowNodeKind::VariableUseSink {
                    pos: make_data_flow_node_position(analyzer, arg_pos),
                }
            } else {
                DataFlowNodeKind::Vertex {
                    pos: None,
                    is_specialized: true,
                }
            },
        }
    };

    // Psalm `ArgumentAnalyzer::processTaintedness`: a string-typed parameter
    // receiving a non-string value casts it through __toString
    // (`castStringAttempt`), routing the method's return taint into the
    // argument node.
    let mut input_parent_nodes = input_type.parent_nodes.clone();
    if param.get_type().is_some_and(union_is_all_string) && !union_is_all_string(input_type) {
        input_parent_nodes.extend(crate::expr::cast_analyzer::add_to_string_call_dataflow(
            analyzer,
            analysis_data,
            input_type,
        ));
    }

    for parent_node in &input_parent_nodes {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &method_node.id,
            PathKind::Default,
            vec![],
            arg_removed_taints.clone(),
        );
    }

    analysis_data.data_flow_graph.add_node(method_node);
}

/// Psalm `TypeExpander::expandNamedObject` with `$expand_generic = true`
/// (ArgumentAnalyzer expands the parameter type this way before comparison):
/// a bare named-object reference to a class with template parameters becomes
/// a generic object filled with each template's declared bound — but only
/// when at least one bound is more specific than `mixed` (`takesA(A $a)`
/// with `@template T as object` checks arguments against `A<object>`).
fn expand_bare_generic_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
) -> TUnion {
    let needs_expansion = param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TNamedObject {
                type_params: None,
                ..
            }
        )
    });
    if !needs_expansion {
        return param_type.clone();
    }

    let mut expanded = param_type.clone();
    for atomic in expanded.types.iter_mut() {
        let TAtomic::TNamedObject {
            name,
            type_params: type_params @ None,
            ..
        } = atomic
        else {
            continue;
        };

        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };

        if class_info.template_types.is_empty()
            || !class_info
                .template_types
                .iter()
                .any(|template_type| !template_type.as_type.is_mixed())
        {
            continue;
        }

        *type_params = Some(
            class_info
                .template_types
                .iter()
                .map(|template_type| template_type.as_type.clone())
                .collect(),
        );
    }

    expanded
}

/// Psalm `Union::isString()`: every atomic is a string type.
fn union_is_all_string(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TTruthyString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
                    | TAtomic::TClassString { .. }
                    | TAtomic::TLiteralClassString { .. }
            )
        })
}

pub fn verify_unpacked_argument(
    analyzer: &StatementsAnalyzer<'_>,
    arg_pos: Pos,
    arg_type: &TUnion,
    callable_name: &str,
    no_named_arguments: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut non_iterable = false;
    let mut invalid_key = false;
    let mut invalid_string_key = false;
    let mut possibly_matches = false;

    for atomic in &arg_type.types {
        let Some(key_type) = get_unpacked_iterable_key_type(analyzer, atomic) else {
            if matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed) {
                non_iterable = true;
                possibly_matches = true;
            } else {
                non_iterable = true;
            }
            continue;
        };

        if !union_contains_only_array_key(analyzer, &key_type) {
            invalid_key = true;
            continue;
        }

        if no_named_arguments && !union_contains_only_int(analyzer, &key_type) {
            invalid_string_key = true;
            continue;
        }

        possibly_matches = true;
    }

    let issue_kind = if possibly_matches {
        IssueKind::PossiblyInvalidArgument
    } else {
        IssueKind::InvalidArgument
    };

    if non_iterable {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            issue_kind,
            format!(
                "Tried to unpack non-iterable {}",
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    }

    if invalid_key {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            issue_kind,
            format!(
                "{} called with unpacked iterable {} with invalid key type",
                callable_name,
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    }

    if invalid_string_key {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::NamedArgumentNotAllowed,
            format!(
                "{} called with named unpacked iterable {}",
                callable_name,
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    }
}

/// Psalm's `coerceValueAfterGatekeeperArgument` (the mixed branch): passing a
/// mixed variable to a natively-typed parameter narrows the variable to the
/// signature type afterwards — the call gatekeeps the value.
fn coerce_value_after_gatekeeper_argument(
    arg: &Argument<'_>,
    arg_type: &TUnion,
    param: &ParamInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    if param
        .get_type()
        .is_none_or(|param_type| param_type.is_mixed())
    {
        return;
    }

    if !arg_type.is_mixed() {
        return;
    }

    let Some(signature_param_type) = param.signature_type.as_ref() else {
        return;
    };

    let Expression::Variable(mago_syntax::cst::cst::variable::Variable::Direct(direct)) =
        arg.value().unparenthesized()
    else {
        return;
    };

    let mut narrowed = signature_param_type.clone();
    narrowed.ignore_nullable_issues = signature_param_type.is_nullable();
    narrowed.parent_nodes = arg_type.parent_nodes.clone();

    // The verification chain holds the context immutably — queue the
    // narrowing for the call analyzer to apply once arguments are done.
    let _ = context;
    let var_id = pzoom_code_info::VarName::new(pzoom_syntax::bytes_to_str(direct.name));
    analysis_data
        .pending_gatekeeper_coercions
        .push((var_id, narrowed));
}

/// Mirror of Hakana `argument_analyzer::get_argument_taints` with the data
/// from Psalm's `dictionaries/InternalTaintSinkMap.php`: builtin functions
/// and methods whose parameters are taint sinks, looked up at call time by
/// lowercase function-like name. This keeps sink data independent of which
/// stub file declared the callee.
pub(crate) fn get_builtin_argument_taints(
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    interner: &pzoom_str::Interner,
) -> Vec<pzoom_code_info::data_flow::node::SinkType> {
    use pzoom_code_info::data_flow::node::SinkType;

    let per_param_sinks: &[&[SinkType]] = match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => {
            match interner.lookup(*function_name).to_lowercase().as_str() {
                "exec" | "passthru" | "pcntl_exec" | "shell_exec" | "system" | "popen" => {
                    &[&[SinkType::Shell]]
                }
                "proc_open" => &[&[SinkType::Shell]],
                "create_function" => &[&[], &[SinkType::Eval]],
                "file_get_contents" => &[&[SinkType::FileSink, SinkType::Ssrf]],
                "file_put_contents" | "fopen" | "unlink" | "file" | "mkdir" | "parse_ini_file"
                | "chown" | "lchown" | "readfile" | "rmdir" | "symlink" | "tempnam" => {
                    &[&[SinkType::FileSink]]
                }
                "copy" => &[&[SinkType::FileSink, SinkType::Ssrf], &[SinkType::FileSink]],
                "link" | "move_uploaded_file" | "rename" => {
                    &[&[SinkType::FileSink], &[SinkType::FileSink]]
                }
                "header" => &[&[SinkType::Header]],
                "igbinary_unserialize" | "unserialize" => &[&[SinkType::Unserialize]],
                "ldap_search" => &[&[], &[SinkType::Ldap], &[SinkType::Ldap]],
                "mysqli_query"
                | "mysqli_real_query"
                | "mysqli_multi_query"
                | "mysqli_prepare"
                | "mysqli_stmt_prepare"
                | "pg_exec"
                | "pg_put_line"
                | "pg_query"
                | "pg_query_params"
                | "pg_send_query"
                | "pg_send_query_params" => &[&[], &[SinkType::Sql]],
                "pg_prepare" | "pg_send_prepare" => &[&[], &[], &[SinkType::Sql]],
                "setcookie" => &[&[SinkType::Cookie], &[SinkType::Cookie]],
                "curl_init" | "getimagesize" => &[&[SinkType::Ssrf]],
                "curl_setopt" => &[&[], &[], &[SinkType::Ssrf]],
                _ => &[],
            }
        }
        FunctionLikeIdentifier::Method(classlike_name, method_name) => {
            match (
                interner.lookup(*classlike_name).to_lowercase().as_str(),
                interner.lookup(*method_name).to_lowercase().as_str(),
            ) {
                ("mysqli", "query" | "real_query" | "multi_query" | "prepare") => {
                    &[&[SinkType::Sql]]
                }
                ("mysqli_stmt", "__construct") => &[&[], &[SinkType::Sql]],
                ("mysqli_stmt", "prepare") => &[&[SinkType::Sql]],
                _ => &[],
            }
        }
        FunctionLikeIdentifier::Closure(..) => &[],
    };

    per_param_sinks
        .get(argument_offset)
        .map(|sinks| sinks.to_vec())
        .unwrap_or_default()
}

/// Resolves type variables appearing inside callable/closure parameter and
/// return positions through their accumulated lower bounds, leaving
/// everything else untouched (callable signature validation compares shapes
/// structurally and cannot defer a variable to bound reconciliation).
fn resolve_nested_type_variables_in_callables(
    param_type: &TUnion,
    type_variable_bounds: &rustc_hash::FxHashMap<String, pzoom_code_info::TypeVariableBounds>,
) -> TUnion {
    let mut resolved = param_type.clone();

    for atomic in resolved.types.iter_mut() {
        let (params, return_type) = match atomic {
            TAtomic::TCallable {
                params: Some(params),
                return_type,
                ..
            }
            | TAtomic::TClosure {
                params: Some(params),
                return_type,
                ..
            } => (params, return_type),
            _ => continue,
        };

        for callable_param in params.iter_mut() {
            callable_param.param_type = crate::template::resolve_type_variables_in_union(
                &callable_param.param_type,
                type_variable_bounds,
            );
        }

        if let Some(return_type) = return_type {
            **return_type =
                crate::template::resolve_type_variables_in_union(return_type, type_variable_bounds);
        }
    }

    resolved
}

/// Whether a param type names a template defined by the function-like whose
/// body is being analyzed (or by its declaring class) — fixed ("rigid") by
/// that function's caller, so the comparison must run rather than bailing on
/// the unresolved-template gap in `verify_type`.
fn param_mentions_enclosing_scope_template(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
) -> bool {
    let mut enclosing_templates: Vec<(pzoom_str::StrId, pzoom_code_info::GenericParent)> =
        Vec::new();
    if let Some(function_info) = analyzer.function_info {
        for template_type in &function_info.template_types {
            enclosing_templates.push((template_type.name, template_type.defining_entity));
        }
    }
    if let Some(class_id) = analyzer.get_declaring_class()
        && let Some(class_info) = analyzer.codebase.get_class(class_id)
    {
        for template_type in &class_info.template_types {
            enclosing_templates.push((template_type.name, template_type.defining_entity));
        }
    }

    if enclosing_templates.is_empty() {
        return false;
    }

    param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                ..
            } if enclosing_templates.contains(&(*name, *defining_entity))
        )
    })
}
