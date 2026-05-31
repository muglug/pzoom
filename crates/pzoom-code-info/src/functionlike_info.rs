//! Storage for function-like entities (functions, methods, closures).
//!
//! Modeled after Psalm's `Storage\FunctionLikeStorage`.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::{
    TUnion,
    class_like_info::{DocblockIssue, Visibility},
};

/// Information about a function or method.
///
/// Modeled after Psalm's FunctionLikeStorage. Stores both the native PHP type hint
/// (`signature_return_type`) and the docblock type (`return_type`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionLikeInfo {
    /// Fully qualified name of the function.
    pub name: StrId,

    /// For methods, the class that declares this method.
    pub declaring_class: Option<StrId>,

    /// Parameters.
    pub params: Vec<ParamInfo>,

    /// Return type (effective type for analysis - docblock if present, else signature).
    pub return_type: Option<TUnion>,

    /// The native PHP return type hint.
    pub signature_return_type: Option<TUnion>,

    /// Whether this function is pure (no side effects).
    pub is_pure: bool,

    /// Whether this function is mutation-free (no state mutations).
    pub is_mutation_free: bool,

    /// Whether this method may mutate `$this` but not external state
    /// (`@psalm-external-mutation-free`, or inferred for getters / simple
    /// property-assigning constructors). Mirrors Psalm's
    /// `MethodStorage::$external_mutation_free`.
    #[serde(default)]
    pub is_external_mutation_free: bool,

    /// Whether the mutation-free / external-mutation-free status was *inferred*
    /// from the body rather than declared. Psalm tracks this
    /// (`mutation_free_inferred`) to avoid trusting inference on non-final
    /// methods that subclasses may override.
    #[serde(default)]
    pub mutation_free_inferred: bool,

    /// Whether this is a static method.
    pub is_static: bool,

    /// Whether this is an abstract method.
    pub is_abstract: bool,

    /// Whether this is a final method.
    pub is_final: bool,

    /// Visibility (for methods).
    pub visibility: Visibility,

    /// Template/generic type parameters.
    pub template_types: Vec<FunctionTemplateType>,

    /// Receiver-type constraint from `@psalm-if-this-is`.
    pub if_this_is_type: Option<TUnion>,

    /// Docblock parse/validation issues collected during scanning.
    pub docblock_issues: Vec<DocblockIssue>,

    /// Whether this method/function docblock requests inherited annotations
    /// via `@inheritdoc`/`@inheritDoc` or inline description marker.
    pub inherits_docblock: bool,

    /// Whether this function has been deprecated.
    pub is_deprecated: bool,

    /// Deprecation message if deprecated.
    pub deprecation_message: Option<String>,

    /// Whether this is an internal function (not for external use).
    pub is_internal: bool,

    /// Internal visibility scopes (`@internal` / `@psalm-internal`).
    ///
    /// Empty means the function/method is publicly accessible.
    pub internal: Vec<StrId>,

    /// Whether this function returns by reference.
    pub returns_by_ref: bool,

    /// Whether this function is variadic.
    pub is_variadic: bool,

    /// Whether named arguments are disallowed for this function/method.
    /// Set by `@no-named-arguments` / `@psalm-no-named-arguments`.
    pub no_named_arguments: bool,

    /// Constants defined by this function via `define("NAME", ...)`.
    pub defined_constants: Vec<(StrId, TUnion)>,

    /// The file where this function is defined.
    pub file_path: StrId,

    /// Start offset in the file.
    pub start_offset: u32,

    /// End offset in the file.
    pub end_offset: u32,

    /// Assertions about parameter types (from @psalm-assert annotations).
    pub assertions: Vec<Assertion>,

    /// If-true assertions (from @psalm-assert-if-true).
    pub if_true_assertions: Vec<Assertion>,

    /// If-false assertions (from @psalm-assert-if-false).
    pub if_false_assertions: Vec<Assertion>,

    /// Whether the method carries the `#[\Override]` attribute.
    #[serde(default)]
    pub has_override_attribute: bool,

    /// Names of `$this->X` properties assigned within this method's body.
    /// Mirrors Psalm's `MethodStorage::$this_property_mutations`, collected
    /// syntactically during scanning. Used to decide which property narrowings
    /// to drop in a caller after a non-mutation-free method call.
    #[serde(default)]
    pub this_property_mutations: Vec<StrId>,
}

