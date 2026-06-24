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

use pzoom_code_info::VarName;
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
use pzoom_code_info::TemplateResult;

pub(crate) fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_result: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
) {
    if func_info.assertions.is_empty() {
        return;
    }

    let mut type_assertions: BTreeMap<VarName, Vec<Vec<Assertion>>> = BTreeMap::new();

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
        let resolved_assertion_type =
            replace_assertion_type_templates(&assertion.assertion_type, template_result);

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

        let var_id = VarName::new(&var_key);

        // Mirror Psalm's CallAnalyzer::applyAssertionsToContext. Each atomic
        // of the assertion's docblock union is a rule (Psalm stores a rule
        // array); template replacement happens per rule. A rule that stays a
        // single atomic becomes a reconciler rule; a rule whose template
        // expanded to a union gets intersect/containment special-casing (with
        // TypeDoesNotContainType on impossibility); template-expanded
        // negations are ignored.
        let mut orred_rules: Vec<Assertion> = Vec::new();
        match assertion_type_union(&assertion.assertion_type) {
            // Truthy/Falsy/NotNull-style assertions carry no atomic type.
            None => {
                orred_rules =
                    assertion_finder::convert_functionlike_assertion_type(&resolved_assertion_type);
            }
            Some(original_union) => {
                for original_atomic in &original_union.types {
                    let replaced = function_call_analyzer::replace_templates_in_union(
                        &TUnion::new(original_atomic.clone()),
                        template_result,
                    );

                    if replaced.types.len() == 1 {
                        // An unresolved template (replacement produced the
                        // param's own `as` bound) asserts nothing.
                        if let TAtomic::TTemplateParam { as_type, .. } = original_atomic
                            && as_type.get_id(Some(analyzer.interner))
                                == replaced.get_id(Some(analyzer.interner))
                        {
                            continue;
                        }
                        let atomic = replaced.types.into_iter().next().unwrap();

                        // A docblock `@psalm-assert A|B $v` whose union has an
                        // atomic disjoint from the argument's type (e.g. `int` in
                        // `string|int` asserted on a `?string`) is a partial
                        // contradiction — Psalm reports TypeDoesNotContainType for
                        // that member, even though the union as a whole still
                        // narrows (to `string`). The single-atomic case is left to
                        // the reconciler (it empties the type and reports there).
                        if original_union.types.len() > 1
                            && matches!(assertion.assertion_type, AssertionType::IsType(_))
                            && let Some(existing_type) = context.locals.get(&var_id)
                        {
                            let atomic_union = TUnion::new(atomic.clone());
                            if !crate::type_comparator::union_type_comparator::can_expression_types_be_identical(
                                analyzer.codebase,
                                &atomic_union,
                                existing_type,
                            ) {
                                emit_assertion_type_does_not_contain_type(
                                    analyzer,
                                    analysis_data,
                                    existing_type,
                                    &atomic_union,
                                    argument.span().start.offset,
                                    argument.span().end.offset,
                                );
                            }
                        }

                        orred_rules.push(make_assertion_rule(&assertion.assertion_type, atomic));
                        continue;
                    }

                    // Template-expanded union rule.
                    let Some(existing_type) = context.locals.get(&var_id).map(|__t| (**__t).clone()) else {
                        continue;
                    };
                    match &assertion.assertion_type {
                        AssertionType::IsEqual(_) => {
                            match assertion_reconciler::intersect_union_with_union(
                                &replaced,
                                &existing_type,
                            ) {
                                None => {
                                    emit_assertion_type_does_not_contain_type(
                                        analyzer,
                                        analysis_data,
                                        &existing_type,
                                        &replaced,
                                        argument.span().start.offset,
                                        argument.span().end.offset,
                                    );
                                    orred_rules.push(Assertion::IsEqual(TAtomic::TNever));
                                }
                                Some(intersection)
                                    if intersection.get_id(Some(analyzer.interner))
                                        == existing_type.get_id(Some(analyzer.interner)) => {}
                                Some(intersection) => {
                                    orred_rules.extend(
                                        intersection.types.iter().cloned().map(Assertion::IsEqual),
                                    );
                                }
                            }
                        }
                        AssertionType::IsType(_) => {
                            if !crate::type_comparator::union_type_comparator::can_expression_types_be_identical(
                                analyzer.codebase,
                                &replaced,
                                &existing_type,
                            ) {
                                emit_assertion_type_does_not_contain_type(
                                    analyzer,
                                    analysis_data,
                                    &existing_type,
                                    &replaced,
                                    argument.span().start.offset,
                                    argument.span().end.offset,
                                );
                            }
                        }
                        // Ignore negations and loose assertions expanded to unions.
                        _ => {}
                    }
                }
            }
        }

        if !orred_rules.is_empty() {
            type_assertions.entry(var_id).or_default().push(orred_rules);
        }
    }

    // Reconcile all collected assertions at once with everything active, so
    // the reconciler reports RedundantCondition / TypeDoesNotContainType
    // exactly as Psalm's Reconciler::reconcileKeyedTypes does here.
    if !type_assertions.is_empty() {
        // Asserting an INTERFACE stays silent in Psalm's docblock-assert
        // application (`@psalm-assert Throwable $p` on null reports
        // nothing); scalar asserts report as usual.
        let group_is_interface_assert = |groups: &Vec<Vec<Assertion>>, offset: usize| {
            groups.get(offset).is_some_and(|group| {
                group.iter().all(|assertion| {
                    matches!(
                        assertion,
                        Assertion::IsType(TAtomic::TNamedObject { name, .. })
                            if analyzer.codebase.get_class(*name).is_some_and(|class_info| {
                                class_info.kind
                                    == pzoom_code_info::class_like_info::ClassLikeKind::Interface
                            })
                    )
                })
            })
        };
        let active_offsets: BTreeMap<VarName, FxHashSet<usize>> = type_assertions
            .iter()
            .map(|(var_id, groups)| {
                (
                    var_id.clone(),
                    (0..groups.len())
                        .filter(|offset| !group_is_interface_assert(groups, *offset))
                        .collect(),
                )
            })
            .collect();
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = context.inside_loop;
        // Psalm's applyAssertionsToContext retracts a MixedAssignment
        // reported at a variable's first assignment when an assertion
        // narrows it from a mixed-bearing type.
        let pre_mixed_vars: FxHashSet<VarName> = type_assertions
            .keys()
            .filter(|var_id| {
                context
                    .locals
                    .get(*var_id)
                    .is_some_and(|var_type| var_type.is_mixed())
            })
            .cloned()
            .collect();
        reconciler::reconcile_keyed_types(
            &type_assertions,
            context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::All,
            Some(&active_offsets),
        );

        for var_id in &changed_var_ids {
            if pre_mixed_vars.contains(var_id)
                && context.locals.contains_key(var_id)
                && let Some(first_appearance) =
                    analysis_data.first_var_appearances.get(var_id).copied()
            {
                analysis_data.remove_issue(IssueKind::MixedAssignment, first_appearance);
            }
        }

        // Docblock-assertion narrowings count as docblock-sourced types, so
        // later redundancies report the *GivenDocblockType kinds (and operand
        // checks stay quiet) exactly as before.
        for var_id in type_assertions.keys() {
            if let Some(narrowed) = context.locals.get_mut_owned(var_id) {
                narrowed.from_docblock = true;
            }
        }
    }
}

