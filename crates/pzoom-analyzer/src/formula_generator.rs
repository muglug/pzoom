//! Turns a conditional expression into a CNF formula (a list of [`Clause`]s).
//!
//! This is a faithful port of Psalm's `FormulaGenerator` / Hakana's
//! `formula_generator`: the logical structure of a condition (`&&`, `||`, `!`) is
//! decomposed here, and only the leaf (atomic) conditions are handed to the
//! assertion scraper. The function layout mirrors Hakana's:
//!
//! * [`get_formula`] dispatches on the condition shape.
//! * [`handle_binop`] routes `&&` to [`handle_and`] and `||` to [`handle_or`].
//! * [`handle_and`] conjoins the two sub-formulae (clause concatenation).
//! * [`handle_or`] distributes them via [`combine_ored_clauses`].
//! * [`handle_uop`] negates `!` sub-formulae, applying De Morgan for `!(a && b)`
//!   and `!(a || b)` and [`negate_formula`] otherwise.
//!
//! Leaf conditions are scraped by [`assertion_finder`] (pzoom's equivalent of
//! Hakana's `assertion_finder::scrape_assertions`); when a leaf yields no
//! assertions we fall back to a single range-keyed `Truthy` clause, exactly as
//! Hakana does.
//!
//! The `inside_negation` flag is threaded through the recursion (flipped by `!`)
//! for parity with Hakana. pzoom does not yet maintain a formula cache, so the
//! `cache` parameter present in Hakana is omitted.

use std::collections::BTreeMap;

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use rustc_hash::FxHashSet;

use pzoom_code_info::Assertion;
use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{
    Clause, ClauseKey, combine_ored_clauses, get_truths_from_formula, negate_formula, simplify_cnf,
};

use crate::assertion_finder;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

fn span_id(expr: &Expression<'_>) -> (u32, u32) {
    (expr.start_offset() as u32, expr.end_offset() as u32)
}

/// The narrowing truths (`var -> AND of OR-groups`) implied when `conditional` is
/// truthy. This is the formula-based replacement for `assertion_finder`'s flat
/// `if_true` map, suitable for passing straight to
/// [`crate::reconciler::reconcile_keyed_types`].
pub fn get_true_assertions(
    conditional: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
) -> BTreeMap<VarName, Vec<Vec<Assertion>>> {
    let cond_id = span_id(conditional);
    let clauses =
        get_formula(cond_id, cond_id, conditional, analyzer, analysis_data, false).unwrap_or_default();
    truths_for_clauses(clauses, Some(cond_id))
}

/// The narrowing truths implied when `conditional` is falsy (its formula negated).
/// Formula-based replacement for `assertion_finder`'s flat `if_false` map.
pub fn get_false_assertions(
    conditional: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
) -> BTreeMap<VarName, Vec<Vec<Assertion>>> {
    let cond_id = span_id(conditional);
    let clauses =
        get_formula(cond_id, cond_id, conditional, analyzer, analysis_data, false).unwrap_or_default();
    let negated = negate_formula(clauses).unwrap_or_default();
    truths_for_clauses(negated, None)
}

fn truths_for_clauses(
    clauses: Vec<Clause>,
    creating_conditional_id: Option<(u32, u32)>,
) -> BTreeMap<VarName, Vec<Vec<Assertion>>> {
    if clauses.is_empty() {
        return BTreeMap::new();
    }
    let simplified = simplify_cnf(clauses.iter().collect());
    let mut referenced = FxHashSet::default();
    let (truths, _active) = get_truths_from_formula(
        simplified.iter().collect(),
        creating_conditional_id,
        &mut referenced,
    );
    truths
}

