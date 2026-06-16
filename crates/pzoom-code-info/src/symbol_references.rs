//! Codebase-wide symbol reference graph, ported wholesale from Hakana's
//! `code_info/symbol_references.rs`.
//!
//! Each analyzed function-like records, into a per-analysis [`SymbolReferences`],
//! every other symbol (class, function, enum) and class member (method,
//! property, class constant, enum case) it references — separately tracking
//! references that appear in a *signature* (parameter/return types, `extends`,
//! `implements`, `use`) versus a *body*. After analysis the per-function graphs
//! are merged ([`SymbolReferences::extend`]) into one codebase-wide graph, which
//! [`crate::unused_symbols`] queries to find definitions referenced nowhere.
//!
//! Symbols are `(name, StrId::EMPTY)`; class members are `(class, member)`.

use crate::data_flow::node::FunctionLikeIdentifier;
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolReferences {
    /// All symbols/members that reference another symbol/member from a body.
    pub symbol_references_to_symbols: FxHashMap<(StrId, StrId), FxHashSet<(StrId, StrId)>>,

    /// All symbols/members that reference another symbol/member from a signature
    /// (parameter/return types, `extends`/`implements`/`use`).
    pub symbol_references_to_symbols_in_signature:
        FxHashMap<(StrId, StrId), FxHashSet<(StrId, StrId)>>,

    /// References to an *overridden* member — a call to `A::foo` where `foo` is
    /// declared on a descendant keeps the descendant override alive.
    pub symbol_references_to_overridden_members:
        FxHashMap<(StrId, StrId), FxHashSet<(StrId, StrId)>>,
}

impl SymbolReferences {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_symbol_reference_to_class_member(
        &mut self,
        referencing_symbol: StrId,
        class_member: (StrId, StrId),
        in_signature: bool,
    ) {
        self.add_symbol_reference_to_symbol(referencing_symbol, class_member.0, false);

        if in_signature {
            self.symbol_references_to_symbols_in_signature
                .entry((referencing_symbol, StrId::EMPTY))
                .or_default()
                .insert(class_member);
        } else {
            self.symbol_references_to_symbols
                .entry((referencing_symbol, StrId::EMPTY))
                .or_default()
                .insert(class_member);
        }
    }

    pub fn add_symbol_reference_to_symbol(
        &mut self,
        referencing_symbol: StrId,
        symbol: StrId,
        in_signature: bool,
    ) {
        if referencing_symbol == symbol {
            return;
        }

        if in_signature {
            self.symbol_references_to_symbols_in_signature
                .entry((referencing_symbol, StrId::EMPTY))
                .or_default()
                .insert((symbol, StrId::EMPTY));
        } else {
            if let Some(symbol_refs_in_signature) = self
                .symbol_references_to_symbols_in_signature
                .get(&(referencing_symbol, StrId::EMPTY))
                && symbol_refs_in_signature.contains(&(symbol, StrId::EMPTY))
            {
                return;
            }

            self.symbol_references_to_symbols
                .entry((referencing_symbol, StrId::EMPTY))
                .or_default()
                .insert((symbol, StrId::EMPTY));
        }
    }

    pub fn add_class_member_reference_to_class_member(
        &mut self,
        referencing_class_member: (StrId, StrId),
        class_member: (StrId, StrId),
        in_signature: bool,
    ) {
        if referencing_class_member == class_member {
            return;
        }

        self.add_symbol_reference_to_symbol(referencing_class_member.0, class_member.0, false);

        self.add_class_member_reference_to_symbol(referencing_class_member, class_member.0, false);

        if in_signature {
            self.symbol_references_to_symbols_in_signature
                .entry(referencing_class_member)
                .or_default()
                .insert(class_member);
        } else {
            self.symbol_references_to_symbols
                .entry(referencing_class_member)
                .or_default()
                .insert(class_member);
        }
    }

    pub fn add_class_member_reference_to_symbol(
        &mut self,
        referencing_class_member: (StrId, StrId),
        symbol: StrId,
        in_signature: bool,
    ) {
        if referencing_class_member.0 == symbol {
            return;
        }

        self.add_symbol_reference_to_symbol(referencing_class_member.0, symbol, false);

        if in_signature {
            self.symbol_references_to_symbols_in_signature
                .entry(referencing_class_member)
                .or_default()
                .insert((symbol, StrId::EMPTY));
        } else {
            if let Some(symbol_refs_in_signature) = self
                .symbol_references_to_symbols_in_signature
                .get(&referencing_class_member)
                && symbol_refs_in_signature.contains(&(symbol, StrId::EMPTY))
            {
                return;
            }

            self.symbol_references_to_symbols
                .entry(referencing_class_member)
                .or_default()
                .insert((symbol, StrId::EMPTY));
        }
    }

