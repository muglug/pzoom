//! Storage for class-like entities (classes, interfaces, traits, enums).
//!
//! Modeled after Psalm's `Storage\ClassLikeStorage`.

use indexmap::IndexMap;
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::ttype::template::GenericParent;
use crate::{FunctionLikeInfo, TAtomic, TUnion};

// `PropertyInfo`, `ClassConstantInfo`, and `Visibility` live in their own modules
// (mirroring Hakana's `property_info.rs` / `class_constant_info.rs` /
// `member_visibility.rs`). Re-exported here so existing `class_like_info::*`
// paths keep resolving.
pub use crate::class_constant_info::ClassConstantInfo;
pub use crate::member_visibility::Visibility;
pub use crate::property_info::PropertyInfo;

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

    /// Whether this class is immutable (@psalm-immutable / @immutable).
    pub is_immutable: bool,

    /// Class-level `@psalm-taint-specialize`: taints on instances and their
    /// non-static method calls are tracked per call site instead of globally
    /// (Psalm's `ClassLikeStorage::$specialize_instance`).
    #[serde(default)]
    pub specialize_instance: bool,

    /// The promised yield type (`@psalm-yield T`): `yield`ing an instance of
    /// this class produces this type (Psalm's `ClassLikeStorage::$yield`).
    #[serde(default)]
    pub yield_type: Option<TUnion>,

    /// The class that declared `yield_type` when inherited (Psalm's
    /// `declaring_yield_fqcn`) — templates in the yield type are defined there.
    #[serde(default)]
    pub declaring_yield_class: Option<StrId>,

    /// Whether this class is externally-mutation-free
    /// (`@psalm-external-mutation-free`): its methods may mutate `$this` but not
    /// any external state, so calling them on a freshly-constructed (reference
    /// -free) instance is allowed from a pure context.
    #[serde(default)]
    pub is_external_mutation_free: bool,

    /// Whether the class is declared `@psalm-api`/`@api` (Psalm's
    /// `public_api`): exempt from UnusedClass/ClassMustBeFinal, and its
    /// public members from PossiblyUnusedMethod/Property.
    #[serde(default)]
    pub is_public_api: bool,

    /// Whether the class is instantiated/invoked dynamically by a framework
    /// (reflectively), so the analyzed code never references it directly. Set by
    /// a plugin's post-populate hook — e.g. the PHPUnit plugin marks `TestCase`
    /// subclasses, which the test runner discovers and runs. Exempt from
    /// `UnusedClass`.
    #[serde(default)]
    pub dynamically_callable: bool,

    /// Whether `@psalm-consistent-templates` is declared: child classes must
    /// keep the parent's template signature, making `new static` on a
    /// templated class safe (Psalm's `enforce_template_inheritance`).
    #[serde(default)]
    pub enforce_template_inheritance: bool,

    /// Whether constructors must keep parameter names across inheritance.
    #[serde(default)]
    pub is_consistent_constructor: bool,

    /// Whether dynamic properties are allowed via `@psalm-no-seal-properties`.
    pub no_seal_properties: bool,

    /// Whether pseudo properties are sealed (unknown pseudo properties are disallowed).
    pub sealed_properties: Option<bool>,

    /// Whether pseudo methods are sealed (unknown magic methods are disallowed).
    pub sealed_methods: Option<bool>,

    /// Whether interface intersections should ignore method visibility checks.
    pub override_method_visibility: bool,

    /// Whether interface intersections should ignore property visibility checks.
    pub override_property_visibility: bool,

    /// Parsed `#[Attribute(...)]` flags when this class is an attribute class.
    ///
    /// `None` means this class is not marked with `#[Attribute]`.
    pub attribute_flags: Option<u8>,

    /// PHP attributes on this class, keyed by resolved attribute-class name, with
    /// each occurrence's evaluated argument list (see [`crate::AttributeMap`]).
    /// Lets plugins read framework attributes (e.g. PHPUnit's
    /// `#[CoversClass(...)]`) without the scanner knowing about them.
    #[serde(default)]
    pub attributes: crate::AttributeMap,

    /// Direct parent class (if any). This is the immediate parent.
    pub parent_class: Option<StrId>,

    /// All parent classes in the inheritance chain (populated during population phase).
    /// Includes parent, grandparent, great-grandparent, etc.
    pub all_parent_classes: Vec<StrId>,

    /// `@psalm-inheritors A|B` — the closed set of allowed subtypes
    /// (Psalm's `ClassLikeStorage::$inheritors`). A negated instanceof on
    /// this type expands to the listed alternatives.
    #[serde(default)]
    pub inheritors: Vec<TAtomic>,

    /// True when a stub file (also) declared this class. Psalm marks such
    /// storages `stubbed`; their constructors are opaque to the property
    /// initialization simulation.
    #[serde(default)]
    pub is_stubbed: bool,

    /// Class names from positive `class_exists(...)`/`interface_exists(...)`
    /// guards on the `if` block this declaration sits in. When any guard class
    /// is unknown after the scan, Psalm's ReflectorVisitor would never have
    /// registered this declaration (ExpressionResolver::enterConditional
    /// returns false), so analysis skips it.
    #[serde(default)]
    pub conditional_guard_classes: Vec<StrId>,

    /// Directly implemented/extended interfaces (declared on this class/interface).
    pub interfaces: FxHashSet<StrId>,

    /// All parent interfaces including inherited ones (populated during population phase).
    pub all_parent_interfaces: Vec<StrId>,

    /// Used traits.
    /// Traits used by this class (insertion order = declaration order, then
    /// inherited traits appended by the populator) — iteration order is
    /// load-bearing for trait member precedence and template remapping.
    pub used_traits: indexmap::IndexSet<StrId, rustc_hash::FxBuildHasher>,

    /// Required parent classes from docblock annotations (e.g. `@psalm-require-extends`).
    pub required_extends: Vec<StrId>,

    /// Required interfaces from docblock annotations (e.g. `@psalm-require-implements`).
    pub required_implements: Vec<StrId>,

    /// Trait method aliases declared via `use Trait { method as alias; }`.
    pub trait_method_aliases: Vec<TraitMethodAlias>,

    /// Methods defined directly in this class (method names only).
    /// The actual method info is stored in the codebase's functionlike_infos.
    pub method_names: FxHashSet<StrId>,

    /// Methods available on this class (including inherited). Values are
    /// `Arc`-shared: the populate phase flattens ancestor methods into every
    /// descendant, and ~90% of entries are inherited — sharing makes those
    /// refcount bumps instead of deep `FunctionLikeInfo` copies (Psalm avoids
    /// the duplication via `declaring_method_ids` indirection instead).
    pub methods: FxHashMap<StrId, std::sync::Arc<FunctionLikeInfo>>,

    /// Lowercase method name -> correctly-cased method name, only for methods
    /// whose declared name differs from its lowercase form. pzoom resolves
    /// method names case-sensitively; this map recovers the declared casing for
    /// diagnostics and for PHP's case-insensitive override matching.
    #[serde(default)]
    pub method_lc_names: FxHashMap<StrId, StrId>,

    /// Pseudo instance methods from `@method` annotations.
    pub pseudo_methods: FxHashMap<StrId, FunctionLikeInfo>,

    /// Pseudo static methods from `@method static` annotations.
    pub pseudo_static_methods: FxHashMap<StrId, FunctionLikeInfo>,

    /// Maps method name to the class that declares it.
    pub declaring_method_ids: FxHashMap<StrId, StrId>,

    /// Maps method name to the class where it appears (for traits, this is the using class).
    pub appearing_method_ids: FxHashMap<StrId, StrId>,

    /// Methods that can be inherited by child classes.
    pub inheritable_method_ids: FxHashMap<StrId, StrId>,

    /// Maps a method name to the set of ancestor classes whose method it
    /// overrides. Populated during inheritance, mirroring Psalm's
    /// `ClassLikeStorage::$overridden_method_ids`: a method overrides an
    /// ancestor when it comes from a parent class, an implemented/extended
    /// interface, or an **abstract** method required by a used trait.
    pub overridden_method_ids: FxHashMap<StrId, FxHashSet<StrId>>,

    /// Psalm `ClassLikeStorage::$documenting_method_ids`: maps an appearing
    /// method name to the ancestor `MethodIdentifier` whose docblock documents
    /// its return type. Computed at populate time (mirroring Psalm's Populator)
    /// and consulted lazily by the method-call return-type fetcher, mirroring
    /// `Methods::getMethodReturnType`.
    #[serde(default)]
    pub documenting_method_ids: FxHashMap<StrId, crate::method_identifier::MethodIdentifier>,

    /// Properties defined in this class.
    /// Properties on this class (including inherited). Values are
    /// `Arc`-shared like `methods`: inheritance flattening is a refcount bump
    /// instead of a deep `PropertyInfo` copy.
    pub properties: FxHashMap<StrId, std::sync::Arc<PropertyInfo>>,

    /// Property name strings -> interned ids, built at populate time so
    /// interner-less contexts (the type comparators matching `object{...}`
    /// shape keys against class properties) can resolve names.
    #[serde(default)]
    pub property_name_lookup: FxHashMap<String, StrId>,

    /// Pseudo property write types from `@property`/`@property-write` docblocks.
    pub pseudo_property_set_types: FxHashMap<StrId, TUnion>,

    /// Pseudo property read types from `@property`/`@property-read` docblocks.
    pub pseudo_property_get_types: FxHashMap<StrId, TUnion>,

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

    /// Templates with no public mutation channel (Hakana's
    /// `template_readonly`): every class template starts here and is removed
    /// when it appears in a public non-constructor method parameter or a
    /// public property type. Readonly templates resolve eagerly at `new`
    /// (no type variable is minted — nothing later can constrain them).
    #[serde(default)]
    pub template_readonly: FxHashSet<StrId>,

    /// Generic arguments provided to extended/implemented classlikes.
    ///
    /// Keyed by the extended/implemented classlike name, values are ordered by
    /// template declaration order on that classlike.
    pub template_extended_offsets: FxHashMap<StrId, Vec<TUnion>>,

    /// Resolved template map for extended/implemented classlikes.
    ///
    /// Keyed by classlike name, then by template parameter name.
    pub template_extended_params: IndexMap<StrId, IndexMap<StrId, TUnion>>,

    /// Mixins declared via `@mixin` annotations.
    pub named_mixins: Vec<TAtomic>,

    /// The class where mixins were originally declared.
    pub mixin_declaring_class: Option<StrId>,

    /// Docblock parse/validation issues collected during scanning.
    pub docblock_issues: Vec<DocblockIssue>,

    /// Duplicate property declarations collected during scanning.
    #[serde(default)]
    pub duplicate_property_issues: Vec<DuplicatePropertyIssue>,

    /// Duplicate constant/enum-case declarations collected during scanning
    /// (Psalm's ClassLikeNodeScanner DuplicateConstant).
    #[serde(default)]
    pub duplicate_constant_issues: Vec<DuplicatePropertyIssue>,

    /// Duplicate method declarations collected during scanning (Psalm's
    /// FunctionLikeNodeScanner DuplicateMethod). Reuses the name+offset holder;
    /// `property_name` carries the method name.
    #[serde(default)]
    pub duplicate_method_issues: Vec<DuplicatePropertyIssue>,

    /// Whether this class has been deprecated.
    pub is_deprecated: bool,

    /// Deprecation message if deprecated.
    pub deprecation_message: Option<String>,

    /// Whether this is an internal class (not for external use).
    pub is_internal: bool,

    /// Internal visibility scopes for this class (`@internal` / `@psalm-internal`).
    ///
    /// Empty means the class is publicly accessible.
    pub internal: Vec<StrId>,

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

    /// Span of the class/interface/trait/enum NAME token — Psalm anchors
    /// class-wide issues here rather than on the whole declaration span.
    pub name_location: Option<(u32, u32)>,
}

