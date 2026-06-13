//! Assertion finder module.
//!
//! This module extracts type assertions from conditional expressions.
//! For example, `$x instanceof Foo` generates an `IsType(TNamedObject(Foo))` assertion.
//!
//! It builds CNF (Conjunctive Normal Form) clauses that can be used for
//! type algebra simplification.

use std::collections::BTreeMap;

use pzoom_code_info::AssertionSet;
use pzoom_code_info::VarName;
use mago_span::HasSpan;
use mago_syntax::ast::ast::access::{Access, ClassConstantAccess};
use mago_syntax::ast::ast::binary::{Binary, BinaryOperator};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::call::{NullSafeMethodCall, StaticMethodCall};
use mago_syntax::ast::ast::class_like::member::{
    ClassLikeConstantSelector, ClassLikeMemberSelector,
};
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::algebra::{Clause, ClauseKey, combine_ored_clauses};
use pzoom_code_info::functionlike_info::{Assertion as FunctionLikeAssertion, AssertionType};
use pzoom_code_info::{ArrayKey, Assertion, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use pzoom_code_info::TemplateResult;
use crate::type_expander::localize_special_class_type_union;

/// Result of assertion extraction.
pub struct AssertionResult {
    /// Clauses that are true when the expression is true (CNF formula).
    pub if_true_clauses: Vec<Clause>,
    /// Clauses that are true when the expression is false (CNF formula).
    pub if_false_clauses: Vec<Clause>,
    /// Assertions that are true when the expression is true. Mirrors Psalm's
    /// AssertionFinder `$if_types` shape (`array<string, list<list<Assertion>>>`):
    /// the outer list is AND-ed groups, each inner list is OR-ed alternatives.
    pub if_true: BTreeMap<VarName, Vec<Vec<Assertion>>>,
    /// Assertions that are true when the expression is false (same
    /// `[var][group][alternative]` shape as `if_true`).
    pub if_false: BTreeMap<VarName, Vec<Vec<Assertion>>>,
    /// Vars narrowed by an UNCONDITIONAL `@psalm-assert` of a call inside the
    /// condition. Psalm applies those to the condition context before the
    /// formula reconciles, so contradictions there never report; pzoom omits
    /// the vars from entry-reconcile reporting instead.
    pub silently_asserted_vars: FxHashSet<VarName>,
}

impl AssertionResult {
    pub fn new() -> Self {
        Self {
            if_true_clauses: Vec::new(),
            if_false_clauses: Vec::new(),
            if_true: BTreeMap::new(),
            if_false: BTreeMap::new(),
            silently_asserted_vars: FxHashSet::default(),
        }
    }
}

/// Extracts type assertions from a conditional expression.
pub fn get_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> AssertionResult {
    let cond_id = get_expr_id(expr);
    let mut result = scrape_assertions(analyzer, expr, cond_id, analysis_data);
    strip_assignment_key_prefix(&mut result.if_true);
    strip_assignment_key_prefix(&mut result.if_false);
    result
}

/// Strip the `=` assignment marker from assertion-map keys (the clause layer
/// keeps the information as Clause::redefined_vars; map consumers reconcile by
/// plain variable name).
fn strip_assignment_key_prefix(map: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>) {
    let prefixed: Vec<VarName> = map
        .keys()
        .filter(|key| key.starts_with('='))
        .cloned()
        .collect();
    for key in prefixed {
        if let Some(assertions) = map.remove(&key) {
            map.entry(VarName::new(key.trim_start_matches('=')))
                .or_default()
                .extend(assertions);
        }
    }
}

fn get_expr_id(expr: &Expression<'_>) -> (u32, u32) {
    (expr.start_offset() as u32, expr.end_offset() as u32)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OtherValuePosition {
    Left,
    Right,
}

fn scrape_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match expr {
        Expression::Call(call) => {
            scrape_function_call_assertions(analyzer, call, &mut result, cond_id, analysis_data);
            add_nullsafe_object_assertions(call, &mut result, cond_id);

            // Psalm's catch-all `$if_types[getExtendedVarId($cond)] = Truthy`:
            // a *memoized* no-arg method call used as a condition asserts on
            // its memoization key (`$e->getPrevious()` re-calls then reuse
            // the narrowed entry). getExtendedVarId only keys method calls
            // flagged `memoizable` by MethodCallPurityAnalyzer.
            // The narrowing applies through the assertion maps only — pzoom's
            // clause algebra flags contradictions on these keys in spots
            // Psalm stays silent (TypeCombiner's nested array_type_params
            // checks), so the keys stay out of the CNF formula.
            if result.if_true.is_empty()
                && result.if_false.is_empty()
                && matches!(call, Call::Method(_) | Call::NullSafeMethod(_))
                && analysis_data
                    .memoizable_method_call_offsets
                    .contains(&(expr.span().start.offset))
                && let Some(var_name) = expression_identifier::get_expression_var_key(expr)
            {
                // The clauses make the narrowing reach the branch contexts'
                // entry reconcile (assertion maps alone are only consulted by
                // the contradiction reporter).
                let truthy_clause =
                    create_single_var_clause(&var_name, Assertion::Truthy, cond_id);
                result.if_true_clauses.push(truthy_clause);
                let falsy_clause = create_single_var_clause(&var_name, Assertion::Falsy, cond_id);
                result.if_false_clauses.push(falsy_clause);
                result
                    .if_true
                    .entry(var_name.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                result
                    .if_false
                    .entry(var_name)
                    .or_default()
                    .push(vec![Assertion::Falsy]);
            }
        }
        Expression::Binary(binary) => {
            return scrape_binary_assertions(analyzer, binary, cond_id, analysis_data);
        }
        Expression::UnaryPrefix(unary) => {
            return scrape_unary_assertions(analyzer, unary, cond_id, analysis_data);
        }
        Expression::Variable(var) => {
            if let Some(var_name) = get_var_name(var) {
                // Create a clause for the true branch: $x is truthy
                let truthy_clause = create_single_var_clause(&var_name, Assertion::Truthy, cond_id);
                result.if_true_clauses.push(truthy_clause);

                // Create a clause for the false branch: $x is falsy
                let falsy_clause = create_single_var_clause(&var_name, Assertion::Falsy, cond_id);
                result.if_false_clauses.push(falsy_clause);

                // Also populate the grouped maps (one singleton AND group each)
                result
                    .if_true
                    .entry(var_name.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                result
                    .if_false
                    .entry(var_name)
                    .or_default()
                    .push(vec![Assertion::Falsy]);
            }
        }
        Expression::Access(Access::Property(_))
        | Expression::Access(Access::NullSafeProperty(_))
        | Expression::Access(Access::StaticProperty(_))
        // Psalm's catch-all `$if_types[getExtendedVarId($cond)] = Truthy`
        // covers array fetches too: `if ($arr['k'])` narrows the
        // `$arr['k']` entry for re-fetches in the body.
        | Expression::ArrayAccess(_) => {
            if let Some(var_name) = expression_identifier::get_expression_var_key(expr) {
                let truthy_clause = create_single_var_clause(&var_name, Assertion::Truthy, cond_id);
                result.if_true_clauses.push(truthy_clause);

                let falsy_clause = create_single_var_clause(&var_name, Assertion::Falsy, cond_id);
                result.if_false_clauses.push(falsy_clause);

                result
                    .if_true
                    .entry(var_name.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                result
                    .if_false
                    .entry(var_name)
                    .or_default()
                    .push(vec![Assertion::Falsy]);
            }
        }
        Expression::Assignment(assignment) => {
            let mut rhs_result =
                scrape_assertions(analyzer, assignment.rhs, cond_id, analysis_data);

            if let Some(assigned_var_name) = get_assignment_target_var_name(assignment.lhs) {
                rhs_result.if_true_clauses.push(create_single_var_clause(
                    &assigned_var_name,
                    Assertion::Truthy,
                    cond_id,
                ));
                rhs_result.if_false_clauses.push(create_single_var_clause(
                    &assigned_var_name,
                    Assertion::Falsy,
                    cond_id,
                ));
                rhs_result
                    .if_true
                    .entry(assigned_var_name.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                rhs_result
                    .if_false
                    .entry(assigned_var_name)
                    .or_default()
                    .push(vec![Assertion::Falsy]);
            }

            return rhs_result;
        }
        Expression::Parenthesized(paren) => {
            return scrape_assertions(analyzer, paren.expression, cond_id, analysis_data);
        }
        Expression::Construct(construct) => {
            return scrape_construct_assertions(construct, cond_id, analysis_data);
        }
        _ => {}
    }

    result
}

fn add_nullsafe_object_assertions(
    call: &Call<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
) {
    let Call::NullSafeMethod(method_call) = call else {
        return;
    };

    let Some(object_var_name) = expression_identifier::get_expression_var_key(method_call.object)
    else {
        return;
    };

    let assertion = Assertion::IsNotType(TAtomic::TNull);
    result.if_true_clauses.push(create_single_var_clause(
        &object_var_name,
        assertion.clone(),
        cond_id,
    ));
    result
        .if_true
        .entry(object_var_name)
        .or_default()
        .push(vec![assertion]);
}

fn scrape_construct_assertions(
    construct: &Construct<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match construct {
        Construct::Isset(isset) => {
            if isset.values.is_empty() {
                return result;
            }

            let mut combined_false_clauses: Option<Vec<Clause>> = None;
            for value in isset.values.iter() {
                let expr_assertions = get_isset_assertions_for_expr(value, cond_id, analysis_data);

                result
                    .if_true_clauses
                    .extend(expr_assertions.if_true_clauses);
                merge_assertion_maps(&mut result.if_true, expr_assertions.if_true);

                combined_false_clauses = Some(match combined_false_clauses.take() {
                    None => expr_assertions.if_false_clauses,
                    Some(existing) => {
                        combine_ored_clauses(existing, expr_assertions.if_false_clauses, cond_id)
                            .unwrap_or_default()
                    }
                });
            }

            if let Some(false_clauses) = combined_false_clauses {
                result.if_false_clauses = false_clauses;
            }
        }
        Construct::Empty(empty) => {
            if let Some(var_name) = get_assertable_var_name(empty.value) {
                // Psalm: a settled plain variable gets Falsy (negating to
                // Truthy); anything else gets Empty_/NonEmpty — NonEmpty
                // additionally drives nested base-isset narrowing for
                // array-path keys (addNestedAssertions).
                let is_settled_plain_var = matches!(
                    empty.value.unparenthesized(),
                    Expression::Variable(Variable::Direct(_))
                );
                let (true_assertion, negated_assertion) = if is_settled_plain_var {
                    (Assertion::Falsy, Assertion::Truthy)
                } else {
                    (Assertion::Empty, Assertion::NonEmpty)
                };
                let true_clause =
                    create_single_var_clause(&var_name, true_assertion.clone(), cond_id);
                let false_clause =
                    create_single_var_clause(&var_name, negated_assertion.clone(), cond_id);
                result.if_true_clauses.push(true_clause);
                result.if_false_clauses.push(false_clause);
                result
                    .if_true
                    .entry(var_name.clone())
                    .or_default()
                    .push(vec![true_assertion]);
                result
                    .if_false
                    .entry(var_name)
                    .or_default()
                    .push(vec![negated_assertion]);
            }
        }
        _ => {}
    }

    result
}

/// Creates a clause with a single variable and assertion.
fn create_single_var_clause(var_name: &str, assertion: Assertion, cond_id: (u32, u32)) -> Clause {
    // `=`-prefixed keys mark assignment-derived assertions: strip the prefix
    // and record the var in the clause's redefined_vars (Psalm strips it in
    // its FormulaGenerator).
    let (var_name, redefined) = match var_name.strip_prefix('=') {
        Some(stripped) => (stripped, true),
        None => (var_name, false),
    };

    // Psalm's FormulaGenerator marks clauses built from equality assertions
    // as `generated`, which exempts them from "has already been asserted"
    // redundancy checks (two dynamic-key issets on one base var legitimately
    // produce identical `=isset` clauses).
    let has_equality = assertion.has_equality();

    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = AssertionSet::default();
    var_possibilities.insert(assertion.to_hash(), assertion);
    possibilities.insert(ClauseKey::Name(VarName::new(var_name)), var_possibilities);

    let clause = Clause::new(possibilities, cond_id, cond_id, None, None, Some(has_equality));
    if redefined {
        clause.mark_redefined(VarName::new(var_name))
    } else {
        clause
    }
}

/// Extracts assertions from a function call (e.g., is_string($x), isset($x)).
fn scrape_function_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) {
    // Handle different call types
    match call {
        Call::Function(func_call) => {
            // Get the function name
            let raw_func_name = match &func_call.function {
                Expression::Identifier(id) => Some(id.value()),
                _ => None,
            };

            let Some(raw_func_name) = raw_func_name else {
                return;
            };
            let normalized_func_name = raw_func_name
                .strip_prefix('\\')
                .unwrap_or(raw_func_name)
                .to_ascii_lowercase();

            // A bare `count($x)` / `sizeof($x)` condition: truthy means the
            // countable is non-empty, falsy empty (Psalm's AssertionFinder
            // count handling without a comparison).
            if (normalized_func_name == "count" || normalized_func_name == "sizeof")
                && let Some(first_arg) = func_call.argument_list.arguments.first()
                && let Some(var_name) =
                    crate::expression_identifier::get_expression_var_key(first_arg.value())
            {
                add_empty_countable_assertions(result, &var_name, false, cond_id);
                return;
            }

            // Handle isset() specially
            if normalized_func_name == "isset" {
                if func_call.argument_list.arguments.is_empty() {
                    return;
                }

                let mut combined_false_clauses: Option<Vec<Clause>> = None;
                for arg in func_call.argument_list.arguments.iter() {
                    let expr_assertions = get_isset_assertions_for_expr(arg.value(), cond_id, analysis_data);
                    result
                        .if_true_clauses
                        .extend(expr_assertions.if_true_clauses);
                    merge_assertion_maps(&mut result.if_true, expr_assertions.if_true);
                    combined_false_clauses = Some(match combined_false_clauses.take() {
                        None => expr_assertions.if_false_clauses,
                        Some(existing) => combine_ored_clauses(
                            existing,
                            expr_assertions.if_false_clauses,
                            cond_id,
                        )
                        .unwrap_or_default(),
                    });
                }

                if let Some(false_clauses) = combined_false_clauses {
                    result.if_false_clauses = false_clauses;
                }
                return;
            }

            if normalized_func_name == "array_key_exists" || normalized_func_name == "key_exists" {
                let Some(key_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(array_arg) = func_call.argument_list.arguments.get(1) else {
                    return;
                };
                let array_var_name = get_argument_var_name(array_arg);
                let key_var_name = get_argument_var_name(key_arg);

                let haystack_type = analysis_data
                    .expr_types.get(&get_expr_id(array_arg.value())).cloned()
                    .map(|union| (*union).clone());
                let array_keys =
                    extract_array_keys_from_expr(analyzer, key_arg.value(), analysis_data);

                // Psalm's getArrayKeyExistsAssertions never keys the assertion
                // path by a class constant's symbolic name: for a
                // ClassConstFetch key it resolves the constant's literal value
                // (`$a['key']`, handled below via `array_keys`) and otherwise
                // drops the var name entirely (AssertionFinder.php).
                let key_is_class_constant = matches!(
                    key_arg.value().unparenthesized(),
                    Expression::Access(Access::ClassConstant(_))
                );

                let mut added_key_presence_assertion = false;
                if let Some(array_var_name) = array_var_name.as_ref() {
                    if let Some(key_var_name) = key_var_name.as_ref() {
                        if is_simple_array_key_identifier(key_var_name) && !key_is_class_constant {
                            add_array_key_exists_path_assertions(
                                result,
                                array_var_name,
                                key_var_name,
                                cond_id,
                            );
                            added_key_presence_assertion = true;
                        }
                    }

                    if !array_keys.is_empty() {
                        for array_key in array_keys {
                            let key_id = format_array_key_for_assertion_path(&array_key);
                            add_array_key_exists_path_assertions(
                                result,
                                array_var_name,
                                &key_id,
                                cond_id,
                            );
                            added_key_presence_assertion = true;
                        }
                    }
                }

                let mut added_key_constraint = false;
                if let (Some(key_var_name), Some(haystack_type)) =
                    (key_var_name, haystack_type.as_ref())
                {
                    if let Some(key_union) = extract_array_key_union_for_key_exists(haystack_type) {
                        if !key_union.is_nothing() {
                            let key_haystack = TUnion::new(TAtomic::TArray {
                                key_type: Box::new(TUnion::array_key()),
                                value_type: Box::new(key_union),
                            });
                            add_in_array_assertions(result, &key_var_name, key_haystack, cond_id);
                            added_key_constraint = true;
                        }
                    }
                }

                if added_key_presence_assertion || added_key_constraint {
                    return;
                }
            }

            if normalized_func_name == "in_array" {
                let Some(needle_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(haystack_arg) = func_call.argument_list.arguments.get(1) else {
                    return;
                };
                let Some(needle_var_name) = get_argument_var_name(needle_arg) else {
                    return;
                };

                let haystack_pos = get_expr_id(haystack_arg.value());
                let Some(haystack_type) = analysis_data
                    .expr_types.get(&haystack_pos).cloned()
                    .map(|union| (*union).clone())
                else {
                    return;
                };

                add_in_array_assertions(result, &needle_var_name, haystack_type, cond_id);
                return;
            }

            if normalized_func_name == "function_exists" {
                let Some(first_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(function_name) = extract_literal_function_name(first_arg.value()) else {
                    // Psalm: `function_exists($var)` asserts the variable is a
                    // callable-string when true (AssertionFinder's
                    // hasFunctionExistsCheck → IsType(TCallableString)).
                    if let Some(var_name) =
                        crate::expression_identifier::get_expression_var_key(first_arg.value())
                    {
                        let assertion = Assertion::IsType(TAtomic::TCallableString);
                        let true_clause =
                            create_single_var_clause(&var_name, assertion.clone(), cond_id);
                        result.if_true_clauses.push(true_clause);
                        result
                            .if_true
                            .entry(var_name)
                            .or_default()
                            .push(vec![assertion]);
                    }
                    return;
                };

                let exists_key = function_exists_assertion_key(&function_name);
                let true_clause = create_single_var_clause(&exists_key, Assertion::Truthy, cond_id);
                let false_clause = create_single_var_clause(&exists_key, Assertion::Falsy, cond_id);
                result.if_true_clauses.push(true_clause);
                result.if_false_clauses.push(false_clause);
                result
                    .if_true
                    .entry(exists_key.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                result
                    .if_false
                    .entry(exists_key)
                    .or_default()
                    .push(vec![Assertion::Falsy]);
                return;
            }

            if normalized_func_name == "method_exists" {
                let Some(target_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(method_arg) = func_call.argument_list.arguments.get(1) else {
                    return;
                };
                let Some(method_name) = extract_literal_string_name(analyzer, method_arg.value())
                else {
                    return;
                };

                let target_key = if let Some(class_id) =
                    extract_class_constant_id(analyzer, target_arg.value())
                {
                    analyzer
                        .interner
                        .lookup(class_id)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase()
                } else if let Some(target_var_name) = get_argument_var_name(target_arg) {
                    target_var_name.to_ascii_lowercase()
                } else {
                    return;
                };

                let exists_key = method_exists_assertion_key(&target_key, &method_name);
                let true_clause = create_single_var_clause(&exists_key, Assertion::Truthy, cond_id);
                let false_clause = create_single_var_clause(&exists_key, Assertion::Falsy, cond_id);
                result.if_true_clauses.push(true_clause);
                result.if_false_clauses.push(false_clause);
                result
                    .if_true
                    .entry(exists_key.clone())
                    .or_default()
                    .push(vec![Assertion::Truthy]);
                result
                    .if_false
                    .entry(exists_key)
                    .or_default()
                    .push(vec![Assertion::Falsy]);

                // Psalm narrows the object itself to an object-with-methods;
                // pzoom models the `__toString` case as stringable-object so
                // a subsequent (string) cast sees it.
                if method_name.eq_ignore_ascii_case("__toString")
                    && let Some(target_var_name) = get_argument_var_name(target_arg)
                {
                    let stringable = TAtomic::TObjectWithProperties {
                        properties: Default::default(),
                        is_stringable: true,
                        is_invokable: false,
                    };
                    let stringable_clause = create_single_var_clause(
                        &target_var_name,
                        Assertion::IsType(stringable.clone()),
                        cond_id,
                    );
                    result.if_true_clauses.push(stringable_clause);
                    result
                        .if_true
                        .entry(target_var_name)
                        .or_default()
                        .push(vec![Assertion::IsType(stringable)]);
                }
                return;
            }

            if matches!(
                normalized_func_name.as_str(),
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
            ) {
                let Some(first_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                if let Some(var_name) = get_argument_var_name(first_arg) {
                    // Only narrow the positive branch to class-string. A failing
                    // class_exists/interface_exists on a general string leaves it a
                    // `string` (it may still name an interface or an as-yet-unloaded
                    // class), so we must not assert `not class-string` in the false branch
                    // - doing so makes a following interface_exists() look paradoxical.
                    // Matches Psalm.
                    //
                    // Psalm's assertion atoms differ per function (TClassString
                    // carries is_interface/is_enum flavors), so a chain like
                    // `!class_exists($x) && !interface_exists($x)` never forms
                    // duplicate clauses. pzoom's TClassString has no flavors;
                    // mark these clauses generated so the "has already been
                    // asserted" duplicate check skips them regardless of the
                    // order the existence checks appear in.
                    add_positive_only_type_assertion(
                        result,
                        var_name,
                        TAtomic::TClassString { as_type: None },
                        cond_id,
                        true,
                    );
                    return;
                }

                if let Some(class_name) = extract_literal_string_name(analyzer, first_arg.value())
                {
                    let exists_key = class_exists_assertion_key(class_name.as_ref());
                    let true_clause = create_single_var_clause(&exists_key, Assertion::Truthy, cond_id);
                    let false_clause = create_single_var_clause(&exists_key, Assertion::Falsy, cond_id);
                    result.if_true_clauses.push(true_clause);
                    result.if_false_clauses.push(false_clause);
                    result
                        .if_true
                        .entry(exists_key.clone())
                        .or_default()
                        .push(vec![Assertion::Truthy]);
                    result
                        .if_false
                        .entry(exists_key)
                        .or_default()
                        .push(vec![Assertion::Falsy]);
                    return;
                }

                if let Some(class_id) = extract_class_constant_id(analyzer, first_arg.value()) {
                    let class_name = analyzer.interner.lookup(class_id);
                    let exists_key = class_exists_assertion_key(class_name.as_ref());
                    let true_clause =
                        create_single_var_clause(&exists_key, Assertion::Truthy, cond_id);
                    let false_clause =
                        create_single_var_clause(&exists_key, Assertion::Falsy, cond_id);
                    result.if_true_clauses.push(true_clause);
                    result.if_false_clauses.push(false_clause);
                    result
                        .if_true
                        .entry(exists_key.clone())
                        .or_default()
                        .push(vec![Assertion::Truthy]);
                    result
                        .if_false
                        .entry(exists_key)
                        .or_default()
                        .push(vec![Assertion::Falsy]);
                }
                return;
            }

            if normalized_func_name == "is_a" {
                let Some(subject_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(class_arg) = func_call.argument_list.arguments.get(1) else {
                    return;
                };
                let Some(var_name) = get_argument_var_name(subject_arg) else {
                    return;
                };
                let subject_type = analysis_data.expr_types.get(&get_expr_id(subject_arg.value())).cloned();
                let subject_prefers_class_string =
                    subject_type.is_some_and(|ty| union_is_definitely_string_like(&ty));

                if let Some(class_id) = resolve_class_string_arg(analyzer, class_arg.value()) {
                    let asserted_type = if subject_prefers_class_string {
                        TAtomic::TClassString {
                            as_type: Some(Box::new(TAtomic::TNamedObject {
                                name: class_id,
                                type_params: None,
                            is_static: false, remapped_params: false })),
                        }
                    } else {
                        TAtomic::TNamedObject {
                            name: class_id,
                            type_params: None,
                        is_static: false, remapped_params: false }
                    };

                    add_type_assertions(result, var_name.clone(), asserted_type, true, cond_id);
                } else if let Some(class_string_union) =
                    analysis_data.expr_types.get(&get_expr_id(class_arg.value())).cloned()
                {
                    if let Some(classlike_atomic) =
                        extract_classlike_from_class_string_union(analyzer, &class_string_union)
                    {
                        let asserted_type = if subject_prefers_class_string {
                            TAtomic::TClassString {
                                as_type: Some(Box::new(classlike_atomic)),
                            }
                        } else {
                            classlike_atomic
                        };

                        add_type_assertions(result, var_name.clone(), asserted_type, true, cond_id);
                    } else {
                        return;
                    }
                } else {
                    return;
                }

                if let Some(subject_var_name) = get_argument_var_name(subject_arg) {
                    if subject_var_name == "@static" {
                        if let Some(class_id) =
                            resolve_class_string_arg(analyzer, class_arg.value())
                        {
                            add_type_assertions(
                                result,
                                VarName::new_static("$this"),
                                TAtomic::TNamedObject {
                                    name: class_id,
                                    type_params: None,
                                is_static: false, remapped_params: false },
                                true,
                                cond_id,
                            );
                        }
                    }
                }
                return;
            }

            if normalized_func_name == "is_subclass_of" {
                let Some(subject_arg) = func_call.argument_list.arguments.first() else {
                    return;
                };
                let Some(class_arg) = func_call.argument_list.arguments.get(1) else {
                    return;
                };
                let Some(var_name) = get_argument_var_name(subject_arg) else {
                    return;
                };

                let subject_type = analysis_data.expr_types.get(&get_expr_id(subject_arg.value())).cloned();
                let subject_prefers_class_string = subject_type.as_ref().is_some_and(|ty| {
                    union_is_definitely_string_like(ty)
                        || ty.types.iter().all(|atomic| {
                            matches!(
                                atomic,
                                TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
                            )
                        })
                });
                // A mixed subject could be either an instance or a class
                // name: Psalm asserts `Foo|class-string<Foo>`.
                let subject_is_ambiguous = subject_type
                    .as_ref()
                    .is_none_or(|ty| ty.is_mixed());

                if subject_is_ambiguous
                    && let Some(class_id) = resolve_class_string_arg(analyzer, class_arg.value())
                {
                    let object_form = TAtomic::TNamedObject {
                        name: class_id,
                        type_params: None,
                        is_static: false,
                        remapped_params: false,
                    };
                    let class_string_form = TAtomic::TClassString {
                        as_type: Some(Box::new(object_form.clone())),
                    };
                    let orred = vec![
                        Assertion::IsType(object_form),
                        Assertion::IsType(class_string_form),
                    ];
                    result
                        .if_true_clauses
                        .push(create_var_clause(&var_name, &orred, cond_id));
                    return;
                }

                let asserted_type = if let Some(class_id) =
                    resolve_class_string_arg(analyzer, class_arg.value())
                {
                    if subject_prefers_class_string {
                        TAtomic::TClassString {
                            as_type: Some(Box::new(TAtomic::TNamedObject {
                                name: class_id,
                                type_params: None,
                            is_static: false, remapped_params: false })),
                        }
                    } else {
                        TAtomic::TNamedObject {
                            name: class_id,
                            type_params: None,
                        is_static: false, remapped_params: false }
                    }
                } else if let Some(class_string_union) =
                    analysis_data.expr_types.get(&get_expr_id(class_arg.value())).cloned()
                {
                    let Some(classlike_atomic) =
                        extract_classlike_from_class_string_union(analyzer, &class_string_union)
                    else {
                        return;
                    };

                    if subject_prefers_class_string {
                        TAtomic::TClassString {
                            as_type: Some(Box::new(classlike_atomic)),
                        }
                    } else {
                        classlike_atomic
                    }
                } else {
                    return;
                };

                add_type_assertions(result, var_name, asserted_type, true, cond_id);
                return;
            }

            // Check if it's a type-checking function
            let (assertion_type, is_narrowing) = match normalized_func_name.as_str() {
                "is_string" => (Some(TAtomic::TString), true),
                "is_int" | "is_integer" | "is_long" => (Some(TAtomic::TInt), true),
                "is_float" | "is_double" | "is_real" => (Some(TAtomic::TFloat), true),
                "is_bool" => (Some(TAtomic::TBool), true),
                "is_array" => {
                    let narrowed_array_type = func_call
                        .argument_list
                        .arguments
                        .first()
                        .and_then(|arg| analysis_data.expr_types.get(&get_expr_id(arg.value())).cloned())
                        .and_then(|arg_type| get_array_assertion_from_union(&arg_type))
                        .unwrap_or_else(|| TAtomic::TArray {
                            key_type: Box::new(TUnion::array_key()),
                            value_type: Box::new(TUnion::mixed()),
                        });
                    (Some(narrowed_array_type), true)
                }
                "is_object" => (Some(TAtomic::TObject), true),
                "is_null" => (Some(TAtomic::TNull), true),
                "is_numeric" => (Some(TAtomic::TNumeric), true),
                "is_callable" => (
                    Some(TAtomic::TCallable {
                        params: None,
                        return_type: None,
                        is_pure: None,
                    }),
                    true,
                ),
                "is_resource" => (Some(TAtomic::TResource), true),
                "is_scalar" => (Some(TAtomic::TScalar), true),
                "is_iterable" => (
                    Some(TAtomic::TIterable {
                        key_type: Box::new(TUnion::mixed()),
                        value_type: Box::new(TUnion::mixed()),
                    }),
                    true,
                ),
                _ => (None, false),
            };

            if !is_narrowing {
                if let Some((function_info, receiver)) =
                    resolve_functionlike_for_call(analyzer, call, analysis_data)
                {
                    apply_callsite_assertions(
                        analyzer,
                        function_info,
                        receiver,
                        call_receiver_var_key(call).as_deref(),
                        func_call.argument_list.arguments.as_slice(),
                        analysis_data,
                        result,
                        cond_id,
                    );
                }
                return;
            }

            let Some(assertion_type) = assertion_type else {
                return;
            };

            // Get the first argument
            let first_arg = func_call.argument_list.arguments.first();
            let Some(var_name) = first_arg.and_then(get_argument_var_name) else {
                return;
            };

            let is_type_clause = create_single_var_clause(
                &var_name,
                Assertion::IsType(assertion_type.clone()),
                cond_id,
            );
            result.if_true_clauses.push(is_type_clause);

            let is_not_type_clause = create_single_var_clause(
                &var_name,
                Assertion::IsNotType(assertion_type.clone()),
                cond_id,
            );
            result.if_false_clauses.push(is_not_type_clause);

            result
                .if_true
                .entry(var_name.clone())
                .or_default()
                .push(vec![Assertion::IsType(assertion_type.clone())]);
            result
                .if_false
                .entry(var_name)
                .or_default()
                .push(vec![Assertion::IsNotType(assertion_type)]);
        }
        _ => {
            if let Some((function_info, receiver)) =
                resolve_functionlike_for_call(analyzer, call, analysis_data)
            {
                let args = match call {
                    Call::Method(method_call) => method_call.argument_list.arguments.as_slice(),
                    Call::NullSafeMethod(method_call) => {
                        method_call.argument_list.arguments.as_slice()
                    }
                    Call::StaticMethod(static_call) => {
                        static_call.argument_list.arguments.as_slice()
                    }
                    _ => return,
                };

                apply_callsite_assertions(
                    analyzer,
                    function_info,
                    receiver,
                    call_receiver_var_key(call).as_deref(),
                    args,
                    analysis_data,
                    result,
                    cond_id,
                );
            }
        }
    }
}

/// Extracts assertions from a binary expression (&&, ||, ===, !==, instanceof).
fn scrape_binary_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    binary: &Binary<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match &binary.operator {
        BinaryOperator::Instanceof(_) => {
            scrape_instanceof_assertions(analyzer, binary, &mut result, cond_id, analysis_data);
        }
        BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
            // For &&, both sides must be true in the true branch
            // CNF: left_true AND right_true
            let mut left_result = scrape_assertions(analyzer, binary.lhs, cond_id, analysis_data);
            let right_result = scrape_assertions(analyzer, binary.rhs, cond_id, analysis_data);
            let mut assigned_in_right = FxHashSet::default();
            collect_assigned_var_names(binary.rhs, &mut assigned_in_right);

            if !assigned_in_right.is_empty() {
                filter_clauses_for_assigned_vars(
                    &mut left_result.if_true_clauses,
                    &assigned_in_right,
                );
                filter_assertion_map_for_assigned_vars(
                    &mut left_result.if_true,
                    &assigned_in_right,
                );
            }

            // True branch: combine all clauses (AND)
            result.if_true_clauses.extend(left_result.if_true_clauses);
            result.if_true_clauses.extend(right_result.if_true_clauses);
            result
                .silently_asserted_vars
                .extend(left_result.silently_asserted_vars.iter().cloned());
            result
                .silently_asserted_vars
                .extend(right_result.silently_asserted_vars.iter().cloned());

            // False branch: negate of (left AND right) = !left OR !right
            // This is handled by combine_ored_clauses on the negated formulas
            if !left_result.if_false_clauses.is_empty() && !right_result.if_false_clauses.is_empty()
            {
                if let Ok(combined) = combine_ored_clauses(
                    left_result.if_false_clauses,
                    right_result.if_false_clauses,
                    cond_id,
                ) {
                    // Rebuild the if_false map from the disjunction's unit
                    // truths: when both sides constrain the same key the OR can
                    // collapse (`!(!($p = a) && !($p = b))` -> `$p` truthy).
                    let mut referenced = FxHashSet::default();
                    let (truths, _) = pzoom_code_info::algebra::get_truths_from_formula(
                        combined.iter().collect(),
                        None,
                        &mut referenced,
                    );
                    for (var, groups) in truths {
                        for group in groups {
                            if group.len() == 1 {
                                result
                                    .if_false
                                    .entry(var.clone())
                                    .or_default()
                                    .push(group);
                            }
                        }
                    }
                    result.if_false_clauses = combined;
                }
            }

            // Both operands' AND groups apply on the true path (Psalm ANDs the
            // two sides' $if_types).
            for (var, groups) in left_result.if_true {
                result.if_true.entry(var).or_default().extend(groups);
            }
            for (var, groups) in right_result.if_true {
                result.if_true.entry(var).or_default().extend(groups);
            }
        }
        BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
            // For ||, at least one side must be true
            // CNF: need to convert (left OR right) to CNF
            let mut left_result = scrape_assertions(analyzer, binary.lhs, cond_id, analysis_data);
            let right_result = scrape_assertions(analyzer, binary.rhs, cond_id, analysis_data);
            let mut assigned_in_right = FxHashSet::default();
            collect_assigned_var_names(binary.rhs, &mut assigned_in_right);

            if !assigned_in_right.is_empty() {
                filter_clauses_for_assigned_vars(
                    &mut left_result.if_false_clauses,
                    &assigned_in_right,
                );
                filter_assertion_map_for_assigned_vars(
                    &mut left_result.if_false,
                    &assigned_in_right,
                );
            }

            // True branch: (left_true OR right_true)
            // This creates a disjunction clause
            if !left_result.if_true_clauses.is_empty() && !right_result.if_true_clauses.is_empty() {
                if let Ok(combined) = combine_ored_clauses(
                    left_result.if_true_clauses,
                    right_result.if_true_clauses,
                    cond_id,
                ) {
                    result.if_true_clauses = combined;
                }
            }

            // False branch: both must be false
            result.if_false_clauses.extend(left_result.if_false_clauses);
            result
                .if_false_clauses
                .extend(right_result.if_false_clauses);

            // Both operands' AND groups apply on the false path (De Morgan on
            // `||`: each side's if_false facts hold).
            for (var, groups) in left_result.if_false {
                result.if_false.entry(var).or_default().extend(groups);
            }
            for (var, groups) in right_result.if_false {
                result.if_false.entry(var).or_default().extend(groups);
            }
        }
        BinaryOperator::Identical(_) => {
            scrape_equality_assertions(
                analyzer,
                binary,
                &mut result,
                true,
                true,
                cond_id,
                analysis_data,
            );
        }
        BinaryOperator::NotIdentical(_) => {
            scrape_equality_assertions(
                analyzer,
                binary,
                &mut result,
                false,
                true,
                cond_id,
                analysis_data,
            );
        }
        BinaryOperator::Equal(_) => {
            scrape_equality_assertions(
                analyzer,
                binary,
                &mut result,
                true,
                false,
                cond_id,
                analysis_data,
            );
        }
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => {
            scrape_equality_assertions(
                analyzer,
                binary,
                &mut result,
                false,
                false,
                cond_id,
                analysis_data,
            );
        }
        BinaryOperator::LessThan(_)
        | BinaryOperator::LessThanOrEqual(_)
        | BinaryOperator::GreaterThan(_)
        | BinaryOperator::GreaterThanOrEqual(_) => {
            scrape_inequality_assertions(binary, &mut result, cond_id);
        }
        _ => {}
    }

    result
}

/// Extracts assertions from instanceof expressions.
fn scrape_instanceof_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) {
    let Some(var_name) = expression_identifier::get_expression_var_key(binary.lhs) else {
        return;
    };

    // `$x instanceof $this`: Psalm's getInstanceOfAssertions emits an
    // IsIdentical assertion on `static` bound to the declaring class
    // (`$x = A&static`), an equality so impossibilities never report.
    if let Expression::Variable(Variable::Direct(direct)) = binary.rhs.unparenthesized()
        && direct.name == "$this"
    {
        if let Some(declaring_class) = analyzer.get_declaring_class() {
            let assertion_type = TAtomic::TNamedObject {
                name: declaring_class,
                type_params: None,
                is_static: true,
                remapped_params: false,
            };
            add_equality_assertions(result, var_name, assertion_type, true, cond_id);
        }
        return;
    }

    let assertion_type = if let Some(class_id) = resolve_class_expression(analyzer, binary.rhs) {
        if class_id == pzoom_str::StrId::EMPTY {
            return;
        }

        // `instanceof self`/`parent` resolve to the concrete class here
        // (Psalm's getInstanceOfAssertions); `static` stays late-bound — the
        // expander binds it per receiver.
        let resolved_class_id = if class_id == pzoom_str::StrId::SELF {
            match analyzer.get_declaring_class() {
                Some(declaring_class) => declaring_class,
                None => class_id,
            }
        } else if class_id == pzoom_str::StrId::PARENT {
            match analyzer.get_declaring_class().and_then(|declaring_class| {
                analyzer
                    .codebase
                    .get_class(declaring_class)
                    .and_then(|class_info| class_info.parent_class)
            }) {
                Some(parent_class) => parent_class,
                None => class_id,
            }
        } else {
            class_id
        };

        TAtomic::TNamedObject {
            name: resolved_class_id,
            type_params: None,
        is_static: false, remapped_params: false }
    } else if let Some(class_string_union) = analysis_data.expr_types.get(&get_expr_id(binary.rhs)).cloned() {
        let Some(classlike_atomic) =
            extract_classlike_from_class_string_union(analyzer, &class_string_union)
        else {
            return;
        };

        classlike_atomic
    } else {
        return;
    };

    let is_type_clause = create_single_var_clause(
        &var_name,
        Assertion::IsType(assertion_type.clone()),
        cond_id,
    );
    result.if_true_clauses.push(is_type_clause);

    let is_not_type_clause = create_single_var_clause(
        &var_name,
        Assertion::IsNotType(assertion_type.clone()),
        cond_id,
    );
    result.if_false_clauses.push(is_not_type_clause);

    result
        .if_true
        .entry(var_name.clone())
        .or_default()
        .push(vec![Assertion::IsType(assertion_type.clone())]);
    result
        .if_false
        .entry(var_name)
        .or_default()
        .push(vec![Assertion::IsNotType(assertion_type)]);
}

/// Extracts assertions from equality comparisons.
fn scrape_equality_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    is_positive: bool,
    is_strict: bool,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) {
    if try_add_count_equality_assertions(result, binary, is_positive, cond_id) {
        return;
    }

    if let Some((var_name, cast_type)) = get_cast_type_comparison(binary.lhs, binary.rhs) {
        add_type_assertions(result, var_name, cast_type, is_positive, cond_id);
        return;
    }

    // `get_class($x) === <expr of type class-string<T>>` narrows `$x` to the
    // template parameter `T`. Psalm's AssertionFinder produces an equality
    // assertion (`IsIdentical`/`IsNotIdentical`) here, not an `is` assertion —
    // equality assertions on templates never report redundancy.
    if let Some((var_name, template_atomic)) =
        get_get_class_template_comparison(analyzer, binary.lhs, binary.rhs, analysis_data)
    {
        add_equality_assertions(result, var_name, template_atomic, is_positive, cond_id);
        return;
    }

    if let Some((var_name, class_id)) = get_get_class_comparison(analyzer, binary.lhs, binary.rhs) {
        if class_id != StrId::EMPTY {
            let resolved_class_id = if class_id == StrId::STATIC || class_id == StrId::SELF {
                analyzer.get_declaring_class().unwrap_or(class_id)
            } else {
                class_id
            };

            let assertion_type = TAtomic::TNamedObject {
                name: resolved_class_id,
                type_params: None,
            is_static: false, remapped_params: false };
            let is_static_origin = var_name == "@static";
            let primary_var = if is_static_origin { "$this" } else { var_name.as_str() };

            // Psalm models get_class comparisons as IsClassEqual/IsClassNotEqual —
            // equality assertions, which never report redundancy. A `!==`
            // comparison produces the *negative* fact as an if-true assertion
            // (Psalm's getGetclassInequalityAssertions), so the narrowing flows
            // through the formula and the else path derives `IsEqual` by clause
            // negation.
            add_equality_assertions(
                result,
                VarName::new(primary_var),
                assertion_type,
                is_positive,
                cond_id,
            );

            if is_static_origin {
                let target_clauses = if is_positive {
                    &mut result.if_true_clauses
                } else {
                    &mut result.if_false_clauses
                };
                let target_map = if is_positive {
                    &mut result.if_true
                } else {
                    &mut result.if_false
                };
                push_assertion(
                    target_clauses,
                    target_map,
                    &var_name,
                    Assertion::IsType(TAtomic::TNamedObject {
                        name: resolved_class_id,
                        type_params: None,
                    is_static: false, remapped_params: false }),
                    cond_id,
                );
            }
        }
        return;
    }

    // A plain class-string variable compared to a literal `B::class`. This must
    // run after the get_class/`$x::class` paths above: `$a::class == B::class`
    // asserts on the *object* `$a` (Psalm's IsClassEqual), not a class-string.
    if let Some((var_name, class_id)) =
        get_class_string_var_comparison(analyzer, binary.lhs, binary.rhs)
    {
        if class_id != StrId::EMPTY {
            let assertion_type = TAtomic::TLiteralClassString {
                name: analyzer.interner.lookup(class_id).to_string(),
            };
            add_type_assertions(result, var_name, assertion_type, is_positive, cond_id);
        }
        return;
    }

    if let Some((other_value_position, literal_bool)) =
        has_literal_boolean_comparison(binary.lhs, binary.rhs)
    {
        let compared_expr = match other_value_position {
            OtherValuePosition::Left => binary.rhs,
            OtherValuePosition::Right => binary.lhs,
        };

        if is_strict && let Some(var_name) = get_assertable_var_name(compared_expr) {
            let assertion_type = if literal_bool {
                TAtomic::TTrue
            } else {
                TAtomic::TFalse
            };

            add_type_assertions(result, var_name.clone(), assertion_type, is_positive, cond_id);
            // A call compared with === true/false still contributes its
            // custom @psalm-assert-if-* facts about OTHER variables (Psalm
            // unwraps the comparison); the call's own truthiness fact is
            // already covered by the strict assertion above.
            if matches!(compared_expr.unparenthesized(), Expression::Call(_)) {
                let mut inner = scrape_assertions(analyzer, compared_expr, cond_id, analysis_data);
                let own_key = var_name;
                inner.if_true.retain(|key, _| *key != own_key);
                inner.if_false.retain(|key, _| *key != own_key);
                let own_clause_key =
                    pzoom_code_info::algebra::clause::ClauseKey::Name(own_key.clone());
                inner
                    .if_true_clauses
                    .retain(|clause| !clause.possibilities.contains_key(&own_clause_key));
                inner
                    .if_false_clauses
                    .retain(|clause| !clause.possibilities.contains_key(&own_clause_key));
                let true_means_inner_true = if is_positive {
                    literal_bool
                } else {
                    !literal_bool
                };
                if true_means_inner_true {
                    result.if_true_clauses.extend(inner.if_true_clauses);
                    result.if_false_clauses.extend(inner.if_false_clauses);
                    merge_assertion_maps(&mut result.if_true, inner.if_true);
                    merge_assertion_maps(&mut result.if_false, inner.if_false);
                } else {
                    result.if_true_clauses.extend(inner.if_false_clauses);
                    result.if_false_clauses.extend(inner.if_true_clauses);
                    merge_assertion_maps(&mut result.if_true, inner.if_false);
                    merge_assertion_maps(&mut result.if_false, inner.if_true);
                }
            }
            return;
        }

        let inner = scrape_assertions(analyzer, compared_expr, cond_id, analysis_data);
        let true_means_inner_true = if is_positive {
            literal_bool
        } else {
            !literal_bool
        };

        if true_means_inner_true {
            result.if_true_clauses.extend(inner.if_true_clauses);
            result.if_false_clauses.extend(inner.if_false_clauses);
            merge_assertion_maps(&mut result.if_true, inner.if_true);
            merge_assertion_maps(&mut result.if_false, inner.if_false);
        } else {
            result.if_true_clauses.extend(inner.if_false_clauses);
            result.if_false_clauses.extend(inner.if_true_clauses);
            merge_assertion_maps(&mut result.if_true, inner.if_false);
            merge_assertion_maps(&mut result.if_false, inner.if_true);
        }
        return;
    }

    if try_add_assignment_null_comparison_assertions(
        binary,
        result,
        is_positive,
        cond_id,
        analysis_data,
    ) {
        return;
    }

    if !is_strict {
        if let Some(null_position) = has_null_comparison(binary.lhs, binary.rhs) {
            let var_expr = match null_position {
                OtherValuePosition::Left => binary.rhs,
                OtherValuePosition::Right => binary.lhs,
            };
            let Some(var_name) = get_assertable_var_name(var_expr) else {
                return;
            };

            let (true_assertion, false_assertion) = if is_positive {
                (Assertion::Falsy, Assertion::Truthy)
            } else {
                (Assertion::Truthy, Assertion::Falsy)
            };

            let true_clause = create_single_var_clause(&var_name, true_assertion.clone(), cond_id);
            let false_clause =
                create_single_var_clause(&var_name, false_assertion.clone(), cond_id);

            result.if_true_clauses.push(true_clause);
            result.if_false_clauses.push(false_clause);
            result
                .if_true
                .entry(var_name.clone())
                .or_default()
                .push(vec![true_assertion]);
            result
                .if_false
                .entry(var_name)
                .or_default()
                .push(vec![false_assertion]);
            return;
        }
    }

    if is_strict
        && let Some(var_expr) = get_empty_string_literal_comparison_target(binary.lhs, binary.rhs)
    {
        let Some(var_name) = get_assertable_var_name(var_expr) else {
            return;
        };

        add_empty_string_assertions(result, &var_name, is_positive, cond_id);
        return;
    }

    if is_strict
        && let Some(var_expr) = get_empty_array_literal_comparison_target(binary.lhs, binary.rhs)
    {
        let Some(var_name) = get_assertable_var_name(var_expr) else {
            return;
        };

        add_empty_countable_assertions(result, &var_name, is_positive, cond_id);
        return;
    }

    if try_add_typed_value_comparison_assertions(
        result,
        binary.lhs,
        binary.rhs,
        is_positive,
        is_strict,
        cond_id,
        analysis_data,
    ) {
        return;
    }

    // Psalm's `scrapeEqualityAssertions` tail: "both side of the Identical can
    // be asserted to the intersection of both". Only `===` conditionals get
    // this, and only on the true path — the false path falls out of formula
    // negation (Psalm's inequality scraper has no such branch).
    if is_positive && is_strict {
        add_identical_intersection_assertions(
            result,
            binary.lhs,
            binary.rhs,
            cond_id,
            analysis_data,
        );
    }
}

/// Port of the tail of Psalm's `AssertionFinder::scrapeEqualityAssertions`:
/// `$a === $b` narrows each operand to the intersection of the two operand
/// types, asserted as orred `IsIdentical` atomics on any operand whose own
/// type differs from the intersection.
///
/// Psalm additionally reports `TypeDoesNotContainType` when
/// `canExpressionTypesBeIdentical` fails; the assertion finder has no issue
/// sink, so an empty intersection just produces no assertions here.
fn add_identical_intersection_assertions(
    result: &mut AssertionResult,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) {
    let Some(var_type) = analysis_data.expr_types.get(&get_expr_id(left_expr)).cloned() else {
        return;
    };
    let Some(other_type) = analysis_data.expr_types.get(&get_expr_id(right_expr)).cloned() else {
        return;
    };

    let Some(intersection_type) =
        assertion_reconciler::intersect_union_with_union(&var_type, &other_type)
    else {
        return;
    };

    let all_assertions: Vec<Assertion> = intersection_type
        .types
        .iter()
        .map(|atomic| Assertion::IsEqual(atomic.clone()))
        .collect();

    let intersection_id = intersection_type.get_id(None);

    if let Some(var_name_left) = expression_identifier::get_expression_var_key(left_expr)
        && var_type.get_id(None) != intersection_id
    {
        result
            .if_true_clauses
            .push(create_var_clause(&var_name_left, &all_assertions, cond_id));
        // Psalm files the orred group into `$if_types` as one AND group; pzoom
        // keeps multi-atomic groups in the clause form only (the map consumers
        // reconcile them via the formula path), matching prior behavior.
        if let [single_assertion] = all_assertions.as_slice() {
            result
                .if_true
                .entry(var_name_left)
                .or_default()
                .push(vec![single_assertion.clone()]);
        }
    }

    if let Some(var_name_right) = expression_identifier::get_expression_var_key(right_expr)
        && other_type.get_id(None) != intersection_id
    {
        result
            .if_true_clauses
            .push(create_var_clause(&var_name_right, &all_assertions, cond_id));
        if let [single_assertion] = all_assertions.as_slice() {
            result
                .if_true
                .entry(var_name_right)
                .or_default()
                .push(vec![single_assertion.clone()]);
        }
    }
}

fn has_null_comparison(
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
) -> Option<OtherValuePosition> {
    if is_null_expr(right_expr) {
        return Some(OtherValuePosition::Right);
    }

    if is_null_expr(left_expr) {
        return Some(OtherValuePosition::Left);
    }

    None
}

fn has_literal_boolean_comparison(
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
) -> Option<(OtherValuePosition, bool)> {
    if let Some(value) = get_literal_boolean(right_expr) {
        return Some((OtherValuePosition::Right, value));
    }

    if let Some(value) = get_literal_boolean(left_expr) {
        return Some((OtherValuePosition::Left, value));
    }

    None
}

fn try_add_typed_value_comparison_assertions(
    result: &mut AssertionResult,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
    is_positive: bool,
    is_strict: bool,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> bool {
    // Psalm's `hasTypedValueComparison` position gates: a typed value on the
    // right only fires when the right side is not a variable-ish expression or
    // the left side is one (`ASSIGNMENT_TO_RIGHT`); a typed value on the left
    // only fires when the left side is not variable-ish (`ASSIGNMENT_TO_LEFT`).
    // Plain `$a === $b` comparisons fall through to the type-intersection
    // branch instead.
    if !is_variable_ish_expression(right_expr) || is_variable_ish_expression(left_expr) {
        if let Some(right_type) = get_expression_assertion_type(right_expr, analysis_data) {
            if is_strict || is_safe_loose_equality_literal_assertion(&right_type) {
                if let Some(left_var_name) = get_assertable_var_name(left_expr) {
                    // `$a['k'] === <value>` implies the entry exists and is
                    // non-null — unless the compared value IS null.
                    let value_is_null = matches!(right_type, TAtomic::TNull);
                    add_generated_type_assertions(result, left_var_name, right_type, is_positive, cond_id);
                    if is_positive && !value_is_null {
                        add_array_access_presence_assertion(result, left_expr, cond_id);
                    }
                    return true;
                }
            }
        }
    }

    if !is_variable_ish_expression(left_expr) {
        if let Some(left_type) = get_expression_assertion_type(left_expr, analysis_data) {
            if is_strict || is_safe_loose_equality_literal_assertion(&left_type) {
                if let Some(right_var_name) = get_assertable_var_name(right_expr) {
                    let value_is_null = matches!(left_type, TAtomic::TNull);
                    add_generated_type_assertions(result, right_var_name, left_type, is_positive, cond_id);
                    if is_positive && !value_is_null {
                        add_array_access_presence_assertion(result, right_expr, cond_id);
                    }
                    return true;
                }
            }
        }
    }

    false
}

/// Psalm's `Variable`/`PropertyFetch`/`StaticPropertyFetch` check in
/// `hasTypedValueComparison` — the expression kinds whose inferred *value*
/// should not be treated as a typed-value comparison target.
fn is_variable_ish_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Variable(_)
            | Expression::Access(Access::Property(_))
            | Expression::Access(Access::StaticProperty(_))
    )
}

fn is_safe_loose_equality_literal_assertion(assertion_type: &TAtomic) -> bool {
    match assertion_type {
        TAtomic::TLiteralString { value } => !is_numeric_like_string(value),
        TAtomic::TLiteralClassString { .. } => true,
        _ => false,
    }
}

fn is_numeric_like_string(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.parse::<f64>().is_ok()
}

fn try_add_assignment_null_comparison_assertions(
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    is_positive: bool,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> bool {
    let assignment_expr = if is_null_expr(binary.rhs) {
        binary.lhs
    } else if is_null_expr(binary.lhs) {
        binary.rhs
    } else {
        return false;
    };

    let Expression::Assignment(assignment) = assignment_expr.unparenthesized() else {
        return false;
    };

    let Some(var_name) = get_assignment_target_var_name(assignment.lhs) else {
        return false;
    };
    // Assignment-derived facts describe the post-assignment value — mark the
    // key with Psalm's `=` prefix (Clause::redefined_vars).
    let var_name = VarName::from(format!("={var_name}"));

    let Some(assigned_union) = analysis_data.expr_types.get(&get_expr_id(assignment_expr)).cloned() else {
        return false;
    };

    let non_null_types: Vec<TAtomic> = assigned_union
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TNull))
        .cloned()
        .collect();

    let non_null_assertion = if non_null_types.len() == 1 {
        Assertion::IsType(non_null_types[0].clone())
    } else {
        Assertion::IsNotType(TAtomic::TNull)
    };

    let (true_assertion, false_assertion) = if is_positive {
        (Assertion::IsType(TAtomic::TNull), non_null_assertion)
    } else {
        (non_null_assertion, Assertion::IsType(TAtomic::TNull))
    };

    push_assertion(
        &mut result.if_true_clauses,
        &mut result.if_true,
        &var_name,
        true_assertion,
        cond_id,
    );
    push_assertion(
        &mut result.if_false_clauses,
        &mut result.if_false,
        &var_name,
        false_assertion,
        cond_id,
    );

    true
}

fn add_array_access_presence_assertion(
    result: &mut AssertionResult,
    expr: &Expression<'_>,
    cond_id: (u32, u32),
) {
    let Expression::ArrayAccess(array_access) = expr.unparenthesized() else {
        return;
    };

    let Some(base_var_name) = expression_identifier::get_expression_var_key(array_access.array)
    else {
        return;
    };

    let Some(array_key) = get_literal_array_key(array_access.index) else {
        return;
    };

    push_assertion(
        &mut result.if_true_clauses,
        &mut result.if_true,
        &base_var_name,
        Assertion::HasNonnullEntryForKey(array_key),
        cond_id,
    );
}

fn get_cast_type_comparison(
    lhs: &Expression<'_>,
    rhs: &Expression<'_>,
) -> Option<(VarName, TAtomic)> {
    fn extract_cast_type(
        cast_expr: &Expression<'_>,
        compared_expr: &Expression<'_>,
    ) -> Option<(VarName, TAtomic)> {
        let Expression::UnaryPrefix(unary) = cast_expr.unparenthesized() else {
            return None;
        };

        let cast_atomic = match unary.operator {
            UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
                TAtomic::TString
            }
            UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
                TAtomic::TInt
            }
            UnaryPrefixOperator::FloatCast(_, _)
            | UnaryPrefixOperator::DoubleCast(_, _)
            | UnaryPrefixOperator::RealCast(_, _) => TAtomic::TFloat,
            UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
                TAtomic::TBool
            }
            UnaryPrefixOperator::ArrayCast(_, _) => TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            UnaryPrefixOperator::ObjectCast(_, _) => TAtomic::TObject,
            _ => return None,
        };

        let cast_operand_key = expression_identifier::get_expression_var_key(unary.operand)?;
        let compared_key = expression_identifier::get_expression_var_key(compared_expr)?;

        if cast_operand_key == compared_key {
            Some((cast_operand_key, cast_atomic))
        } else {
            None
        }
    }

    extract_cast_type(lhs, rhs).or_else(|| extract_cast_type(rhs, lhs))
}

fn scrape_inequality_assertions(
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
) {
    if try_add_count_inequality_assertions(result, binary, cond_id) {
        return;
    }

    // Psalm's hasSuperiorNumberCheck only matches single *int* literal
    // comparisons (handled above); a float-literal comparison like
    // `$avg > 1.1` asserts nothing.
    if try_add_int_range_inequality_assertions(result, binary, cond_id) {
        return;
    }
}

fn try_add_int_range_inequality_assertions(
    result: &mut AssertionResult,
    binary: &Binary<'_>,
    cond_id: (u32, u32),
) -> bool {
    let Some((var_name, literal, operator)) = get_int_inequality_comparison(binary) else {
        return false;
    };

    let (true_range, false_range) = match operator {
        CountCmpOp::Gt => (
            TAtomic::TIntRange {
                min: Some(literal.saturating_add(1)),
                max: None,
            },
            TAtomic::TIntRange {
                min: None,
                max: Some(literal),
            },
        ),
        CountCmpOp::Ge => (
            TAtomic::TIntRange {
                min: Some(literal),
                max: None,
            },
            TAtomic::TIntRange {
                min: None,
                max: Some(literal.saturating_sub(1)),
            },
        ),
        CountCmpOp::Lt => (
            TAtomic::TIntRange {
                min: None,
                max: Some(literal.saturating_sub(1)),
            },
            TAtomic::TIntRange {
                min: Some(literal),
                max: None,
            },
        ),
        CountCmpOp::Le => (
            TAtomic::TIntRange {
                min: None,
                max: Some(literal),
            },
            TAtomic::TIntRange {
                min: Some(literal.saturating_add(1)),
                max: None,
            },
        ),
    };

    result.if_true_clauses.push(create_single_var_clause(
        &var_name,
        Assertion::IsType(true_range.clone()),
        cond_id,
    ));
    result.if_false_clauses.push(create_single_var_clause(
        &var_name,
        Assertion::IsType(false_range.clone()),
        cond_id,
    ));
    result
        .if_true
        .entry(var_name.clone())
        .or_default()
        .push(vec![Assertion::IsType(true_range)]);
    result
        .if_false
        .entry(var_name)
        .or_default()
        .push(vec![Assertion::IsType(false_range)]);

    true
}

fn add_empty_string_assertions(
    result: &mut AssertionResult,
    var_name: &str,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    let empty_string = TAtomic::TLiteralString {
        value: String::new(),
    };

    // Psalm keeps the literal comparison in the assertion itself
    // (`IsIdentical('')` / `IsNotIdentical('')`), so formula negation is
    // exact — the fall-through of `if ($s !== "") {…}` knows `$s === ""`.
    // The non-empty-string refinement happens in the reconciler
    // (`string − "" ⇒ non-empty-string`, Psalm's
    // `handleLiteralNegatedEquality`), not in the assertion.
    let (true_assertion, false_assertion) = if is_positive {
        (
            Assertion::IsEqual(empty_string.clone()),
            Assertion::IsNotEqual(empty_string),
        )
    } else {
        (
            Assertion::IsNotEqual(empty_string.clone()),
            Assertion::IsEqual(empty_string),
        )
    };

    result.if_true_clauses.push(create_single_var_clause(
        var_name,
        true_assertion.clone(),
        cond_id,
    ));
    result
        .if_true
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![true_assertion]);
    result.if_false_clauses.push(create_single_var_clause(
        var_name,
        false_assertion.clone(),
        cond_id,
    ));
    result
        .if_false
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![false_assertion]);
}

