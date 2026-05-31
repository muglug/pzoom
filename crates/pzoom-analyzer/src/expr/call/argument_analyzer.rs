//! Single argument analyzer.

use mago_syntax::ast::ast::argument::Argument;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use mago_syntax::ast::ast::expression::Expression;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::combine_union_types;
use super::callable_validation::*;


/// Analyze a single function/method argument.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    argument: &Argument<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    let arg_pos = expression_analyzer::analyze(analyzer, argument.value(), analysis_data, context);

    // Check if this is a named argument
    if let Argument::Named(named) = argument {
        // Named arguments are handled differently in argument resolution
        // The name is available via named.name
        let _ = named.name;
    }

    // Check if this is a variadic/spread argument (...$arg)
    if argument.is_unpacked() {
        // Spread arguments should be arrays/iterables
        if let Some(arg_type) = analysis_data.get_expr_type(arg_pos) {
            if arg_type.is_mixed() {
                // Unpacking a mixed value: Psalm/Hakana report MixedArgument rather
                // than silently treating mixed as a valid collection.
                if !analyzer.config.is_issue_suppressed("MixedArgument") {
                    let (line, col) = analyzer.get_line_column(arg_pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MixedArgument,
                        "Unpacking requires a collection type, but mixed was provided"
                            .to_string(),
                        analyzer.file_path,
                        arg_pos.0,
                        arg_pos.1,
                        line,
                        col,
                    ));
                }
            } else {
                let is_iterable = arg_type.types.iter().any(|t| {
                    matches!(
                        t,
                        TAtomic::TArray { .. }
                            | TAtomic::TNonEmptyArray { .. }
                            | TAtomic::TList { .. }
                            | TAtomic::TNonEmptyList { .. }
                            | TAtomic::TKeyedArray { .. }
                            | TAtomic::TIterable { .. }
                    ) || is_traversable_object(analyzer, t)
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
) {
    if param.by_ref {
        if !is_valid_by_ref_arg(analyzer, arg, context) {
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

    let Some(param_type) = param.get_type() else {
        return;
    };

    // NOTE: Psalm/Hakana emit `NoValue` (IssueKind::NoValue) when `arg_type.is_nothing()`.
    // pzoom currently over-produces `never` for some array-element / reconciliation
    // results, so emitting it here yields false positives (e.g. arrayKeysNoEmpty,
    // removeCallableString). Deferred until `never` inference is accurate upstream.

    let param_accepts_null = param_type.is_nullable
        || param_type.is_mixed()
        || param_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        || param
            .default_type
            .as_ref()
            .is_some_and(|default_type| default_type.is_nullable || default_type.is_null());
    let param_accepts_null = param_accepts_null
        || (param.is_optional
            && argument_offset == 2
            && callable_name.eq_ignore_ascii_case("InvalidArgumentException::__construct"));
    let param_type = if param_accepts_null && !param_type.is_nullable && !param_type.is_mixed() {
        combine_union_types(param_type, &TUnion::null(), false)
    } else {
        param_type.clone()
    };
    let param_type = normalize_class_constant_param_type(analyzer, &param_type, callable_name);


    if let Some(undefined_class_name) = find_undefined_named_object_in_union(analyzer, &param_type)
    {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::UndefinedClass,
            format!("Class {} does not exist", undefined_class_name),
        );
        return;
    }

    if argument_offset == 0
        && (callable_name.eq_ignore_ascii_case("is_a")
            || callable_name.eq_ignore_ascii_case("is_subclass_of"))
        && (arg_type.has_object() || union_is_string_like(arg_type))
    {
        return;
    }

    let normalized_arg_type = normalize_class_string_argument(analyzer, arg_type, &param_type);
    let mut adjusted_arg_type = normalized_arg_type.clone();
    if adjusted_arg_type.ignore_falsable_issues {
        adjusted_arg_type
            .types
            .retain(|atomic| !matches!(atomic, TAtomic::TFalse));
        adjusted_arg_type.is_falsable = adjusted_arg_type.types.iter().any(|t| t.is_falsable());
        adjusted_arg_type.is_nullable = adjusted_arg_type.types.iter().any(|t| t.is_nullable());
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

            if signature_has_callable != docblock_has_callable && !suppress_callable_string_mismatch
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

    // Psalm emits `MixedArgument` here and, only for a *fully* mixed input
    // (`Union::isMixed` — every atomic is mixed), stops. A `mixed|null` or
    // `mixed|<type>` input is not fully mixed, so it continues to the containment
    // and null/false checks below. (The `MixedArgument`/`NoValue` emission itself is
    // deferred while reconciliation over-produces `mixed`/`never` — see
    // PSALM_HAKANA_MAPPING.md.)
    if arg_type.is_only_mixed() {
        return;
    }

    // Psalm replaces a single-callable parameter's string/array inputs with their
    // callable form and validates them; pzoom keeps this as a dedicated pass.
    if union_has_callable(&param_type) {
        if union_has_callable(arg_type) && union_has_untyped_mixed_callable(&param_type) {
            return;
        }

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

    // pzoom template-model gaps: an unconstrained or not-yet-resolved template
    // parameter accepts any argument.
    if is_unconstrained_template_union(&param_type)
        || is_likely_unresolved_template_named_object_union(analyzer, &param_type)
    {
        return;
    }

    if callable_name.eq_ignore_ascii_case("ReflectionClass::__construct") && argument_offset == 0 {
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

    // Psalm: the input is a parent (less specific) type of the parameter.
    if comparison_result.type_coerced.unwrap_or(false) && !arg_has_mixed {
        let kind = if comparison_result
            .type_coerced_from_nested_mixed
            .unwrap_or(false)
        {
            IssueKind::MixedArgumentTypeCoercion
        } else {
            IssueKind::ArgumentTypeCoercion
        };

        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            kind,
            format!(
                "Argument {} of {} expects {}, but parent type {} provided",
                argument_offset + 1,
                callable_name,
                param_type.get_id(Some(analyzer.interner)),
                arg_type.get_id(Some(analyzer.interner))
            ),
        );

        if let Some(undefined_class_name) = explicit_undefined_class_name.as_ref() {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", undefined_class_name),
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

        let types_can_be_identical =
            union_type_comparator::can_be_contained_by(analyzer.codebase, arg_type, &param_type);
        let strict_types = file_uses_strict_types(analyzer);
        // pzoom proxy for Psalm's `!container_type_part->from_docblock`: a scalar
        // mismatch only escalates to `InvalidScalarArgument` for a native/signature
        // parameter type, not a docblock-only one.
        let should_emit_scalar_mismatch =
            param.signature_type.is_some() || (!param.has_docblock_type && param.param_type.is_some());

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
            format!("Class {} does not exist", undefined_class_name),
        );
    }

    // Psalm: null argument checks (the parameter does not accept null).
    if !param_type.is_nullable && !is_echo_or_print {
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

        if arg_type.is_nullable && !arg_type.ignore_nullable_issues {
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

    if !param_type.is_falsable
        && !param_has_bool
        && !param_has_scalar
        && !is_echo_or_print
    {
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

        // NOTE: the `PossiblyFalseArgument` emission (a possibly-`false` input) is
        // still gated on two upstream gaps that currently cause false positives:
        //   * scan-time docblock parsing drops `@psalm-ignore-falsable-return` when it
        //     follows a long multi-line `@psalm-return` (e.g. `glob`), so a falsable
        //     return that Psalm marks ignorable is reported here; and
        //   * `TUnion` derives `PartialEq` over `is_falsable`, so the flag leaks into
        //     reconciliation/dedup equality (Psalm compares by id), perturbing
        //     unrelated results (e.g. `IntRange/assertOutOfRange`).
        // The resolution-layer `ignore_falsable` propagation is fixed; the issue kind
        // exists; enable this emission once the two gaps above are closed.
        let _ = arg_type.ignore_falsable_issues;
    }
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