impl FunctionLikeInfo {
    /// Get the effective return type for analysis: the docblock `return_type` if present,
    /// otherwise the native `signature_return_type`. Mirrors Psalm using
    /// `return_type ?: signature_return_type` at use sites while keeping the two stored
    /// separately.
    pub fn get_return_type(&self) -> Option<&TUnion> {
        self.return_type.as_ref().or(self.signature_return_type.as_ref())
    }

    /// Check if this function has an explicit return type (either signature or docblock).
    pub fn has_return_type(&self) -> bool {
        self.return_type.is_some() || self.signature_return_type.is_some()
    }
}

/// Information about a function parameter.
///
/// Modeled after Psalm's FunctionLikeParameter. Stores both the native PHP type hint
/// (`signature_type`) and the docblock type (`param_type`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: StrId,
    /// The effective type for analysis (docblock if present, else signature).
    pub param_type: Option<TUnion>,
    /// The by-ref out type from `@param-out`/`@psalm-param-out`.
    pub param_out_type: Option<TUnion>,
    /// The native PHP type hint.
    pub signature_type: Option<TUnion>,
    /// Whether this param has a docblock type annotation.
    pub has_docblock_type: bool,
    pub is_optional: bool,
    pub is_variadic: bool,
    pub by_ref: bool,
    pub is_promoted: bool,
    pub default_type: Option<TUnion>,
    pub description: Option<String>,
    pub start_offset: u32,
}

impl Default for ParamInfo {
    fn default() -> Self {
        Self {
            name: StrId::EMPTY,
            param_type: None,
            param_out_type: None,
            signature_type: None,
            has_docblock_type: false,
            is_optional: false,
            is_variadic: false,
            by_ref: false,
            is_promoted: false,
            default_type: None,
            description: None,
            start_offset: 0,
        }
    }
}

impl ParamInfo {
    /// Get the effective type for analysis: the docblock `param_type` if present,
    /// otherwise the native `signature_type`. Mirrors Psalm's `param_type ?:
    /// signature_type` while keeping the two stored separately.
    pub fn get_type(&self) -> Option<&TUnion> {
        self.param_type.as_ref().or(self.signature_type.as_ref())
    }

    /// Check if this parameter has an explicit type (either signature or docblock).
    pub fn has_type(&self) -> bool {
        self.param_type.is_some() || self.signature_type.is_some()
    }
}

/// Template type parameter for a function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionTemplateType {
    pub name: StrId,
    /// The entity that defines this template: the function's name for plain
    /// functions, `"Class::method"` for methods (Psalm's `$defining_class`,
    /// which uses `fn-`-prefixed ids for function-likes).
    pub defining_entity: StrId,
    pub as_type: TUnion,
}

// Conditional return types are a type-level concern (Psalm's `Type\Atomic\TConditional`),
// carried on the return TUnion as `TAtomic::TConditional`, not stored here.
pub use crate::t_atomic::{ConditionalReturnCondition, ConditionalReturnType};

/// An assertion about a parameter type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assertion {
    /// The parameter name or `$this`.
    pub var_id: StrId,
    /// The asserted type or type negation.
    pub assertion_type: AssertionType,
}

/// The type of assertion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AssertionType {
    /// Assert that the variable IS this type.
    IsType(TUnion),
    /// Assert that the variable is strictly identical to this type/value.
    IsEqual(TUnion),
    /// Assert that the variable is loosely equal to this type/value.
    IsLooselyEqual(TUnion),
    /// Assert that the variable is NOT this type.
    IsNotType(TUnion),
    /// Assert that the variable is not strictly identical to this type/value.
    IsNotEqual(TUnion),
    /// Assert that the variable is not loosely equal to this type/value.
    IsNotLooselyEqual(TUnion),
    /// Assert that the variable is truthy.
    Truthy,
    /// Assert that the variable is falsy.
    Falsy,
    /// Assert that the variable is not null.
    NotNull,
    /// Assert that the variable is not empty.
    NotEmpty,
}