/// Map an assertion kind plus a (template-replaced) atomic to a reconciler rule.
fn make_assertion_rule(assertion_type: &AssertionType, atomic: TAtomic) -> Assertion {
    match assertion_type {
        AssertionType::IsType(_) => Assertion::IsType(atomic),
        AssertionType::IsEqual(_) => Assertion::IsEqual(atomic),
        AssertionType::IsLooselyEqual(_) => Assertion::IsLooselyEqual(atomic),
        AssertionType::IsNotType(_) => Assertion::IsNotType(atomic),
        AssertionType::IsNotEqual(_) => Assertion::IsNotEqual(atomic),
        AssertionType::IsNotLooselyEqual(_) => Assertion::IsNotLooselyEqual(atomic),
        AssertionType::Truthy | AssertionType::NotEmpty => Assertion::Truthy,
        AssertionType::Falsy => Assertion::Falsy,
        AssertionType::NotNull => Assertion::IsNotType(TAtomic::TNull),
    }
}

/// The union carried by an assertion type, if any.
fn assertion_type_union(assertion_type: &AssertionType) -> Option<&TUnion> {
    match assertion_type {
        AssertionType::IsType(union)
        | AssertionType::IsEqual(union)
        | AssertionType::IsLooselyEqual(union)
        | AssertionType::IsNotType(union)
        | AssertionType::IsNotEqual(union)
        | AssertionType::IsNotLooselyEqual(union) => Some(union),
        _ => None,
    }
}

fn emit_assertion_type_does_not_contain_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    existing_type: &TUnion,
    asserted_type: &TUnion,
    start_offset: u32,
    end_offset: u32,
) {
    let (line, col) = analyzer.get_line_column(start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::TypeDoesNotContainType,
        format!(
            "{} is not contained by {}",
            existing_type.get_id(Some(analyzer.interner)),
            asserted_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        start_offset,
        end_offset,
        line,
        col,
    ));
}

