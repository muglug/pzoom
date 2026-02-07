//! Callable argument validation, modeled on Psalm's callable checks.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{
    FunctionLikeInfo, FunctionLikeParameter, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::callable_type_comparator;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableValidationOutcome {
    NotApplicable,
    Valid,
    IssueEmitted,
}

pub fn union_has_callable(union: &TUnion) -> bool {
    union.types.iter().any(atomic_has_callable)
}

fn atomic_has_callable(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
        TAtomic::TNamedObject { name, .. } => *name == StrId::CLOSURE,
        TAtomic::TTemplateParam { as_type, .. } => union_has_callable(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_has_callable(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_has_callable),
        _ => false,
    }
}

pub fn infer_callee_return_type(callee_type: &TUnion) -> Option<TUnion> {
    let mut combined: Option<TUnion> = None;

    for atomic in &callee_type.types {
        let return_type = match atomic {
            TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
                return_type
                    .as_ref()
                    .map(|t| (**t).clone())
                    .unwrap_or_else(TUnion::mixed)
            }
            _ => continue,
        };

        combined = Some(if let Some(existing) = combined {
            combine_union_types(&existing, &return_type, false)
        } else {
            return_type
        });
    }

    combined
}

pub fn verify_argument_type(
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
                        "Argument {} of {} expects {}, parent type {} provided",
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

    if arg_type.is_nullable && !param_accepts_null {
        if arg_type.types.len() == 1 && matches!(arg_type.types.first(), Some(TAtomic::TNull)) {
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
    }

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
            CallableValidationOutcome::NotApplicable => {
                if arg_type.is_mixed() {
                    add_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::MixedArgumentTypeCoercion,
                        format!(
                            "Argument {} of {} expects {}, parent type {} provided",
                            argument_offset + 1,
                            callable_name,
                            param_type.get_id(Some(analyzer.interner)),
                            arg_type.get_id(Some(analyzer.interner))
                        ),
                    );
                    return;
                }
            }
        }
    }

    let arg_has_mixed = arg_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed));

    if arg_type.is_nullable && !param_accepts_null && arg_has_mixed {
        if !arg_type.ignore_nullable_issues {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::PossiblyNullArgument,
                format!(
                    "Argument {} of {} expects {}, possibly different type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        }
        return;
    }

    if param_type.is_mixed() {
        return;
    }

    if arg_type.is_mixed() {
        let non_mixed_types = arg_type
            .types
            .iter()
            .filter(|atomic| !matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
            .cloned()
            .collect::<Vec<_>>();

        if non_mixed_types.is_empty() {
            if let Expression::Variable(variable) = arg.value().unparenthesized()
                && let mago_syntax::ast::ast::variable::Variable::Direct(direct) = variable
            {
                let var_id = analyzer.interner.intern(direct.name);
                let alternate_var_id = if let Some(stripped) = direct.name.strip_prefix('$') {
                    analyzer.interner.find(stripped)
                } else {
                    analyzer.interner.find(&format!("${}", direct.name))
                };

                if context.possibly_assigned_var_ids.contains(&var_id)
                    || alternate_var_id.is_some_and(|alt_var_id| {
                        context.possibly_assigned_var_ids.contains(&alt_var_id)
                    })
                {
                    add_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::PossiblyInvalidArgument,
                        format!(
                            "Argument {} of {} expects {}, possibly different type {} provided",
                            argument_offset + 1,
                            callable_name,
                            param_type.get_id(Some(analyzer.interner)),
                            arg_type.get_id(Some(analyzer.interner))
                        ),
                    );
                }
            }
            return;
        }

        let non_mixed_union = TUnion::from_types(non_mixed_types);
        let mut non_mixed_comparison = TypeComparisonResult::new();
        let non_mixed_is_contained = union_type_comparator::is_contained_by(
            analyzer.codebase,
            &non_mixed_union,
            &param_type,
            false,
            false,
            &mut non_mixed_comparison,
        );

        if !non_mixed_is_contained {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::PossiblyInvalidArgument,
                format!(
                    "Argument {} of {} expects {}, possibly different type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
        }

        return;
    }

    if callable_name.eq_ignore_ascii_case("ReflectionClass::__construct") && argument_offset == 0 {
        return;
    }

    if is_unconstrained_template_union(&param_type) {
        return;
    }

    if is_likely_unresolved_template_named_object_union(analyzer, &param_type) {
        return;
    }

    let mut comparison_result = TypeComparisonResult::new();
    let is_contained = union_type_comparator::is_contained_by(
        analyzer.codebase,
        arg_type,
        &param_type,
        false,
        false,
        &mut comparison_result,
    );


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

    if is_contained {
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

    if is_runtime_alias_union_contained(analyzer, arg_type, &param_type, context) {
        return;
    }

    if !file_uses_strict_types(analyzer)
        && param_allows_string_like(&param_type)
        && union_is_stringable_object(analyzer, arg_type)
    {
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
        return;
    }

    if comparison_result.type_coerced.unwrap_or(false) {
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
                "Argument {} of {} expects {}, parent type {} provided",
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

    if expects_class_string_union(&param_type) {
        if let Some(undefined_class_name) = explicit_undefined_class_name.as_ref() {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", undefined_class_name),
            );
        }
    }

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
                "Argument {} of {} expects {}, parent type {} provided",
                argument_offset + 1,
                callable_name,
                param_type.get_id(Some(analyzer.interner)),
                arg_type.get_id(Some(analyzer.interner))
            ),
        );

        return;
    }

    if callable_name.eq_ignore_ascii_case("in_array")
        && argument_offset == 0
        && union_has_template_class_string_argument(arg_type)
        && union_is_specific_class_string_set(&param_type)
    {
        return;
    }

    let types_can_be_identical = union_type_comparator::can_expression_types_be_identical(
        analyzer.codebase,
        arg_type,
        &param_type,
    );
    let strict_types = file_uses_strict_types(analyzer);
    let should_emit_scalar_mismatch =
        param.signature_type.is_some() || (!param.has_docblock_type && param.param_type.is_some());

    if comparison_result.scalar_type_match_found {
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
                "Argument {} of {} expects {}, {} provided",
                argument_offset + 1,
                callable_name,
                param_type.get_id(Some(analyzer.interner)),
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    } else if types_can_be_identical {
        let is_array_like_coercion = union_is_array_like(arg_type)
            && union_is_array_like(&param_type)
            && (expects_class_string_union(&param_type)
                || (union_is_list_like(&param_type) && !union_is_list_like(arg_type)));

        if is_array_like_coercion {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::ArgumentTypeCoercion,
                format!(
                    "Argument {} of {} expects {}, parent type {} provided",
                    argument_offset + 1,
                    callable_name,
                    param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
                ),
            );
            return;
        }

        if arg_type.is_nullable && !param_accepts_null {
            let arg_without_null = subtract_null_atomic(arg_type);
            if !arg_without_null.types.is_empty()
                && is_scalar_only_union(&arg_without_null)
                && is_scalar_only_union(&param_type)
            {
                let has_incompatible_scalar = arg_without_null.types.iter().any(|atomic| {
                    let atomic_union = TUnion::new(atomic.clone());
                    let mut atomic_comparison = TypeComparisonResult::new();

                    !union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &atomic_union,
                        &param_type,
                        false,
                        false,
                        &mut atomic_comparison,
                    )
                });

                if has_incompatible_scalar {
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
                            "Argument {} of {} expects {}, {} provided",
                            argument_offset + 1,
                            callable_name,
                            param_type.get_id(Some(analyzer.interner)),
                            arg_type.get_id(Some(analyzer.interner))
                        ),
                    );
                    return;
                }
            }
        }

        let issue_kind = if arg_type.is_nullable && !param_accepts_null {
            IssueKind::PossiblyNullArgument
        } else {
            IssueKind::PossiblyInvalidArgument
        };

        if issue_kind == IssueKind::PossiblyNullArgument && arg_type.ignore_nullable_issues {
            return;
        }

        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            issue_kind,
            format!(
                "Argument {} of {} expects {}, possibly different type {} provided",
                argument_offset + 1,
                callable_name,
                param_type.get_id(Some(analyzer.interner)),
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    } else {
        let issue_kind = if is_scalar_union(arg_type) && is_scalar_union(&param_type) {
            if !should_emit_scalar_mismatch || strict_types {
                IssueKind::InvalidArgument
            } else {
                IssueKind::InvalidScalarArgument
            }
        } else if union_is_array_like(arg_type)
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
            issue_kind,
            format!(
                "Argument {} of {} expects {}, {} provided",
                argument_offset + 1,
                callable_name,
                param_type.get_id(Some(analyzer.interner)),
                arg_type.get_id(Some(analyzer.interner))
            ),
        );
    }
}

