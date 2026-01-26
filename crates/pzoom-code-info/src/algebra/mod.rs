//! Algebra module for CNF formula manipulation.
//!
//! This module provides operations on Conjunctive Normal Form (CNF) formulas
//! used for type narrowing in conditional branches.

pub mod clause;

pub use clause::{Clause, ClauseKey};

use std::collections::BTreeMap;

use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::assertion::Assertion;

/// Checks if two IndexMaps have the same keys.
fn index_keys_match<T: Eq + std::hash::Hash, U, V>(
    map1: &IndexMap<T, U>,
    map2: &IndexMap<T, V>,
) -> bool {
    map1.len() == map2.len() && map1.keys().all(|k| map2.contains_key(k))
}

/// Checks if two BTrees have the same keys.
fn keys_match<T: Eq + Ord, U, V>(map1: &BTreeMap<T, U>, map2: &BTreeMap<T, V>) -> bool {
    map1.len() == map2.len() && map1.keys().all(|k| map2.contains_key(k))
}

/// Simplifies CNF formulas using heuristics and unit propagation.
///
/// This handles cases like:
/// - `($a) && ($a || $b)` => `$a`
/// - `(!$a) && (!$b) && ($a || $b || $c)` => `$c`
/// - Unit propagation: `($a) && (!$a || $b)` => `($a) && ($b)`
pub fn simplify_cnf(clauses: Vec<&Clause>) -> Vec<Clause> {
    let clause_count = clauses.len();

    // Performance cutoff
    if clause_count > 50 {
        let mut all_has_unknown = true;

        for clause in clauses.iter() {
            let mut clause_has_unknown = false;

            for (key, _) in clause.possibilities.iter() {
                if matches!(key, ClauseKey::Range(..)) {
                    clause_has_unknown = true;
                    break;
                }
            }

            if !clause_has_unknown {
                all_has_unknown = false;
                break;
            }
        }

        if all_has_unknown {
            return clauses.into_iter().cloned().collect();
        }
    }

    let mut working_clauses: Vec<Clause> = clauses.into_iter().cloned().collect();
    let mut max_iterations = 10; // Prevent infinite loops

    // Iterate until no more changes (fixpoint)
    loop {
        if max_iterations == 0 {
            break;
        }
        max_iterations -= 1;

        // Clone to avoid borrow conflicts
        let unique_clauses: Vec<Clause> = working_clauses.iter().unique().cloned().collect();
        let mut removed_clause_hashes = FxHashSet::default();
        let mut added_clauses = vec![];

        // Unit propagation: for each unit clause, remove its negation from other clauses
        'outer: for clause_a in &unique_clauses {
            if !clause_a.reconcilable || clause_a.wedge {
                continue;
            }

            let mut is_clause_a_simple: bool = true;

            if clause_a.possibilities.len() != 1 {
                is_clause_a_simple = false;
            } else {
                for var_possibilities in clause_a.possibilities.values() {
                    if var_possibilities.len() != 1 {
                        is_clause_a_simple = false;
                    }
                }
            }

            if !is_clause_a_simple {
                'inner: for clause_b in &unique_clauses {
                    if clause_a == clause_b || !clause_b.reconcilable || clause_b.wedge {
                        continue;
                    }

                    if keys_match(&clause_a.possibilities, &clause_b.possibilities) {
                        let mut opposing_keys = vec![];

                        for (key, a_possibilities) in clause_a.possibilities.iter() {
                            let b_possibilities = &clause_b.possibilities[key];
                            if index_keys_match(a_possibilities, b_possibilities) {
                                continue;
                            }

                            if a_possibilities.len() == 1
                                && b_possibilities.len() == 1
                                && a_possibilities
                                    .values()
                                    .next()
                                    .unwrap()
                                    .is_negation_of(b_possibilities.values().next().unwrap())
                            {
                                opposing_keys.push(key.clone());
                                continue;
                            }

                            continue 'inner;
                        }

                        if opposing_keys.len() == 1 {
                            removed_clause_hashes.insert(clause_a.hash);

                            let maybe_new_clause = clause_a.remove_possibilities(&opposing_keys[0]);

                            if maybe_new_clause.is_none() {
                                continue 'outer;
                            }

                            added_clauses.push(maybe_new_clause.unwrap());
                        }
                    }
                }

                continue;
            }

            // Unit clause: propagate to remove negations from other clauses
            for (clause_var, var_possibilities) in &clause_a.possibilities {
                let only_type = &var_possibilities.values().next().unwrap();
                let negated_clause_type = only_type.get_negation();
                let negated_hash = negated_clause_type.to_hash();

                for clause_b in &unique_clauses {
                    if clause_a == clause_b || !clause_b.reconcilable || clause_b.wedge {
                        continue;
                    }

                    if let Some(matching_clause_possibilities) =
                        clause_b.possibilities.get(clause_var)
                    {
                        if matching_clause_possibilities.contains_key(&negated_hash) {
                            let mut clause_var_possibilities = matching_clause_possibilities.clone();

                            clause_var_possibilities.retain(|k, _| k != &negated_hash);

                            removed_clause_hashes.insert(clause_b.hash);

                            if clause_var_possibilities.is_empty() {
                                let maybe_updated_clause = clause_b.remove_possibilities(clause_var);

                                if let Some(x) = maybe_updated_clause {
                                    added_clauses.push(x);
                                }
                            } else {
                                let updated_clause = clause_b
                                    .add_possibility(clause_var.clone(), clause_var_possibilities);

                                added_clauses.push(updated_clause);
                            }
                        }
                    }
                }
            }
        }

        // If no changes, we've reached fixpoint
        if removed_clause_hashes.is_empty() && added_clauses.is_empty() {
            break;
        }

        // Apply changes
        working_clauses.retain(|f| !removed_clause_hashes.contains(&f.hash));
        working_clauses.extend(added_clauses);
        working_clauses = working_clauses.into_iter().unique().collect();
    }

    // Remove redundant clauses (clauses that subsume others)
    let mut simplified_clauses = vec![];

    for clause_a in &working_clauses {
        let mut is_redundant = false;

        for clause_b in &working_clauses {
            if clause_a == clause_b || !clause_b.reconcilable || clause_b.wedge || clause_a.wedge {
                continue;
            }

            if clause_a.contains(clause_b) {
                is_redundant = true;
                break;
            }
        }

        if !is_redundant {
            simplified_clauses.push(clause_a.clone());
        }
    }

    // Simplify (A || X) && (!A || Y) && (X || Y) to (A || X) && (!A || Y)
    if simplified_clauses.len() > 2 && simplified_clauses.len() < 256 {
        let mut compared_clauses = FxHashSet::default();
        let mut clauses_to_remove = FxHashSet::default();

        for clause_a in &simplified_clauses {
            for clause_b in &simplified_clauses {
                if clause_a == clause_b
                    || compared_clauses.contains(&(clause_b.hash, clause_a.hash))
                {
                    continue;
                }

                compared_clauses.insert((clause_a.hash, clause_b.hash));

                let common_keys = clause_a
                    .possibilities
                    .iter()
                    .filter(|(var_id, _)| clause_b.possibilities.contains_key(*var_id))
                    .map(|(var_id, _)| var_id)
                    .collect::<FxHashSet<_>>();

                if !common_keys.is_empty() {
                    let mut common_negated_keys = FxHashSet::default();

                    for common_key in common_keys {
                        let clause_a_possibilities =
                            clause_a.possibilities.get(common_key).unwrap();
                        let clause_b_possibilities =
                            clause_b.possibilities.get(common_key).unwrap();
                        if clause_a_possibilities.len() == 1
                            && clause_b_possibilities.len() == 1
                            && clause_a_possibilities
                                .values()
                                .next()
                                .unwrap()
                                .is_negation_of(clause_b_possibilities.values().next().unwrap())
                        {
                            common_negated_keys.insert(common_key);
                        }
                    }

                    if !common_negated_keys.is_empty() {
                        let mut new_possibilities = BTreeMap::new();

                        for (var_id, possibilities) in &clause_a.possibilities {
                            if common_negated_keys.contains(var_id) {
                                continue;
                            }

                            new_possibilities
                                .entry(var_id.clone())
                                .or_insert_with(IndexMap::new)
                                .extend(possibilities.clone());
                        }

                        for (var_id, possibilities) in &clause_b.possibilities {
                            if common_negated_keys.contains(var_id) {
                                continue;
                            }

                            new_possibilities
                                .entry(var_id.clone())
                                .or_insert_with(IndexMap::new)
                                .extend(possibilities.clone());
                        }

                        let conflict_clause = Clause::new(
                            new_possibilities,
                            clause_a.creating_conditional_id,
                            clause_a.creating_object_id,
                            None,
                            None,
                            None,
                        );

                        clauses_to_remove.insert(conflict_clause);
                    }
                }
            }
        }

        simplified_clauses.retain(|f| !clauses_to_remove.contains(f));
    }

    simplified_clauses
}

