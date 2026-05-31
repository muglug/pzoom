//! Post-call assertion handling for function calls.
//!
//! Holds the `@assert`/`@psalm-assert`-style assertion application and the
//! truthy/falsy narrowing helpers that pzoom applies after a call. In Psalm/Hakana
//! this work is split across the assertion finder and the type reconciler; grouping
//! it here keeps `function_call_analyzer` close to hakana-core's lean shape.

use std::collections::BTreeMap;

use mago_span::HasSpan;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::algebra::{get_truths_from_formula, simplify_cnf};
use pzoom_code_info::functionlike_info::AssertionType;
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;

use super::function_call_analyzer;
use crate::template::TemplateMap;

pub(crate) fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    analysis_data: &mut FunctionAnalysisData,
) {
    if func_info.assertions.is_empty() {
        return;
    }

    for assertion in &func_info.assertions {
        let Some(param_idx) =
            find_assertion_param_index(analyzer, &func_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(argument) = func_call.argument_list.arguments.get(param_idx) else {
            continue;
        };
        let Some(param_name) = func_info
            .params
            .get(param_idx)
            .map(|param| analyzer.interner.lookup(param.name))
        else {
            continue;
        };
        let resolved_assertion_type = replace_assertion_type_templates(
            &assertion.assertion_type,
            template_replacements,
            template_defaults,
        );

        emit_undefined_docblock_class_issues_from_assertion_type(
            analyzer,
            analysis_data,
            &resolved_assertion_type,
            argument.span().start.offset,
            argument.span().end.offset,
        );

        let argument_var_key = expression_identifier::get_expression_var_key(argument.value());
        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        let Some(var_key) = argument_var_key.as_deref().and_then(|argument_var_name| {
            map_assertion_var_to_argument(
                assertion_name.as_ref(),
                param_name.as_ref(),
                argument_var_name,
            )
        }) else {
            apply_assertion_to_argument_expression(
                analyzer,
                argument.value(),
                &resolved_assertion_type,
                context,
                analysis_data,
            );
            continue;
        };

        let var_id = analyzer.interner.intern(&var_key);
        let existing_type = context
            .locals
            .get(&var_id)
            .cloned()
            .unwrap_or_else(TUnion::mixed);
        if let AssertionType::IsType(asserted_type) = &resolved_assertion_type {
            if !existing_type.is_nothing()
                && assertion_reconciler::intersect_union_with_union(&existing_type, asserted_type)
                    .is_none()
            {
                let (line, col) = analyzer.get_line_column(argument.span().start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::TypeDoesNotContainType,
                    format!(
                        "{} does not contain {}",
                        existing_type.get_id(Some(analyzer.interner)),
                        asserted_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    argument.span().start.offset,
                    argument.span().end.offset,
                    line,
                    col,
                ));
            }
        }

        let narrowed_type =
            apply_functionlike_assertion_to_union(&existing_type, &resolved_assertion_type);
        context.locals.insert(var_id, narrowed_type);
    }
}

pub(crate) fn replace_assertion_type_templates(
    assertion_type: &AssertionType,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => AssertionType::IsType(function_call_analyzer::replace_templates_in_union(
            asserted_type,
            template_replacements,
            template_defaults,
        )),
        AssertionType::IsEqual(asserted_type) => AssertionType::IsEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsLooselyEqual(asserted_type) => AssertionType::IsLooselyEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotType(asserted_type) => AssertionType::IsNotType(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotEqual(asserted_type) => AssertionType::IsNotEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotLooselyEqual(asserted_type) => AssertionType::IsNotLooselyEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

pub(crate) fn apply_assertion_to_argument_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let assertion_result = assertion_finder::get_assertions(analyzer, expr, analysis_data);

    let assertion_map = match assertion_type {
        AssertionType::Truthy | AssertionType::NotEmpty | AssertionType::NotNull => {
            &assertion_result.if_true
        }
        AssertionType::Falsy => &assertion_result.if_false,
        AssertionType::IsType(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsType(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        AssertionType::IsEqual(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsEqual(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        AssertionType::IsLooselyEqual(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsLooselyEqual(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        _ => return,
    };

    if assertion_map.is_empty() {
        return;
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        &reconciler::to_and_groups(assertion_map),
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        false,
        None,
    );
}

pub(crate) fn is_boolean_true_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TTrue))
}

pub(crate) fn is_boolean_false_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TFalse))
}

pub(crate) fn emit_undefined_docblock_class_issues_from_assertion_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    assertion_type: &AssertionType,
    start: u32,
    end: u32,
) {
    let union = match assertion_type {
        AssertionType::IsType(union)
        | AssertionType::IsEqual(union)
        | AssertionType::IsLooselyEqual(union)
        | AssertionType::IsNotType(union)
        | AssertionType::IsNotEqual(union)
        | AssertionType::IsNotLooselyEqual(union) => union,
        _ => return,
    };

    for atomic in &union.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };

        if !looks_like_docblock_class_reference(analyzer.interner.lookup(*name).as_ref()) {
            continue;
        }

        let class_reference = get_docblock_class_reference(*name, analyzer);

        if matches!(class_reference, StrId::SELF | StrId::STATIC | StrId::PARENT) {
            continue;
        }

        if analyzer.codebase.get_class(class_reference).is_some() {
            continue;
        }

        let (line, col) = analyzer.get_line_column(start);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedDocblockClass,
            format!(
                "Docblock class {} does not exist",
                analyzer.interner.lookup(*name)
            ),
            analyzer.file_path,
            start,
            end,
            line,
            col,
        ));
    }
}

