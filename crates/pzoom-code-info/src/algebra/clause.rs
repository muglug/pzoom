//! Clause data structure for CNF formulas.
//!
//! A clause represents a disjunction (OR) of assertions about variables,
//! used in the type narrowing system.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::num::Wrapping;

use indexmap::IndexMap;
use pzoom_str::Interner;

use crate::assertion::Assertion;
use crate::var_name::VarName;

/// Insertion-ordered set of assertions keyed by `Assertion::to_hash`. The
/// keys are already FxHasher outputs, so the map uses Fx rather than the
/// default SipHash.
pub type AssertionSet =
    IndexMap<u64, Assertion, std::hash::BuildHasherDefault<rustc_hash::FxHasher>>;

/// A key identifying a variable in a clause.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClauseKey {
    /// A named variable (e.g., `$a`).
    Name(VarName),
    /// A range expression identifier (start, end offsets).
    Range(u32, u32),
}

impl Display for ClauseKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClauseKey::Name(name) => write!(f, "{}", name),
            ClauseKey::Range(start, end) => write!(f, "{}-{}", start, end),
        }
    }
}

/// A clause in Conjunctive Normal Form (CNF).
///
/// A clause represents a disjunction of assertions about variables.
/// For example:
/// ```text
/// {
///     '$a' => ['falsy'],
///     '$b' => ['!falsy'],
///     '$c' => ['!null'],
///     '$d' => ['string', 'int']
/// }
/// ```
/// represents the formula:
/// ```text
/// !$a || $b || $c !== null || is_string($d) || is_int($d)
/// ```
#[derive(Clone, Debug)]
pub struct Clause {
    /// The conditional ID that created this clause.
    pub creating_conditional_id: (u32, u32),

    /// The object ID that created this clause.
    pub creating_object_id: (u32, u32),

    /// Pre-computed hash for fast comparison.
    pub hash: u32,

    /// Bloom filter over var keys and assertion hashes (bit `hash & 63` per
    /// element). `contains` uses it to reject non-subsets without map lookups.
    keys_bloom: u64,

    /// Maps variables to their possible assertion types. Shared copy-on-write:
    /// cloning a `Clause` bumps a refcount; rewrites (`remove_possibilities`,
    /// `add_possibility`) clone the map once.
    pub possibilities: std::rc::Rc<BTreeMap<ClauseKey, AssertionSet>>,

    /// Whether this is a "wedge" clause (contradiction).
    pub wedge: bool,

    /// Whether this clause can be reconciled.
    pub reconcilable: bool,

    /// Whether this clause was generated (vs. directly from source).
    pub generated: bool,

    /// Variables this clause's conditional *reassigned* (Psalm's
    /// `redefined_vars`, from `=`-prefixed assertion keys like
    /// `($v = expr) === null`). Facts about a redefined var describe its
    /// post-assignment value: `combine_ored_clauses` drops the other side's
    /// pre-assignment possibilities and `get_truths_from_formula` replaces
    /// (rather than conjoins) earlier truths.
    pub redefined_vars: std::collections::BTreeSet<crate::var_name::VarName>,
}

impl PartialEq for Clause {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for Clause {}

impl Hash for Clause {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl Clause {
    /// Creates a new clause.
    pub fn new(
        possibilities: BTreeMap<ClauseKey, AssertionSet>,
        creating_conditional_id: (u32, u32),
        creating_object_id: (u32, u32),
        wedge: Option<bool>,
        reconcilable: Option<bool>,
        generated: Option<bool>,
    ) -> Clause {
        let wedge = wedge.unwrap_or(false);
        let reconcilable = reconcilable.unwrap_or(true);
        let generated = generated.unwrap_or(false);

        Clause {
            creating_conditional_id,
            creating_object_id,
            wedge,
            reconcilable,
            generated,
            hash: compute_hash(&possibilities, creating_object_id, wedge, reconcilable),
            keys_bloom: compute_keys_bloom(&possibilities),
            possibilities: std::rc::Rc::new(possibilities),
            redefined_vars: std::collections::BTreeSet::new(),
        }
    }

    /// Mark a variable as redefined by this clause's conditional (Psalm's
    /// `redefined_vars`).
    pub fn mark_redefined(mut self, var: crate::var_name::VarName) -> Clause {
        self.redefined_vars.insert(var);
        self
    }