/// Extracts definite facts from CNF clauses.
///
/// Returns a map of variable names to their possible assertion lists,
/// and a map of which truths were created by specific conditionals.
pub fn get_truths_from_formula(
    clauses: Vec<&Clause>,
    creating_conditional_id: Option<(u32, u32)>,
    cond_referenced_var_ids: &mut FxHashSet<String>,
) -> (
    BTreeMap<String, Vec<Vec<Assertion>>>,
    BTreeMap<String, FxHashSet<usize>>,
) {
    let mut truths = BTreeMap::new();
    let mut active_truths = BTreeMap::new();

    for clause in clauses {
        if !clause.reconcilable || clause.possibilities.len() != 1 {
            continue;
        }

        for (clause_key, possible_types) in &clause.possibilities {
            let var_name = match clause_key {
                ClauseKey::Name(name) => name,
                ClauseKey::Range(_, _) => {
                    continue;
                }
            };

            if possible_types.len() == 1 {
                let possible_type = possible_types.values().next().unwrap();

                truths
                    .entry(var_name.clone())
                    .or_insert_with(Vec::new)
                    .push(vec![possible_type.clone()]);

                if let Some(creating_conditional_id) = creating_conditional_id {
                    if creating_conditional_id == clause.creating_conditional_id {
                        active_truths
                            .entry(var_name.clone())
                            .or_insert_with(FxHashSet::default)
                            .insert(truths.get(var_name).unwrap().len() - 1);
                    }
                }
            } else {
                if clause.generated {
                    cond_referenced_var_ids.remove(var_name);
                }

                truths.insert(
                    var_name.clone(),
                    vec![possible_types
                        .iter()
                        .map(|(_, v)| v.clone())
                        .collect::<Vec<_>>()],
                );

                if let Some(creating_conditional_id) = creating_conditional_id {
                    if creating_conditional_id == clause.creating_conditional_id {
                        active_truths
                            .entry(var_name.clone())
                            .or_insert_with(FxHashSet::default)
                            .insert(truths.get(var_name).unwrap().len() - 1);
                    }
                }
            }
        }
    }

    (truths, active_truths)
}

