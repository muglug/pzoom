//! Union types - a wrapper around multiple atomic types.
//!
//! Modeled after Psalm's `Type\Union`.

use serde::{Deserialize, Serialize};

use crate::{TAtomic, data_flow::node::DataFlowNode};
use pzoom_str::Interner;

/// A union of atomic types.
///
/// Represents types like `int|string` or `Foo|null`. A union with a single
/// atomic type represents that type directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TUnion {
    /// The atomic types in this union.
    pub types: Vec<TAtomic>,

    /// Whether this type came from a docblock (vs inferred or from signature).
    pub from_docblock: bool,

    /// Per-atomic docblock provenance: bit `i` set means `types[i]` came from
    /// a docblock. Psalm tracks this on each Atomic; pzoom keeps the atomics
    /// comparison-clean and stores the flags here instead.
    ///
    /// Only meaningful while `docblock_bits_len == types.len()` (and the union
    /// has <= 32 members): code that mutates `types` without resyncing simply
    /// invalidates the stamp, and [`Self::atomic_from_docblock`] falls back to
    /// the union-level `from_docblock` flag — i.e. staleness degrades to the
    /// coarse semantics, never to wrong per-atomic answers.
    #[serde(default)]
    pub from_docblock_bits: u32,

    /// Length stamp validating `from_docblock_bits` (see above).
    #[serde(default)]
    pub docblock_bits_len: u8,

    /// Whether this type originated from an arithmetic calculation where PHP may
    /// promote integer results to float due to overflow semantics.
    #[serde(default)]
    pub from_calculation: bool,

    /// Whether the type has been fully resolved.
    pub is_resolved: bool,

    /// Parent nodes in the data flow graph.
    pub parent_nodes: Vec<DataFlowNode>,

    /// Whether this type should be ignored for type checking.
    pub ignore_nullable_issues: bool,
    pub ignore_falsable_issues: bool,

    /// Whether this value holds no references to external mutable state — set on
    /// the result of `new` for an externally-mutation-free class (Psalm's
    /// `Union::$reference_free`). Used by purity checks: calling a possibly
    /// -mutating method on a reference-free receiver is allowed from a pure
    /// context because the mutation can't escape.
    #[serde(default)]
    pub reference_free: bool,
    /// Whether properties of this value may be assigned in a mutation-free
    /// context (Psalm `Union::$allow_mutations`). True by default; `$this` in
    /// a non-constructor external-mutation-free method sets it false, and
    /// `clone` results reset it to true.
    #[serde(default = "default_allow_mutations")]
    pub allow_mutations: bool,

    /// Whether this union was produced by filling a template slot from its
    /// declared default/extends mapping rather than a concrete inference
    /// (Psalm's `Union::$from_template_default`). A docblock-sourced mixed in
    /// such a slot coerces leniently (as-mixed) instead of reporting
    /// Mixed*Coercion issues.
    #[serde(default)]
    pub from_template_default: bool,

    /// Whether this mixed was predeclared for an undefined variable passed to
    /// a by-ref parameter. Psalm models such arguments as having *no* type and
    /// skips argument verification for them entirely; pzoom predeclares them
    /// as mixed and uses this marker to skip the same checks.
    #[serde(default)]
    pub from_undefined_by_ref: bool,

    /// Whether possibly_undefined came from a try block whose assignment may
    /// not have completed (Psalm's `Union::$possibly_undefined_from_try`).
    /// Cleared when every catch also definitely assigns the variable; variable
    /// fetches report "Possibly undefined variable ... defined in try block".
    #[serde(default)]
    pub possibly_undefined_from_try: bool,
}

impl TUnion {
    /// Whether `types[index]` came from a docblock. Uses the per-atomic bits
    /// when the stamp is valid, otherwise the union-level flag.
    pub fn atomic_from_docblock(&self, index: usize) -> bool {
        if self.docblock_bits_valid() && index < 32 {
            self.from_docblock_bits & (1 << index) != 0
        } else {
            self.from_docblock
        }
    }

    /// Whether the per-atomic docblock bits are in sync with `types`.
    pub fn docblock_bits_valid(&self) -> bool {
        !self.types.is_empty()
            && self.types.len() <= 32
            && self.docblock_bits_len as usize == self.types.len()
    }