    /// Mark this clause as generated (exempt from "has already been asserted"
    /// redundancy reporting, like Psalm's equality-derived clauses).
    pub fn mark_generated(mut self) -> Clause {
        self.generated = true;
        self
    }

    /// Removes possibilities for a variable from this clause.
    /// Returns None if removing the variable would leave the clause empty.
    pub fn remove_possibilities(&self, var_id: &ClauseKey) -> Option<Clause> {
        let mut possibilities = (*self.possibilities).clone();
        possibilities.remove(var_id);

        if possibilities.is_empty() {
            return None;
        }

        Some(Clause {
            hash: compute_hash(
                &possibilities,
                self.creating_object_id,
                self.wedge,
                self.reconcilable,
            ),
            keys_bloom: compute_keys_bloom(&possibilities),
            possibilities: std::rc::Rc::new(possibilities),
            creating_conditional_id: self.creating_conditional_id,
            creating_object_id: self.creating_object_id,
            wedge: self.wedge,
            reconcilable: self.reconcilable,
            generated: self.generated,
            redefined_vars: self.redefined_vars.clone(),
        })
    }

    /// Adds a possibility for a variable to this clause.
    pub fn add_possibility(&self, var_id: ClauseKey, new_possibility: AssertionSet) -> Clause {
        let mut possibilities = (*self.possibilities).clone();
        possibilities.insert(var_id, new_possibility);

        Clause {
            hash: compute_hash(
                &possibilities,
                self.creating_object_id,
                self.wedge,
                self.reconcilable,
            ),
            keys_bloom: compute_keys_bloom(&possibilities),
            possibilities: std::rc::Rc::new(possibilities),
            creating_conditional_id: self.creating_conditional_id,
            creating_object_id: self.creating_object_id,
            wedge: self.wedge,
            reconcilable: self.reconcilable,
            generated: self.generated,
            redefined_vars: self.redefined_vars.clone(),
        }
    }

    /// Returns true if this clause subsumes another clause.
    pub fn contains(&self, other_clause: &Self) -> bool {
        if other_clause.possibilities.len() > self.possibilities.len() {
            return false;
        }

        // A subset clause's keys and assertion hashes all appear in this
        // clause, so its bloom bits must too. Bail without map lookups when
        // the other clause sets a bit this clause does not.
        if other_clause.keys_bloom & !self.keys_bloom != 0 {
            return false;
        }

        other_clause
            .possibilities
            .iter()
            .all(|(var, possible_types)| {
                self.possibilities
                    .get(var)
                    .map(|local_possibilities| {
                        possible_types
                            .keys()
                            .all(|k| local_possibilities.contains_key(k))
                    })
                    .unwrap_or(false)
            })
    }

    /// Gets the impossibilities (negated assertions) from this clause.
    pub fn get_impossibilities(&self) -> BTreeMap<ClauseKey, Vec<Assertion>> {
        let mut impossibilities = BTreeMap::new();

        for (var_key, possibility) in self.possibilities.iter() {
            let mut impossibility = vec![];

            for (_, assertion) in possibility {
                match assertion {
                    // Psalm's `Clause::calculateNegation` skips (wedges) any
                    // equality assertion whose atomic is not a literal
                    // int/float/string / class-const / enum-case. `IsEqual`
                    // models Psalm's `IsClassEqual`/`IsIdentical`
                    // (`hasEquality() === true`): a named-object or template
                    // equality (`get_class($x) === C`, `instanceof self`,
                    // `get_class($x) === $cs`) therefore WEDGES — "not exactly
                    // class C" is not a sound narrowing, so the else/negated
                    // branch must derive nothing on the variable.
                    Assertion::IsEqual(atomic) | Assertion::IsLooselyEqual(atomic) => {
                        if atomic.is_literal() || matches!(atomic, crate::TAtomic::TEnumCase { .. })
                        {
                            impossibility.push(assertion.get_negation());
                        }
                    }
                    // `IsNotEqual` of a named object models Psalm's
                    // `IsClassNotEqual` (`hasEquality() === false`), which
                    // negates cleanly — `get_class($a) !== B` lets the else
                    // branch recover `= B`. Literal/enum-case inequalities
                    // (`IsNotIdentical`) also negate. A non-literal,
                    // non-named-object inequality (e.g. `IsNotIdentical` of a
                    // template/union) wedges, mirroring `hasEquality()`.
                    Assertion::IsNotEqual(atomic) | Assertion::IsNotLooselyEqual(atomic) => {
                        if atomic.is_literal()
                            || matches!(
                                atomic,
                                crate::TAtomic::TNamedObject { .. } | crate::TAtomic::TEnumCase { .. }
                            )
                        {
                            impossibility.push(assertion.get_negation());
                        }
                    }
                    // Psalm's Clause::calculateNegation skips equality
                    // assertions with no literal value; `=isset` negates to
                    // `Any`, which must never enter a clause.
                    Assertion::IsEqualIsset => {}
                    _ => {
                        impossibility.push(assertion.get_negation());
                    }
                }
            }

            if !impossibility.is_empty() {
                impossibilities.insert(var_key.clone(), impossibility);
            }
        }

        impossibilities
    }