/// Groups impossibilities from clauses into combined clauses.
fn group_impossibilities(mut clauses: Vec<Clause>) -> Result<Vec<Clause>, String> {
    let mut complexity = 1;
    let mut seed_clauses = vec![];

    let clause = clauses.pop();

    if clause.is_none() {
        panic!("there should be clauses");
    }

    let clause = clause.unwrap();

    if !clause.wedge {
        let impossibilities = clause.get_impossibilities();

        for (var, impossible_types) in impossibilities.iter() {
            for impossible_type in impossible_types.iter() {
                let mut seed_clause_possibilities = BTreeMap::new();
                seed_clause_possibilities.insert(
                    var.clone(),
                    IndexMap::from([(impossible_type.to_hash(), impossible_type.clone())]),
                );

                let seed_clause = Clause::new(
                    seed_clause_possibilities,
                    clause.creating_conditional_id,
                    clause.creating_object_id,
                    None,
                    None,
                    None,
                );

                seed_clauses.push(seed_clause);

                complexity += 1;
            }
        }
    }

    if clauses.is_empty() || seed_clauses.is_empty() {
        return Ok(seed_clauses);
    }

    let mut upper_bound = seed_clauses.len();

    for c in &clauses {
        let mut i = 0;
        for p in c.possibilities.values() {
            i += p.len();
        }
        upper_bound *= i;
        if upper_bound > 20000 {
            return Err("Complicated".to_string());
        }
    }

    while let Some(clause) = clauses.pop() {
        let mut new_clauses = vec![];

        for grouped_clause in &seed_clauses {
            let clause_impossibilities = clause.get_impossibilities();

            for (var, impossible_types) in clause_impossibilities {
                'next: for impossible_type in impossible_types {
                    if let Some(new_insert_value) = grouped_clause.possibilities.get(&var) {
                        for (_, a) in new_insert_value {
                            if a.is_negation_of(&impossible_type) {
                                break 'next;
                            }
                        }
                    }

                    let mut new_clause_possibilities = grouped_clause.possibilities.clone();

                    new_clause_possibilities
                        .entry(var.clone())
                        .or_insert_with(IndexMap::new)
                        .insert(impossible_type.to_hash(), impossible_type);

                    new_clauses.push(Clause::new(
                        new_clause_possibilities,
                        grouped_clause.creating_conditional_id,
                        clause.creating_object_id,
                        Some(false),
                        Some(true),
                        Some(true),
                    ));

                    complexity += 1;

                    if complexity > 20000 {
                        return Err("Complicated".to_string());
                    }
                }
            }
        }

        seed_clauses = new_clauses;
    }

    seed_clauses.reverse();

    Ok(seed_clauses)
}

