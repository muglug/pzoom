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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TUnion {
    /// The atomic types in this union.
    pub types: Vec<TAtomic>,

    /// Whether this type came from a docblock (vs inferred or from signature).
    pub from_docblock: bool,

    /// Whether this type originated from an arithmetic calculation where PHP may
    /// promote integer results to float due to overflow semantics.
    #[serde(default)]
    pub from_calculation: bool,

    /// Whether this union represents a keyed-array entry that may be undefined.
    #[serde(default)]
    pub possibly_undefined: bool,

    /// Whether this union can be null (optimization for quick null checks).
    pub is_nullable: bool,

    /// Whether this union can be falsy.
    pub is_falsable: bool,

    /// Whether the type has been fully resolved.
    pub is_resolved: bool,

    /// Parent nodes in the data flow graph.
    pub parent_nodes: Vec<DataFlowNode>,

    /// Whether this type should be ignored for type checking.
    pub ignore_nullable_issues: bool,
    pub ignore_falsable_issues: bool,
}

impl TUnion {
    /// Create a new union from a single atomic type.
    pub fn new(atomic: TAtomic) -> Self {
        let is_nullable = atomic.is_nullable();
        let is_falsable = atomic.is_falsable();
        Self {
            types: vec![atomic],
            from_docblock: false,
            from_calculation: false,
            possibly_undefined: false,
            is_nullable,
            is_falsable,
            is_resolved: true,
            parent_nodes: Vec::new(),
            ignore_nullable_issues: false,
            ignore_falsable_issues: false,
        }
    }

    /// Create a union from multiple atomic types.
    pub fn from_types(types: Vec<TAtomic>) -> Self {
        let is_nullable = types.iter().any(|t| t.is_nullable());
        let is_falsable = types.iter().any(|t| t.is_falsable());
        Self {
            types,
            from_docblock: false,
            from_calculation: false,
            possibly_undefined: false,
            is_nullable,
            is_falsable,
            is_resolved: true,
            parent_nodes: Vec::new(),
            ignore_nullable_issues: false,
            ignore_falsable_issues: false,
        }
    }

    /// Check if this union contains only a single atomic type.
    pub fn is_single(&self) -> bool {
        self.types.len() == 1
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

    /// Check if this union is the mixed type.
    pub fn is_mixed(&self) -> bool {
        self.types.iter().any(|t| matches!(t, TAtomic::TMixed))
    }

    /// Check if this union is the nothing/never type.
    pub fn is_nothing(&self) -> bool {
        self.types.iter().all(|t| matches!(t, TAtomic::TNothing))
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
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TPositiveInt
                    | TAtomic::TNegativeInt
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
                    | TAtomic::TClassString { .. }
            )
        })
    }

    /// Add an atomic type to this union.
    pub fn add_type(&mut self, atomic: TAtomic) {
        if atomic.is_nullable() {
            self.is_nullable = true;
        }
        if atomic.is_falsable() {
            self.is_falsable = true;
        }
        self.types.push(atomic);
    }

    /// Remove null from this union.
    pub fn remove_null(&mut self) {
        self.types.retain(|t| !matches!(t, TAtomic::TNull));
        self.is_nullable = false;
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
        Self::new(TAtomic::TNothing)
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
        assert!(!t.is_nullable);

        t.add_type(TAtomic::TNull);
        assert!(t.is_nullable);

        t.remove_null();
        assert!(!t.is_nullable);
    }
}