pub(crate) fn emit_non_mutation_free_magic_property_assertion_issues(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    func_call: &FunctionCall<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for assertion in function_info
        .if_true_assertions
        .iter()
        .chain(function_info.if_false_assertions.iter())
    {
        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if !assertion_name.contains("->") {
            continue;
        }

        let Some(param_idx) =
            find_assertion_param_index(analyzer, &function_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(param) = function_info.params.get(param_idx) else {
            continue;
        };
        let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) else {
            continue;
        };

        for atomic in &param_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };
            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                continue;
            };
            let Some(getter_info) = class_info.methods.get(&StrId::GET) else {
                continue;
            };

            if getter_info.is_mutation_free {
                continue;
            }

            let span = func_call.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidDocblock,
                format!(
                    "{}::__get is not mutation-free",
                    analyzer.interner.lookup(*name)
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
    }
}

pub(crate) fn apply_assert_builtin_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    func_call: &FunctionCall<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if function_id != StrId::ASSERT {
        return;
    }

    let Some(first_arg) = func_call.argument_list.arguments.first() else {
        return;
    };
    if first_arg.is_unpacked() {
        return;
    }

    let assertion_result =
        assertion_finder::get_assertions(analyzer, first_arg.value(), analysis_data);

    // Psalm's `processAssertFunctionEffects` checks the assert formula against
    // the context clauses before applying it, reporting `RedundantCondition`
    // ("$x has already been asserted") / `ParadoxicalCondition`.
    crate::algebra_analyzer::check_for_paradox(
        analyzer,
        &context.clauses,
        &assertion_result.if_true_clauses,
        analysis_data,
        (
            first_arg.value().start_offset() as u32,
            first_arg.value().end_offset() as u32,
        ),
    );

    let mut prior_truth_var_names: FxHashSet<String> = FxHashSet::default();
    if !context.clauses.is_empty() {
        let prior_clause_refs: Vec<_> = context
            .clauses
            .iter()
            .map(|clause| clause.as_ref())
            .collect();
        let prior_simplified_clauses = simplify_cnf(prior_clause_refs);
        let mut prior_cond_referenced_var_ids = FxHashSet::default();
        let (prior_truths, _) = get_truths_from_formula(
            prior_simplified_clauses.iter().collect(),
            None,
            &mut prior_cond_referenced_var_ids,
        );
        prior_truth_var_names.extend(prior_truths.into_keys());
    }

    let mut combined_clauses: Vec<_> = context
        .clauses
        .iter()
        .map(|clause| clause.as_ref())
        .collect();
    combined_clauses.extend(assertion_result.if_true_clauses.iter());

    let simplified_clauses = simplify_cnf(combined_clauses);
    let assert_conditional_id = (
        first_arg.value().start_offset() as u32,
        first_arg.value().end_offset() as u32,
    );

    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, active_truths) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(assert_conditional_id),
        &mut cond_referenced_var_ids,
    );

    let mut flattened_assertions = assertion_result.if_true.clone();
    let mut flattened_active_assertion_offsets: BTreeMap<String, FxHashSet<usize>> =
        BTreeMap::new();

    for (var_name, assertion_lists) in truths {
        let entry = flattened_assertions.entry(var_name.clone()).or_default();

        for (assertion_list_index, assertion_list) in assertion_lists.into_iter().enumerate() {
            let is_active = active_truths
                .get(&var_name)
                .is_some_and(|offsets| offsets.contains(&assertion_list_index));

            for assertion in assertion_list {
                let is_truthiness_assertion =
                    matches!(&assertion, Assertion::Truthy | Assertion::Falsy);
                let next_offset = entry.len();
                entry.push(assertion);

                if is_active {
                    if !is_truthiness_assertion {
                        continue;
                    }

                    if !prior_truth_var_names.contains(&var_name) {
                        continue;
                    }

                    let should_skip_truthiness_assertion = is_truthiness_assertion
                        && resolve_assertion_var_id(analyzer, &var_name)
                            .is_some_and(|var_id| context.is_possibly_assigned(var_id));

                    if !should_skip_truthiness_assertion {
                        flattened_active_assertion_offsets
                            .entry(var_name.clone())
                            .or_default()
                            .insert(next_offset);
                    }
                }
            }
        }
    }

    let mut changed_var_ids = FxHashSet::default();
    if !flattened_assertions.is_empty() {
        reconciler::reconcile_keyed_types(
            &reconciler::to_and_groups(&flattened_assertions),
            context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            context.inside_loop,
            false,
            true,
            Some(&flattened_active_assertion_offsets),
        );
    }

    let simplified_clauses: Vec<_> = simplified_clauses
        .into_iter()
        .map(std::rc::Rc::new)
        .collect();
    context.clauses = if !changed_var_ids.is_empty() {
        BlockContext::remove_reconciled_clause_refs(
            &simplified_clauses,
            &changed_var_ids,
            analyzer.interner,
        )
        .0
    } else {
        simplified_clauses
    };
}