fn union_has_untyped_mixed_callable(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| atomic_is_untyped_mixed_callable(atomic))
}

fn atomic_is_untyped_mixed_callable(atomic: &TAtomic) -> bool {
    let (params, return_type) = match atomic {
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => (params.as_ref(), return_type.as_ref()),
        _ => return false,
    };

    let params_are_mixed = params.is_none_or(|params| {
        params.iter().all(|param| {
            param.param_type.is_mixed()
                || param
                    .param_type
                    .types
                    .iter()
                    .all(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        })
    });
    let return_is_mixed =
        return_type.is_none_or(|return_type| return_type.is_mixed() || return_type.is_nothing());

    params_are_mixed && return_is_mixed
}

fn file_uses_strict_types(analyzer: &StatementsAnalyzer<'_>) -> bool {
    analyzer
        .codebase
        .files
        .get(&analyzer.file_path)
        .is_some_and(|file_info| {
            file_info
                .contents
                .chars()
                .take(512)
                .collect::<String>()
                .replace(char::is_whitespace, "")
                .contains("declare(strict_types=1)")
        })
}

fn is_runtime_alias_union_contained(
    analyzer: &StatementsAnalyzer<'_>,
    input_type: &TUnion,
    container_type: &TUnion,
    context: &BlockContext,
) -> bool {
    if input_type.types.is_empty() || container_type.types.is_empty() {
        return false;
    }

    input_type.types.iter().all(|input_atomic| {
        container_type.types.iter().any(|container_atomic| {
            runtime_alias_atomic_is_contained_by(analyzer, input_atomic, container_atomic, context)
        })
    })
}

fn runtime_alias_atomic_is_contained_by(
    analyzer: &StatementsAnalyzer<'_>,
    input_atomic: &TAtomic,
    container_atomic: &TAtomic,
    context: &BlockContext,
) -> bool {
    if input_atomic == container_atomic {
        return true;
    }

    let (
        TAtomic::TNamedObject {
            name: input_name,
            type_params: input_type_params,
            ..
        },
        TAtomic::TNamedObject {
            name: container_name,
            type_params: container_type_params,
            ..
        },
    ) = (input_atomic, container_atomic)
    else {
        return false;
    };

    if input_name == container_name {
        return input_type_params == container_type_params;
    }

    // Runtime class_alias containment is only valid for non-generic object names.
    if input_type_params.is_some() || container_type_params.is_some() {
        return false;
    }

    is_class_subtype_with_runtime_aliases(analyzer, *input_name, *container_name, context)
}

fn is_class_subtype_with_runtime_aliases(
    analyzer: &StatementsAnalyzer<'_>,
    input_class: StrId,
    container_class: StrId,
    context: &BlockContext,
) -> bool {
    let target = resolve_runtime_alias_class(container_class, context);
    let mut to_visit = vec![resolve_runtime_alias_class(input_class, context)];
    let mut visited = FxHashSet::default();

    while let Some(current_class) = to_visit.pop() {
        if !visited.insert(current_class) {
            continue;
        }

        if current_class == target {
            return true;
        }

        if let Some(class_info) = analyzer.codebase.get_class(current_class) {
            if let Some(parent_class) = class_info.parent_class {
                to_visit.push(resolve_runtime_alias_class(parent_class, context));
            }

            for interface_id in &class_info.interfaces {
                to_visit.push(resolve_runtime_alias_class(*interface_id, context));
            }
        } else if let Some(alias_target) = context.class_aliases.get(&current_class).copied() {
            to_visit.push(resolve_runtime_alias_class(alias_target, context));
        }
    }

    false
}

fn resolve_runtime_alias_class(class_id: StrId, context: &BlockContext) -> StrId {
    context
        .class_aliases
        .get(&class_id)
        .copied()
        .unwrap_or(class_id)
}

fn param_allows_string_like(param_type: &TUnion) -> bool {
    param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TTruthyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TNumericString
        )
    })
}

fn union_is_stringable_object(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    !union.types.is_empty()
        && union
            .types
            .iter()
            .all(|atomic| atomic_is_stringable_object(analyzer, atomic))
}

fn atomic_is_stringable_object(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .is_some_and(|class_info| class_info.methods.contains_key(&StrId::TO_STRING)),
        TAtomic::TTemplateParam { as_type, .. } => union_is_stringable_object(analyzer, as_type),
        _ => false,
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

fn get_unpacked_iterable_key_type(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<TUnion> {
    match atomic {
        TAtomic::TArray { key_type, .. }
        | TAtomic::TNonEmptyArray { key_type, .. }
        | TAtomic::TIterable { key_type, .. } => Some((**key_type).clone()),
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => Some(TUnion::int()),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            ..
        } => {
            let mut key_type = fallback_key_type
                .as_deref()
                .cloned()
                .unwrap_or_else(TUnion::nothing);

            for key in properties.keys() {
                let key_union = match key {
                    ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
                    ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                        value: value.clone(),
                    }),
                };

                key_type = if key_type.is_nothing() {
                    key_union
                } else {
                    combine_union_types(&key_type, &key_union, false)
                };
            }

            if key_type.is_nothing() {
                Some(TUnion::array_key())
            } else {
                Some(key_type)
            }
        }
        TAtomic::TNamedObject { name, type_params } => {
            if !named_object_is_traversable(analyzer, *name) {
                return None;
            }

            Some(
                type_params
                    .as_ref()
                    .and_then(|params| params.first().cloned())
                    .unwrap_or_else(TUnion::array_key),
            )
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut combined: Option<TUnion> = None;
            for nested in &as_type.types {
                let Some(nested_key_type) = get_unpacked_iterable_key_type(analyzer, nested) else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });
            }

            combined
        }
        TAtomic::TObjectIntersection { types } => {
            let mut combined: Option<TUnion> = None;
            for nested in types {
                let Some(nested_key_type) = get_unpacked_iterable_key_type(analyzer, nested) else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });
            }

            combined
        }
        _ => None,
    }
}