    /// Stamp the per-atomic bits as "every member from a docblock" /
    /// "no member from a docblock", matching the union-level flag.
    pub fn sync_docblock_bits_from_union_flag(&mut self) {
        if self.types.len() > 32 {
            self.invalidate_docblock_bits();
            return;
        }
        self.docblock_bits_len = self.types.len() as u8;
        self.from_docblock_bits = if self.from_docblock {
            u32::MAX >> (32 - self.types.len().max(1))
        } else {
            0
        };
    }

    /// Set the docblock provenance of a single member (stamps the mask from
    /// the union flag first if it was invalid).
    pub fn set_atomic_from_docblock(&mut self, index: usize, value: bool) {
        if !self.docblock_bits_valid() {
            self.sync_docblock_bits_from_union_flag();
        }
        if index >= 32 || !self.docblock_bits_valid() {
            return;
        }
        if value {
            self.from_docblock_bits |= 1 << index;
        } else {
            self.from_docblock_bits &= !(1 << index);
        }
    }

    /// Refresh provenance after this union was narrowed to (a subset of)
    /// `source`'s members: per-atomic bits are matched by equality, and the
    /// union-level flag becomes "any kept member was docblock-sourced" —
    /// so removing the only docblock members (e.g. the inferred half of a
    /// branch merge surviving) stops mis-flagging the result as docblock.
    pub fn inherit_docblock_provenance_from(&mut self, source: &TUnion) {
        if self.types.is_empty() || self.types.len() > 32 {
            self.invalidate_docblock_bits();
            self.from_docblock = source.from_docblock;
            return;
        }

        let mut bits = 0u32;
        for (index, atomic) in self.types.iter().enumerate() {
            let from_docblock = match source.types.iter().position(|t| t == atomic) {
                Some(source_index) => source.atomic_from_docblock(source_index),
                // Synthesized/transformed member: fall back to the source flag.
                None => source.from_docblock,
            };
            if from_docblock {
                bits |= 1 << index;
            }
        }
        self.from_docblock_bits = bits;
        self.docblock_bits_len = self.types.len() as u8;
        self.from_docblock = bits != 0;
    }

    /// Drop the per-atomic bits; lookups fall back to the union-level flag.
    pub fn invalidate_docblock_bits(&mut self) {
        self.docblock_bits_len = 0;
        self.from_docblock_bits = 0;
    }

    /// Create a new union from a single atomic type.
    pub fn new(atomic: TAtomic) -> Self {
        Self {
            types: vec![atomic],
            from_docblock: false,
            from_docblock_bits: 0,
            docblock_bits_len: 0,
            from_calculation: false,
            is_resolved: true,
            parent_nodes: Vec::new(),
            ignore_nullable_issues: false,
            ignore_falsable_issues: false,
            reference_free: false,
            allow_mutations: true,
            from_template_default: false,
            from_undefined_by_ref: false,
            possibly_undefined_from_try: false,
        }
    }

    /// Create a union from multiple atomic types.
    pub fn from_types(types: Vec<TAtomic>) -> Self {
        Self {
            types,
            from_docblock: false,
            from_docblock_bits: 0,
            docblock_bits_len: 0,
            from_calculation: false,
            is_resolved: true,
            parent_nodes: Vec::new(),
            ignore_nullable_issues: false,
            ignore_falsable_issues: false,
            reference_free: false,
            allow_mutations: true,
            from_template_default: false,
            from_undefined_by_ref: false,
            possibly_undefined_from_try: false,
        }
    }

    /// Check if this union contains only a single atomic type.
    pub fn is_single(&self) -> bool {
        self.types.len() == 1
    }

    /// Whether this value is reference-free (holds no external mutable state).
    /// Mirrors Psalm's `NodeDataProvider::isPureCompatible` type-side check.
    pub fn is_reference_free(&self) -> bool {
        self.reference_free
    }

    /// Mark this union as reference-free (or not) and return it, for fluent
    /// construction. Kept as a helper so callers don't touch the field directly
    /// and future `TUnion` shape changes stay localized.
    #[must_use]
    pub fn with_reference_free(mut self, reference_free: bool) -> Self {
        self.reference_free = reference_free;
        self
    }

    /// Get the single atomic type if this union contains exactly one.
    pub fn get_single(&self) -> Option<&TAtomic> {
        if self.types.len() == 1 {
            self.types.first()
        } else {
            None
        }
    }

    /// Get a mutable reference to the single atomic type.
    pub fn get_single_mut(&mut self) -> Option<&mut TAtomic> {
        if self.types.len() == 1 {
            self.types.first_mut()
        } else {
            None
        }
    }