fn replace_assertion_type_templates(
    assertion_type: &AssertionType,
    template_result: &TemplateResult,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => AssertionType::IsType(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::IsEqual(asserted_type) => AssertionType::IsEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::IsLooselyEqual(asserted_type) => AssertionType::IsLooselyEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::IsNotType(asserted_type) => AssertionType::IsNotType(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::IsNotEqual(asserted_type) => AssertionType::IsNotEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::IsNotLooselyEqual(asserted_type) => AssertionType::IsNotLooselyEqual(
            function_call_analyzer::replace_templates_in_union(asserted_type, template_result),
        ),
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

fn apply_assertion_to_argument_expression(
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
        assertion_map,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        crate::reconciler::EmissionMode::Silent,
        None,
    );
}

fn is_boolean_true_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TTrue))
}

fn is_boolean_false_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TFalse))
}

fn emit_undefined_docblock_class_issues_from_assertion_type(
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

    // Port of Psalm's `processAssertFunctionEffects`: build the assert
    // formula, check it against the context clauses (paradox / "has already
    // been asserted"), then reconcile the truths of the COMBINED formula with
    // every truth active, stamping changed vars as docblock-sourced.
    let assert_conditional_id = (
        first_arg.value().start_offset() as u32,
        first_arg.value().end_offset() as u32,
    );

    let assert_clauses = crate::formula_generator::get_formula(
        assert_conditional_id,
        assert_conditional_id,
        first_arg.value(),
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();

    crate::algebra_analyzer::check_for_paradox(
        analyzer,
        &context.clauses,
        &assert_clauses,
        analysis_data,
        assert_conditional_id,
    );

    let mut combined_clauses: Vec<_> = context
        .clauses
        .iter()
        .map(|clause| clause.as_ref())
        .collect();
    combined_clauses.extend(assert_clauses.iter());
    let simplified_clauses = simplify_cnf(combined_clauses);

    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, _active_truths) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        None,
        &mut cond_referenced_var_ids,
    );

    let mut changed_var_ids = FxHashSet::default();
    if !truths.is_empty() {
        // Psalm reconciles every truth as active with the assert's location.
        // Re-flagging at subsequent asserts is prevented one level down:
        // redundant reconciles count as changed ($failed_reconciliation), so
        // their clauses are removed from the context below.
        let active_offsets: BTreeMap<VarName, FxHashSet<usize>> = truths
            .iter()
            .map(|(var_id, groups)| (var_id.clone(), (0..groups.len()).collect()))
            .collect();
        // Psalm's processAssertFunctionEffects retracts a MixedAssignment
        // reported at a variable's first assignment when the assert narrows
        // it from a mixed-bearing type (IssueBuffer::remove keyed on the
        // first appearance).
        let pre_mixed_vars: FxHashSet<VarName> = truths
            .keys()
            .filter(|var_id| {
                context
                    .locals
                    .get(*var_id)
                    .is_some_and(|var_type| var_type.is_mixed())
            })
            .cloned()
            .collect();
        reconciler::reconcile_keyed_types(
            &truths,
            context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            context.inside_loop,
            false,
            crate::reconciler::EmissionMode::All,
            Some(&active_offsets),
        );

        // Psalm stamps every changed var as docblock-sourced after an assert.
        for var_id in &changed_var_ids {
            if let Some(narrowed) = context.locals.get_mut_owned(var_id) {
                narrowed.from_docblock = true;
                narrowed.sync_docblock_bits_from_union_flag();
            }
            if pre_mixed_vars.contains(var_id)
                && context.locals.contains_key(var_id)
                && let Some(first_appearance) =
                    analysis_data.first_var_appearances.get(var_id).copied()
            {
                analysis_data.remove_issue(IssueKind::MixedAssignment, first_appearance);
            }
        }
    }

    let simplified_clauses: Vec<_> = simplified_clauses
        .into_iter()
        .map(std::rc::Rc::new)
        .collect();
    context.clauses = if !changed_var_ids.is_empty() {
        BlockContext::remove_reconciled_clause_refs(&simplified_clauses, &changed_var_ids).0
    } else {
        simplified_clauses
    };
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

fn get_docblock_class_reference(name: StrId, analyzer: &StatementsAnalyzer<'_>) -> StrId {
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
        .find(class_name.trim_start_matches('\\'))
        .unwrap_or(pzoom_str::StrId::EMPTY)
}

fn looks_like_docblock_class_reference(raw_name: &str) -> bool {
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
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNever => {}
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
