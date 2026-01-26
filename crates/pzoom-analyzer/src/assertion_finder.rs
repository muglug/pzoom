//! Assertion finder module.
//!
//! This module extracts type assertions from conditional expressions.
//! For example, `$x instanceof Foo` generates an `IsType(TNamedObject(Foo))` assertion.
//!
//! It builds CNF (Conjunctive Normal Form) clauses that can be used for
//! type algebra simplification.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::{Binary, BinaryOperator};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::algebra::{combine_ored_clauses, Clause, ClauseKey};
use pzoom_code_info::{Assertion, TAtomic, TUnion};

use crate::statements_analyzer::StatementsAnalyzer;

/// Result of assertion extraction.
pub struct AssertionResult {
    /// Clauses that are true when the expression is true (CNF formula).
    pub if_true_clauses: Vec<Clause>,
    /// Clauses that are true when the expression is false (CNF formula).
    pub if_false_clauses: Vec<Clause>,
    /// Assertions that are true when the expression is true (flat map for compatibility).
    pub if_true: BTreeMap<String, Vec<Assertion>>,
    /// Assertions that are true when the expression is false (flat map for compatibility).
    pub if_false: BTreeMap<String, Vec<Assertion>>,
}

impl AssertionResult {
    pub fn new() -> Self {
        Self {
            if_true_clauses: Vec::new(),
            if_false_clauses: Vec::new(),
            if_true: BTreeMap::new(),
            if_false: BTreeMap::new(),
        }
    }
}

/// Extracts type assertions from a conditional expression.
pub fn get_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> AssertionResult {
    let cond_id = get_expr_id(expr);
    get_assertions_inner(analyzer, expr, cond_id)
}

fn get_expr_id(expr: &Expression<'_>) -> (u32, u32) {
    (expr.start_offset() as u32, expr.end_offset() as u32)
}

fn get_assertions_inner(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    cond_id: (u32, u32),
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match expr {
        Expression::Call(call) => {
            get_function_call_assertions(analyzer, call, &mut result, cond_id);
        }
        Expression::Binary(binary) => {
            return get_binary_assertions(analyzer, binary, cond_id);
        }
        Expression::UnaryPrefix(unary) => {
            return get_unary_assertions(analyzer, unary, cond_id);
        }
        Expression::Variable(var) => {
            if let Some(var_name) = get_var_name(var) {
                // Create a clause for the true branch: $x is truthy
                let truthy_clause = create_single_var_clause(
                    &var_name,
                    Assertion::Truthy,
                    cond_id,
                );
                result.if_true_clauses.push(truthy_clause);

                // Create a clause for the false branch: $x is falsy
                let falsy_clause = create_single_var_clause(
                    &var_name,
                    Assertion::Falsy,
                    cond_id,
                );
                result.if_false_clauses.push(falsy_clause);

                // Also populate flat maps for compatibility
                result.if_true
                    .entry(var_name.clone())
                    .or_default()
                    .push(Assertion::Truthy);
                result.if_false
                    .entry(var_name)
                    .or_default()
                    .push(Assertion::Falsy);
            }
        }
        Expression::Parenthesized(paren) => {
            return get_assertions_inner(analyzer, paren.expression, cond_id);
        }
        _ => {}
    }

    result
}

/// Creates a clause with a single variable and assertion.
fn create_single_var_clause(var_name: &str, assertion: Assertion, cond_id: (u32, u32)) -> Clause {
    let mut possibilities = BTreeMap::new();
    let mut var_possibilities = IndexMap::new();
    var_possibilities.insert(assertion.to_hash(), assertion);
    possibilities.insert(ClauseKey::Name(var_name.to_string()), var_possibilities);

    Clause::new(possibilities, cond_id, cond_id, None, None, None)
}