pub(crate) fn resolve_assertion_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    analyzer.interner.find(var_name).or_else(|| {
        if let Some(stripped) = var_name.strip_prefix('$') {
            analyzer.interner.find(stripped)
        } else {
            analyzer.interner.find(&format!("${}", var_name))
        }
    })
}

pub(crate) fn find_assertion_param_index(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertion_var_id: pzoom_str::StrId,
) -> Option<usize> {
    let assertion_name = analyzer.interner.lookup(assertion_var_id);

    params.iter().position(|param| {
        if param.name == assertion_var_id {
            return true;
        }

        let param_name = analyzer.interner.lookup(param.name);
        assertion_targets_param(assertion_name.as_ref(), param_name.as_ref())
    })
}

pub(crate) fn assertion_targets_param(assertion_name: &str, param_name: &str) -> bool {
    let normalized_assertion = assertion_name.strip_prefix('$').unwrap_or(assertion_name);
    let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name);

    if normalized_assertion == normalized_param {
        return true;
    }

    normalized_assertion
        .strip_prefix(normalized_param)
        .is_some_and(|suffix| {
            suffix.starts_with("->") || suffix.starts_with("::") || suffix.starts_with('[')
        })
}

pub(crate) fn map_assertion_var_to_argument(
    assertion_name: &str,
    param_name: &str,
    argument_var_name: &str,
) -> Option<String> {
    let normalized_assertion = assertion_name.strip_prefix('$').unwrap_or(assertion_name);
    let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name);

    let suffix = normalized_assertion.strip_prefix(normalized_param)?;

    if suffix.is_empty() {
        return Some(argument_var_name.to_string());
    }

    if suffix.starts_with("->") || suffix.starts_with("::") || suffix.starts_with('[') {
        return Some(format!("{}{}", argument_var_name, suffix));
    }

    None
}

