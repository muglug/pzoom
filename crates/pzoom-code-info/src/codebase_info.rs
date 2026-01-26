//! Codebase-wide information storage.
//!
//! Stores all collected type information about the codebase.

use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{ClassLikeInfo, FunctionLikeInfo, TUnion};

/// Central storage for all codebase type information.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodebaseInfo {
    /// All classes, interfaces, traits, and enums.
    pub classlike_infos: FxHashMap<StrId, ClassLikeInfo>,

    /// All top-level functions.
    pub functionlike_infos: FxHashMap<StrId, FunctionLikeInfo>,

    /// Global constants.
    pub constants: FxHashMap<StrId, ConstantInfo>,

    /// Type aliases.
    pub type_aliases: FxHashMap<StrId, TypeAliasInfo>,

    /// Files that have been scanned.
    pub files: FxHashMap<StrId, FileInfo>,

    /// Map from classlike to all its descendants (classes, interfaces extending/implementing it).
    /// Populated during the populate phase.
    pub all_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,

    /// Map from classlike to its direct descendants only.
    /// Populated during the populate phase.
    pub direct_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,
}

/// Information about a global constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub name: StrId,
    pub constant_type: TUnion,
    pub file_path: StrId,
    pub start_offset: u32,
}

/// Information about a type alias.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeAliasInfo {
    pub name: StrId,
    pub aliased_type: TUnion,
    pub file_path: StrId,
    pub start_offset: u32,
}

/// Information about a scanned file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: StrId,
    /// Classes defined in this file.
    pub classes: Vec<StrId>,
    /// Functions defined in this file.
    pub functions: Vec<StrId>,
    /// Constants defined in this file.
    pub constants: Vec<StrId>,
    /// Hash of file contents for cache invalidation.
    pub content_hash: String,
    /// The file contents (for re-parsing during analysis).
    pub contents: String,
}

impl CodebaseInfo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get information about a class by name.
    pub fn get_class(&self, name: StrId) -> Option<&ClassLikeInfo> {
        self.classlike_infos.get(&name)
    }

    /// Get mutable information about a class by name.
    pub fn get_class_mut(&mut self, name: StrId) -> Option<&mut ClassLikeInfo> {
        self.classlike_infos.get_mut(&name)
    }

    /// Get information about a function by name.
    pub fn get_function(&self, name: StrId) -> Option<&FunctionLikeInfo> {
        self.functionlike_infos.get(&name)
    }

    /// Get mutable information about a function by name.
    pub fn get_function_mut(&mut self, name: StrId) -> Option<&mut FunctionLikeInfo> {
        self.functionlike_infos.get_mut(&name)
    }

    /// Check if a class exists.
    pub fn class_exists(&self, name: StrId) -> bool {
        self.classlike_infos.contains_key(&name)
    }

    /// Check if a function exists.
    pub fn function_exists(&self, name: StrId) -> bool {
        self.functionlike_infos.contains_key(&name)
    }

    /// Register a class in the codebase.
    pub fn register_class(&mut self, info: ClassLikeInfo) {
        self.classlike_infos.insert(info.name, info);
    }

    /// Register a function in the codebase.
    pub fn register_function(&mut self, info: FunctionLikeInfo) {
        self.functionlike_infos.insert(info.name, info);
    }
}