fn union_contains_only_array_key(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let mut comparison_result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        union,
        &TUnion::array_key(),
        false,
        false,
        &mut comparison_result,
    )
}

fn union_contains_only_int(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let mut comparison_result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        union,
        &TUnion::int(),
        false,
        false,
        &mut comparison_result,
    )
}

fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
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

fn subtract_null_atomic(union: &TUnion) -> TUnion {
    let mut filtered_types = union
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TNull))
        .cloned()
        .collect::<Vec<_>>();

    if filtered_types.is_empty() {
        filtered_types.push(TAtomic::TNull);
    }

    let mut result = union.clone();
    result.types = filtered_types;
    result.is_nullable = result.types.iter().any(|t| t.is_nullable());
    result.is_falsable = result.types.iter().any(|t| t.is_falsable());
    result
}

fn looks_like_unresolved_conditional_docblock_type(type_id: &str) -> bool {
    if type_id.contains("array{") {
        return false;
    }

    type_id.contains("|:") || type_id.contains(" : ")
}

fn normalize_class_string_argument(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    param_type: &TUnion,
) -> TUnion {
    let expects_class_string = expects_class_string_union(param_type);

    if !expects_class_string {
        return arg_type.clone();
    }

    let mut changed = false;
    let mut normalized_types = Vec::with_capacity(arg_type.types.len());

    for atomic in &arg_type.types {
        if let TAtomic::TLiteralClassString { name } = atomic {
            if let Some(class_id) = resolve_known_class_id_for_literal(analyzer, name) {
                normalized_types.push(TAtomic::TClassString {
                    as_type: Some(Box::new(TAtomic::TNamedObject {
                        name: class_id,
                        type_params: None,
                    })),
                });
                changed = true;
                continue;
            }
        }

        if let TAtomic::TLiteralString { value } = atomic {
            if let Some(class_id) = resolve_known_class_id_for_literal(analyzer, value) {
                normalized_types.push(TAtomic::TClassString {
                    as_type: Some(Box::new(TAtomic::TNamedObject {
                        name: class_id,
                        type_params: None,
                    })),
                });
                changed = true;
                continue;
            }
        }

        normalized_types.push(atomic.clone());
    }

    if !changed {
        return arg_type.clone();
    }

    let mut normalized = TUnion::from_types(normalized_types);
    normalized.from_docblock = arg_type.from_docblock;
    normalized.is_resolved = arg_type.is_resolved;
    normalized.parent_nodes = arg_type.parent_nodes.clone();
    normalized.ignore_nullable_issues = arg_type.ignore_nullable_issues;
    normalized.ignore_falsable_issues = arg_type.ignore_falsable_issues;
    normalized
}

fn resolve_known_class_id_for_literal(
    analyzer: &StatementsAnalyzer<'_>,
    literal: &str,
) -> Option<StrId> {
    let class_id = analyzer.interner.intern(literal);
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let fq_candidate = format!("\\{}", literal.trim_start_matches('\\'));
    let fq_id = analyzer.interner.intern(&fq_candidate);
    analyzer.codebase.get_class(fq_id).map(|_| fq_id)
}

fn expects_class_string_union(param_type: &TUnion) -> bool {
    union_contains_class_string(param_type)
}

fn union_contains_class_string(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_class_string)
}

fn atomic_contains_class_string(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => union_contains_class_string(value_type),
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            properties.values().any(union_contains_class_string)
                || fallback_value_type
                    .as_ref()
                    .is_some_and(|fallback| union_contains_class_string(fallback))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_class_string(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_class_string),
        _ => false,
    }
}

fn has_plain_string_like_atomic(arg_type: &TUnion) -> bool {
    arg_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLiteralString { .. }
        )
    })
}

fn union_has_template_class_string_argument(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(atomic_has_template_class_string_argument)
}

fn atomic_has_template_class_string_argument(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(inner),
        } => matches!(
            inner.as_ref(),
            TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
        ),
        TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => {
            union_has_template_class_string_argument(as_type)
        }
        _ => false,
    }
}

fn union_is_specific_class_string_set(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TClassString {
                as_type: Some(inner),
            } => matches!(inner.as_ref(), TAtomic::TNamedObject { .. }),
            TAtomic::TLiteralClassString { .. } => true,
            _ => false,
        })
}

fn accepts_unconstrained_class_string(param_type: &TUnion) -> bool {
    let mut saw_class_string = false;

    for atomic in &param_type.types {
        match atomic {
            TAtomic::TClassString { as_type: None } => saw_class_string = true,
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if !atomic_is_unconstrained_class_bound(as_type.as_ref()) {
                    return false;
                }
                saw_class_string = true;
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !atomic_is_unconstrained_class_bound(as_type.as_ref()) {
                    return false;
                }
                saw_class_string = true;
            }
            _ => return false,
        }
    }

    saw_class_string
}

fn atomic_is_unconstrained_class_bound(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject => true,
        TAtomic::TTemplateParam { as_type, .. } => union_is_unconstrained_class_bound(as_type),
        _ => false,
    }
}

fn union_is_unconstrained_class_bound(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(atomic_is_unconstrained_class_bound)
}

fn is_unconstrained_template_union(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TTemplateParam { as_type, .. } => as_type.is_mixed(),
            TAtomic::TTemplateParamClass { as_type, .. } => {
                atomic_is_unconstrained_class_bound(as_type)
            }
            _ => false,
        })
}

fn is_likely_unresolved_template_named_object_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TNamedObject {
                name,
                type_params: None,
            } => {
                if analyzer.codebase.get_class(*name).is_some() {
                    return false;
                }

                let name_str = analyzer.interner.lookup(*name);
                let raw = name_str.as_ref();
                !raw.contains('\\') && !raw.contains("::") && is_template_identifier_like(raw)
            }
            _ => false,
        })
}