/// Extracts assertions from a function call (e.g., is_string($x), isset($x)).
fn get_function_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
) {
    // Handle different call types
    match call {
        Call::Function(func_call) => {
            // Get the function name
            let func_name = match &func_call.function {
                Expression::Identifier(id) => Some(id.value()),
                _ => None,
            };

            let Some(func_name) = func_name else {
                return;
            };

            // Handle isset() specially
            if func_name == "isset" {
                for arg in func_call.argument_list.arguments.iter() {
                    if let Some(var_name) = get_argument_var_name(arg) {
                        let isset_clause = create_single_var_clause(
                            &var_name,
                            Assertion::IsIsset,
                            cond_id,
                        );
                        result.if_true_clauses.push(isset_clause);

                        let not_isset_clause = create_single_var_clause(
                            &var_name,
                            Assertion::IsNotIsset,
                            cond_id,
                        );
                        result.if_false_clauses.push(not_isset_clause);

                        result.if_true
                            .entry(var_name.clone())
                            .or_default()
                            .push(Assertion::IsIsset);
                        result.if_false
                            .entry(var_name)
                            .or_default()
                            .push(Assertion::IsNotIsset);
                    }
                }
                return;
            }

            // Check if it's a type-checking function
            let (assertion_type, is_narrowing) = match func_name {
                "is_string" => (Some(TAtomic::TString), true),
                "is_int" | "is_integer" | "is_long" => (Some(TAtomic::TInt), true),
                "is_float" | "is_double" | "is_real" => (Some(TAtomic::TFloat), true),
                "is_bool" => (Some(TAtomic::TBool), true),
                "is_array" => (Some(TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                }), true),
                "is_object" => (Some(TAtomic::TObject), true),
                "is_null" => (Some(TAtomic::TNull), true),
                "is_numeric" => (Some(TAtomic::TNumeric), true),
                "is_callable" => (Some(TAtomic::TCallable {
                    params: None,
                    return_type: None,
                }), true),
                "is_resource" => (Some(TAtomic::TResource), true),
                "is_scalar" => (Some(TAtomic::TScalar), true),
                "is_iterable" => (Some(TAtomic::TIterable {
                    key_type: Box::new(TUnion::mixed()),
                    value_type: Box::new(TUnion::mixed()),
                }), true),
                _ => (None, false),
            };

            if !is_narrowing {
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

            result.if_true
                .entry(var_name.clone())
                .or_default()
                .push(Assertion::IsType(assertion_type.clone()));
            result.if_false
                .entry(var_name)
                .or_default()
                .push(Assertion::IsNotType(assertion_type));
        }
        _ => {}
    }
}

/// Extracts assertions from a binary expression (&&, ||, ===, !==, instanceof).
fn get_binary_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    binary: &Binary<'_>,
    cond_id: (u32, u32),
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match &binary.operator {
        BinaryOperator::Instanceof(_) => {
            get_instanceof_assertions(analyzer, binary, &mut result, cond_id);
        }
        BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
            // For &&, both sides must be true in the true branch
            // CNF: left_true AND right_true
            let left_result = get_assertions_inner(analyzer, binary.lhs, cond_id);
            let right_result = get_assertions_inner(analyzer, binary.rhs, cond_id);

            // True branch: combine all clauses (AND)
            result.if_true_clauses.extend(left_result.if_true_clauses);
            result.if_true_clauses.extend(right_result.if_true_clauses);

            // False branch: negate of (left AND right) = !left OR !right
            // This is handled by combine_ored_clauses on the negated formulas
            if !left_result.if_false_clauses.is_empty() && !right_result.if_false_clauses.is_empty() {
                if let Ok(combined) = combine_ored_clauses(
                    left_result.if_false_clauses,
                    right_result.if_false_clauses,
                    cond_id,
                ) {
                    result.if_false_clauses = combined;
                }
            } else if !left_result.if_false_clauses.is_empty() {
                result.if_false_clauses = left_result.if_false_clauses;
            } else {
                result.if_false_clauses = right_result.if_false_clauses;
            }

            // Flat map compatibility
            for (var, assertions) in left_result.if_true {
                result.if_true.entry(var).or_default().extend(assertions);
            }
            for (var, assertions) in right_result.if_true {
                result.if_true.entry(var).or_default().extend(assertions);
            }
        }
        BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
            // For ||, at least one side must be true
            // CNF: need to convert (left OR right) to CNF
            let left_result = get_assertions_inner(analyzer, binary.lhs, cond_id);
            let right_result = get_assertions_inner(analyzer, binary.rhs, cond_id);

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
            } else if !left_result.if_true_clauses.is_empty() {
                result.if_true_clauses = left_result.if_true_clauses;
            } else {
                result.if_true_clauses = right_result.if_true_clauses;
            }

            // False branch: both must be false
            result.if_false_clauses.extend(left_result.if_false_clauses);
            result.if_false_clauses.extend(right_result.if_false_clauses);

            // Flat map compatibility
            for (var, assertions) in left_result.if_false {
                result.if_false.entry(var).or_default().extend(assertions);
            }
            for (var, assertions) in right_result.if_false {
                result.if_false.entry(var).or_default().extend(assertions);
            }
        }
        BinaryOperator::Identical(_) => {
            get_equality_assertions(binary, &mut result, true, cond_id);
        }
        BinaryOperator::NotIdentical(_) => {
            get_equality_assertions(binary, &mut result, false, cond_id);
        }
        BinaryOperator::Equal(_) => {
            get_equality_assertions(binary, &mut result, true, cond_id);
        }
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => {
            get_equality_assertions(binary, &mut result, false, cond_id);
        }
        _ => {}
    }

    result
}