/// Combines two sets of clauses with OR logic.
///
/// Produces the Cartesian product of the clause sets with safeguards
/// against combinatorial explosion.
pub fn combine_ored_clauses(
    left_clauses: Vec<Clause>,
    right_clauses: Vec<Clause>,
    conditional_object_id: (u32, u32),
) -> Result<Vec<Clause>, String> {
    let mut clauses = vec![];

    let mut all_wedges = true;
    let mut has_wedge = false;

    let left_clauses_len = left_clauses.len();
    let right_clauses_len = right_clauses.len();

    let upper_bound_output = left_clauses_len * right_clauses_len;

    if upper_bound_output > 2048 {
        return Err("too many clauses".to_string());
    }

    if left_clauses.is_empty() || right_clauses.is_empty() {
        return Ok(vec![]);
    }

    for left_clause in &left_clauses {
        for right_clause in &right_clauses {
            all_wedges = all_wedges && (left_clause.wedge && right_clause.wedge);
            has_wedge = has_wedge || (left_clause.wedge && right_clause.wedge);
        }
    }

    if all_wedges {
        return Ok(vec![Clause::new(
            BTreeMap::new(),
            conditional_object_id,
            conditional_object_id,
            Some(true),
            None,
            None,
        )]);
    }

    for left_clause in left_clauses {
        'right: for right_clause in &right_clauses {
            if left_clause.wedge && right_clause.wedge {
                // Handled below
                continue;
            }

            let mut possibilities = BTreeMap::new();

            let can_reconcile = !left_clause.wedge
                && !right_clause.wedge
                && left_clause.reconcilable
                && right_clause.reconcilable;

            for (var, possible_types) in &left_clause.possibilities {
                possibilities
                    .entry(var.clone())
                    .or_insert_with(IndexMap::new)
                    .extend(possible_types.clone());
            }

            for (var, possible_types) in &right_clause.possibilities {
                possibilities
                    .entry(var.clone())
                    .or_insert_with(IndexMap::new)
                    .extend(possible_types.clone());
            }

            for var_possibilities in possibilities.values() {
                if var_possibilities.len() == 2 {
                    let vals = var_possibilities.values().collect::<Vec<_>>();
                    if vals[0].is_negation_of(vals[1]) {
                        continue 'right;
                    }
                }
            }

            let creating_conditional_id =
                if right_clause.creating_conditional_id == left_clause.creating_conditional_id {
                    right_clause.creating_conditional_id
                } else {
                    conditional_object_id
                };

            let is_generated = right_clause.generated
                || left_clause.generated
                || left_clauses_len > 1
                || right_clauses_len > 1;

            clauses.push(Clause::new(
                possibilities,
                creating_conditional_id,
                creating_conditional_id,
                Some(false),
                Some(can_reconcile),
                Some(is_generated),
            ));
        }
    }

    if has_wedge {
        clauses.push(Clause::new(
            BTreeMap::new(),
            conditional_object_id,
            conditional_object_id,
            Some(true),
            None,
            None,
        ));
    }

    Ok(clauses)
}

/// Negates a set of clauses.
///
/// Examples:
/// - `negateClauses([$a || $b])` => `!$a && !$b`
/// - `negateClauses([$a, $b])` => `!$a || !$b`
/// - `negateClauses([$a, $b || $c])` => `(!$a || !$b) && (!$a || !$c)`
pub fn negate_formula(mut clauses: Vec<Clause>) -> Result<Vec<Clause>, String> {
    clauses.retain(|clause| clause.reconcilable);

    if clauses.is_empty() {
        let n2: u32 = rand::random();
        return Ok(vec![Clause::new(
            BTreeMap::new(),
            (n2, n2),
            (n2, n2),
            Some(true),
            None,
            None,
        )]);
    }

    let impossible_clauses = group_impossibilities(clauses)?;

    if impossible_clauses.is_empty() {
        let n2: u32 = rand::random();
        return Ok(vec![Clause::new(
            BTreeMap::new(),
            (n2, n2),
            (n2, n2),
            Some(true),
            None,
            None,
        )]);
    }

    let negated = simplify_cnf(impossible_clauses.iter().collect());

    if negated.is_empty() {
        let n2: u32 = rand::random();
        return Ok(vec![Clause::new(
            BTreeMap::new(),
            (n2, n2),
            (n2, n2),
            Some(true),
            None,
            None,
        )]);
    }

    Ok(negated)
}