fn add_empty_countable_assertions(
    result: &mut AssertionResult,
    var_name: &str,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    let true_assertion = if is_positive {
        Assertion::EmptyCountable
    } else {
        Assertion::NonEmptyCountable(true)
    };
    let false_assertion = if is_positive {
        Assertion::NonEmptyCountable(true)
    } else {
        Assertion::EmptyCountable
    };

    result.if_true_clauses.push(create_single_var_clause(
        var_name,
        true_assertion.clone(),
        cond_id,
    ));
    result.if_false_clauses.push(create_single_var_clause(
        var_name,
        false_assertion.clone(),
        cond_id,
    ));
    result
        .if_true
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![true_assertion]);
    result
        .if_false
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![false_assertion]);
}

fn try_add_count_equality_assertions(
    result: &mut AssertionResult,
    binary: &Binary<'_>,
    is_positive: bool,
    cond_id: (u32, u32),
) -> bool {
    let Some((var_name, count)) = get_count_literal_comparison(binary.lhs, binary.rhs) else {
        return false;
    };

    let true_assertion = if count == 0 {
        if is_positive {
            Assertion::EmptyCountable
        } else {
            Assertion::NonEmptyCountable(true)
        }
    } else if is_positive {
        Assertion::HasExactCount(count)
    } else {
        Assertion::DoesNotHaveExactCount(count)
    };

    let false_assertion = if count == 0 {
        if is_positive {
            Assertion::NonEmptyCountable(true)
        } else {
            Assertion::EmptyCountable
        }
    } else if is_positive {
        Assertion::DoesNotHaveExactCount(count)
    } else {
        Assertion::HasExactCount(count)
    };

    result.if_true_clauses.push(create_single_var_clause(
        &var_name,
        true_assertion.clone(),
        cond_id,
    ));
    result.if_false_clauses.push(create_single_var_clause(
        &var_name,
        false_assertion.clone(),
        cond_id,
    ));
    result
        .if_true
        .entry(var_name.clone())
        .or_default()
        .push(vec![true_assertion]);
    result
        .if_false
        .entry(var_name)
        .or_default()
        .push(vec![false_assertion]);

    true
}