/// Extracts assertions from instanceof expressions.
fn get_instanceof_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    cond_id: (u32, u32),
) {
    let var_name = match binary.lhs {
        Expression::Variable(var) => get_var_name(var),
        _ => None,
    };

    let Some(var_name) = var_name else {
        return;
    };

    // Get the class name from the right side
    let class_name = match binary.rhs {
        Expression::Identifier(id) => Some(id.value()),
        _ => None,
    };

    let Some(class_name) = class_name else {
        return;
    };

    let class_id = analyzer.interner.intern(class_name);
    let assertion_type = TAtomic::TNamedObject {
        name: class_id,
        type_params: None,
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

    result.if_true
        .entry(var_name.clone())
        .or_default()
        .push(Assertion::IsType(assertion_type.clone()));
    result.if_false
        .entry(var_name)
        .or_default()
        .push(Assertion::IsNotType(assertion_type));
}

/// Extracts assertions from equality comparisons.
fn get_equality_assertions(
    binary: &Binary<'_>,
    result: &mut AssertionResult,
    is_positive: bool,
    cond_id: (u32, u32),
) {
    // Check for $x === null or null === $x patterns
    let (var_expr, _value_expr) = if is_null_expr(binary.rhs) {
        (binary.lhs, binary.rhs)
    } else if is_null_expr(binary.lhs) {
        (binary.rhs, binary.lhs)
    } else {
        return;
    };

    let var_name = match var_expr {
        Expression::Variable(var) => get_var_name(var),
        _ => None,
    };

    let Some(var_name) = var_name else {
        return;
    };

    if is_positive {
        // $x === null means $x is null
        let is_null_clause = create_single_var_clause(
            &var_name,
            Assertion::IsType(TAtomic::TNull),
            cond_id,
        );
        result.if_true_clauses.push(is_null_clause);

        let not_null_clause = create_single_var_clause(
            &var_name,
            Assertion::IsNotType(TAtomic::TNull),
            cond_id,
        );
        result.if_false_clauses.push(not_null_clause);

        result.if_true
            .entry(var_name.clone())
            .or_default()
            .push(Assertion::IsType(TAtomic::TNull));
        result.if_false
            .entry(var_name)
            .or_default()
            .push(Assertion::IsNotType(TAtomic::TNull));
    } else {
        // $x !== null means $x is not null
        let not_null_clause = create_single_var_clause(
            &var_name,
            Assertion::IsNotType(TAtomic::TNull),
            cond_id,
        );
        result.if_true_clauses.push(not_null_clause);

        let is_null_clause = create_single_var_clause(
            &var_name,
            Assertion::IsType(TAtomic::TNull),
            cond_id,
        );
        result.if_false_clauses.push(is_null_clause);

        result.if_true
            .entry(var_name.clone())
            .or_default()
            .push(Assertion::IsNotType(TAtomic::TNull));
        result.if_false
            .entry(var_name)
            .or_default()
            .push(Assertion::IsType(TAtomic::TNull));
    }
}

/// Extracts assertions from a unary prefix expression (!$x).
fn get_unary_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    cond_id: (u32, u32),
) -> AssertionResult {
    let mut result = AssertionResult::new();

    match &unary.operator {
        UnaryPrefixOperator::Not(_) => {
            // !expr swaps if_true and if_false
            let inner_result = get_assertions_inner(analyzer, unary.operand, cond_id);

            result.if_true_clauses = inner_result.if_false_clauses;
            result.if_false_clauses = inner_result.if_true_clauses;
            result.if_true = inner_result.if_false;
            result.if_false = inner_result.if_true;
        }
        _ => {}
    }

    result
}

/// Gets the variable name from a variable.
fn get_var_name(var: &Variable<'_>) -> Option<String> {
    match var {
        Variable::Direct(direct) => Some(direct.name.to_string()),
        _ => None,
    }
}

/// Gets the variable name from an argument.
fn get_argument_var_name(arg: &mago_syntax::ast::ast::argument::Argument<'_>) -> Option<String> {
    let expr = arg.value();
    match expr {
        Expression::Variable(var) => get_var_name(var),
        _ => None,
    }
}

/// Checks if an expression is null.
fn is_null_expr(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Literal(Literal::Null(_)))
}