fn is_template_identifier_like(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    first.is_ascii_uppercase() && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn normalize_class_constant_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
    callable_name: &str,
) -> TUnion {
    let mut changed = false;
    let mut normalized_types = Vec::new();

    for atomic in &param_type.types {
        if let Some(constant_type) =
            resolve_class_constant_like_atomic(analyzer, atomic, callable_name)
        {
            changed = true;
            for constant_atomic in constant_type.types {
                if !normalized_types.contains(&constant_atomic) {
                    normalized_types.push(constant_atomic);
                }
            }
            continue;
        }

        if !normalized_types.contains(atomic) {
            normalized_types.push(atomic.clone());
        }
    }

    if !changed {
        return param_type.clone();
    }

    let mut normalized = TUnion::from_types(normalized_types);
    normalized.from_docblock = param_type.from_docblock;
    normalized.is_resolved = param_type.is_resolved;
    normalized.parent_nodes = param_type.parent_nodes.clone();
    normalized.ignore_nullable_issues = param_type.ignore_nullable_issues;
    normalized.ignore_falsable_issues = param_type.ignore_falsable_issues;
    normalized
}

fn resolve_class_constant_like_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    callable_name: &str,
) -> Option<TUnion> {
    let TAtomic::TNamedObject {
        name,
        type_params: None,
    } = atomic
    else {
        return None;
    };

    let raw = analyzer.interner.lookup(*name);
    let (class_part, const_part) = raw.rsplit_once("::")?;
    let class_id = resolve_class_reference_for_constant(analyzer, class_part, callable_name)?;
    let class_info = analyzer.codebase.get_class(class_id)?;
    if let Some(prefix) = const_part.strip_suffix('*') {
        let mut combined: Option<TUnion> = None;
        for (constant_name, constant_info) in &class_info.constants {
            let candidate_name = analyzer.interner.lookup(*constant_name);
            if !candidate_name.starts_with(prefix) {
                continue;
            }

            combined = Some(match combined {
                Some(existing) => {
                    combine_union_types(&existing, &constant_info.constant_type, false)
                }
                None => constant_info.constant_type.clone(),
            });
        }

        return combined;
    }

    let constant_id = analyzer.interner.intern(const_part);
    class_info
        .constants
        .get(&constant_id)
        .map(|constant_info| constant_info.constant_type.clone())
}

fn resolve_class_reference_for_constant(
    analyzer: &StatementsAnalyzer<'_>,
    class_part: &str,
    callable_name: &str,
) -> Option<StrId> {
    let self_class = resolve_self_class_for_callable(analyzer, callable_name);

    if class_part.eq_ignore_ascii_case("self") || class_part.eq_ignore_ascii_case("static") {
        return self_class;
    }

    if class_part.eq_ignore_ascii_case("parent") {
        let self_class = self_class?;
        return analyzer
            .codebase
            .get_class(self_class)
            .and_then(|class_info| class_info.parent_class);
    }

    let class_id = analyzer.interner.intern(class_part);
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let fq = format!("\\{}", class_part.trim_start_matches('\\'));
    let fq_id = analyzer.interner.intern(&fq);
    analyzer.codebase.get_class(fq_id).map(|_| fq_id)
}

fn resolve_self_class_for_callable(
    analyzer: &StatementsAnalyzer<'_>,
    callable_name: &str,
) -> Option<StrId> {
    if let Some((class_name, _)) = callable_name.split_once("::") {
        let class_id = analyzer.interner.intern(class_name);
        if analyzer.codebase.get_class(class_id).is_some() {
            return Some(class_id);
        }

        let fq = format!("\\{}", class_name.trim_start_matches('\\'));
        let fq_id = analyzer.interner.intern(&fq);
        if analyzer.codebase.get_class(fq_id).is_some() {
            return Some(fq_id);
        }
    }

    analyzer.get_declaring_class()
}

fn validate_callable_argument(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_pos: Pos,
    arg_type: &TUnion,
    expected_type: &TUnion,
    argument_offset: usize,
    callable_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    prefer_invalid_argument_for_undefined: bool,
) -> CallableValidationOutcome {
    let expected_callables = get_expected_callable_atomics(expected_type);
    if expected_callables.is_empty() {
        return CallableValidationOutcome::NotApplicable;
    }

    let candidate = if let Some(candidate) = resolve_callable_from_concat_expr(
        analyzer,
        arg.value(),
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
    ) {
        candidate
    } else if let Some(candidate) = resolve_candidate_from_union(
        analyzer,
        arg_type,
        &expected_callables,
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
        context,
    ) {
        candidate
    } else {
        return CallableValidationOutcome::NotApplicable;
    };

    let mut selected_issue_kind: Option<IssueKind> = None;
    let candidate_from_resolved_reference = !arg_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }));

    for expected_callable in expected_callables {
        let mut comparison_result = TypeComparisonResult::new();
        let is_match = callable_type_comparator::is_contained_by(
            analyzer.codebase,
            &candidate,
            expected_callable,
            &mut comparison_result,
        );

        if is_match {
            if candidate_from_resolved_reference
                && is_optional_param_gap_mismatch(&candidate, expected_callable)
            {
                selected_issue_kind = Some(select_preferred_callable_issue_kind(
                    selected_issue_kind,
                    IssueKind::PossiblyInvalidArgument,
                ));
                continue;
            }

            return CallableValidationOutcome::Valid;
        }

        let issue_kind = determine_callable_mismatch_issue_kind(
            analyzer,
            &candidate,
            expected_callable,
            &comparison_result,
        );

        selected_issue_kind = Some(select_preferred_callable_issue_kind(
            selected_issue_kind,
            issue_kind,
        ));
    }

    let kind = selected_issue_kind.unwrap_or(IssueKind::InvalidArgument);

    add_issue(
        analyzer,
        analysis_data,
        arg_pos,
        kind,
        format!(
            "Argument {} of {} expects {}, callable {} provided",
            argument_offset + 1,
            callable_name,
            expected_type.get_id(Some(analyzer.interner)),
            candidate.get_id(Some(analyzer.interner))
        ),
    );

    CallableValidationOutcome::IssueEmitted
}

fn resolve_callable_from_concat_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    prefer_invalid_argument_for_undefined: bool,
) -> Option<TAtomic> {
    let Expression::Binary(binary) = expr else {
        return None;
    };

    if !matches!(binary.operator, BinaryOperator::StringConcat(_)) {
        return None;
    }

    let class_id = get_class_from_class_const_expr(analyzer, binary.lhs)?;
    let method_name = get_literal_string(binary.rhs)?
        .strip_prefix("::")?
        .to_string();

    resolve_method_callable(
        analyzer,
        class_id,
        &method_name,
        true,
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
    )
}