fn try_add_count_inequality_assertions(
    result: &mut AssertionResult,
    binary: &Binary<'_>,
    cond_id: (u32, u32),
) -> bool {
    let Some((var_name, count, operator)) = get_count_inequality_comparison(binary) else {
        return false;
    };

    // Mirror Psalm's AssertionFinder::getGreaterAssertions / getSmallerAssertions:
    // `count($a) >= n` yields HasAtLeastCount(n) (NonEmptyCountable for the n==1
    // boundary), and `count($a) < n` yields DoesNotHaveAtLeastCount(n) (EmptyCountable
    // for the empty boundary). `operator`/`count` are normalized so count() is on the
    // left, i.e. `count($a) <operator> count`.
    let (Some(true_assertion), Some(false_assertion)) = (match operator {
        // count($a) > 0  -> non-empty
        CountCmpOp::Gt if count == 0 => (
            Some(Assertion::NonEmptyCountable(true)),
            Some(Assertion::EmptyCountable),
        ),
        // count($a) > n (n >= 1)  -> at least n + 1
        CountCmpOp::Gt => (
            Some(Assertion::HasAtLeastCount(count + 1)),
            Some(Assertion::DoesNotHaveAtLeastCount(count + 1)),
        ),
        // count($a) >= 1  -> non-empty
        CountCmpOp::Ge if count == 1 => (
            Some(Assertion::NonEmptyCountable(true)),
            Some(Assertion::EmptyCountable),
        ),
        // count($a) >= n (n >= 2)  -> at least n  (count($a) >= 0 is always true)
        CountCmpOp::Ge if count >= 2 => (
            Some(Assertion::HasAtLeastCount(count)),
            Some(Assertion::DoesNotHaveAtLeastCount(count)),
        ),
        // count($a) < 1  (or the degenerate < 0)  -> empty
        CountCmpOp::Lt if count <= 1 => (
            Some(Assertion::EmptyCountable),
            Some(Assertion::NonEmptyCountable(true)),
        ),
        // count($a) < n (n >= 2)  -> fewer than n
        CountCmpOp::Lt => (
            Some(Assertion::DoesNotHaveAtLeastCount(count)),
            Some(Assertion::HasAtLeastCount(count)),
        ),
        // count($a) <= 0  -> empty
        CountCmpOp::Le if count == 0 => (
            Some(Assertion::EmptyCountable),
            Some(Assertion::NonEmptyCountable(true)),
        ),
        // count($a) <= n (n >= 1)  -> fewer than n + 1
        CountCmpOp::Le => (
            Some(Assertion::DoesNotHaveAtLeastCount(count + 1)),
            Some(Assertion::HasAtLeastCount(count + 1)),
        ),
        _ => (None, None),
    }) else {
        return false;
    };

    result.if_true_clauses.push(create_single_var_clause(
        &var_name,
        true_assertion.clone(),
        cond_id,
    ));
    result.if_false_clauses.push(create_single_var_clause(
        &var_name,
        false_assertion.clone(),
        cond_id,
    ));
    result
        .if_true
        .entry(var_name.clone())
        .or_default()
        .push(vec![true_assertion]);
    result
        .if_false
        .entry(var_name)
        .or_default()
        .push(vec![false_assertion]);

    true
}

