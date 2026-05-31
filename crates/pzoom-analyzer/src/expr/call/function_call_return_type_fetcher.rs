//! Function call return type fetcher.
//!
//! Mirrors Psalm/Hakana's dedicated function return-type fetcher flow:
//! special-case builtins first, then function storage return type.

use mago_syntax::ast::ast::argument::Argument;
use rustc_hash::FxHashMap;

use pzoom_code_info::functionlike_info::ConditionalReturnCondition;
use pzoom_code_info::{ArrayKey, FunctionLikeInfo, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

use super::function_call_analyzer;
use crate::template::TemplateMap;

pub(crate) fn fetch(
    analyzer: &StatementsAnalyzer<'_>,
    normalized_name: &str,
    function_info: Option<&FunctionLikeInfo>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    template_defaults: Option<&TemplateMap>,
    template_replacements: Option<&TemplateMap>,
) -> Option<TUnion> {
    let normalized_name = normalized_name
        .strip_prefix('\\')
        .unwrap_or(normalized_name);

    // Function return-type providers (Psalm-style extension point).
    if let Some(return_type) = crate::return_type_provider::dispatch_function_return_type(
        &crate::return_type_provider::FunctionReturnTypeProviderEvent {
            analyzer,
            function_id: normalized_name,
            args,
            arg_positions,
            context,
        },
        analysis_data,
    ) {
        return Some(return_type);
    }

    let Some(function_info) = function_info else {
        return None;
    };

    if function_info.get_return_type().is_none() {
        return None;
    }

    let empty_template_defaults = TemplateMap::new();
    let empty_template_replacements = TemplateMap::new();
    let template_defaults = template_defaults.unwrap_or(&empty_template_defaults);
    let template_replacements = template_replacements.unwrap_or(&empty_template_replacements);

    // Map each parameter to its argument's inferred type, so conditional return
    // types of the form `($param is X ? A : B)` can be evaluated at the call site.
    let mut param_arg_types: FxHashMap<StrId, TUnion> = FxHashMap::default();
    for (index, param) in function_info.params.iter().enumerate() {
        if let Some(arg_pos) = arg_positions.get(index) {
            if let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) {
                param_arg_types.insert(param.name, (*arg_type).clone());
            }
        }
    }

    resolve_functionlike_return_type(
        analyzer,
        function_info,
        template_defaults,
        template_replacements,
        &param_arg_types,
        args.len(),
    )
    .or_else(|| Some(TUnion::mixed()))
}

pub(crate) fn fetch_microtime_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let arg_pos = *arg_positions.first()?;
    let arg_type = analysis_data.get_expr_type(arg_pos)?;

    if arg_type.is_always_truthy() {
        Some(TUnion::float())
    } else if arg_type.is_always_falsy() {
        Some(TUnion::string())
    } else {
        None
    }
}

pub(crate) fn fetch_preg_split_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let pattern_pos = *arg_positions.first()?;
    let subject_pos = *arg_positions.get(1)?;
    let pattern_type = analysis_data.get_expr_type(pattern_pos)?;
    let subject_type = analysis_data.get_expr_type(subject_pos)?;

    if !union_is_string_like(&pattern_type) || !union_is_string_like(&subject_type) {
        return None;
    }

    let list_atomic = if let Some(flags_pos) = arg_positions.get(3).copied() {
        let flags_type = analysis_data.get_expr_type(flags_pos)?;
        match get_single_literal_int(&flags_type) {
            Some(0 | 2) => TAtomic::TNonEmptyList {
                value_type: Box::new(TUnion::string()),
            },
            Some(1 | 3) => TAtomic::TList {
                value_type: Box::new(TUnion::string()),
            },
            Some(_) => TAtomic::TList {
                value_type: Box::new(TUnion::new(make_offset_capture_shape())),
            },
            None => TAtomic::TNonEmptyList {
                value_type: Box::new(TUnion::string()),
            },
        }
    } else {
        TAtomic::TNonEmptyList {
            value_type: Box::new(TUnion::string()),
        }
    };

    let mut result = TUnion::from_types(vec![list_atomic, TAtomic::TFalse]);
    result.ignore_falsable_issues = true;
    Some(result)
}

pub(crate) fn fetch_hrtime_return_type(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let tuple_type = TAtomic::TNonEmptyList {
        value_type: Box::new(TUnion::int()),
    };

    if args.is_empty() {
        return Some(TUnion::new(tuple_type));
    }

    let first_arg_pos = *arg_positions.first()?;
    let first_arg_type = analysis_data.get_expr_type(first_arg_pos)?;

    match get_single_literal_bool(&first_arg_type) {
        Some(true) => Some(TUnion::int()),
        Some(false) => Some(TUnion::new(tuple_type)),
        None => Some(TUnion::from_types(vec![TAtomic::TInt, tuple_type])),
    }
}

fn union_is_string_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TClassString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
        )
    })
}

fn get_single_literal_int(union: &TUnion) -> Option<i64> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}