pub(crate) fn apply_functionlike_assertion_to_union(
    existing_type: &TUnion,
    assertion_type: &AssertionType,
) -> TUnion {
    let mut narrowed = match assertion_type {
        AssertionType::IsType(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsEqual(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsLooselyEqual(_) => existing_type.clone(),
        AssertionType::IsNotType(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotEqual(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotLooselyEqual(_) => existing_type.clone(),
        AssertionType::Truthy | AssertionType::NotEmpty => narrow_union_to_truthy(existing_type),
        AssertionType::Falsy => narrow_union_to_falsy(existing_type),
        AssertionType::NotNull => subtract_union(existing_type, &TUnion::new(TAtomic::TNull)),
    };

    if matches!(
        assertion_type,
        AssertionType::Truthy | AssertionType::NotEmpty
    ) {
        narrowed.is_falsable = false;
        narrowed.is_nullable = false;
    } else if matches!(assertion_type, AssertionType::NotNull) {
        narrowed.is_nullable = false;
    }

    narrowed.from_docblock = true;
    narrowed
}

pub(crate) fn get_docblock_class_reference(name: StrId, analyzer: &StatementsAnalyzer<'_>) -> StrId {
    let raw_name = analyzer.interner.lookup(name);
    let trimmed_name = raw_name.trim();
    let class_name = trimmed_name
        .split_once("::")
        .map_or(trimmed_name, |(class_name, _)| class_name.trim());

    if class_name.eq_ignore_ascii_case("self") {
        return StrId::SELF;
    }
    if class_name.eq_ignore_ascii_case("static") {
        return StrId::STATIC;
    }
    if class_name.eq_ignore_ascii_case("parent") {
        return StrId::PARENT;
    }

    analyzer
        .interner
        .intern(class_name.trim_start_matches('\\'))
}

pub(crate) fn looks_like_docblock_class_reference(raw_name: &str) -> bool {
    let trimmed_name = raw_name.trim();
    if trimmed_name.is_empty() {
        return false;
    }

    let class_name = trimmed_name
        .split_once("::")
        .map_or(trimmed_name, |(class_name, _)| class_name.trim());
    if class_name.is_empty() {
        return false;
    }

    !class_name.chars().any(|ch| {
        matches!(
            ch,
            ':' | '?' | '|' | '&' | '(' | ')' | '<' | '>' | ',' | '=' | ' ' | '\t' | '\n' | '\r'
        )
    })
}

pub(crate) fn narrow_union_to_truthy(existing_type: &TUnion) -> TUnion {
    let mut filtered = Vec::new();

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing => {}
            TAtomic::TBool => filtered.push(TAtomic::TTrue),
            TAtomic::TLiteralInt { value } if *value == 0 => {}
            TAtomic::TLiteralFloat { value } if *value == 0.0 => {}
            TAtomic::TLiteralString { value } if value.is_empty() || value == "0" => {}
            _ => filtered.push(atomic.clone()),
        }
    }

    if filtered.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered)
    }
}

pub(crate) fn narrow_union_to_falsy(existing_type: &TUnion) -> TUnion {
    let mut filtered = Vec::new();

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing => filtered.push(atomic.clone()),
            TAtomic::TBool => filtered.push(TAtomic::TFalse),
            TAtomic::TLiteralInt { value } if *value == 0 => filtered.push(atomic.clone()),
            TAtomic::TLiteralFloat { value } if *value == 0.0 => filtered.push(atomic.clone()),
            TAtomic::TLiteralString { value } if value.is_empty() || value == "0" => {
                filtered.push(atomic.clone());
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString => filtered.push(atomic.clone()),
            _ => {}
        }
    }

    if filtered.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered)
    }
}

pub(crate) fn subtract_union(existing_type: &TUnion, type_to_remove: &TUnion) -> TUnion {
    let filtered_types: Vec<_> = existing_type
        .types
        .iter()
        .filter(|atomic| !type_to_remove.types.contains(atomic))
        .cloned()
        .collect();

    if filtered_types.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered_types)
    }
}