/// Extracts assertions from a unary prefix expression (!$x).
fn scrape_unary_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match &unary.operator {
        UnaryPrefixOperator::Not(_) => {
            // !expr swaps if_true and if_false
            let inner_result = scrape_assertions(analyzer, unary.operand, cond_id, analysis_data);

            result.if_true_clauses = inner_result.if_false_clauses;
            result.if_false_clauses = inner_result.if_true_clauses.clone();
            result.if_true = inner_result.if_false;
            result.if_false = inner_result.if_true;

            // Psalm negates the operand's FORMULA (FormulaGenerator's
            // BooleanNot → negateFormula). For a bare call whose clauses come
            // solely from custom assertions, that negation is exact:
            // `!$this->assertsNull()` implies ¬(a is null). Boolean structure
            // inside the operand would need range-keyed opaque clauses to
            // negate soundly, so only the call case derives facts.
            if result.if_true.is_empty()
                && result.if_true_clauses.is_empty()
                && matches!(unary.operand.unparenthesized(), Expression::Call(_))
                && !inner_result.if_true_clauses.is_empty()
                && let Ok(negated) =
                    pzoom_code_info::algebra::negate_formula(inner_result.if_true_clauses)
            {
                for clause in &negated {
                    if clause.possibilities.len() == 1
                        && let Some((
                            pzoom_code_info::algebra::clause::ClauseKey::Name(var_name),
                            possibilities,
                        )) = clause.possibilities.iter().next()
                    {
                        result
                            .if_true
                            .entry(var_name.clone())
                            .or_default()
                            .push(possibilities.values().cloned().collect());
                    }
                }
                result.if_true_clauses = negated;
            }
        }
        _ => {}
    }

    result
}