fn resolve_candidate_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    expected_callables: &[&TAtomic],
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    prefer_invalid_argument_for_undefined: bool,
    context: &BlockContext,
) -> Option<TAtomic> {
    for atomic in &arg_type.types {
        match atomic {
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => return Some(atomic.clone()),
            TAtomic::TNamedObject { name, .. } => {
                if let Some(candidate) = resolve_named_object_callable(analyzer, *name) {
                    return Some(candidate);
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(candidate) = resolve_candidate_from_union(
                    analyzer,
                    as_type,
                    expected_callables,
                    arg_pos,
                    analysis_data,
                    prefer_invalid_argument_for_undefined,
                    context,
                ) {
                    return Some(candidate);
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for nested_atomic in types {
                    if let Some(candidate) = resolve_candidate_from_union(
                        analyzer,
                        &TUnion::new(nested_atomic.clone()),
                        expected_callables,
                        arg_pos,
                        analysis_data,
                        prefer_invalid_argument_for_undefined,
                        context,
                    ) {
                        return Some(candidate);
                    }
                }
            }
            TAtomic::TLiteralString { value } => {
                if let Some(candidate) = resolve_string_callable(
                    analyzer,
                    value,
                    expected_callables,
                    arg_pos,
                    analysis_data,
                    prefer_invalid_argument_for_undefined,
                    context,
                ) {
                    return Some(candidate);
                }
            }
            TAtomic::TKeyedArray { properties, .. } => {
                if let Some(candidate) =
                    resolve_array_callable(analyzer, properties, arg_pos, analysis_data, context)
                {
                    return Some(candidate);
                }
            }
            _ => {}
        }
    }

    None
}

fn resolve_array_callable(
    analyzer: &StatementsAnalyzer<'_>,
    properties: &rustc_hash::FxHashMap<ArrayKey, TUnion>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TAtomic> {
    if properties.len() != 2 {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable must have exactly two elements",
        );
        return None;
    }

    let Some(first) = properties.get(&ArrayKey::Int(0)) else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable first element must be at offset 0",
        );
        return None;
    };

    let Some(second) = properties.get(&ArrayKey::Int(1)) else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable second element must be at offset 1",
        );
        return None;
    };

    let Some(method_name) = get_literal_string_from_union(second) else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable method name must be a literal string",
        );
        return None;
    };

    if method_name.starts_with("::") || method_name.is_empty() {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable method name is malformed",
        );
        return None;
    }

    if let Some(class_id) = get_class_id_from_union(analyzer, first, context) {
        return resolve_method_callable(
            analyzer,
            class_id,
            method_name,
            true,
            arg_pos,
            analysis_data,
            true,
        );
    }

    if let Some(object_class_id) = get_object_class_id_from_union(first) {
        return resolve_method_callable(
            analyzer,
            object_class_id,
            method_name,
            false,
            arg_pos,
            analysis_data,
            true,
        );
    }

    add_issue(
        analyzer,
        analysis_data,
        arg_pos,
        IssueKind::InvalidArgument,
        "Array callable first element must be a class string or object",
    );

    None
}

fn resolve_named_object_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> Option<TAtomic> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let invoke_method = class_info.methods.get(&StrId::INVOKE)?;
    if invoke_method.visibility != Visibility::Public {
        return None;
    }

    Some(functionlike_to_callable(invoke_method))
}

fn resolve_string_callable(
    analyzer: &StatementsAnalyzer<'_>,
    raw: &str,
    expected_callables: &[&TAtomic],
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_invalid_argument: bool,
    context: &BlockContext,
) -> Option<TAtomic> {
    let cleaned = raw.strip_prefix('\\').unwrap_or(raw);

    if cleaned.is_empty() {
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Callable string cannot be empty",
            );
        }
        return None;
    }

    if let Some((class_name, method_name)) = cleaned.split_once("::") {
        let Some(class_id) = resolve_class_id(analyzer, class_name, context) else {
            if emit_invalid_argument {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::InvalidArgument,
                    "Invalid callable class reference",
                );
            } else {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::UndefinedClass,
                    format!("Class {} does not exist", class_name),
                );
            }
            return None;
        };

        return resolve_method_callable(
            analyzer,
            class_id,
            method_name,
            true,
            arg_pos,
            analysis_data,
            emit_invalid_argument,
        );
    }

    if let Some(function_info) = resolve_callable_function(analyzer, cleaned, context) {
        return Some(functionlike_to_callable(function_info));
    }

    if has_local_function_declaration(analyzer, cleaned) {
        if let Some(expected_callable) = expected_callables.first() {
            return Some((*expected_callable).clone());
        }

        return Some(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: Some(false),
        });
    }

    if emit_invalid_argument {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Invalid callable function reference",
        );
    } else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::UndefinedFunction,
            format!("Function {} is not defined", cleaned),
        );
    }
    None
}

fn resolve_method_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: pzoom_str::StrId,
    method_name: &str,
    class_style: bool,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_invalid_argument: bool,
) -> Option<TAtomic> {
    let Some(class_info) = analyzer.codebase.get_class(class_id) else {
        let class_name = analyzer.interner.lookup(class_id);
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Invalid callable class reference",
            );
        } else {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", class_name),
            );
        }
        return None;
    };

    let Some(method_info) = get_method_info_case_insensitive(analyzer, class_info, method_name)
    else {
        let class_name = analyzer.interner.lookup(class_id);
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Invalid callable method reference",
            );
        } else {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedMethod,
                format!("Method {}::{} does not exist", class_name, method_name),
            );
        }
        return None;
    };

    if class_style {
        if !method_info.is_static || method_info.visibility != Visibility::Public {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                format!(
                    "Callable {}::{} must reference a public static method",
                    analyzer.interner.lookup(class_id),
                    method_name
                ),
            );
            return None;
        }
    } else if method_info.visibility != Visibility::Public {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            format!(
                "Callable {}::{} must reference a public method",
                analyzer.interner.lookup(class_id),
                method_name
            ),
        );
        return None;
    }

    Some(functionlike_to_callable(method_info))
}

fn functionlike_to_callable(function_info: &FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TCallable {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure || function_info.is_mutation_free),
    }
}

fn is_optional_param_gap_mismatch(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    if candidate_params.len() >= expected_params.len() {
        return false;
    }

    expected_params[candidate_params.len()..]
        .iter()
        .all(|p| p.is_optional || p.is_variadic)
}

fn get_callable_params(atomic: &TAtomic) -> Option<&Vec<FunctionLikeParameter>> {
    match atomic {
        TAtomic::TCallable { params, .. } | TAtomic::TClosure { params, .. } => params.as_ref(),
        _ => None,
    }
}

fn has_scalar_callable_mismatch(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let candidate_params = get_callable_params(candidate);
    let expected_params = get_callable_params(expected);

    if let (Some(candidate_params), Some(expected_params)) = (candidate_params, expected_params) {
        let shared = candidate_params.len().min(expected_params.len());
        for idx in 0..shared {
            let candidate_param = &candidate_params[idx].param_type;
            let expected_param = &expected_params[idx].param_type;
            if is_scalar_only_union(candidate_param)
                && is_scalar_only_union(expected_param)
                && candidate_param.get_id(None) != expected_param.get_id(None)
            {
                return true;
            }
        }
    }

    let candidate_return = match candidate {
        TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
            return_type
        }
        _ => return false,
    };

    let expected_return = match expected {
        TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
            return_type
        }
        _ => return false,
    };

    if let (Some(candidate_return), Some(expected_return)) = (candidate_return, expected_return) {
        if is_scalar_only_union(candidate_return)
            && is_scalar_only_union(expected_return)
            && candidate_return.get_id(None) != expected_return.get_id(None)
        {
            return true;
        }
    }

    false
}