    /// Resolve `referencing` (the function-like currently being analyzed, from
    /// `context.function_context`) to its `(symbol, member)` key. A closure or a
    /// missing function-like falls back to the enclosing class, mirroring
    /// Hakana's `function_context.calling_class` branch.
    fn referencing_key(
        referencing: Option<&FunctionLikeIdentifier>,
        calling_class: Option<StrId>,
    ) -> Option<(StrId, StrId)> {
        match referencing {
            Some(FunctionLikeIdentifier::Function(function_name)) => {
                Some((*function_name, StrId::EMPTY))
            }
            Some(FunctionLikeIdentifier::Method(class_name, function_name)) => {
                Some((*class_name, *function_name))
            }
            // Closures attribute to their enclosing class (Hakana panics here as
            // closures carry the enclosing context; PHP closures fall through).
            _ => calling_class.map(|c| (c, StrId::EMPTY)),
        }
    }

    pub fn add_reference_to_class_member(
        &mut self,
        referencing: Option<&FunctionLikeIdentifier>,
        calling_class: Option<StrId>,
        class_member: (StrId, StrId),
        in_signature: bool,
    ) {
        match Self::referencing_key(referencing, calling_class) {
            Some((class, StrId::EMPTY)) => {
                self.add_symbol_reference_to_class_member(class, class_member, in_signature)
            }
            Some(member) => {
                self.add_class_member_reference_to_class_member(member, class_member, in_signature)
            }
            None => {}
        }
    }

    pub fn add_reference_to_overridden_class_member(
        &mut self,
        referencing: Option<&FunctionLikeIdentifier>,
        calling_class: Option<StrId>,
        class_member: (StrId, StrId),
    ) {
        if let Some(key) = Self::referencing_key(referencing, calling_class) {
            self.symbol_references_to_overridden_members
                .entry(key)
                .or_default()
                .insert(class_member);
        }
    }

    pub fn add_reference_to_symbol(
        &mut self,
        referencing: Option<&FunctionLikeIdentifier>,
        calling_class: Option<StrId>,
        symbol: StrId,
        in_signature: bool,
    ) {
        match Self::referencing_key(referencing, calling_class) {
            Some((class, StrId::EMPTY)) => {
                self.add_symbol_reference_to_symbol(class, symbol, in_signature)
            }
            Some(member) => self.add_class_member_reference_to_symbol(member, symbol, in_signature),
            None => {}
        }
    }

    /// Merge another (per-function) graph into this one.
    pub fn extend(&mut self, other: Self) {
        for (k, v) in other.symbol_references_to_symbols {
            self.symbol_references_to_symbols
                .entry(k)
                .or_default()
                .extend(v);
        }

        for (k, v) in other.symbol_references_to_symbols_in_signature {
            self.symbol_references_to_symbols_in_signature
                .entry(k)
                .or_default()
                .extend(v);
        }

        for (k, v) in other.symbol_references_to_overridden_members {
            self.symbol_references_to_overridden_members
                .entry(k)
                .or_default()
                .extend(v);
        }
    }

    /// The set of every symbol/member that is referenced by anything (body or
    /// signature). A definition absent from this set is referenced nowhere.
    pub fn get_referenced_symbols_and_members(&self) -> FxHashSet<(StrId, StrId)> {
        let mut referenced = FxHashSet::default();

        for refs in self.symbol_references_to_symbols.values() {
            referenced.extend(refs.iter().copied());
        }

        for refs in self.symbol_references_to_symbols_in_signature.values() {
            referenced.extend(refs.iter().copied());
        }

        referenced
    }

    /// For each member, the set of overridden-member references pointing at it
    /// (used to keep interface/abstract method overrides alive).
    pub fn get_referenced_overridden_class_members(&self) -> FxHashSet<(StrId, StrId)> {
        let mut referenced = FxHashSet::default();

        for refs in self.symbol_references_to_overridden_members.values() {
            referenced.extend(refs.iter().copied());
        }

        referenced
    }
}
