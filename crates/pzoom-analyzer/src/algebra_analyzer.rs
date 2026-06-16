//! Paradox/redundancy detection over CNF clause formulas.
//!
//! Port of hakana-core's `algebra_analyzer::check_for_paradox`. Given the clauses
//! already established in the context (`formula_1`) and the clauses produced by a
//! new condition (`formula_2`), this reports:
//! - `RedundantCondition` when a condition clause has already been asserted, and
//! - `ParadoxicalCondition` when a condition clause contradicts an established one.

use std::rc::Rc;

use rustc_hash::FxHashSet;

use pzoom_code_info::algebra::{Clause, negate_formula};
use pzoom_code_info::{Assertion, Issue, IssueKind};

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub fn check_for_paradox(
    analyzer: &StatementsAnalyzer<'_>,
    formula_1: &[Rc<Clause>],
    formula_2: &[Clause],
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
) {
    let Ok(negated_formula_2) = negate_formula(formula_2.to_vec()) else {
        return;
    };

    let formula_1_hashes: FxHashSet<&Clause> = formula_1.iter().map(|v| &**v).collect();

    // First pass: a condition clause that repeats one already asserted (either in the
    // surrounding context or earlier in this same condition) is redundant.
    let mut formula_2_hashes: FxHashSet<&Clause> = FxHashSet::default();

    for formula_2_clause in formula_2 {
        if !formula_2_clause.generated
            && !formula_2_clause.wedge
            && formula_2_clause.reconcilable
            && (formula_1_hashes.contains(formula_2_clause)
                || formula_2_hashes.contains(formula_2_clause))
        {
            let clause_string = formula_2_clause.to_string(analyzer.interner);
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::RedundantCondition,
                format!("{} has already been asserted", clause_string),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        formula_2_hashes.insert(formula_2_clause);
    }

    // Second pass: if negating a condition clause yields a clause that contains an
    // already-established clause, the condition contradicts what is already known.
    for negated_clause_2 in &negated_formula_2 {
        if !negated_clause_2.reconcilable || negated_clause_2.wedge {
            continue;
        }

        for clause_1 in formula_1 {
            if !clause_1.reconcilable || clause_1.wedge {
                continue;
            }

            let mut negated_clause_2_contains_1_possibilities = true;

            'outer: for (key, clause_1_possibilities) in clause_1.possibilities.iter() {
                if let Some(clause_2_possibilities) = negated_clause_2.possibilities.get(key) {
                    if clause_2_possibilities != clause_1_possibilities {
                        negated_clause_2_contains_1_possibilities = false;
                        break;
                    }
                } else {
                    negated_clause_2_contains_1_possibilities = false;
                    break;
                }

                for possibility in clause_1_possibilities.values() {
                    if matches!(
                        possibility,
                        Assertion::InArray(_) | Assertion::NotInArray(_)
                    ) {
                        negated_clause_2_contains_1_possibilities = false;
                        break 'outer;
                    }
                }
            }

            if negated_clause_2_contains_1_possibilities {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ParadoxicalCondition,
                    format!(
                        "Condition ({}) contradicts a previously-established condition ({})",
                        negated_clause_2.to_string(analyzer.interner),
                        clause_1.to_string(analyzer.interner)
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));

                return;
            }
        }
    }
}
