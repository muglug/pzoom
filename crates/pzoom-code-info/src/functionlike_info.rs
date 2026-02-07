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

    /// Conditional return type metadata from docblocks.
    /// Used to evaluate Psalm-style conditional return branches at callsites.
    pub conditional_return_type: Option<ConditionalReturnType>,

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
}

impl FunctionLikeInfo {
    /// Get the effective return type for analysis.
    /// Returns the return_type if set, otherwise None.
    pub fn get_return_type(&self) -> Option<&TUnion> {
        self.return_type.as_ref()
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
    /// Get the effective type for analysis.
    /// Returns the param_type if set, otherwise None.
    pub fn get_type(&self) -> Option<&TUnion> {
        self.param_type.as_ref()
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
    pub as_type: TUnion,
}

/// Conditional return type information from a docblock return annotation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionalReturnType {
    pub condition: ConditionalReturnCondition,
    pub if_true_type: TUnion,
    pub if_false_type: TUnion,
}

/// Condition controlling a conditional return type branch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConditionalReturnCondition {
    /// Template condition, e.g. `TType is 'array'`.
    TemplateIs {
        template_name: StrId,
        asserted_type: TUnion,
    },
    /// Argument-count condition, e.g. `func_num_args() is 1`.
    FuncNumArgsIs { count: usize },
}

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
