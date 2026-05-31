//! Two-level template type map.
//!
//! Mirrors the keying of Psalm's `TemplateResult` bounds
//! (`lower_bounds[$param_name][$defining_class]`) and Hakana's
//! `IndexMap<StrId, FxHashMap<GenericParent, _>>`: template types are keyed by
//! the template's name *and* the entity that defines it, so same-named
//! templates from different classes/functions in a hierarchy never collide.
//!
//! pzoom keeps `StrId` for the defining entity (Psalm's model — plain strings,
//! with methods qualified as `"Class::method"`), rather than Hakana's
//! `GenericParent` enum.

use indexmap::IndexMap;
use pzoom_code_info::{TUnion, combine_union_types};
use pzoom_str::StrId;

/// `name -> defining_entity -> type`, in insertion order on both levels so
/// fallback lookups are deterministic.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateMap {
    map: IndexMap<StrId, IndexMap<StrId, TUnion>>,
}

impl TemplateMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Inserts (or overwrites) the type for `(name, defining_entity)`.
    pub fn insert(&mut self, name: StrId, defining_entity: StrId, union: TUnion) {
        self.map.entry(name).or_default().insert(defining_entity, union);
    }

    /// Inserts the type for `(name, defining_entity)`, combining with any
    /// existing entry (Psalm's `Type::combineUnionTypes` merge semantics).
    pub fn insert_combined(&mut self, name: StrId, defining_entity: StrId, union: TUnion) {
        let entry = self.map.entry(name).or_default();
        match entry.get(&defining_entity) {
            Some(existing) => {
                let combined = combine_union_types(existing, &union, false);
                entry.insert(defining_entity, combined);
            }
            None => {
                entry.insert(defining_entity, union);
            }
        }
    }

    /// Exact `[name][defining_entity]` lookup — Psalm's strict bound lookup.
    pub fn get_exact(&self, name: StrId, defining_entity: StrId) -> Option<&TUnion> {
        self.map.get(&name)?.get(&defining_entity)
    }

    /// Exact lookup, falling back to the first-inserted entry for `name` when
    /// the exact entity has no binding.
    ///
    /// The fallback is transitional: call sites that cannot yet thread the
    /// right defining entity behave like the old name-keyed maps (which had a
    /// single slot per name), deterministically via insertion order. Strict
    /// paths that fix real cross-entity collisions use [`Self::get_exact`].
    pub fn get(&self, name: StrId, defining_entity: StrId) -> Option<&TUnion> {
        let entry = self.map.get(&name)?;
        entry.get(&defining_entity).or_else(|| entry.values().next())
    }

    /// Name-only lookup for call sites with no defining entity in scope:
    /// returns the sole entry for `name`, or `None` when the name is absent or
    /// ambiguous (defined by several entities).
    pub fn get_by_name(&self, name: StrId) -> Option<&TUnion> {
        let entry = self.map.get(&name)?;
        if entry.len() == 1 {
            entry.values().next()
        } else {
            None
        }
    }

    /// Whether any entity defines `name`.
    pub fn contains_name(&self, name: StrId) -> bool {
        self.map.contains_key(&name)
    }

    /// The sole entity defining `name`, or `None` when absent or ambiguous.
    pub fn entity_for_name(&self, name: StrId) -> Option<StrId> {
        let entry = self.map.get(&name)?;
        if entry.len() == 1 {
            entry.keys().next().copied()
        } else {
            None
        }
    }

    /// Overlays `incoming` onto `self`, overwriting per `(name, entity)` key
    /// (the old `overlay_template_replacements`).
    pub fn extend_overlay(&mut self, incoming: TemplateMap) {
        for (name, entities) in incoming.map {
            for (entity, union) in entities {
                self.insert(name, entity, union);
            }
        }
    }

    /// Merges `incoming` into `self`, combining unions on key collisions (the
    /// old `merge_template_replacements`).
    pub fn extend_combined(&mut self, incoming: TemplateMap) {
        for (name, entities) in incoming.map {
            for (entity, union) in entities {
                self.insert_combined(name, entity, union);
            }
        }
    }

    /// Iterates `(name, defining_entity, type)` in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (StrId, StrId, &TUnion)> {
        self.map.iter().flat_map(|(name, entities)| {
            entities
                .iter()
                .map(move |(entity, union)| (*name, *entity, union))
        })
    }

    /// Iterates the template names in insertion order.
    pub fn names(&self) -> impl Iterator<Item = StrId> + '_ {
        self.map.keys().copied()
    }

    /// Returns a new map keeping only entries whose name passes `pred`.
    pub fn filter_names(&self, mut pred: impl FnMut(StrId) -> bool) -> TemplateMap {
        TemplateMap {
            map: self
                .map
                .iter()
                .filter(|(name, _)| pred(**name))
                .map(|(name, entities)| (*name, entities.clone()))
                .collect(),
        }
    }
}

impl FromIterator<(StrId, StrId, TUnion)> for TemplateMap {
    fn from_iter<I: IntoIterator<Item = (StrId, StrId, TUnion)>>(iter: I) -> Self {
        let mut map = TemplateMap::new();
        for (name, entity, union) in iter {
            map.insert(name, entity, union);
        }
        map
    }
}