impl ClassLikeInfo {
    /// Find the correctly-cased method for a reference that failed exact
    /// lookup. Returns the declared name when it differs only by case from
    /// `requested`; never returns `requested`.
    pub fn cased_method_for(
        &self,
        interner: &pzoom_str::Interner,
        requested: StrId,
    ) -> Option<StrId> {
        let lc = interner.lookup(requested).to_ascii_lowercase();
        let lc_id = interner.intern(&lc);
        let cased = if self.methods.contains_key(&lc_id) {
            lc_id
        } else {
            *self.method_lc_names.get(&lc_id)?
        };
        (cased != requested).then_some(cased)
    }
}

/// Docblock issue captured during scanning.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocblockIssue {
    pub message: String,
    pub start_offset: u32,
    pub end_offset: u32,
}

/// Duplicate property declaration issue captured during scanning.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuplicatePropertyIssue {
    pub property_name: StrId,
    pub start_offset: u32,
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
/// Template/generic type parameter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateType {
    pub name: StrId,
    /// The class-like that defines this template (Psalm's `$defining_class`).
    pub defining_entity: GenericParent,
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

/// Metadata for a trait method alias adaptation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraitMethodAlias {
    /// Explicit trait for the aliased method (`TraitName::method as alias`), if provided.
    pub trait_name: Option<StrId>,
    /// Original method name from the trait.
    pub original_name: StrId,
    /// Alias method name that should be added to the consuming class/trait.
    pub alias_name: StrId,
    /// Optional visibility override from the adaptation (`as private`, etc).
    pub visibility: Option<Visibility>,
}