fn get_single_literal_bool(union: &TUnion) -> Option<bool> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn make_offset_capture_shape() -> TAtomic {
    let mut properties = FxHashMap::default();
    properties.insert(ArrayKey::Int(0), TUnion::string());
    properties.insert(ArrayKey::Int(1), TUnion::int());

    TAtomic::TKeyedArray {
        properties,
        is_list: true,
        sealed: true,
        fallback_key_type: None,
        fallback_value_type: None,
    }
}

pub(crate) fn resolve_functionlike_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let return_type = function_info.get_return_type()?;
    let mut effective_template_replacements = template_replacements.clone();
    inject_fetcher_template_replacements(
        analyzer,
        function_info,
        arg_count,
        &mut effective_template_replacements,
    );

    // A conditional return type is carried as a single TConditional atomic on the
    // return type (Psalm's Type\Atomic\TConditional); evaluate it against the
    // argument/template the condition tests.
    if return_type.types.len() == 1 {
        if let Some(TAtomic::TConditional(conditional_return_type)) = return_type.types.first() {
            let mut resolved = resolve_conditional_return_type(
                analyzer,
                conditional_return_type,
                template_defaults,
                &effective_template_replacements,
                param_arg_types,
                arg_count,
            );

            // Keep top-level docblock suppression flags when resolving conditional
            // branches. Psalm stores these flags on the return union itself.
            resolved.from_docblock |= return_type.from_docblock;
            resolved.ignore_nullable_issues |= return_type.ignore_nullable_issues;
            resolved.ignore_falsable_issues |= return_type.ignore_falsable_issues;

            // Collapse any conditional nested in the resolved type's parameters.
            crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut resolved,
        &crate::type_expander::TypeExpansionOptions { evaluate_conditional_types: true, ..Default::default() },
    );

            return Some(resolved);
        }
    }

    let mut resolved = if template_defaults.is_empty() && effective_template_replacements.is_empty()
    {
        return_type.clone()
    } else {
        function_call_analyzer::replace_templates_in_union(
            return_type,
            &effective_template_replacements,
            template_defaults,
        )
    };
    // Template substitution rebuilds the union from its atomics, dropping the
    // docblock suppression flags Psalm stores on the return union itself
    // (`@psalm-ignore-nullable-return` / `@psalm-ignore-falsable-return`). Carry them
    // over from the declared return type, mirroring the conditional branch above.
    resolved.ignore_nullable_issues |= return_type.ignore_nullable_issues;
    resolved.ignore_falsable_issues |= return_type.ignore_falsable_issues;

    // Collapse any conditional nested in a non-conditional return type's parameters.
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut resolved,
        &crate::type_expander::TypeExpansionOptions { evaluate_conditional_types: true, ..Default::default() },
    );
    Some(resolved)
}

pub(crate) fn inject_fetcher_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    arg_count: usize,
    template_replacements: &mut TemplateMap,
) {
    for template_type in &function_info.template_types {
        if template_replacements.contains_name(template_type.name) {
            continue;
        }

        let template_name = analyzer.interner.lookup(template_type.name);
        let replacement = if template_name
            .as_ref()
            .eq_ignore_ascii_case("TFunctionArgCount")
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: arg_count as i64,
            })
        } else if template_name
            .as_ref()
            .eq_ignore_ascii_case("TPhpMajorVersion")
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_major_version(analyzer),
            })
        } else if template_name.as_ref().eq_ignore_ascii_case("TPhpVersionId") {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_version_id(analyzer),
            })
        } else {
            TUnion::nothing()
        };

        template_replacements.insert(template_type.name, template_type.defining_entity, replacement);
    }
}

pub(crate) fn get_configured_php_major_version(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, _, _) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    major as i64
}

pub(crate) fn get_configured_php_version_id(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, minor, patch) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    (major as i64) * 10_000 + (minor as i64) * 100 + (patch as i64)
}

pub(crate) fn parse_php_version_tuple(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    (major, minor, patch)
}