/// Get the CNF formula (clauses that hold when the condition is true) for
/// `conditional`.
///
/// `conditional_object_id` tags the produced clauses with the originating
/// conditional (used by [`combine_ored_clauses`]); `creating_object_id` mirrors
/// Hakana's parameter and identifies the sub-expression currently being scraped.
pub fn get_formula(
    conditional_object_id: (u32, u32),
    creating_object_id: (u32, u32),
    conditional: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    let conditional = conditional.unparenthesized();

    if let Expression::Binary(binary) = conditional {
        if let Some(clauses) = handle_binop(
            conditional_object_id,
            &binary.operator,
            binary.lhs,
            binary.rhs,
            analyzer,
            analysis_data,
            inside_negation,
        ) {
            return clauses;
        }
    }

    if let Expression::UnaryPrefix(unary) = conditional {
        if let Some(clauses) = handle_uop(
            conditional_object_id,
            &unary.operator,
            unary.operand,
            analyzer,
            analysis_data,
            inside_negation,
        ) {
            return clauses;
        }
    }

    // Leaf (atomic) condition. pzoom's assertion scraper already produces clause
    // form for atomic conditions, so reuse it.
    let leaf_clauses = assertion_finder::get_assertions(analyzer, conditional, analysis_data)
        .if_true_clauses;

    if !leaf_clauses.is_empty() {
        return Ok(leaf_clauses);
    }

    // No assertions: the condition is simply "this expression is truthy",
    // keyed by its source range (mirrors Hakana's fallback clause).
    let (start, end) = span_id(conditional);
    let mut possibilities = BTreeMap::new();
    let mut orred = pzoom_code_info::AssertionSet::default();
    orred.insert(Assertion::Truthy.to_hash(), Assertion::Truthy);
    possibilities.insert(ClauseKey::Range(start, end), orred);

    let _ = creating_object_id;

    Ok(vec![Clause::new(
        possibilities,
        conditional_object_id,
        conditional_object_id,
        None,
        None,
        None,
    )])
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn handle_binop(
    conditional_object_id: (u32, u32),
    operator: &BinaryOperator<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
    inside_negation: bool,
) -> Option<Result<Vec<Clause>, String>> {
    match operator {
        BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => Some(handle_and(
            conditional_object_id,
            left,
            right,
            analyzer,
            analysis_data,
            inside_negation,
        )),
        BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => Some(handle_or(
            conditional_object_id,
            left,
            right,
            analyzer,
            analysis_data,
            inside_negation,
        )),
        _ => None,
    }
}

#[inline]
fn handle_and(
    conditional_object_id: (u32, u32),
    left: &Expression<'_>,
    right: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    let mut left_clauses = get_formula(
        conditional_object_id,
        span_id(left),
        left,
        analyzer,
        analysis_data,
        inside_negation,
    )?;

    let right_clauses = get_formula(
        conditional_object_id,
        span_id(right),
        right,
        analyzer,
        analysis_data,
        inside_negation,
    )?;

    left_clauses.extend(right_clauses);

    Ok(left_clauses)
}

#[inline]
fn handle_or(
    conditional_object_id: (u32, u32),
    left: &Expression<'_>,
    right: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    let left_clauses = get_formula(
        conditional_object_id,
        span_id(left),
        left,
        analyzer,
        analysis_data,
        inside_negation,
    )?;

    let right_clauses = get_formula(
        conditional_object_id,
        span_id(right),
        right,
        analyzer,
        analysis_data,
        inside_negation,
    )?;

    combine_ored_clauses(left_clauses, right_clauses, conditional_object_id)
}

#[inline]
fn handle_uop(
    conditional_object_id: (u32, u32),
    operator: &UnaryPrefixOperator<'_>,
    operand: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
    inside_negation: bool,
) -> Option<Result<Vec<Clause>, String>> {
    if !matches!(operator, UnaryPrefixOperator::Not(_)) {
        return None;
    }

    // De Morgan: !(a && b) == (!a || !b) and !(a || b) == (!a && !b).
    if let Expression::Binary(inner) = operand.unparenthesized() {
        match inner.operator {
            BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
                return Some(negated_and(
                    conditional_object_id,
                    inner.lhs,
                    inner.rhs,
                    analyzer,
                    analysis_data,
                ));
            }
            BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
                return Some(negated_or(
                    conditional_object_id,
                    inner.lhs,
                    inner.rhs,
                    analyzer,
                    analysis_data,
                ));
            }
            _ => {}
        }
    }

    let original_clauses = match get_formula(
        conditional_object_id,
        span_id(operand),
        operand,
        analyzer,
        analysis_data,
        !inside_negation,
    ) {
        Ok(clauses) => clauses,
        Err(e) => return Some(Err(e)),
    };

    Some(negate_formula(original_clauses))
}

/// `!a && !b` — the negation of `a || b`.
fn negated_and(
    conditional_object_id: (u32, u32),
    left: &Expression<'_>,
    right: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Result<Vec<Clause>, String> {
    let mut left_clauses = negate_formula(get_formula(
        conditional_object_id,
        span_id(left),
        left,
        analyzer,
        analysis_data,
        true,
    )?)?;
    let right_clauses = negate_formula(get_formula(
        conditional_object_id,
        span_id(right),
        right,
        analyzer,
        analysis_data,
        true,
    )?)?;
    left_clauses.extend(right_clauses);
    Ok(left_clauses)
}

/// `!a || !b` — the negation of `a && b`.
fn negated_or(
    conditional_object_id: (u32, u32),
    left: &Expression<'_>,
    right: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Result<Vec<Clause>, String> {
    let left_clauses = negate_formula(get_formula(
        conditional_object_id,
        span_id(left),
        left,
        analyzer,
        analysis_data,
        true,
    )?)?;
    let right_clauses = negate_formula(get_formula(
        conditional_object_id,
        span_id(right),
        right,
        analyzer,
        analysis_data,
        true,
    )?)?;
    combine_ored_clauses(left_clauses, right_clauses, conditional_object_id)
}
