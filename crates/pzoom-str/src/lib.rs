//! String interning for pzoom.
//!
//! This crate provides efficient string interning via `StrId` and `Interner`.
//! Interned strings are stored once and referenced by a compact ID, reducing
//! memory usage and enabling fast equality comparisons.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A unique identifier for an interned string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

include!(concat!(env!("OUT_DIR"), "/interned_strings.rs"));

impl Default for StrId {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Thread-safe string interner.
///
/// Stores strings and assigns each unique string a `StrId`. The same string
/// will always receive the same ID.
#[derive(Debug, Default)]
pub struct Interner {
    map: RwLock<FxHashMap<Arc<str>, StrId>>,
    vec: RwLock<Vec<Arc<str>>>,
}

impl Interner {
    pub fn new() -> Self {
        let interner = Self {
            map: RwLock::new(FxHashMap::default()),
            vec: RwLock::new(Vec::new()),
        };
        for value in PRELOADED_STRINGS {
            interner.intern(value);
        }
        interner
    }

    /// Intern a string, returning its unique ID.
    /// This method uses interior mutability and can be called on `&self`.
    pub fn intern(&self, s: &str) -> StrId {
        // Fast path: check if already interned
        {
            let map = self.map.read();
            if let Some(&id) = map.get(s) {
                return id;
            }
        }

        // Slow path: insert new string
        let mut map = self.map.write();
        let mut vec = self.vec.write();

        // Double-check after acquiring write lock
        if let Some(&id) = map.get(s) {
            return id;
        }

        let id = StrId(vec.len() as u32);
        let arc: Arc<str> = Arc::from(s);
        vec.push(arc.clone());
        map.insert(arc, id);
        id
    }

    /// Look up a string by its ID.
    pub fn lookup(&self, id: StrId) -> Arc<str> {
        let vec = self.vec.read();
        vec[id.0 as usize].clone()
    }

    /// Look up a string and return its ID if already interned.
    pub fn find(&self, s: &str) -> Option<StrId> {
        let map = self.map.read();
        map.get(s).copied()
    }

    /// Get the number of interned strings.
    pub fn len(&self) -> usize {
        self.vec.read().len()
    }

    /// Check if the interner is empty (only has the empty string).
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }
}

impl Clone for Interner {
    fn clone(&self) -> Self {
        let map = self.map.read();
        let vec = self.vec.read();
        Self {
            map: RwLock::new(map.clone()),
            vec: RwLock::new(vec.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_lookup() {
        let interner = Interner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("world");
        let id3 = interner.intern("hello");

        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(&*interner.lookup(id1), "hello");
        assert_eq!(&*interner.lookup(id2), "world");
    }

    #[test]
    fn test_empty_string() {
        let interner = Interner::new();
        assert_eq!(&*interner.lookup(StrId::EMPTY), "");
    }
}
