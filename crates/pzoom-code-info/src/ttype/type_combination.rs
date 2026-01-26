//! Type combination state for merging multiple atomic types.

use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

use crate::t_atomic::ArrayKey;
use crate::t_union::TUnion;
use crate::TAtomic;

/// Holds intermediate state while combining multiple atomic types into a union.
#[derive(Debug)]
pub(crate) struct TypeCombination {
    /// Non-generic value types keyed by type name (e.g., "int", "string", "bool")
    pub value_types: FxHashMap<String, TAtomic>,

    /// Whether we've seen the object top type
    pub has_object_top_type: bool,

    /// Generic object type parameters keyed by type key (e.g., "Iterator<int, string>")
    pub object_type_params: FxHashMap<String, (StrId, Vec<TUnion>)>,

    /// Track static qualifier for named objects
    pub object_static: FxHashMap<StrId, bool>,

    /// Track exact array counts when combining keyed arrays
    pub array_counts: Option<FxHashSet<usize>>,
    pub array_min_counts: Option<FxHashSet<usize>>,

    /// Keyed array (shape) entries
    pub objectlike_entries: BTreeMap<ArrayKey, TUnion>,
    pub objectlike_sealed: bool,
    pub objectlike_key_type: Option<TUnion>,
    pub objectlike_value_type: Option<TUnion>,

    /// Generic array type parameters [key_type, value_type]
    pub array_type_params: Option<(TUnion, TUnion)>,

    /// Track whether arrays are always filled (non-empty)
    pub array_always_filled: bool,
    pub array_sometimes_filled: bool,

    /// Track whether all arrays are lists
    pub all_arrays_lists: bool,

    /// Track whether all arrays are callable
    pub all_arrays_callable: bool,

    /// Builtin type params (iterable, Traversable, Generator)
    pub builtin_type_params: FxHashMap<String, Vec<TUnion>>,

    /// Named object types (without generic params)
    pub named_object_types: Option<FxHashMap<String, TAtomic>>,

    /// Extra types for intersection
    pub extra_types: FxHashMap<String, TAtomic>,

    /// Mixed type state
    pub has_mixed: bool,
    pub empty_mixed: bool,
    pub non_empty_mixed: bool,
    pub mixed_from_loop_isset: Option<bool>,

    /// Literal strings (when under the limit)
    pub strings: Option<FxHashMap<String, TAtomic>>,
    pub literal_string_limit_exceeded: bool,

    /// Literal ints (when under the limit)
    pub ints: Option<FxHashMap<String, TAtomic>>,

    /// Literal floats (when under the limit)
    pub floats: Option<FxHashMap<String, TAtomic>>,

    /// Class string types
    pub class_string_types: FxHashMap<String, TAtomic>,
}

impl TypeCombination {
    pub(crate) fn new() -> Self {
        Self {
            value_types: FxHashMap::default(),
            has_object_top_type: false,
            object_type_params: FxHashMap::default(),
            object_static: FxHashMap::default(),
            array_counts: Some(FxHashSet::default()),
            array_min_counts: Some(FxHashSet::default()),
            objectlike_entries: BTreeMap::new(),
            objectlike_sealed: true,
            objectlike_key_type: None,
            objectlike_value_type: None,
            array_type_params: None,
            array_always_filled: true,
            array_sometimes_filled: false,
            all_arrays_lists: true,
            all_arrays_callable: false,
            builtin_type_params: FxHashMap::default(),
            named_object_types: Some(FxHashMap::default()),
            extra_types: FxHashMap::default(),
            has_mixed: false,
            empty_mixed: false,
            non_empty_mixed: false,
            mixed_from_loop_isset: None,
            strings: Some(FxHashMap::default()),
            literal_string_limit_exceeded: false,
            ints: Some(FxHashMap::default()),
            floats: Some(FxHashMap::default()),
            class_string_types: FxHashMap::default(),
        }
    }

    /// Check if this combination has only a single simple value type
    #[inline]
    pub(crate) fn is_simple(&self) -> bool {
        if self.value_types.len() == 1 {
            if self.array_type_params.is_none() {
                return self.objectlike_entries.is_empty()
                    && self.object_type_params.is_empty()
                    && self.builtin_type_params.is_empty()
                    && self.strings.as_ref().map_or(true, |s| s.is_empty())
                    && self.ints.as_ref().map_or(true, |i| i.is_empty())
                    && self.floats.as_ref().map_or(true, |f| f.is_empty())
                    && self.class_string_types.is_empty()
                    && self.named_object_types.as_ref().map_or(true, |n| n.is_empty());
            }
        }
        false
    }

    /// Check if a key would be contained in the fallback type
    #[allow(dead_code)]
    pub fn fallback_key_contains(&self, key: &ArrayKey) -> bool {
        if let Some(ref key_type) = self.objectlike_key_type {
            match key {
                ArrayKey::Int(i) => {
                    key_type.types.iter().any(|t| match t {
                        TAtomic::TInt => true,
                        TAtomic::TLiteralInt { value } => value == i,
                        TAtomic::TArrayKey => true,
                        TAtomic::TIntRange { min, max } => {
                            let in_range = match (min, max) {
                                (Some(min), Some(max)) => *i >= *min && *i <= *max,
                                (Some(min), None) => *i >= *min,
                                (None, Some(max)) => *i <= *max,
                                (None, None) => true,
                            };
                            in_range
                        }
                        _ => false,
                    })
                }
                ArrayKey::String(s) => {
                    key_type.types.iter().any(|t| match t {
                        TAtomic::TString => true,
                        TAtomic::TLiteralString { value } => value == s,
                        TAtomic::TArrayKey => true,
                        TAtomic::TNonEmptyString
                        | TAtomic::TNumericString
                        | TAtomic::TTruthyString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString => true,
                        _ => false,
                    })
                }
            }
        } else {
            false
        }
    }
}