fn is_scalar_union(union: &TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    matches!(
        union.get_single(),
        Some(
            TAtomic::TInt
                | TAtomic::TFloat
                | TAtomic::TString
                | TAtomic::TBool
                | TAtomic::TTrue
                | TAtomic::TFalse
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralFloat { .. }
                | TAtomic::TLiteralString { .. }
        )
    )
}

fn is_scalar_only_union(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(is_scalar_atomic)
}

fn is_scalar_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
    )
}

fn union_is_string_like(union: &TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    matches!(
        union.get_single(),
        Some(
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
        )
    )
}

fn callable_allows_unknown_runtime_class(callable_name: &str) -> bool {
    callable_name.eq_ignore_ascii_case("class_exists")
        || callable_name.eq_ignore_ascii_case("interface_exists")
        || callable_name.eq_ignore_ascii_case("trait_exists")
        || callable_name.eq_ignore_ascii_case("enum_exists")
        || callable_name.eq_ignore_ascii_case("is_a")
        || callable_name.eq_ignore_ascii_case("is_subclass_of")
}

fn union_is_array_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. } => true,
            TAtomic::TTemplateParam { as_type, .. } => union_is_array_like(as_type),
            _ => false,
        })
}

fn union_is_list_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => true,
            TAtomic::TKeyedArray { is_list, .. } => *is_list,
            TAtomic::TTemplateParam { as_type, .. } => union_is_list_like(as_type),
            _ => false,
        })
}

fn is_untyped_callable_union(union: &TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    match union.get_single() {
        Some(
            TAtomic::TCallable {
                params: None,
                return_type: None,
                ..
            }
            | TAtomic::TClosure {
                params: None,
                return_type: None,
                ..
            },
        ) => true,
        Some(TAtomic::TNamedObject { name, .. }) => *name == StrId::CLOSURE,
        _ => false,
    }
}

fn has_typed_callable_signature_union(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| match atomic {
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => params.is_some() || return_type.is_some(),
        _ => false,
    })
}

fn get_expected_callable_atomics(union: &TUnion) -> Vec<&TAtomic> {
    union
        .types
        .iter()
        .filter(|t| matches!(t, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
        .collect()
}

fn determine_callable_mismatch_issue_kind(
    analyzer: &StatementsAnalyzer<'_>,
    candidate: &TAtomic,
    expected_callable: &TAtomic,
    comparison_result: &TypeComparisonResult,
) -> IssueKind {
    if has_non_overlapping_callable_arity(candidate, expected_callable) {
        return IssueKind::InvalidArgument;
    }

    if is_optional_param_gap_mismatch(candidate, expected_callable) {
        return IssueKind::PossiblyInvalidArgument;
    }

    if has_scalar_callable_mismatch(candidate, expected_callable)
        || comparison_result.scalar_type_match_found
    {
        return IssueKind::InvalidScalarArgument;
    }

    if comparison_result
        .type_coerced_from_nested_mixed
        .unwrap_or(false)
    {
        return IssueKind::MixedArgumentTypeCoercion;
    }

    let candidate_union = TUnion::new(candidate.clone());
    let expected_union = TUnion::new(expected_callable.clone());

    if union_type_comparator::can_be_contained_by(
        analyzer.codebase,
        &candidate_union,
        &expected_union,
    ) {
        IssueKind::PossiblyInvalidArgument
    } else {
        IssueKind::InvalidArgument
    }
}

fn has_non_overlapping_callable_arity(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    let candidate_required = candidate_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();
    let expected_required = expected_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    let candidate_max = if candidate_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(candidate_params.len())
    };
    let expected_max = if expected_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(expected_params.len())
    };

    if let Some(expected_max) = expected_max
        && candidate_required > expected_max
    {
        return true;
    }

    if let Some(candidate_max) = candidate_max
        && candidate_max < expected_required
    {
        return true;
    }

    false
}

fn select_preferred_callable_issue_kind(
    current: Option<IssueKind>,
    incoming: IssueKind,
) -> IssueKind {
    let incoming_priority = callable_issue_priority(incoming);

    match current {
        None => incoming,
        Some(existing) => {
            if incoming_priority < callable_issue_priority(existing) {
                incoming
            } else {
                existing
            }
        }
    }
}

fn callable_issue_priority(kind: IssueKind) -> u8 {
    match kind {
        IssueKind::PossiblyInvalidArgument => 0,
        IssueKind::MixedArgumentTypeCoercion => 1,
        IssueKind::InvalidScalarArgument => 2,
        IssueKind::InvalidArgument => 3,
        _ => 4,
    }
}

fn get_literal_string<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Literal(Literal::String(s)) => s.value,
        _ => None,
    }
}

fn get_literal_string_from_union(union: &TUnion) -> Option<&str> {
    if !union.is_single() {
        return None;
    }

    match union.get_single()? {
        TAtomic::TLiteralString { value } => Some(value.as_str()),
        _ => None,
    }
}

fn get_class_from_class_const_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<pzoom_str::StrId> {
    let Expression::Access(access) = expr else {
        return None;
    };

    let mago_syntax::ast::ast::access::Access::ClassConstant(const_access) = access else {
        return None;
    };

    let mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector::Identifier(id) =
        &const_access.constant
    else {
        return None;
    };

    if !id.value.eq_ignore_ascii_case("class") {
        return None;
    }

    resolve_class_id_from_expr(analyzer, const_access.class)
}

