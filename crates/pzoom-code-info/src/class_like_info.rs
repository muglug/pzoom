//! Storage for class-like entities (classes, interfaces, traits, enums).
//!
//! Modeled after Psalm's `Storage\ClassLikeStorage`.

use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{FunctionLikeInfo, TUnion};

/// Information about a class, interface, trait, or enum.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClassLikeInfo {
    /// Fully qualified name of the class.
    pub name: StrId,

    /// What kind of class-like this is.
    pub kind: ClassLikeKind,

    /// Whether this class is final.
    pub is_final: bool,

    /// Whether this class is abstract.
    pub is_abstract: bool,

    /// Whether this class is read-only (PHP 8.2+).
    pub is_readonly: bool,

    /// Direct parent class (if any). This is the immediate parent.
    pub parent_class: Option<StrId>,

    /// All parent classes in the inheritance chain (populated during population phase).
    /// Includes parent, grandparent, great-grandparent, etc.
    pub all_parent_classes: Vec<StrId>,

    /// Directly implemented/extended interfaces (declared on this class/interface).
    pub interfaces: FxHashSet<StrId>,

    /// All parent interfaces including inherited ones (populated during population phase).
    pub all_parent_interfaces: Vec<StrId>,

    /// Used traits.
    pub used_traits: FxHashSet<StrId>,

    /// Methods defined directly in this class (method names only).
    /// The actual method info is stored in the codebase's functionlike_infos.
    pub method_names: FxHashSet<StrId>,

    /// Methods available on this class (including inherited).
    pub methods: FxHashMap<StrId, FunctionLikeInfo>,

    /// Maps method name to the class that declares it.
    pub declaring_method_ids: FxHashMap<StrId, StrId>,

    /// Maps method name to the class where it appears (for traits, this is the using class).
    pub appearing_method_ids: FxHashMap<StrId, StrId>,

    /// Methods that can be inherited by child classes.
    pub inheritable_method_ids: FxHashMap<StrId, StrId>,

    /// Properties defined in this class.
    pub properties: FxHashMap<StrId, PropertyInfo>,

    /// Maps property name to the class that declares it.
    pub declaring_property_ids: FxHashMap<StrId, StrId>,

    /// Maps property name to the class where it appears.
    pub appearing_property_ids: FxHashMap<StrId, StrId>,

    /// Properties that can be inherited by child classes.
    pub inheritable_property_ids: FxHashMap<StrId, StrId>,

    /// Class constants.
    pub constants: FxHashMap<StrId, ClassConstantInfo>,

    /// Template/generic type parameters.
    pub template_types: Vec<TemplateType>,

    /// Whether this class has been deprecated.
    pub is_deprecated: bool,

    /// Deprecation message if deprecated.
    pub deprecation_message: Option<String>,

    /// Whether this is an internal class (not for external use).
    pub is_internal: bool,

    /// Dependencies that couldn't be resolved (missing classes).
    pub invalid_dependencies: Vec<StrId>,

    /// Whether this class has been fully populated.
    pub is_populated: bool,

    /// The file where this class is defined.
    pub file_path: StrId,

    /// Start offset in the file.
    pub start_offset: u32,

    /// End offset in the file.
    pub end_offset: u32,
}

/// The kind of class-like entity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClassLikeKind {
    #[default]
    Class,
    Interface,
    Trait,
    Enum,
}

/// Information about a class property.
///
/// Modeled after Psalm's PropertyStorage. Stores both the native PHP type hint
/// (`signature_type`) and the docblock type (`type`). The `type` field is the
/// effective type used for analysis - it's the docblock type if present,
/// otherwise the signature type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyInfo {
    pub name: StrId,
    pub declaring_class: StrId,
    /// The effective type for analysis (docblock type if present, else signature type).
    pub property_type: Option<TUnion>,
    /// The native PHP type hint (from property declaration).
    pub signature_type: Option<TUnion>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
    pub has_default: bool,
    pub is_promoted: bool,
    pub is_deprecated: bool,
    pub description: Option<String>,
    pub start_offset: u32,
}

impl PropertyInfo {
    /// Get the effective type for analysis.
    /// Returns the property_type if set, otherwise None.
    pub fn get_type(&self) -> Option<&TUnion> {
        self.property_type.as_ref()
    }

    /// Check if this property has an explicit type declaration (either signature or docblock).
    pub fn has_type(&self) -> bool {
        self.property_type.is_some() || self.signature_type.is_some()
    }
}

/// Information about a class constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassConstantInfo {
    pub name: StrId,
    pub declaring_class: StrId,
    pub constant_type: TUnion,
    pub visibility: Visibility,
    pub is_final: bool,
    pub is_deprecated: bool,
    pub start_offset: u32,
}

/// Template/generic type parameter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateType {
    pub name: StrId,
    pub as_type: TUnion,
    pub variance: TemplateVariance,
}

/// Variance for template types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateVariance {
    #[default]
    Invariant,
    Covariant,
    Contravariant,
}

/// Visibility modifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    #[default]
    Public,
    Protected,
    Private,
}
