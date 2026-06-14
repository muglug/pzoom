//! Storage for function-like entities (functions, methods, closures).
//!
//! Modeled after Psalm's `Storage\FunctionLikeStorage`.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::{
    TUnion,
    class_like_info::{DocblockIssue, Visibility},
    data_flow::node::SinkType,
    ttype::template::GenericParent,
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

    /// `@global Type $var` declarations on the function docblock (Psalm's
    /// `FunctionLikeStorage::$global_types`). Keyed by the interned variable
    /// name (including the leading `$`); consulted when a `global $var;`
    /// statement imports the variable.
    #[serde(default)]
    pub global_types: Vec<(StrId, TUnion)>,

    /// Return type (effective type for analysis - docblock if present, else signature).
    pub return_type: Option<TUnion>,
    /// The docblock @return mentions `static::CONST` — a late-static constant
    /// the scanner resolved against the DECLARING class. Inheritors must not
    /// copy it (their own constant may differ); Psalm keeps such types
    /// unresolved until call time.
    #[serde(default)]
    pub return_type_mentions_static_const: bool,

    /// Span of the declared return type (the `@return` docblock type string),
    /// used as the dataflow origin for "Consider improving the type at …"
    /// suffixes on Mixed* issues (Psalm's `return_type_location`).
    pub return_type_location: Option<(u32, u32)>,

    /// Span of the function/method NAME token: the issue position for
    /// declarations lacking a return type node (Psalm's name location).
    pub name_location: Option<(u32, u32)>,

    /// Whether the populator has processed this function's types (lets
    /// repeated populate passes skip already-populated symbols, like the
    /// classlike `is_populated` flag).
    pub is_populated: bool,

    /// The native PHP return type hint.
    pub signature_return_type: Option<TUnion>,

    /// Whether this function is pure (no side effects).
    pub is_pure: bool,

    /// Whether this function appears in the PHP CallMap — Psalm treats
    /// CallMap builtins as pure unless impure-listed, regardless of where
    /// the declaration was scanned from (stubs or a vendor polyfill).
    pub in_call_map: bool,

    /// Whether the docblock declares `@throws` (Psalm's
    /// `FunctionLikeStorage::$throws`, used to exempt mutation-free methods
    /// called for their exception effect from UnusedMethodCall).
    #[serde(default)]
    pub has_throws: bool,

    /// `@psalm-api`/`@api` on the member itself (Psalm's
    /// `MethodStorage::$public_api`) — exempt from unused-member reporting.
    #[serde(default)]
    pub is_public_api: bool,

    /// Docblock `@param` tags that name no signature parameter, when the
    /// signature has no undertyped params (Psalm's
    /// `unused_docblock_parameters` — UnusedDocblockParam under
    /// find_unused_code).
    #[serde(default)]
    pub unused_docblock_params: Vec<(String, u32)>,

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

    /// Psalm `MethodStorage::$inherited_return_type`: the docblock return type
    /// was inherited from an overridden method (populator's documenting-method
    /// pass), so signature-mismatch comparisons must not treat it as the
    /// method's own declaration.
    #[serde(default)]
    pub inherited_return_type: bool,

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

    /// Whether the method carries the `#[\ReturnTypeWillChange]` attribute,
    /// which exempts it from native return-type signature checks against
    /// inherited methods (Psalm's MethodComparator attribute check).
    #[serde(default)]
    pub has_return_type_will_change_attribute: bool,

    /// Names of `$this->X` properties assigned within this method's body.
    /// Mirrors Psalm's `MethodStorage::$this_property_mutations`, collected
    /// syntactically during scanning. Used to decide which property narrowings
    /// to drop in a caller after a non-mutation-free method call.
    #[serde(default)]
    pub this_property_mutations: Vec<StrId>,

    /// Taint-tracking metadata from `@psalm-taint-*` / `@psalm-flow`
    /// docblock tags and the builtin sink map.
    #[serde(default)]
    pub taints: FunctionLikeTaints,

    /// Declared under an `if (!function_exists('name'))` polyfill guard:
    /// when the function already exists the declaration never runs (Psalm's
    /// enterConditional skips the branch), so it neither clashes with nor
    /// replaces the existing definition.
    #[serde(default)]
    pub declared_if_not_exists: bool,

    /// Ordered initialization-relevant events of this method's body
    /// (assignments to `$this->X`, `$this`-bound calls, exhaustive
    /// alternations). The property-initialization check expands these the way
    /// Psalm's `collect_initializations` constructor simulation would.
    #[serde(default)]
    pub initializer_events: Vec<InitializerEvent>,

    /// `(property, offset)` reads of `$this->X` reached before any assignment
    /// or `$this` method call in this body (Psalm's `UninitializedProperty`
    /// candidates when this method is a constructor).
    #[serde(default)]
    pub initializer_uninit_reads: Vec<(StrId, u32)>,
}