    /// Converts the clause to a human-readable string for debugging.
    pub fn to_string(&self, interner: &Interner) -> String {
        let mut clause_strings = vec![];

        if self.possibilities.is_empty() {
            return "<empty>".to_string();
        }

        for (var_id, values) in self.possibilities.iter() {
            let var_id_str = match var_id {
                ClauseKey::Name(name) => name.to_string(),
                ClauseKey::Range(_, _) => "<expr>".to_string(),
            };

            let mut clause_string_parts = vec![];

            for (_, value) in values {
                match value {
                    Assertion::Any => {
                        clause_string_parts.push(format!("{} is any", var_id_str));
                    }
                    Assertion::Falsy => {
                        clause_string_parts.push(format!("!{}", var_id_str));
                        continue;
                    }
                    Assertion::Truthy => {
                        clause_string_parts.push(var_id_str.clone());
                        continue;
                    }
                    Assertion::IsType(value) | Assertion::IsEqual(value) => {
                        clause_string_parts.push(format!(
                            "{} is {}",
                            var_id_str,
                            value.get_id(Some(interner))
                        ));
                    }
                    Assertion::IsLooselyEqual(value) => {
                        clause_string_parts.push(format!(
                            "{} is loosely {}",
                            var_id_str,
                            value.get_id(Some(interner))
                        ));
                    }
                    Assertion::IsNotType(value) | Assertion::IsNotEqual(value) => {
                        clause_string_parts.push(format!(
                            "{} is not {}",
                            var_id_str,
                            value.get_id(Some(interner))
                        ));
                    }
                    Assertion::IsNotLooselyEqual(value) => {
                        clause_string_parts.push(format!(
                            "{} is not loosely {}",
                            var_id_str,
                            value.get_id(Some(interner))
                        ));
                    }
                    _ => {
                        clause_string_parts.push(value.to_string(Some(interner)));
                    }
                }
            }

            if clause_string_parts.len() > 1 {
                let bracketed = format!("({})", clause_string_parts.join(") || ("));
                clause_strings.push(bracketed);
            } else if !clause_string_parts.is_empty() {
                clause_strings.push(clause_string_parts[0].clone());
            }
        }

        let joined_clause = clause_strings.join(") || (");

        if clause_strings.len() > 1 {
            format!("({})", joined_clause)
        } else {
            joined_clause
        }
    }
}

/// Computes the `keys_bloom` prefilter for a clause's possibilities.
#[inline]
fn compute_keys_bloom(possibilities: &BTreeMap<ClauseKey, AssertionSet>) -> u64 {
    let mut bloom = 0u64;
    for (key, assertions) in possibilities {
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        bloom |= 1u64 << (hasher.finish() & 63);
        for assertion_hash in assertions.keys() {
            bloom |= 1u64 << (assertion_hash & 63);
        }
    }
    bloom
}

/// Computes a hash for a clause.
#[inline]
fn compute_hash(
    possibilities: &BTreeMap<ClauseKey, AssertionSet>,
    creating_object_id: (u32, u32),
    wedge: bool,
    reconcilable: bool,
) -> u32 {
    if wedge || !reconcilable {
        (Wrapping(creating_object_id.0)
            + Wrapping(creating_object_id.1)
            + Wrapping(if wedge { 100000 } else { 0 }))
        .0
    } else {
        let mut hasher = rustc_hash::FxHasher::default();

        for (key, possibility) in possibilities {
            key.hash(&mut hasher);
            0u8.hash(&mut hasher);

            for i in possibility.keys() {
                i.hash(&mut hasher);
                1u8.hash(&mut hasher);
            }
        }

        hasher.finish() as u32
    }
}