pub(crate) fn resolve_conditional_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    conditional_return_type: &pzoom_code_info::functionlike_info::ConditionalReturnType,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> TUnion {
    let resolve_branch = |branch: &TUnion| -> TUnion {
        if template_defaults.is_empty() && template_replacements.is_empty() {
            branch.clone()
        } else {
            function_call_analyzer::replace_templates_in_union(branch, template_replacements, template_defaults)
        }
    };

    match &conditional_return_type.condition {
        // `$param is X ? A : B`, evaluated against the argument type bound to the
        // parameter (mirroring how Psalm reconciles the conditional against the input):
        // arg contained by X -> if_true; arg cannot be X -> if_false; otherwise union.
        ConditionalReturnCondition::ParamIs {
            param_id,
            asserted_type,
        } => {
            let asserted_type = resolve_branch(asserted_type);
            let fallback_true = resolve_branch(&conditional_return_type.if_true_type);
            let fallback_false = resolve_branch(&conditional_return_type.if_false_type);

            let Some(arg_type) = param_arg_types.get(param_id) else {
                return combine_union_types(&fallback_true, &fallback_false, false);
            };

            // Evaluate the condition per argument atomic and union the branch results
            // (mirroring the TemplateIs arm and Psalm): an atomic contained by the
            // asserted type takes if_true, one that cannot be takes if_false, and an
            // overlapping/indeterminate one (e.g. mixed) takes the union. Without this
            // per-atomic split, `mixed|array<never,never>` would pick if_false wholesale.
            let mut combined: Option<TUnion> = None;
            for arg_atomic in &arg_type.types {
                let arg_atomic_union = TUnion::new(arg_atomic.clone());

                let mut comparison_result = TypeComparisonResult::new();
                let definitely_true = union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &arg_atomic_union,
                    &asserted_type,
                    false,
                    false,
                    &mut comparison_result,
                );

                let arg_atomic_is_mixed =
                    matches!(arg_atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed);

                let branch_result = if definitely_true {
                    fallback_true.clone()
                } else if arg_atomic_is_mixed
                    || union_type_comparator::can_be_contained_by(
                        analyzer.codebase,
                        &arg_atomic_union,
                        &asserted_type,
                    )
                {
                    // A mixed argument could satisfy or fail the condition, so the
                    // result is the union of both branches (Psalm treats it as
                    // indeterminate rather than always-if_false).
                    combine_union_types(&fallback_true, &fallback_false, false)
                } else {
                    fallback_false.clone()
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &branch_result, false)
                } else {
                    branch_result
                });
            }

            combined.unwrap_or_else(|| combine_union_types(&fallback_true, &fallback_false, false))
        }
        ConditionalReturnCondition::FuncNumArgsIs { count } => {
            let selected_branch = if arg_count == *count {
                &conditional_return_type.if_true_type
            } else {
                &conditional_return_type.if_false_type
            };

            if template_defaults.is_empty() && template_replacements.is_empty() {
                selected_branch.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    selected_branch,
                    template_replacements,
                    template_defaults,
                )
            }
        }
        ConditionalReturnCondition::TemplateIs {
            template_name,
            asserted_type,
        } => {
            let template_value = get_template_binding_case_insensitive(
                analyzer,
                template_replacements,
                *template_name,
            )
            .or_else(|| {
                get_template_binding_case_insensitive(analyzer, template_defaults, *template_name)
            });

            let asserted_type = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                asserted_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults)
            };

            let fallback_true = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                conditional_return_type.if_true_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    &conditional_return_type.if_true_type,
                    template_replacements,
                    template_defaults,
                )
            };
            let fallback_false = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                conditional_return_type.if_false_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    &conditional_return_type.if_false_type,
                    template_replacements,
                    template_defaults,
                )
            };

            let Some(template_value) = template_value else {
                return combine_union_types(&fallback_true, &fallback_false, false);
            };

            let template_entity = template_replacements
                .entity_for_name(*template_name)
                .or_else(|| template_defaults.entity_for_name(*template_name))
                .unwrap_or(StrId::EMPTY);

            let mut combined: Option<TUnion> = None;
            for template_atomic in &template_value.types {
                let template_atomic_union = TUnion::new(template_atomic.clone());

                let mut branch_template_replacements = template_replacements.clone();
                branch_template_replacements.insert(
                    *template_name,
                    template_entity,
                    template_atomic_union.clone(),
                );

                let true_branch = function_call_analyzer::replace_templates_in_union(
                    &conditional_return_type.if_true_type,
                    &branch_template_replacements,
                    template_defaults,
                );
                let false_branch = function_call_analyzer::replace_templates_in_union(
                    &conditional_return_type.if_false_type,
                    &branch_template_replacements,
                    template_defaults,
                );

                let mut comparison_result = TypeComparisonResult::new();
                let definitely_true = union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &template_atomic_union,
                    &asserted_type,
                    false,
                    false,
                    &mut comparison_result,
                );

                let branch_result = if definitely_true {
                    true_branch
                } else if union_type_comparator::can_be_contained_by(
                    analyzer.codebase,
                    &template_atomic_union,
                    &asserted_type,
                ) {
                    combine_union_types(&true_branch, &false_branch, false)
                } else {
                    false_branch
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &branch_result, false)
                } else {
                    branch_result
                });
            }

            combined.unwrap_or_else(|| combine_union_types(&fallback_true, &fallback_false, false))
        }
    }
}

pub(crate) fn get_template_binding_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    bindings: &TemplateMap,
    template_name: StrId,
) -> Option<TUnion> {
    if let Some(binding) = bindings.get_by_name(template_name) {
        return Some(binding.clone());
    }

    let target = analyzer.interner.lookup(template_name);
    bindings.iter().find_map(|(candidate_name, _, binding)| {
        analyzer
            .interner
            .lookup(candidate_name)
            .as_ref()
            .eq_ignore_ascii_case(target.as_ref())
            .then_some(binding.clone())
    })
}
