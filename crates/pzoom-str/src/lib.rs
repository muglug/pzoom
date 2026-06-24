//! String interning for pzoom.
//!
//! This crate provides efficient string interning via `StrId` and `Interner`.
//! Interned strings are stored once and referenced by a compact ID, reducing
//! memory usage and enabling fast equality comparisons.

use parking_lot::Mutex;
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

/// A parent [`Interner`] shared across scanning worker threads, behind a mutex
/// so each thread's [`ThreadedInterner`] can intern into it concurrently.
/// After scanning, [`Arc::try_unwrap`] + `into_inner` recovers the owned
/// `Interner` that analysis borrows immutably.
pub type SharedInterner = Arc<Mutex<Interner>>;

/// Create a fresh [`SharedInterner`] preloaded with the generated constants.
pub fn shared_interner() -> SharedInterner {
    Arc::new(Mutex::new(Interner::new()))
}

/// Recover the owned [`Interner`] from a [`SharedInterner`] once every
/// [`ThreadedInterner`] handle has been dropped. Panics if a handle is still
/// alive.
pub fn unwrap_shared(shared: SharedInterner) -> Interner {
    match Arc::try_unwrap(shared) {
        Ok(mutex) => mutex.into_inner(),
        Err(_) => panic!("ThreadedInterner handles outlived the shared interner"),
    }
}

impl Interner {
    /// Wrap an owned interner into a [`SharedInterner`] for a post-scan,
    /// single-threaded build pass (e.g. applying the CallMap) that still needs
    /// to intern through a [`ThreadedInterner`]. Recover it with
    /// [`unwrap_shared`].
    pub fn into_shared(self) -> SharedInterner {
        Arc::new(Mutex::new(self))
    }
}

/// String interner.
///
/// Stores strings and assigns each unique string a `StrId`. The same string
/// will always receive the same ID.
///
/// Interning requires `&mut self`, so once scanning produces an `Interner` and
/// hands analysis a shared `&Interner`, no new strings can be interned during
/// analysis — interning is a compile error there. Concurrent interning during
/// scanning goes through [`ThreadedInterner`], which guards a shared parent
/// `Interner` behind a mutex (mirroring Hakana's threaded-scanner design).
#[derive(Debug, Clone)]
pub struct Interner {
    map: FxHashMap<Arc<str>, StrId>,
    vec: Vec<Arc<str>>,
}

impl Default for Interner {
    /// Same as [`Interner::new`]: an interner without `PRELOADED_STRINGS`
    /// would disagree with the generated `StrId` constants, so an "empty"
    /// default is never sound.
    fn default() -> Self {
        Self::new()
    }
}

impl Interner {
    pub fn new() -> Self {
        let mut interner = Self {
            map: FxHashMap::default(),
            vec: Vec::new(),
        };
        for value in PRELOADED_STRINGS {
            interner.intern(value);
        }
        interner
    }

    /// Intern a string, returning its unique ID.
    ///
    /// Requires `&mut self`: interning only happens while the codebase is
    /// being built (scanning / populate). Analysis holds a shared `&Interner`
    /// and therefore cannot intern.
    pub fn intern(&mut self, s: &str) -> StrId {
        if let Some(&id) = self.map.get(s) {
            return id;
        }

        let id = StrId(self.vec.len() as u32);
        let arc: Arc<str> = Arc::from(s);
        self.vec.push(arc.clone());
        self.map.insert(arc, id);
        id
    }

    /// Look up a string by its ID.
    pub fn lookup(&self, id: StrId) -> Arc<str> {
        self.vec[id.0 as usize].clone()
    }

    /// Look up a string and return its ID if already interned.
    pub fn find(&self, s: &str) -> Option<StrId> {
        self.map.get(s).copied()
    }

    /// Get the number of interned strings.
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Check if the interner is empty (only has the empty string).
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }
}

/// Port of Hakana's `ThreadedInterner`: a per-thread interning handle that
/// caches resolved ids locally and delegates unseen strings to the shared
/// parent interner. Ids come from the parent, so every thread's symbols are
/// "merged" by construction - the local map only exists to keep repeat
/// interning off the parent's lock.
///
/// The parent is an `Arc<Mutex<Interner>>` (as in Hakana): interning now
/// requires `&mut Interner`, so the shared parent is mutated under the mutex.
/// The local cache uses a `RefCell` so `intern` can still take `&self` - the
/// type is deliberately `!Sync`, one per scanning thread.
#[derive(Debug)]
pub struct ThreadedInterner {
    map: std::cell::RefCell<FxHashMap<Arc<str>, StrId>>,
    parent: Arc<Mutex<Interner>>,
}

impl ThreadedInterner {
    pub fn new(parent: Arc<Mutex<Interner>>) -> Self {
        ThreadedInterner {
            map: std::cell::RefCell::new(FxHashMap::default()),
            parent,
        }
    }

    /// Build a threaded interner that owns a fresh parent behind a new mutex.
    /// Convenient for single-threaded one-off scans and tests.
    pub fn standalone(interner: Interner) -> Self {
        Self::new(Arc::new(Mutex::new(interner)))
    }

    /// Intern a string, returning its globally unique ID.
    pub fn intern(&self, s: &str) -> StrId {
        if let Some(&id) = self.map.borrow().get(s) {
            return id;
        }

        let id = self.parent.lock().intern(s);
        // Cache a local Arc so repeat interning of `s` stays off the parent's
        // lock.
        self.map.borrow_mut().insert(Arc::from(s), id);
        id
    }

    /// Look up a string by its ID.
    pub fn lookup(&self, id: StrId) -> Arc<str> {
        self.parent.lock().lookup(id)
    }

    /// Look up a string's ID if it is already interned anywhere.
    pub fn find(&self, s: &str) -> Option<StrId> {
        if let Some(&id) = self.map.borrow().get(s) {
            return Some(id);
        }
        self.parent.lock().find(s)
    }

    pub fn parent(&self) -> Arc<Mutex<Interner>> {
        self.parent.clone()
    }

    /// Lock the shared parent interner, for the few scanning helpers that need
    /// a `&Interner` (read-only lookups via `TAtomic::get_id`, etc.). Hold the
    /// guard only for the read - never across an `intern`, which re-locks.
    pub fn lock_parent(&self) -> parking_lot::MutexGuard<'_, Interner> {
        self.parent.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_lookup() {
        let mut interner = Interner::new();
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