    /// Check if this union contains the mixed type (any atomic is mixed).
    pub fn is_mixed(&self) -> bool {
        self.types
            .iter()
            .any(|t| matches!(t, TAtomic::TMixed | TAtomic::TMixedFromLoopIsset))
    }

    /// Psalm `Union::getTaintsToRemove`: numeric types can't be tainted
    /// (except sleep), neither can bool. Applied as removed taints on
    /// argument paths and on param-seeding edges in the taint graph.
    pub fn get_taints_to_remove(&self) -> Vec<crate::data_flow::node::SinkType> {
        use crate::data_flow::node::SinkType;

        if self.types.is_empty() {
            return vec![];
        }

        let all_int = self.types.iter().all(|t| {
            matches!(
                t,
                TAtomic::TInt
                    | TAtomic::TNonspecificLiteralInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
            )
        });
        let all_float = self
            .types
            .iter()
            .all(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }));
        let all_bool = self
            .types
            .iter()
            .all(|t| matches!(t, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse));

        if all_int || all_float || all_bool {
            // NUMERIC_ONLY == BOOL_ONLY == INPUT_SLEEP in Psalm.
            return SinkType::all_input()
                .into_iter()
                .filter(|kind| !matches!(kind, SinkType::Sleep))
                .collect();
        }

        vec![]
    }

    /// Hakana `TUnion::has_taintable_value`: whether any atomic could carry
    /// tainted data (numbers, bools, null, literals and enum cases cannot).
    pub fn has_taintable_value(&self) -> bool {
        self.types.iter().any(|atomic| {
            !matches!(
                atomic,
                TAtomic::TInt
                    | TAtomic::TFloat
                    | TAtomic::TNull
                    | TAtomic::TLiteralClassString { .. }
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralFloat { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TBool
                    | TAtomic::TTrue
                    | TAtomic::TFalse
                    | TAtomic::TEnumCase { .. }
                    | TAtomic::TIntRange { .. }
            )
        })
    }

    /// Whether *every* atomic in this union is a mixed type (Psalm's
    /// `Union::isMixed`). A `mixed|null`/`mixed|int` union is not "only mixed".
    pub fn is_only_mixed(&self) -> bool {
        !self.types.is_empty()
            && self.types.iter().all(|t| {
                matches!(
                    t,
                    TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset
                )
            })
    }

    /// Check if this union is the nothing/never type.
    pub fn is_nothing(&self) -> bool {
        self.types.iter().all(|t| matches!(t, TAtomic::TNever))
    }

    /// Check if this union is the void type.
    pub fn is_void(&self) -> bool {
        self.types.len() == 1 && matches!(self.types.first(), Some(TAtomic::TVoid))
    }

    /// Check if this union is null.
    pub fn is_null(&self) -> bool {
        self.types.len() == 1 && matches!(self.types.first(), Some(TAtomic::TNull))
    }

    /// Check if this union is null or void.
    pub fn is_null_or_void(&self) -> bool {
        self.types.len() == 1
            && matches!(
                self.types.first(),
                Some(TAtomic::TNull) | Some(TAtomic::TVoid)
            )
    }

    /// Check if this union is always truthy.
    ///
    /// Returns true if all types in the union are definitely truthy.
    pub fn is_always_truthy(&self) -> bool {
        // Psalm's isAlwaysTruthy: a type from a try block whose assignment may
        // not have run is never always-truthy.
        if self.possibly_undefined_from_try {
            return false;
        }
        !self.types.is_empty() && self.types.iter().all(|t| t.is_truthy())
    }

    /// Check if this union is always falsy.
    ///
    /// Returns true if all types in the union are definitely falsy.
    pub fn is_always_falsy(&self) -> bool {
        !self.types.is_empty() && self.types.iter().all(|t| t.is_falsy())
    }

    /// Check if this union has any object types.
    pub fn has_object(&self) -> bool {
        self.types.iter().any(|t| {
            matches!(
                t,
                TAtomic::TNamedObject { .. }
                    | TAtomic::TObject
                    | TAtomic::TObjectIntersection { .. }
            )
        })
    }

    /// Check if this union contains any int types.
    pub fn has_int(&self) -> bool {
        self.types.iter().any(|t| {
            matches!(
                t,
                TAtomic::TInt
                    | TAtomic::TNonspecificLiteralInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
            )
        })
    }

    /// Check if this union contains any string types.
    pub fn has_string(&self) -> bool {
        self.types.iter().any(|t| {
            matches!(
                t,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TLiteralClassString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TCallableString
                    | TAtomic::TClassString { .. }
            )
        })
    }

    /// Check if this union contains any float types.
    pub fn has_float(&self) -> bool {
        self.types
            .iter()
            .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
    }

    /// Add an atomic type to this union.
    pub fn add_type(&mut self, atomic: TAtomic) {
        self.types.push(atomic);
    }

    /// Whether this union can be null — derived by scanning the atomics
    /// (Psalm's `Union::isNullable`); no cached flag to fall out of sync.
    pub fn is_nullable(&self) -> bool {
        self.types.iter().any(|t| t.is_nullable())
    }

    /// Whether this union can be false — derived by scanning the atomics.
    pub fn is_falsable(&self) -> bool {
        self.types.iter().any(|t| t.is_falsable())
    }

    /// Whether this union is exactly `false` (Psalm's `Union::isFalse`).
    pub fn is_false(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TFalse)
    }

    /// Remove null from this union.
    pub fn remove_null(&mut self) {
        self.types.retain(|t| !matches!(t, TAtomic::TNull));
    }

    /// Create common type constructors.
    pub fn int() -> Self {
        Self::new(TAtomic::TInt)
    }

    pub fn int_from_calculation() -> Self {
        let mut int_type = Self::int();
        int_type.from_calculation = true;
        int_type
    }

    pub fn float() -> Self {
        Self::new(TAtomic::TFloat)
    }

    pub fn string() -> Self {
        Self::new(TAtomic::TString)
    }

    pub fn bool() -> Self {
        Self::new(TAtomic::TBool)
    }

    pub fn null() -> Self {
        Self::new(TAtomic::TNull)
    }

    pub fn mixed() -> Self {
        Self::new(TAtomic::TMixed)
    }

    pub fn nothing() -> Self {
        Self::new(TAtomic::TNever)
    }

    pub fn void() -> Self {
        Self::new(TAtomic::TVoid)
    }

    pub fn array_key() -> Self {
        Self::new(TAtomic::TArrayKey)
    }

    /// Returns a human-readable type identifier, resolving class names through an
    /// interner when available.
    pub fn get_id(&self, interner: Option<&Interner>) -> String {
        if self.types.is_empty() {
            return "empty".to_string();
        }
        let mut type_ids: Vec<String> = Vec::with_capacity(self.types.len());
        for atomic in &self.types {
            let atomic_id = atomic.get_id(interner);
            if !type_ids.contains(&atomic_id) {
                type_ids.push(atomic_id);
            }
        }

        // Psalm's Union::getId sorts the rendered members (byte-wise, after
        // dedup), so e.g. `string|null` always displays as `null|string`
        // regardless of internal order.
        type_ids.sort_unstable();

        // Psalm parenthesizes `T as U` members in multi-member unions.
        if type_ids.len() > 1 {
            for type_id in &mut type_ids {
                if type_id.contains(" as ") && !type_id.contains('(') {
                    *type_id = format!("({type_id})");
                }
            }
        }

        type_ids.join("|")
    }
}