/// One initialization-relevant step of a method body, in execution order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InitializerEvent {
    /// `$this->prop = ...` — a direct assignment.
    Assign(StrId),
    /// A bare `$this->prop` passed to a followable call by-ref (kept distinct
    /// from a direct assignment: the analysis-time constructor re-analysis
    /// supersedes direct assignments but cannot see a by-ref write-back).
    AssignByRef(StrId),
    /// `$this->m()`, `self::m()`, `static::m()` — resolved against the class
    /// being checked, so overriding methods win.
    ThisCall(StrId),
    /// `parent::m()` — resolved against the declaring class's parent.
    ParentCall(StrId),
    /// `SomeClass::m()` — the class name as written, resolved at check time
    /// against the checked class's ancestors.
    NamedCall(StrId, StrId),
    /// An exhaustive alternation: every path takes exactly one alternative.
    Branch(Vec<Vec<InitializerEvent>>),
}

/// Taint-tracking metadata for a function-like (Psalm keeps these directly on
/// `FunctionLikeStorage`; pzoom groups them so construction sites stay small).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionLikeTaints {
    /// `@psalm-taint-source <kind>`: calling this function introduces these
    /// taints on its return value.
    pub taint_source_types: Vec<SinkType>,
    /// `@psalm-taint-unescape <kind>`: taints added to data flowing through.
    pub added_taints: Vec<SinkType>,
    /// `@psalm-taint-escape <kind>`: taints removed from data flowing through.
    pub removed_taints: Vec<SinkType>,
    /// `@psalm-taint-escape (<conditional>)`: parsed conditional types whose
    /// subject is a parameter (`($escape is true ? "html" : null)`), resolved
    /// against call-site arguments (Psalm's `conditionally_removed_taints`).
    #[serde(default)]
    pub conditionally_removed_taints: Vec<crate::t_atomic::ConditionalReturnType>,
    /// `@psalm-flow ($a, $b) -> return`: param indexes whose taints flow into
    /// the return value, with the flow's path type (Psalm's
    /// `return_source_params`).
    pub return_source_params: Vec<(usize, String)>,
    /// `@psalm-flow proxy other_fn($a, $b) [-> return]`: calling this
    /// function behaves like calling `other_fn` with the named params
    /// (Psalm's `proxy_calls`, which it implements with a fake call node).
    pub proxy_calls: Vec<TaintProxyCall>,
    /// `@psalm-taint-specialize` (also implied by `@psalm-pure`): taints are
    /// tracked per-callsite instead of globally.
    pub specialize_call: bool,
}

/// One `@psalm-flow proxy` declaration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaintProxyCall {
    /// The proxied function (`some_fn`) or `Class::method` id, as written.
    pub fqn: String,
    /// Indexes of this function's params that become the proxied call's
    /// arguments, in order.
    pub params: Vec<usize>,
    /// Whether the proxied call's return value flows into this function's
    /// return value (`-> return`).
    pub returns: bool,
}

impl FunctionLikeTaints {
    pub fn is_empty(&self) -> bool {
        self.taint_source_types.is_empty()
            && self.added_taints.is_empty()
            && self.removed_taints.is_empty()
            && self.conditionally_removed_taints.is_empty()
            && self.return_source_params.is_empty()
            && self.proxy_calls.is_empty()
            && !self.specialize_call
    }
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
    /// Psalm's `expect_variable`: internal-stub params named `haystack` expect
    /// a non-literal value (passing a literal flags InvalidLiteralArgument).
    #[serde(default)]
    pub expect_variable: bool,
    pub default_type: Option<TUnion>,
    pub description: Option<String>,
    pub start_offset: u32,
    /// `@psalm-taint-sink <kind> $param` (and the builtin sink map): tainted
    /// data must not reach this parameter.
    #[serde(default)]
    pub sinks: Vec<SinkType>,
    /// `@psalm-assert-untainted $param`: the argument loses its dataflow
    /// parents after the call.
    #[serde(default)]
    pub assert_untainted: bool,
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
            expect_variable: false,
            default_type: None,
            description: None,
            start_offset: 0,
            sinks: Vec::new(),
            assert_untainted: false,
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
    /// True when this template is the subject of a conditional type in the
    /// same function (`(T is X ? ... : ...)` in @return / @param-out /
    /// @psalm-taint-escape). Conditional branch picking discriminates on
    /// literal values, so bounds for these templates keep argument literals
    /// (Psalm semantics) instead of Hakana's `generalize_literals`.
    #[serde(default)]
    pub conditional_subject: bool,
    /// The entity that defines this template (Hakana's `GenericParent`):
    /// `FunctionLike` of the function's name for plain functions or of
    /// `"Class::method"` for methods (Psalm's `$defining_class` strings,
    /// without the `fn-` prefix).
    pub defining_entity: GenericParent,
    pub as_type: TUnion,
}

// Conditional return types are a type-level concern (Psalm's `Type\Atomic\TConditional`),
// carried on the return TUnion as `TAtomic::TConditional`, not stored here.
pub use crate::t_atomic::ConditionalReturnType;

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