/// The receiver of a resolved instance call: class id plus any generic type
/// params, used to substitute class templates in callsite assertions.
type CallReceiver = Option<(StrId, Option<Vec<TUnion>>)>;

fn resolve_functionlike_for_call<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    call: &Call<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<(&'a pzoom_code_info::FunctionLikeInfo, CallReceiver)> {
    match call {
        Call::Function(func_call) => {
            let Expression::Identifier(identifier) = func_call.function.unparenthesized() else {
                return None;
            };

            let resolved = analyzer
                .get_resolved_name(identifier.start_offset() as u32)
                .unwrap_or_else(|| analyzer.interner.intern(identifier.value()));

            let bare_name = identifier.value().trim_start_matches('\\');

            analyzer
                .codebase
                .get_function(resolved)
                .or_else(|| {
                    analyzer
                        .codebase
                        .get_function(analyzer.interner.intern(bare_name))
                })
                .or_else(|| {
                    analyzer
                        .codebase
                        .functionlike_infos
                        .values()
                        .find(|function_info| {
                            if function_info.file_path != analyzer.file_path {
                                return false;
                            }

                            let function_name = analyzer.interner.lookup(function_info.name);
                            function_name.as_ref() == bare_name
                                || function_name
                                    .rsplit('\\')
                                    .next()
                                    .is_some_and(|segment| segment == bare_name)
                        })
                })
                .map(|function_info| (function_info, None))
        }
        Call::Method(method_call) => resolve_methodlike_for_instance_call(
            analyzer,
            method_call.object,
            &method_call.method,
            analysis_data,
        ),
        Call::NullSafeMethod(method_call) => {
            resolve_methodlike_for_nullsafe_call(analyzer, method_call, analysis_data)
        }
        Call::StaticMethod(static_call) => {
            resolve_methodlike_for_static_call(analyzer, static_call)
                .map(|function_info| (function_info, None))
        }
    }
}

fn resolve_methodlike_for_instance_call<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    object: &Expression<'_>,
    method: &ClassLikeMemberSelector<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<(&'a pzoom_code_info::FunctionLikeInfo, CallReceiver)> {
    let ClassLikeMemberSelector::Identifier(method_identifier) = method else {
        return None;
    };

    let receiver = match object.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) if direct.name == "$this" => {
            analyzer.get_declaring_class().map(|class_id| (class_id, None))
        }
        other => {
            // Non-$this receivers: use the inferred expression type, so class
            // templates in the assertion resolve through the receiver's type
            // params (e.g. `Type<list<int>>::is(...)` asserting `T $toCheck`).
            analysis_data
                .expr_types.get(&get_expr_id(other)).cloned()
                .and_then(|object_type| {
                    object_type.types.iter().find_map(|atomic| match atomic {
                        TAtomic::TNamedObject {
                            name, type_params, ..
                        } => Some((*name, type_params.clone())),
                        _ => None,
                    })
                })
        }
    }?;

    let class_info = analyzer.codebase.get_class(receiver.0)?;
    let method_id = analyzer.interner.intern(method_identifier.value);
    class_info
        .methods
        .get(&method_id)
        .map(|method| (&**method, Some(receiver)))
}

fn resolve_methodlike_for_nullsafe_call<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    method_call: &NullSafeMethodCall<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<(&'a pzoom_code_info::FunctionLikeInfo, CallReceiver)> {
    resolve_methodlike_for_instance_call(
        analyzer,
        method_call.object,
        &method_call.method,
        analysis_data,
    )
}

fn resolve_methodlike_for_static_call<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let ClassLikeMemberSelector::Identifier(method_identifier) = &static_call.method else {
        return None;
    };

    let mut class_id = resolve_class_expression(analyzer, static_call.class)?;
    // self::/static:: calls resolve against the declaring class so their
    // method storage (and custom @psalm-assert-if-* annotations) is found;
    // instanceof keeps the late-bound `static` semantics, so the rewrite
    // lives here rather than in resolve_class_expression.
    if class_id == StrId::SELF || class_id == StrId::STATIC {
        class_id = analyzer.get_declaring_class()?;
    }
    let class_info = analyzer.codebase.get_class(class_id)?;
    let method_id = analyzer.interner.intern(method_identifier.value);
    class_info.methods.get(&method_id).map(|method| &**method)
}

/// The context key for a call's receiver expression, used to localize
/// `$this->...`-rooted docblock assertions (Psalm rewrites `$this->` to the
/// receiver's var id in processCustomAssertion).
fn call_receiver_var_key(call: &Call<'_>) -> Option<String> {
    match call {
        Call::Method(method_call) => {
            expression_identifier::get_expression_var_key(method_call.object)
                .map(|key| key.to_string())
        }
        Call::NullSafeMethod(method_call) => {
            expression_identifier::get_expression_var_key(method_call.object)
                .map(|key| key.to_string())
        }
        _ => None,
    }
}