impl Default for TUnion {
    fn default() -> Self {
        Self::mixed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_type() {
        let t = TUnion::int();
        assert!(t.is_single());
        assert!(matches!(t.get_single(), Some(TAtomic::TInt)));
    }

    #[test]
    fn test_union_type() {
        let t = TUnion::from_types(vec![TAtomic::TInt, TAtomic::TString]);
        assert!(!t.is_single());
        assert!(t.get_single().is_none());
    }

    #[test]
    fn test_nullable() {
        let mut t = TUnion::int();
        assert!(!t.is_nullable());

        t.add_type(TAtomic::TNull);
        assert!(t.is_nullable());

        t.remove_null();
        assert!(!t.is_nullable());
    }
}

fn default_allow_mutations() -> bool {
    true
}

impl PartialEq for TUnion {
    fn eq(&self, other: &Self) -> bool {
        // Identical to the old derive, minus the per-atomic docblock bits:
        // provenance metadata must not make otherwise-equal types unequal.
        self.types == other.types
            && self.from_docblock == other.from_docblock
            && self.from_calculation == other.from_calculation
            && self.is_resolved == other.is_resolved
            && self.parent_nodes == other.parent_nodes
            && self.ignore_nullable_issues == other.ignore_nullable_issues
            && self.ignore_falsable_issues == other.ignore_falsable_issues
            && self.reference_free == other.reference_free
            && self.allow_mutations == other.allow_mutations
    }
}