fn resolve_class_id_from_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<pzoom_str::StrId> {
    match expr {
        Expression::Identifier(id) => analyzer
            .get_resolved_name(id.span().start.offset)
            .or_else(|| Some(analyzer.interner.intern(id.value()))),
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer
            .get_declaring_class()
            .and_then(|class_id| analyzer.codebase.get_class(class_id))
            .and_then(|class_info| class_info.parent_class),
        Expression::MagicConstant(mc) => {
            if matches!(
                mc,
                mago_syntax::ast::ast::magic_constant::MagicConstant::Class(_)
            ) {
                analyzer.get_declaring_class()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn resolve_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    let normalized = class_name.strip_prefix('\\').unwrap_or(class_name);
    if normalized.is_empty() {
        return None;
    }

    match normalized.to_ascii_lowercase().as_str() {
        "self" | "static" => analyzer.get_declaring_class(),
        "parent" => analyzer
            .get_declaring_class()
            .and_then(|class_id| analyzer.codebase.get_class(class_id))
            .and_then(|class_info| class_info.parent_class),
        _ => {
            let class_id = analyzer.interner.intern(normalized);
            if analyzer.codebase.get_class(class_id).is_some() {
                return Some(class_id);
            }

            if !normalized.contains('\\') {
                if let Some(namespace_id) = context.namespace {
                    let namespace = analyzer.interner.lookup(namespace_id);
                    let namespaced_id = analyzer
                        .interner
                        .intern(&format!("{}\\{}", namespace, normalized));
                    if analyzer.codebase.get_class(namespaced_id).is_some() {
                        return Some(namespaced_id);
                    }
                }
            }

            None
        }
    }
}

fn get_class_id_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    let mut class_id = None;

    for atomic in &union.types {
        let atomic_class_id = get_class_id_from_atomic(analyzer, atomic, context)?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_object_class_id_from_union(union: &TUnion) -> Option<pzoom_str::StrId> {
    let mut class_id = None;

    for atomic in &union.types {
        let atomic_class_id = get_object_class_id_from_atomic(atomic)?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_class_id_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TLiteralClassString { name } => resolve_class_id(analyzer, name, context),
        TAtomic::TLiteralString { value } => resolve_class_id(analyzer, value, context),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => get_named_class_id_from_atomic(as_type),
        TAtomic::TTemplateParam { as_type, .. } => {
            get_class_id_from_union(analyzer, as_type, context)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn get_object_class_id_from_atomic(atomic: &TAtomic) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut class_id = None;

            for nested in &as_type.types {
                let nested_id = get_object_class_id_from_atomic(nested)?;

                if let Some(existing) = class_id {
                    if existing != nested_id {
                        return None;
                    }
                } else {
                    class_id = Some(nested_id);
                }
            }

            class_id
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn get_named_class_id_from_atomic(atomic: &TAtomic) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            if as_type.is_single() {
                get_named_class_id_from_atomic(as_type.get_single()?)
            } else {
                None
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn resolve_callable_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    _context: &BlockContext,
) -> Option<&'a FunctionLikeInfo> {
    if let Some(stripped) = name.strip_prefix('\\') {
        let function_id = analyzer.interner.intern(stripped);
        if let Some(function_info) = analyzer.codebase.get_function(function_id) {
            return Some(function_info);
        }

        return find_function_case_insensitive(analyzer, stripped);
    }

    let function_id = analyzer.interner.intern(name);
    analyzer
        .codebase
        .get_function(function_id)
        .or_else(|| find_function_case_insensitive(analyzer, name))
}

fn find_function_case_insensitive<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    target_name: &str,
) -> Option<&'a FunctionLikeInfo> {
    analyzer
        .codebase
        .functionlike_infos
        .iter()
        .find_map(|(function_id, function_info)| {
            analyzer
                .interner
                .lookup(*function_id)
                .as_ref()
                .eq_ignore_ascii_case(target_name)
                .then_some(function_info)
        })
}

fn has_local_function_declaration(analyzer: &StatementsAnalyzer<'_>, target_name: &str) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let start = function_info.start_offset as usize;
    let end = function_info.end_offset as usize;
    if start >= end || end > analyzer.source.len() {
        return false;
    }

    let source_window = &analyzer.source[start..end];
    has_function_declaration_in_source(source_window, target_name)
}

fn has_function_declaration_in_source(source: &str, target_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;

    while i + 8 <= bytes.len() {
        if bytes[i..i + 8].eq_ignore_ascii_case(b"function") {
            // Ensure "function" token boundary.
            if i > 0 && is_ident_byte(bytes[i - 1]) {
                i += 1;
                continue;
            }
            if i + 8 < bytes.len() && is_ident_byte(bytes[i + 8]) {
                i += 1;
                continue;
            }

            let mut cursor = i + 8;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }

            if cursor < bytes.len() && bytes[cursor] == b'&' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
            }

            let name_start = cursor;
            while cursor < bytes.len() && is_ident_byte(bytes[cursor]) {
                cursor += 1;
            }

            if name_start == cursor {
                i += 1;
                continue;
            }

            let declared_name = &source[name_start..cursor];
            if declared_name.eq_ignore_ascii_case(target_name) {
                let mut after_name = cursor;
                while after_name < bytes.len() && bytes[after_name].is_ascii_whitespace() {
                    after_name += 1;
                }

                if after_name < bytes.len() && bytes[after_name] == b'(' {
                    return true;
                }
            }
        }

        i += 1;
    }

    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn get_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a pzoom_code_info::ClassLikeInfo,
    method_name: &str,
) -> Option<&'a FunctionLikeInfo> {
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

fn find_undefined_named_object_in_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> Option<String> {
    for atomic in &union.types {
        if let Some(name) = find_undefined_named_object_in_atomic(analyzer, atomic) {
            return Some(name);
        }
    }

    None
}

fn find_undefined_named_object_in_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<String> {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                return None;
            }

            let requested = analyzer.interner.lookup(*name);
            if looks_like_unresolved_conditional_docblock_type(requested.as_ref())
                || (requested.contains(':') && !requested.contains("::"))
            {
                return None;
            }
            if let Some((class_part, _)) = requested.rsplit_once("::") {
                let class_part = class_part.trim();
                let class_part = class_part.strip_prefix('\\').unwrap_or(class_part);

                if class_part.eq_ignore_ascii_case("self")
                    || class_part.eq_ignore_ascii_case("static")
                    || class_part.eq_ignore_ascii_case("parent")
                {
                    return None;
                }

                let class_part_id = analyzer.interner.intern(class_part);
                if analyzer.codebase.get_class(class_part_id).is_some() {
                    return None;
                }

                let exists_case_insensitive =
                    analyzer.codebase.classlike_infos.keys().any(|existing_id| {
                        analyzer
                            .interner
                            .lookup(*existing_id)
                            .as_ref()
                            .eq_ignore_ascii_case(class_part)
                    });

                if !exists_case_insensitive {
                    return Some(class_part.to_string());
                }

                return None;
            }

            if analyzer.codebase.get_class(*name).is_none() {
                let exists_case_insensitive =
                    analyzer.codebase.classlike_infos.keys().any(|existing_id| {
                        analyzer
                            .interner
                            .lookup(*existing_id)
                            .as_ref()
                            .eq_ignore_ascii_case(requested.as_ref())
                    });

                if !exists_case_insensitive {
                    return Some(requested.to_string());
                }
            }

            if let Some(type_params) = type_params {
                for param_type in type_params {
                    if let Some(name) = find_undefined_named_object_in_union(analyzer, param_type) {
                        return Some(name);
                    }
                }
            }

            None
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            find_undefined_named_object_in_union(analyzer, as_type)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            find_undefined_named_object_in_atomic(analyzer, as_type)
        }
        TAtomic::TObjectIntersection { types } => {
            for intersection_atomic in types {
                if let Some(name) =
                    find_undefined_named_object_in_atomic(analyzer, intersection_atomic)
                {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

fn find_undefined_class_string_literal_in_argument(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    param_type: &TUnion,
    context: &BlockContext,
) -> Option<String> {
    let expr = expr.unparenthesized();

    if !matches!(expr, Expression::Array(_) | Expression::LegacyArray(_)) {
        let single_param_atomic = param_type.get_single()?;
        return find_undefined_class_string_literal_in_argument_for_atomic(
            analyzer,
            expr,
            single_param_atomic,
            context,
        );
    }

    for atomic in &param_type.types {
        if let Some(name) = find_undefined_class_string_literal_in_argument_for_atomic(
            analyzer, expr, atomic, context,
        ) {
            return Some(name);
        }
    }

    None
}

fn find_undefined_class_string_literal_in_argument_for_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    param_atomic: &TAtomic,
    context: &BlockContext,
) -> Option<String> {
    let expr = expr.unparenthesized();

    match param_atomic {
        TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TTemplateParamClass { .. } => {
            let literal = get_literal_string_value(expr)?;
            if classlike_exists_case_insensitive(analyzer, &literal, context) {
                None
            } else {
                Some(literal)
            }
        }
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => {
            find_undefined_class_string_literal_in_array_argument(
                analyzer, expr, value_type, context,
            )
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let all_properties_expect_class_string =
                !properties.is_empty() && properties.values().all(union_contains_class_string);
            let fallback_expects_class_string = fallback_value_type
                .as_ref()
                .is_some_and(|fallback| union_contains_class_string(fallback));

            if all_properties_expect_class_string || fallback_expects_class_string {
                let value_type = fallback_value_type
                    .as_ref()
                    .map(|fallback| fallback.as_ref())
                    .or_else(|| properties.values().next())?;

                return find_undefined_class_string_literal_in_array_argument(
                    analyzer, expr, value_type, context,
                );
            }

            None
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            find_undefined_class_string_literal_in_argument(analyzer, expr, as_type, context)
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                if let Some(name) = find_undefined_class_string_literal_in_argument_for_atomic(
                    analyzer, expr, nested, context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        _ => None,
    }
}

fn find_undefined_class_string_literal_in_array_argument(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    value_param_type: &TUnion,
    context: &BlockContext,
) -> Option<String> {
    if !union_contains_class_string(value_param_type) {
        return None;
    }

    let expr = expr.unparenthesized();

    match expr {
        Expression::Array(array) => {
            for element in array.elements.iter() {
                let value_expr = match element {
                    ArrayElement::KeyValue(kv) => kv.value,
                    ArrayElement::Value(value) => value.value,
                    ArrayElement::Variadic(_) | ArrayElement::Missing(_) => continue,
                };

                if let Some(name) = find_undefined_class_string_literal_in_argument(
                    analyzer,
                    value_expr,
                    value_param_type,
                    context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        Expression::LegacyArray(array) => {
            for element in array.elements.iter() {
                let value_expr = match element {
                    ArrayElement::KeyValue(kv) => kv.value,
                    ArrayElement::Value(value) => value.value,
                    ArrayElement::Variadic(_) | ArrayElement::Missing(_) => continue,
                };

                if let Some(name) = find_undefined_class_string_literal_in_argument(
                    analyzer,
                    value_expr,
                    value_param_type,
                    context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        _ => None,
    }
}

fn get_literal_string_value(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::String(string_literal)) => {
            string_literal.value.map(ToString::to_string)
        }
        Expression::Binary(binary)
            if matches!(binary.operator, BinaryOperator::StringConcat(_)) =>
        {
            let lhs = get_literal_string_value(binary.lhs)?;
            let rhs = get_literal_string_value(binary.rhs)?;
            Some(format!("{}{}", lhs, rhs))
        }
        _ => None,
    }
}

fn classlike_exists_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if normalized.is_empty() {
        return false;
    }

    if resolve_class_id(analyzer, normalized, context).is_some() {
        return true;
    }

    let normalized_id = analyzer.interner.intern(normalized);
    if analyzer.codebase.get_class(normalized_id).is_some() {
        return true;
    }

    let has_case_insensitive_match = analyzer.codebase.classlike_infos.keys().any(|existing_id| {
        analyzer
            .interner
            .lookup(*existing_id)
            .as_ref()
            .trim_start_matches('\\')
            .eq_ignore_ascii_case(normalized)
    });

    if has_case_insensitive_match {
        return true;
    }

    if !normalized.contains('\\')
        && let Some(namespace_id) = context.namespace
    {
        let namespace = analyzer.interner.lookup(namespace_id);
        let namespaced_candidate = format!("{}\\{}", namespace, normalized);

        return analyzer.codebase.classlike_infos.keys().any(|existing_id| {
            analyzer
                .interner
                .lookup(*existing_id)
                .as_ref()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(namespaced_candidate.trim_start_matches('\\'))
        });
    }

    false
}

fn is_valid_by_ref_arg(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    context: &BlockContext,
) -> bool {
    let expr = arg.value().unparenthesized();
    if matches!(
        expr,
        Expression::Variable(_)
            | Expression::ArrayAccess(_)
            | Expression::Access(_)
            | Expression::Assignment(_)
    ) {
        return true;
    }

    let Expression::Call(call) = expr else {
        return false;
    };

    call_returns_by_ref(analyzer, call, context)
}

fn call_returns_by_ref(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    context: &BlockContext,
) -> bool {
    match call {
        Call::Function(function_call) => {
            let Expression::Identifier(function_id) = function_call.function.unparenthesized()
            else {
                return false;
            };

            let resolved_id = analyzer
                .get_resolved_name(function_id.span().start.offset)
                .unwrap_or_else(|| analyzer.interner.intern(function_id.value()));

            analyzer
                .codebase
                .get_function(resolved_id)
                .is_some_and(|function_info| function_info.returns_by_ref)
        }
        Call::Method(method_call) => {
            let ClassLikeMemberSelector::Identifier(method_id) = &method_call.method else {
                return false;
            };

            let method_name_id = analyzer.interner.intern(method_id.value);
            let Expression::Variable(Variable::Direct(direct)) =
                method_call.object.unparenthesized()
            else {
                return false;
            };

            let var_id = analyzer.interner.intern(direct.name);
            let Some(object_type) = context.get_var_type(var_id) else {
                return false;
            };

            object_type.types.iter().any(|atomic| {
                matches!(atomic, TAtomic::TNamedObject { name, .. } if analyzer
                    .codebase
                    .get_class(*name)
                    .and_then(|class_info| class_info.methods.get(&method_name_id))
                    .is_some_and(|method_info| method_info.returns_by_ref))
            })
        }
        _ => false,
    }
}

fn check_by_ref_property_mutability(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Expression::Access(access) = arg.value() else {
        return;
    };

    let Access::Property(property_access) = access else {
        return;
    };

    let ClassLikeMemberSelector::Identifier(id) = &property_access.property else {
        return;
    };

    let object_span = property_access.object.span();
    let Some(object_type) =
        analysis_data.get_expr_type((object_span.start.offset, object_span.end.offset))
    else {
        return;
    };

    let property_id = analyzer.interner.intern(id.value);

    for atomic in &object_type.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };

        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };

        let Some(prop_info) = class_info.properties.get(&property_id) else {
            continue;
        };

        if prop_info.is_readonly || class_info.is_immutable {
            let class_name = analyzer.interner.lookup(*name);
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InaccessibleProperty,
                format!(
                    "Cannot pass readonly or immutable property {}::${} by reference",
                    class_name, id.value
                ),
            );
            break;
        }
    }
}

fn add_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
    kind: IssueKind,
    message: impl Into<String>,
) {
    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}