fn apply_callsite_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    receiver: CallReceiver,
    receiver_var_key: Option<&str>,
    args: &[mago_syntax::ast::ast::argument::Argument<'_>],
    analysis_data: &FunctionAnalysisData,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
) {
    let mut template_result = function_call_analyzer::get_template_defaults(function_info);
    let arg_refs: Vec<_> = args.iter().collect();
    let arg_positions: Vec<_> = args.iter().map(|arg| get_expr_id(arg.value())).collect();
    function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        &arg_refs,
        &arg_positions,
        &function_info.params,
        &mut template_result,
        analysis_data,
        &BlockContext::new(),
    );

    // Class templates in the assertion resolve through the receiver: its
    // generic type params first, then any `@extends` substitutions.
    if let Some((receiver_class_id, receiver_type_params)) = receiver
        && let Some(receiver_class_info) = analyzer.codebase.get_class(receiver_class_id)
    {
        let mut receiver_replacements =
            function_call_analyzer::infer_class_template_replacements_from_type_params(
                receiver_class_info,
                receiver_type_params.as_deref(),
            );
        function_call_analyzer::infer_class_template_replacements_from_extended_params(
            &mut receiver_replacements,
            receiver_class_info,
        );
        function_call_analyzer::overlay_template_replacements(
            &mut template_result,
            receiver_replacements,
        );
    }

    // Unconditional @psalm-assert targets narrow before the conditional
    // formula reconciles (Psalm applies them to the condition context), so
    // their contradictions never report at the if entry.
    for assertion in &function_info.assertions {
        let assertion_name_str = analyzer.interner.lookup(assertion.var_id);
        let target_name = assertion_name_str.trim_start_matches('$');
        if let Some(param_idx) = function_info.params.iter().position(|param| {
            analyzer.interner.lookup(param.name).as_ref().trim_start_matches('$') == target_name
        }) && let Some(argument) = args.get(param_idx)
            && let Some(var_key) =
                expression_identifier::get_expression_var_key(argument.value())
        {
            result.silently_asserted_vars.insert(var_key);
        }
    }

    apply_assertion_list(
        analyzer,
        &function_info.params,
        &function_info.if_true_assertions,
        args,
        receiver_var_key,
        &template_result,
        false,
        &mut result.if_true,
        &mut result.if_true_clauses,
        cond_id,
        function_info.declaring_class,
    );
    apply_assertion_list(
        analyzer,
        &function_info.params,
        &function_info.if_false_assertions,
        args,
        receiver_var_key,
        &template_result,
        false,
        &mut result.if_false,
        &mut result.if_false_clauses,
        cond_id,
        function_info.declaring_class,
    );
    // Psalm folds `@psalm-assert-if-false X` into the *if-true* assertion set as
    // the negation of X (processCustomAssertion's `rule[0]->getNegation()`), so
    // the fact reaches the condition formula and the else path recovers X via
    // clause negation.
    apply_assertion_list(
        analyzer,
        &function_info.params,
        &function_info.if_false_assertions,
        args,
        receiver_var_key,
        &template_result,
        true,
        &mut result.if_true,
        &mut result.if_true_clauses,
        cond_id,
        function_info.declaring_class,
    );
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn apply_assertion_list(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertions: &[FunctionLikeAssertion],
    args: &[mago_syntax::ast::ast::argument::Argument<'_>],
    receiver_var_key: Option<&str>,
    template_result: &TemplateResult,
    negate: bool,
    target_map: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    target_clauses: &mut Vec<Clause>,
    cond_id: (u32, u32),
    declaring_class: Option<StrId>,
) {
    for assertion in assertions {
        let assertion_name_str = analyzer.interner.lookup(assertion.var_id);
        // `$this->prop` / `$this->method()` assertion targets localize to the
        // call's receiver (Psalm's processCustomAssertion rewrites the
        // `$this->` prefix to the receiver var id).
        let var_name: String = if assertion_name_str.as_ref() == "$this" {
            // A bare `$this` assertion (`@psalm-assert-if-true T $this`)
            // narrows the call's receiver (Psalm rewrites it to the
            // receiver's var id).
            let Some(receiver_key) = receiver_var_key else {
                continue;
            };
            receiver_key.to_string()
        } else if let Some(rest) = assertion_name_str.strip_prefix("$this->") {
            let Some(receiver_key) = receiver_var_key else {
                continue;
            };
            format!("{receiver_key}->{rest}")
        } else if assertion_name_str.contains("::$") {
            // A static property target (`self::$q`, `A::$q`) keys the scope
            // entry verbatim (expression_identifier spells fetches the same
            // way).
            assertion_name_str.to_string()
        } else {
            let Some(param_idx) = find_assertion_param_index(analyzer, params, assertion.var_id)
            else {
                continue;
            };
            let Some(argument) = args.get(param_idx) else {
                continue;
            };
            let Some(argument_var_name) =
                expression_identifier::get_expression_var_key(argument.value())
            else {
                continue;
            };
            let Some(param_name) = params
                .get(param_idx)
                .map(|param| analyzer.interner.lookup(param.name))
            else {
                continue;
            };
            let Some(mapped) = map_assertion_var_to_argument(
                assertion_name_str.as_ref(),
                param_name.as_ref(),
                &argument_var_name,
            ) else {
                continue;
            };
            mapped
        };

        // Scope keys lowercase `name()` call segments (PHP method names are
        // case-insensitive); canonicalize the assertion target the same way.
        let var_name = canonicalize_call_segments(&var_name);

        let mut resolved_assertion_type =
            replace_assertion_templates(
                analyzer.codebase,
                &assertion.assertion_type,
                template_result,
            );
        // `self::T*`-style tokens in the assertion type resolve against the
        // declaring class (Psalm's TypeExpander pass on assertion rules).
        if let Some(declaring_class) = declaring_class {
            let parent_class = analyzer
                .codebase
                .get_class(declaring_class)
                .and_then(|class_info| class_info.parent_class);
            resolved_assertion_type = match resolved_assertion_type {
                AssertionType::IsType(union) => AssertionType::IsType(
                    crate::type_expander::localize_special_class_type_union(
                        analyzer.codebase,
                        analyzer.interner,
                        &union,
                        declaring_class,
                        declaring_class,
                        parent_class,
                    ),
                ),
                other => other,
            };
        }
        let converted_assertions =
            convert_functionlike_assertion_type(&resolved_assertion_type);
        if converted_assertions.is_empty() {
            continue;
        }

        if negate {
            // De Morgan: ¬(A ∨ B) = ¬A ∧ ¬B — one single-assertion clause (and
            // one singleton AND group) per negated alternative.
            for converted in converted_assertions {
                let negated = converted.get_negation();
                if matches!(negated, Assertion::Any) {
                    continue;
                }
                target_clauses.push(create_var_clause(
                    &var_name,
                    std::slice::from_ref(&negated),
                    cond_id,
                ));
                target_map
                    .entry(VarName::new(&var_name))
                    .or_default()
                    .push(vec![negated]);
            }
        } else {
            // A union assertion (`@psalm-assert-if-true A|B $x`) is one OR
            // group — Psalm's processCustomAssertion files the whole orred rule
            // as a single `$if_types` entry, not AND-ed facts.
            target_clauses.push(create_var_clause(&var_name, &converted_assertions, cond_id));
            target_map
                .entry(var_name.into())
                .or_default()
                .push(converted_assertions);
        }
    }
}

/// Lowercase the `name()` call segments of a var key (scope keys for
/// memoized method calls are lowercased; docblock assertion targets may
/// spell the method differently).
fn canonicalize_call_segments(var_name: &str) -> String {
    var_name
        .split("->")
        .map(|segment| {
            if let Some(call) = segment.strip_suffix("()") {
                format!("{}()", call.to_ascii_lowercase())
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("->")
}

fn find_assertion_param_index(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertion_var_id: StrId,
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

fn assertion_targets_param(assertion_name: &str, param_name: &str) -> bool {
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

fn map_assertion_var_to_argument(
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

pub(crate) fn convert_functionlike_assertion_type(assertion_type: &AssertionType) -> Vec<Assertion> {
    match assertion_type {
        AssertionType::IsType(union) => {
            union
                .types
                .iter()
                // Reserved-word string literals alongside other members
                // ('self'/'static' in ReflectionNamedType::isBuiltin's
                // assertion) drop out of the narrowing in Psalm.
                .filter(|atomic| {
                    union.types.len() == 1
                        || !matches!(
                            atomic,
                            TAtomic::TLiteralString { value }
                                if matches!(value.as_str(), "self" | "static" | "parent")
                        )
                })
                .cloned()
                .map(Assertion::IsType)
                .collect()
        }
        AssertionType::IsEqual(union) | AssertionType::IsLooselyEqual(union) => union
            .types
            .iter()
            .cloned()
            .map(Assertion::IsEqual)
            .collect(),
        AssertionType::IsNotType(union) => union
            .types
            .iter()
            .cloned()
            .map(Assertion::IsNotType)
            .collect(),
        AssertionType::IsNotEqual(union) | AssertionType::IsNotLooselyEqual(union) => union
            .types
            .iter()
            .cloned()
            .map(Assertion::IsNotEqual)
            .collect(),
        AssertionType::Truthy => vec![Assertion::Truthy],
        AssertionType::Falsy => vec![Assertion::Falsy],
        AssertionType::NotNull => vec![Assertion::IsNotType(TAtomic::TNull)],
        AssertionType::NotEmpty => vec![Assertion::Truthy],
    }
}

/// Replaces template references in a docblock assertion's type. The codebase
/// is threaded through so a template-conditional assertion
/// (`@psalm-assert-if-true =(T is '' ? string : non-empty-string) $haystack`)
/// picks its branch from the call's inferred template bounds, the way Psalm's
/// TemplateInferredTypeReplacer resolves conditionals when processing custom
/// assertions.
fn replace_assertion_templates(
    codebase: &pzoom_code_info::CodebaseInfo,
    assertion_type: &AssertionType,
    template_result: &TemplateResult,
) -> AssertionType {
    let replace = |union: &TUnion| {
        function_call_analyzer::replace_templates_in_union_in(
            Some(codebase),
            union,
            template_result,
        )
    };
    match assertion_type {
        AssertionType::IsType(union) => AssertionType::IsType(replace(union)),
        AssertionType::IsEqual(union) => AssertionType::IsEqual(replace(union)),
        AssertionType::IsLooselyEqual(union) => AssertionType::IsLooselyEqual(replace(union)),
        AssertionType::IsNotType(union) => AssertionType::IsNotType(replace(union)),
        AssertionType::IsNotEqual(union) => AssertionType::IsNotEqual(replace(union)),
        AssertionType::IsNotLooselyEqual(union) => {
            AssertionType::IsNotLooselyEqual(replace(union))
        }
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

/// Replaces template references in a functionlike assertion's type and
/// localizes `self`/`static`/`parent` to the call's concrete classes —
/// Psalm's `Possibilities::getUntemplatedCopy`.
pub(crate) fn get_untemplated_copy(
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    assertion_type: &AssertionType,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => {
            AssertionType::IsType(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsEqual(asserted_type) => {
            AssertionType::IsEqual(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsLooselyEqual(asserted_type) => {
            AssertionType::IsLooselyEqual(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotType(asserted_type) => {
            AssertionType::IsNotType(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotEqual(asserted_type) => {
            AssertionType::IsNotEqual(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::IsNotLooselyEqual(asserted_type) => {
            AssertionType::IsNotLooselyEqual(localize_special_class_type_union(codebase, interner,
                &function_call_analyzer::replace_templates_in_union(
                    asserted_type,
                    template_result,
                ),
                self_class_id,
                static_class_id,
                parent_class_id,
            ))
        }
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

fn create_var_clause(var_name: &str, assertions: &[Assertion], cond_id: (u32, u32)) -> Clause {
    // Like Psalm's FormulaGenerator, equality-derived clauses are `generated`
    // (keyed off the first assertion, matching `$orred_types[0]->hasEquality()`).
    let has_equality = assertions.first().is_some_and(Assertion::has_equality);

    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = AssertionSet::default();
    for assertion in assertions {
        var_possibilities.insert(assertion.to_hash(), assertion.clone());
    }
    possibilities.insert(ClauseKey::Name(VarName::new(var_name)), var_possibilities);

    Clause::new(possibilities, cond_id, cond_id, None, None, Some(has_equality))
}

fn resolve_class_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Identifier(id) => {
            if id.value().eq_ignore_ascii_case("self") {
                return Some(StrId::SELF);
            }
            if id.value().eq_ignore_ascii_case("static") {
                return Some(StrId::STATIC);
            }
            if id.value().eq_ignore_ascii_case("parent") {
                return Some(StrId::PARENT);
            }

            analyzer
                .get_resolved_name(id.start_offset() as u32)
                .or_else(|| Some(analyzer.interner.intern(id.value())))
        }
        Expression::Self_(_) => Some(StrId::SELF),
        Expression::Static(_) => Some(StrId::STATIC),
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }
}

/// Gets the variable name from a variable.
fn get_var_name(var: &Variable<'_>) -> Option<VarName> {
    match var {
        Variable::Direct(direct) => Some(VarName::new(direct.name)),
        _ => None,
    }
}

/// Gets the variable name from an argument.
fn get_argument_var_name(arg: &mago_syntax::ast::ast::argument::Argument<'_>) -> Option<VarName> {
    expression_identifier::get_expression_var_key(arg.value())
        .or_else(|| extract_class_constant_origin_var_name(arg.value()))
}

struct IssetExprAssertions {
    if_true_clauses: Vec<Clause>,
    if_false_clauses: Vec<Clause>,
    if_true: BTreeMap<VarName, Vec<Vec<Assertion>>>,
    if_false: BTreeMap<VarName, Vec<Vec<Assertion>>>,
}

impl IssetExprAssertions {
    fn new() -> Self {
        Self {
            if_true_clauses: Vec::new(),
            if_false_clauses: Vec::new(),
            if_true: BTreeMap::new(),
            if_false: BTreeMap::new(),
        }
    }
}

fn push_assertion(
    branch_clauses: &mut Vec<Clause>,
    branch_map: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    var_name: &str,
    assertion: Assertion,
    cond_id: (u32, u32),
) {
    branch_clauses.push(create_single_var_clause(
        var_name,
        assertion.clone(),
        cond_id,
    ));
    branch_map
        .entry(VarName::new(var_name.strip_prefix('=').unwrap_or(var_name)))
        .or_default()
        .push(vec![assertion]);
}

fn get_isset_assertions_for_expr(
    expr: &Expression<'_>,
    cond_id: (u32, u32),
    analysis_data: &FunctionAnalysisData,
) -> IssetExprAssertions {
    let mut result = IssetExprAssertions::new();

    if let Some(var_name) = expression_identifier::get_expression_var_key(expr) {
        // Mirror Psalm's AssertionFinder: when the isset target is a plain variable
        // whose type is already known, non-mixed and not possibly-undefined, `isset`
        // is purely a null check (`!null`). Otherwise it is a definedness check
        // (`isset`). Psalm emits exactly one of the two — never both — so a `?T`
        // narrows to `T` without a spurious follow-up "never null" redundancy.
        let is_direct_variable = matches!(
            expr.unparenthesized(),
            Expression::Variable(Variable::Direct(_))
        );

        let var_is_defined_non_null_checkable = is_direct_variable && {
            let var_pos = (expr.start_offset() as u32, expr.end_offset() as u32);
            analysis_data.expr_types.get(&var_pos).cloned().is_some_and(|var_type| {
                !var_type.is_mixed()
                    && !var_type.possibly_undefined
                    // A var assigned inside a try (or catch) may be undefined
                    // here even though its type looks settled — Psalm keeps
                    // the `isset` assertion for those
                    // (`!$var_type->possibly_undefined_from_try`).
                    && !var_type.possibly_undefined_from_try
            })
        };

        if var_is_defined_non_null_checkable {
            push_assertion(
                &mut result.if_true_clauses,
                &mut result.if_true,
                &var_name,
                Assertion::IsNotType(TAtomic::TNull),
                cond_id,
            );
            push_assertion(
                &mut result.if_false_clauses,
                &mut result.if_false,
                &var_name,
                Assertion::IsType(TAtomic::TNull),
                cond_id,
            );
        } else {
            push_assertion(
                &mut result.if_true_clauses,
                &mut result.if_true,
                &var_name,
                Assertion::IsIsset,
                cond_id,
            );
            push_assertion(
                &mut result.if_false_clauses,
                &mut result.if_false,
                &var_name,
                Assertion::IsNotIsset,
                cond_id,
            );
        }
    } else {
        // Psalm's AssertionFinder: when the full isset target has no resolvable
        // var id (e.g. a dynamic array key), walk up the array-access chain to
        // the first resolvable prefix and assert `=isset` there. Only the
        // if_true direction exists — `=isset` is an equality assertion whose
        // negation is `Any`, so nothing is learned on the false branch.
        let mut array_root = expr.unparenthesized();
        while let Expression::ArrayAccess(array_access) = array_root {
            array_root = array_access.array.unparenthesized();
            if let Some(var_name) = expression_identifier::get_expression_var_key(array_root) {
                push_assertion(
                    &mut result.if_true_clauses,
                    &mut result.if_true,
                    &var_name,
                    Assertion::IsEqualIsset,
                    cond_id,
                );
                break;
            }
        }
    }

    result
}

fn add_in_array_assertions(
    result: &mut AssertionResult,
    var_name: &str,
    haystack_type: TUnion,
    cond_id: (u32, u32),
) {
    let Some(value_type) = extract_in_array_value_union(&haystack_type) else {
        return;
    };

    if value_type
        .types
        .iter()
        .all(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
    {
        return;
    }

    let in_array_clause =
        create_single_var_clause(var_name, Assertion::InArray(value_type.clone()), cond_id);
    result.if_true_clauses.push(in_array_clause);

    let not_in_array_clause = create_single_var_clause(
        var_name,
        Assertion::NotInArray(value_type.clone()),
        cond_id,
    );
    result.if_false_clauses.push(not_in_array_clause);

    result
        .if_true
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![Assertion::InArray(value_type.clone())]);
    result
        .if_false
        .entry(VarName::new(var_name))
        .or_default()
        .push(vec![Assertion::NotInArray(value_type)]);
}

fn extract_in_array_value_union(haystack_type: &TUnion) -> Option<TUnion> {
    let mut value_union: Option<TUnion> = None;

    for atomic in &haystack_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                merge_in_array_value_union(&mut value_union, value_type);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                for property_type in properties.values() {
                    merge_in_array_value_union(&mut value_union, property_type);
                }

                if let Some(fallback_value_type) = fallback_value_type {
                    merge_in_array_value_union(&mut value_union, fallback_value_type);
                }
            }
            _ => {}
        }
    }

    value_union
}

fn merge_in_array_value_union(target: &mut Option<TUnion>, incoming: &TUnion) {
    let merged = match target.take() {
        Some(existing) => combine_union_types(&existing, incoming, false),
        None => incoming.clone(),
    };

    *target = Some(merged);
}

fn get_array_assertion_from_union(union: &TUnion) -> Option<TAtomic> {
    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for atomic in &union.types {
        match atomic {
            TAtomic::TArray {
                key_type: atomic_key_type,
                value_type: atomic_value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type: atomic_key_type,
                value_type: atomic_value_type,
            } => {
                key_type = Some(match key_type {
                    Some(existing) => combine_union_types(&existing, atomic_key_type, false),
                    None => (**atomic_key_type).clone(),
                });
                value_type = Some(match value_type {
                    Some(existing) => combine_union_types(&existing, atomic_value_type, false),
                    None => (**atomic_value_type).clone(),
                });
            }
            TAtomic::TList {
                value_type: atomic_value_type,
            }
            | TAtomic::TNonEmptyList {
                value_type: atomic_value_type,
            } => {
                key_type = Some(match key_type {
                    Some(existing) => combine_union_types(&existing, &TUnion::int(), false),
                    None => TUnion::int(),
                });
                value_type = Some(match value_type {
                    Some(existing) => combine_union_types(&existing, atomic_value_type, false),
                    None => (**atomic_value_type).clone(),
                });
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                if let Some(fallback_key_type) = fallback_key_type {
                    key_type = Some(match key_type {
                        Some(existing) => combine_union_types(&existing, fallback_key_type, false),
                        None => (**fallback_key_type).clone(),
                    });
                }

                if let Some(fallback_value_type) = fallback_value_type {
                    value_type = Some(match value_type {
                        Some(existing) => {
                            combine_union_types(&existing, fallback_value_type, false)
                        }
                        None => (**fallback_value_type).clone(),
                    });
                }

                for (prop_key, prop_type) in properties.iter() {
                    let prop_key_type = match prop_key {
                        ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
                        ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                            value: value.clone(),
                        }),
                    };

                    key_type = Some(match key_type {
                        Some(existing) => combine_union_types(&existing, &prop_key_type, false),
                        None => prop_key_type,
                    });

                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, prop_type, false),
                        None => prop_type.clone(),
                    });
                }
            }
            TAtomic::TIterable {
                key_type: iterable_key_type,
                value_type: iterable_value_type,
            } => {
                let narrowed_key_type = assertion_reconciler::intersect_union_with_union(
                    iterable_key_type,
                    &TUnion::array_key(),
                )
                .unwrap_or_else(TUnion::array_key);

                key_type = Some(match key_type {
                    Some(existing) => combine_union_types(&existing, &narrowed_key_type, false),
                    None => narrowed_key_type,
                });
                value_type = Some(match value_type {
                    Some(existing) => combine_union_types(&existing, iterable_value_type, false),
                    None => (**iterable_value_type).clone(),
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(template_atomic) = get_array_assertion_from_union(as_type) {
                    if let TAtomic::TArray {
                        key_type: template_key_type,
                        value_type: template_value_type,
                    } = template_atomic
                    {
                        key_type = Some(match key_type {
                            Some(existing) => {
                                combine_union_types(&existing, &template_key_type, false)
                            }
                            None => (*template_key_type).clone(),
                        });
                        value_type = Some(match value_type {
                            Some(existing) => {
                                combine_union_types(&existing, &template_value_type, false)
                            }
                            None => (*template_value_type).clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(TAtomic::TArray {
        key_type: Box::new(key_type.unwrap_or_else(TUnion::array_key)),
        value_type: Box::new(value_type.unwrap_or_else(TUnion::mixed)),
    })
}

fn union_is_definitely_string_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
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
                    | TAtomic::TClassString { .. }
                    | TAtomic::TLiteralClassString { .. }
                    | TAtomic::TTemplateParamClass { .. }
            )
        })
}

fn extract_classlike_from_class_string_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> Option<TAtomic> {
    for atomic in &union.types {
        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => return Some((**as_type).clone()),
            TAtomic::TTemplateParamClass { as_type, .. } => return Some((**as_type).clone()),
            TAtomic::TLiteralClassString { name } => {
                return Some(TAtomic::TNamedObject {
                    name: analyzer.interner.intern(name),
                    type_params: None,
                is_static: false, remapped_params: false });
            }
            _ => {}
        }
    }

    None
}

fn add_array_key_exists_path_assertions(
    result: &mut AssertionResult,
    array_root: &str,
    key_id: &str,
    cond_id: (u32, u32),
) {
    let assertion_var_name = format!("{}[{}]", array_root, key_id);

    let has_key_assertion = Assertion::ArrayKeyExists;
    let not_has_key_assertion = Assertion::ArrayKeyDoesNotExist;

    result.if_true_clauses.push(create_single_var_clause(
        &assertion_var_name,
        has_key_assertion,
        cond_id,
    ));
    result.if_false_clauses.push(create_single_var_clause(
        &assertion_var_name,
        not_has_key_assertion,
        cond_id,
    ));

    result
        .if_true
        .entry(VarName::new(&assertion_var_name))
        .or_default()
        .push(vec![Assertion::ArrayKeyExists]);
    result
        .if_false
        .entry(assertion_var_name.into())
        .or_default()
        .push(vec![Assertion::ArrayKeyDoesNotExist]);
}

fn format_array_key_for_assertion_path(array_key: &ArrayKey) -> String {
    match array_key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => format!("'{}'", value.replace('\'', "\\'")),
    }
}

fn is_simple_array_key_identifier(key_id: &str) -> bool {
    !key_id.contains("->") && !key_id.contains('[')
}

fn extract_array_keys_from_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Vec<ArrayKey> {
    let mut keys = Vec::new();

    if let Some(literal_key) = get_literal_array_key(expr) {
        keys.push(literal_key);
        return keys;
    }

    let expr_pos = get_expr_id(expr);
    let Some(expr_type) = analysis_data.expr_types.get(&expr_pos).cloned() else {
        return keys;
    };

    for atomic in &expr_type.types {
        let array_key = match atomic {
            TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
            TAtomic::TLiteralString { value } => Some(ArrayKey::String(value.clone())),
            TAtomic::TLiteralClassString { name } => Some(ArrayKey::String(name.clone())),
            TAtomic::TClassString { as_type } => {
                if let Some(as_type) = as_type {
                    if let TAtomic::TNamedObject { name, .. } = as_type.as_ref() {
                        Some(ArrayKey::String(
                            analyzer.interner.lookup(*name).to_string(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(array_key) = array_key {
            if !keys.contains(&array_key) {
                keys.push(array_key);
            }
        }
    }

    keys
}

fn extract_array_key_union_for_key_exists(array_union: &TUnion) -> Option<TUnion> {
    let mut key_types = Vec::new();

    for atomic in &array_union.types {
        match atomic {
            TAtomic::TArray { key_type, .. } | TAtomic::TNonEmptyArray { key_type, .. } => {
                for key_atomic in &key_type.types {
                    add_loose_array_key_atomic(&mut key_types, key_atomic);
                }
            }
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => {
                add_loose_array_key_atomic(&mut key_types, &TAtomic::TInt);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                ..
            } => {
                for key in properties.keys() {
                    let key_atomic = match key {
                        ArrayKey::Int(value) => TAtomic::TLiteralInt { value: *value },
                        ArrayKey::String(value) => TAtomic::TLiteralString {
                            value: value.clone(),
                        },
                    };
                    add_loose_array_key_atomic(&mut key_types, &key_atomic);
                }

                if let Some(fallback_key_type) = fallback_key_type {
                    for key_atomic in &fallback_key_type.types {
                        add_loose_array_key_atomic(&mut key_types, key_atomic);
                    }
                }
            }
            _ => {}
        }
    }

    if key_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(key_types))
    }
}

fn add_loose_array_key_atomic(target: &mut Vec<TAtomic>, key_atomic: &TAtomic) {
    match key_atomic {
        TAtomic::TLiteralInt { value } => {
            push_unique_atomic(target, TAtomic::TLiteralInt { value: *value });
            push_unique_atomic(
                target,
                TAtomic::TLiteralString {
                    value: value.to_string(),
                },
            );
        }
        TAtomic::TLiteralString { value } => {
            push_unique_atomic(
                target,
                TAtomic::TLiteralString {
                    value: value.clone(),
                },
            );

            if let Some(int_value) = parse_canonical_int_string(value) {
                push_unique_atomic(target, TAtomic::TLiteralInt { value: int_value });
            }
        }
        TAtomic::TInt => {
            push_unique_atomic(target, TAtomic::TInt);
            push_unique_atomic(target, TAtomic::TString);
        }
        TAtomic::TString => {
            push_unique_atomic(target, TAtomic::TString);
            push_unique_atomic(target, TAtomic::TInt);
        }
        _ => push_unique_atomic(target, key_atomic.clone()),
    }
}

fn push_unique_atomic(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !target.contains(&atomic) {
        target.push(atomic);
    }
}

fn parse_canonical_int_string(value: &str) -> Option<i64> {
    let parsed = value.parse::<i64>().ok()?;

    if value == parsed.to_string() {
        Some(parsed)
    } else {
        None
    }
}

fn get_literal_array_key(expr: &Expression<'_>) -> Option<ArrayKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(ArrayKey::Int),
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                // Match PHP array key juggling for canonical integer strings.
                if value == int_value.to_string() {
                    return ArrayKey::Int(int_value);
                }
            }

            ArrayKey::String(value.to_string())
        }),
        _ => None,
    }
}

fn get_assertable_var_name(expr: &Expression<'_>) -> Option<VarName> {
    if let Some(var_name) = expression_identifier::get_expression_var_key(expr) {
        return Some(var_name);
    }

    if let Some(class_string_origin) = extract_class_constant_origin_var_name(expr) {
        return Some(class_string_origin);
    }

    match expr.unparenthesized() {
        Expression::Assignment(assignment) => {
            // Psalm prefixes assignment-derived keys with `=` so the formula
            // layer knows the fact describes the var's *post-assignment*
            // value (Clause::redefined_vars).
            get_assignment_target_var_name(assignment.lhs)
                .map(|var_name| VarName::from(format!("={var_name}")))
        }
        Expression::Parenthesized(parenthesized) => {
            get_assertable_var_name(parenthesized.expression)
        }
        _ => None,
    }
}

fn get_assignment_target_var_name(expr: &Expression<'_>) -> Option<VarName> {
    expression_identifier::get_expression_var_key(expr)
}

fn collect_assigned_var_names(expr: &Expression<'_>, assigned: &mut FxHashSet<VarName>) {
    match expr.unparenthesized() {
        Expression::Assignment(assignment) => {
            if let Some(var_name) = expression_identifier::get_expression_var_key(assignment.lhs) {
                assigned.insert(var_name);
            }

            collect_assigned_var_names(assignment.rhs, assigned);
        }
        Expression::Binary(binary) => {
            collect_assigned_var_names(binary.lhs, assigned);
            collect_assigned_var_names(binary.rhs, assigned);
        }
        Expression::UnaryPrefix(unary) => {
            collect_assigned_var_names(unary.operand, assigned);
        }
        Expression::Parenthesized(parenthesized) => {
            collect_assigned_var_names(parenthesized.expression, assigned);
        }
        _ => {}
    }
}

fn filter_clauses_for_assigned_vars(clauses: &mut Vec<Clause>, assigned: &FxHashSet<VarName>) {
    clauses.retain_mut(|clause| {
        std::rc::Rc::make_mut(&mut clause.possibilities).retain(|key, _| match key {
            ClauseKey::Name(var_name) => !matches_assigned_var(var_name, assigned),
            ClauseKey::Range(..) => true,
        });

        !clause.possibilities.is_empty()
    });
}

fn filter_assertion_map_for_assigned_vars(
    assertions: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    assigned: &FxHashSet<VarName>,
) {
    assertions.retain(|var_name, _| !matches_assigned_var(var_name, assigned));
}

fn matches_assigned_var(var_name: &str, assigned: &FxHashSet<VarName>) -> bool {
    assigned.iter().any(|assigned_var| {
        var_name == assigned_var
            || var_name.starts_with(&format!("{}[", assigned_var))
            || var_name.starts_with(&format!("{}->", assigned_var))
    })
}

fn merge_assertion_maps(
    target: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    source: BTreeMap<VarName, Vec<Vec<Assertion>>>,
) {
    for (var_name, groups) in source {
        target.entry(var_name).or_default().extend(groups);
    }
}

/// Add only the positive (if-true) type assertion, leaving the false branch unchanged.
/// Used for existence checks (`class_exists` etc.) whose negation must not narrow the
/// variable away from a general `string`.
fn add_positive_only_type_assertion(
    result: &mut AssertionResult,
    var_name: VarName,
    assertion_type: TAtomic,
    cond_id: (u32, u32),
    clause_is_generated: bool,
) {
    let mut is_type_clause = create_single_var_clause(
        &var_name,
        Assertion::IsType(assertion_type.clone()),
        cond_id,
    );
    if clause_is_generated {
        is_type_clause = Clause::new(
            (*is_type_clause.possibilities).clone(),
            is_type_clause.creating_conditional_id,
            is_type_clause.creating_object_id,
            Some(is_type_clause.wedge),
            Some(is_type_clause.reconcilable),
            Some(true),
        );
    }
    result.if_true_clauses.push(is_type_clause);
    result
        .if_true
        .entry(var_name)
        .or_default()
        .push(vec![Assertion::IsType(assertion_type)]);
}

fn add_type_assertions(
    result: &mut AssertionResult,
    var_name: VarName,
    assertion_type: TAtomic,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    add_type_assertions_in(result, var_name, assertion_type, is_positive, cond_id, false)
}

/// [`add_type_assertions`] with the clauses force-marked `generated`. The
/// typed-value comparison path uses this: Psalm's
/// `getTypedValueEqualityAssertions` produces `IsIdentical` assertions there,
/// whose `hasEquality()` makes the clauses generated — exempt from
/// "has already been asserted" redundancy reporting.
fn add_generated_type_assertions(
    result: &mut AssertionResult,
    var_name: VarName,
    assertion_type: TAtomic,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    add_type_assertions_in(result, var_name, assertion_type, is_positive, cond_id, true)
}

fn add_type_assertions_in(
    result: &mut AssertionResult,
    var_name: VarName,
    assertion_type: TAtomic,
    is_positive: bool,
    cond_id: (u32, u32),
    generated: bool,
) {
    let create_clause = |var_name: &VarName, assertion: Assertion| {
        let clause = create_single_var_clause(var_name, assertion, cond_id);
        if generated { clause.mark_generated() } else { clause }
    };
    if is_positive {
        let is_type_clause = create_clause(
            &var_name,
            Assertion::IsType(assertion_type.clone()),
        );
        result.if_true_clauses.push(is_type_clause);

        let is_not_type_clause = create_clause(
            &var_name,
            Assertion::IsNotType(assertion_type.clone()),
        );
        result.if_false_clauses.push(is_not_type_clause);

        result
            .if_true
            .entry(var_name.clone())
            .or_default()
            .push(vec![Assertion::IsType(assertion_type.clone())]);
        result
            .if_false
            .entry(var_name)
            .or_default()
            .push(vec![Assertion::IsNotType(assertion_type)]);
    } else {
        let is_not_type_clause = create_clause(
            &var_name,
            Assertion::IsNotType(assertion_type.clone()),
        );
        result.if_true_clauses.push(is_not_type_clause);

        let is_type_clause = create_clause(
            &var_name,
            Assertion::IsType(assertion_type.clone()),
        );
        result.if_false_clauses.push(is_type_clause);

        result
            .if_true
            .entry(var_name.clone())
            .or_default()
            .push(vec![Assertion::IsNotType(assertion_type.clone())]);
        result
            .if_false
            .entry(var_name)
            .or_default()
            .push(vec![Assertion::IsType(assertion_type)]);
    }
}

/// Like [`add_type_assertions`] but with equality assertions
/// (`IsEqual`/`IsNotEqual`), matching Psalm's `IsIdentical`/`IsNotIdentical`.
fn add_equality_assertions(
    result: &mut AssertionResult,
    var_name: VarName,
    assertion_type: TAtomic,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    let (true_assertion, false_assertion) = if is_positive {
        (
            Assertion::IsEqual(assertion_type.clone()),
            Assertion::IsNotEqual(assertion_type),
        )
    } else {
        (
            Assertion::IsNotEqual(assertion_type.clone()),
            Assertion::IsEqual(assertion_type),
        )
    };

    let true_clause = create_single_var_clause(&var_name, true_assertion.clone(), cond_id);
    result.if_true_clauses.push(true_clause);

    let false_clause = create_single_var_clause(&var_name, false_assertion.clone(), cond_id);
    result.if_false_clauses.push(false_clause);

    result
        .if_true
        .entry(var_name.clone())
        .or_default()
        .push(vec![true_assertion]);
    result
        .if_false
        .entry(var_name)
        .or_default()
        .push(vec![false_assertion]);
}

/// Detects `get_class($x) === <expr typed class-string<T>>` and returns the
/// origin variable of `$x` together with the template parameter `T` it should
/// be narrowed to.
fn get_get_class_template_comparison(
    analyzer: &StatementsAnalyzer<'_>,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<(VarName, TAtomic)> {
    if let Some(var_name) = extract_get_class_origin_var_name(left_expr)
        && let Some(template_atomic) =
            class_string_template_atomic(analyzer, right_expr, analysis_data)
    {
        return Some((var_name, template_atomic));
    }

    if let Some(var_name) = extract_get_class_origin_var_name(right_expr)
        && let Some(template_atomic) =
            class_string_template_atomic(analyzer, left_expr, analysis_data)
    {
        return Some((var_name, template_atomic));
    }

    None
}

/// If `expr` has type `class-string<T>` for some template parameter `T`, returns
/// that template parameter as an instance type (the `T` an instance would have).
fn class_string_template_atomic(
    _analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<TAtomic> {
    let expr_type = analysis_data.expr_types.get(&get_expr_id(expr)).cloned()?;

    for atomic in &expr_type.types {
        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } if matches!(as_type.as_ref(), TAtomic::TTemplateParam { .. }) => {
                return Some((**as_type).clone());
            }
            TAtomic::TTemplateParamClass {
                name,
                defining_entity,
                as_type,
            } => {
                return Some(TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(TUnion::new((**as_type).clone())),
                });
            }
            _ => {}
        }
    }

    None
}

fn get_get_class_comparison(
    analyzer: &StatementsAnalyzer<'_>,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
) -> Option<(VarName, StrId)> {
    if let (Some(var_name), Some(class_id)) = (
        extract_get_class_origin_var_name(left_expr)
            .or_else(|| extract_class_constant_origin_var_name(left_expr)),
        extract_class_constant_id(analyzer, right_expr),
    ) {
        return Some((var_name, class_id));
    }

    if let (Some(var_name), Some(class_id)) = (
        extract_get_class_origin_var_name(right_expr)
            .or_else(|| extract_class_constant_origin_var_name(right_expr)),
        extract_class_constant_id(analyzer, left_expr),
    ) {
        return Some((var_name, class_id));
    }

    None
}

fn get_class_string_var_comparison(
    analyzer: &StatementsAnalyzer<'_>,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
) -> Option<(VarName, StrId)> {
    if let (Some(var_name), Some(class_id)) = (
        get_assertable_var_name(left_expr),
        extract_class_constant_id(analyzer, right_expr),
    ) {
        if var_name == "@static" || var_name == "@parent" {
            return None;
        }
        return Some((var_name, class_id));
    }

    if let (Some(var_name), Some(class_id)) = (
        get_assertable_var_name(right_expr),
        extract_class_constant_id(analyzer, left_expr),
    ) {
        if var_name == "@static" || var_name == "@parent" {
            return None;
        }
        return Some((var_name, class_id));
    }

    None
}

fn extract_get_class_origin_var_name(expr: &Expression<'_>) -> Option<VarName> {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    if !function_name.value().eq_ignore_ascii_case("get_class") {
        return None;
    }

    let first_arg = function_call.argument_list.arguments.first()?;
    expression_identifier::get_expression_var_key(first_arg.value())
}

fn extract_class_constant_origin_var_name(expr: &Expression<'_>) -> Option<VarName> {
    let Expression::Access(Access::ClassConstant(ClassConstantAccess {
        class,
        constant: ClassLikeConstantSelector::Identifier(constant),
        ..
    })) = expr.unparenthesized()
    else {
        return None;
    };

    if !constant.value.eq_ignore_ascii_case("class") {
        return None;
    }

    if let Some(var_name) = expression_identifier::get_expression_var_key(class) {
        return Some(var_name);
    }

    match class.unparenthesized() {
        Expression::Static(_) | Expression::Self_(_) => Some(VarName::new_static("@static")),
        Expression::Parent(_) => Some(VarName::new_static("@parent")),
        _ => None,
    }
}

fn extract_class_constant_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    let Expression::Access(Access::ClassConstant(ClassConstantAccess {
        class,
        constant: ClassLikeConstantSelector::Identifier(constant),
        ..
    })) = expr.unparenthesized()
    else {
        return None;
    };

    if !constant.value.eq_ignore_ascii_case("class") {
        return None;
    }

    resolve_class_expression(analyzer, class)
}

fn function_exists_assertion_key(function_name: &str) -> VarName {
    format!("@function_exists({})", function_name)
    .into()
}

pub(crate) fn method_exists_assertion_key(class_name_or_var: &str, method_name: &str) -> VarName {
    format!(
        "@method_exists({},{})",
        class_name_or_var
            .trim_start_matches('\\')
            .to_ascii_lowercase(),
        method_name.to_ascii_lowercase()
    )
    .into()
}

fn class_exists_assertion_key(class_name: &str) -> VarName {
    format!(
        "@class_exists({})",
        class_name.trim_start_matches('\\').to_ascii_lowercase()
    )
    .into()
}

fn extract_literal_string_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    let mut normalized = string_lit.value?.to_string();
    if !normalized.contains('\\') {
        let span = string_lit.span();
        let raw_literal = analyzer.get_source_substring(span.start.offset as usize, span.end.offset as usize);
        if let Some(raw_inner) = strip_wrapping_quotes(raw_literal.trim()) && raw_inner.contains('\\') {
            normalized = raw_inner.to_string();
        }
    }

    Some(normalized.trim_start_matches('\\').to_ascii_lowercase())
}

fn strip_wrapping_quotes(raw: &str) -> Option<&str> {
    if raw.len() < 2 {
        return None;
    }

    let first = raw.as_bytes()[0] as char;
    let last = raw.as_bytes()[raw.len() - 1] as char;
    if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
        Some(&raw[1..raw.len() - 1])
    } else {
        None
    }
}

fn extract_literal_function_name(expr: &Expression<'_>) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    let value = string_lit.value?;
    Some(value.trim_start_matches('\\').to_ascii_lowercase())
}

fn resolve_class_string_arg(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<StrId> {
    if let Some(class_id) = extract_class_constant_id(analyzer, expr) {
        return Some(class_id);
    }

    if let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() {
        return string_lit
            .value
            .map(|value| analyzer.interner.intern(value.trim_start_matches('\\')));
    }

    resolve_class_expression(analyzer, expr)
}

/// Checks if an expression is null.
fn is_null_expr(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Literal(Literal::Null(_)))
}

fn get_literal_boolean(expr: &Expression<'_>) -> Option<bool> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::True(_)) => Some(true),
        Expression::Literal(Literal::False(_)) => Some(false),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum CountCmpOp {
    Lt,
    Le,
    Gt,
    Ge,
}

fn get_empty_string_literal_comparison_target<'a>(
    left: &'a Expression<'_>,
    right: &'a Expression<'_>,
) -> Option<&'a Expression<'a>> {
    if is_empty_string_literal(right) {
        return Some(left);
    }
    if is_empty_string_literal(left) {
        return Some(right);
    }
    None
}

fn get_empty_array_literal_comparison_target<'a>(
    left: &'a Expression<'_>,
    right: &'a Expression<'_>,
) -> Option<&'a Expression<'a>> {
    if is_empty_array_literal(right) {
        return Some(left);
    }
    if is_empty_array_literal(left) {
        return Some(right);
    }
    None
}

fn is_empty_string_literal(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Literal(Literal::String(string_lit)) => {
            string_lit.value.is_some_and(|value| value.is_empty())
        }
        _ => false,
    }
}

fn is_empty_array_literal(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Array(array) => array.elements.is_empty(),
        Expression::LegacyArray(array) => array.elements.is_empty(),
        _ => false,
    }
}

fn get_count_literal_comparison(
    left: &Expression<'_>,
    right: &Expression<'_>,
) -> Option<(VarName, usize)> {
    if let (Some(count_arg), Some(count)) =
        (get_count_arg_expression(left), get_usize_literal(right))
    {
        let var_name = get_assertable_var_name(count_arg)?;
        return Some((var_name, count));
    }

    if let (Some(count_arg), Some(count)) =
        (get_count_arg_expression(right), get_usize_literal(left))
    {
        let var_name = get_assertable_var_name(count_arg)?;
        return Some((var_name, count));
    }

    None
}

fn get_count_inequality_comparison(binary: &Binary<'_>) -> Option<(VarName, usize, CountCmpOp)> {
    if let (Some(count_arg), Some(count), Some(operator)) = (
        get_count_arg_expression(binary.lhs),
        get_usize_literal(binary.rhs),
        map_binary_to_count_op(&binary.operator),
    ) {
        let var_name = get_assertable_var_name(count_arg)?;
        return Some((var_name, count, operator));
    }

    if let (Some(count_arg), Some(count), Some(operator)) = (
        get_count_arg_expression(binary.rhs),
        get_usize_literal(binary.lhs),
        map_binary_to_count_op(&binary.operator).map(reverse_count_op),
    ) {
        let var_name = get_assertable_var_name(count_arg)?;
        return Some((var_name, count, operator));
    }

    None
}

fn get_int_inequality_comparison(binary: &Binary<'_>) -> Option<(VarName, i64, CountCmpOp)> {
    if let (Some(var_name), Some(value), Some(operator)) = (
        get_assertable_var_name(binary.lhs),
        get_i64_literal(binary.rhs),
        map_binary_to_count_op(&binary.operator),
    ) {
        return Some((var_name, value, operator));
    }

    if let (Some(var_name), Some(value), Some(operator)) = (
        get_assertable_var_name(binary.rhs),
        get_i64_literal(binary.lhs),
        map_binary_to_count_op(&binary.operator).map(reverse_count_op),
    ) {
        return Some((var_name, value, operator));
    }

    None
}

fn get_count_arg_expression<'a>(expr: &'a Expression<'_>) -> Option<&'a Expression<'a>> {
    let Expression::Call(Call::Function(func_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = func_call.function.unparenthesized() else {
        return None;
    };

    let normalized_name = function_name
        .value()
        .strip_prefix('\\')
        .unwrap_or(function_name.value())
        .to_ascii_lowercase();

    if normalized_name != "count" {
        return None;
    }

    func_call
        .argument_list
        .arguments
        .first()
        .filter(|arg| !arg.is_unpacked())
        .map(|arg| arg.value())
}

fn get_usize_literal(expr: &Expression<'_>) -> Option<usize> {
    let Expression::Literal(Literal::Integer(int_lit)) = expr.unparenthesized() else {
        return None;
    };
    let value = int_lit.value?;
    usize::try_from(value).ok()
}

fn get_i64_literal(expr: &Expression<'_>) -> Option<i64> {
    let Expression::Literal(Literal::Integer(int_lit)) = expr.unparenthesized() else {
        return None;
    };

    int_lit.value.and_then(|value| i64::try_from(value).ok())
}

fn map_binary_to_count_op(operator: &BinaryOperator<'_>) -> Option<CountCmpOp> {
    match operator {
        BinaryOperator::LessThan(_) => Some(CountCmpOp::Lt),
        BinaryOperator::LessThanOrEqual(_) => Some(CountCmpOp::Le),
        BinaryOperator::GreaterThan(_) => Some(CountCmpOp::Gt),
        BinaryOperator::GreaterThanOrEqual(_) => Some(CountCmpOp::Ge),
        _ => None,
    }
}

fn reverse_count_op(operator: CountCmpOp) -> CountCmpOp {
    match operator {
        CountCmpOp::Lt => CountCmpOp::Gt,
        CountCmpOp::Le => CountCmpOp::Ge,
        CountCmpOp::Gt => CountCmpOp::Lt,
        CountCmpOp::Ge => CountCmpOp::Le,
    }
}

fn get_literal_assertion_type(expr: &Expression<'_>) -> Option<TAtomic> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Null(_)) => Some(TAtomic::TNull),
        Expression::Literal(Literal::True(_)) => Some(TAtomic::TTrue),
        Expression::Literal(Literal::False(_)) => Some(TAtomic::TFalse),
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(|value| TAtomic::TLiteralInt { value }),
        Expression::Literal(Literal::Float(float_lit)) => Some(TAtomic::TLiteralFloat {
            value: float_lit.value.into_inner(),
        }),
        Expression::Literal(Literal::String(string_lit)) => {
            string_lit.value.map(|value| TAtomic::TLiteralString {
                value: value.to_string(),
            })
        }
        _ => None,
    }
}

fn get_expression_assertion_type(
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<TAtomic> {
    if let Some(literal_assertion_type) = get_literal_assertion_type(expr) {
        return Some(literal_assertion_type);
    }

    let expr_type = analysis_data.expr_types.get(&get_expr_id(expr)).cloned()?;
    if !expr_type.is_single() {
        return None;
    }

    match expr_type.get_single()? {
        TAtomic::TNull => Some(TAtomic::TNull),
        TAtomic::TTrue => Some(TAtomic::TTrue),
        TAtomic::TFalse => Some(TAtomic::TFalse),
        TAtomic::TLiteralInt { value } => Some(TAtomic::TLiteralInt { value: *value }),
        TAtomic::TLiteralFloat { value } => Some(TAtomic::TLiteralFloat { value: *value }),
        TAtomic::TLiteralString { value } => Some(TAtomic::TLiteralString {
            value: value.clone(),
        }),
        TAtomic::TEnumCase {
            enum_name,
            case_name,
        } => Some(TAtomic::TEnumCase {
            enum_name: *enum_name,
            case_name: *case_name,
        }),
        _ => None,
    }
}
