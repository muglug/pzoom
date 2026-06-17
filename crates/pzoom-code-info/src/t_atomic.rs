//! Atomic types - the building blocks of the type system.
//!
//! Modeled after Psalm's `Type\Atomic` hierarchy.

use pzoom_str::{Interner, PRELOADED_STRINGS, StrId};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::TUnion;
use crate::ttype::template::GenericParent;

/// Marker used to represent Psalm/Hakana's non-specific `literal-string`.
pub const NON_SPECIFIC_LITERAL_STRING_VALUE: &str = "@@pzoom_literal_string@@";

/// An atomic type - represents a single, non-union type.
///
/// This enum covers all PHP types that Psalm understands, including
/// literal types, generic types, and special types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TAtomic {
    // Scalar types
    TInt,
    /// `literal-int` — an int known to come from a literal in the codebase
    /// (Psalm's TNonspecificLiteralInt).
    TNonspecificLiteralInt,
    TFloat,
    TString,
    TBool,
    TTrue,
    TFalse,
    TNull,

    // Literal types
    TLiteralInt {
        value: i64,
    },
    TLiteralFloat {
        value: f64,
    },
    TLiteralString {
        value: String,
    },
    TLiteralClassString {
        name: String,
    },

    /// The result type of `get_class($x)` where `$x` is a variable: a
    /// class-string that *depends on* `$x`. Mirrors Psalm's
    /// `Type\Atomic\TDependentGetClass` (whose field is `$typeof`; `typeof` is a
    /// reserved word in Rust, so the variable id is `var_id` here). `as_type` is
    /// `$x`'s type at the call (`object` when mixed). Lets a later
    /// `get_class($x) === Foo::class` / `switch (get_class($x))` narrow `$x`.
    TDependentGetClass {
        var_id: crate::var_name::VarName,
        as_type: Box<TUnion>,
    },
    /// The result type of `gettype($x)` where `$x` is a variable. Mirrors Psalm's
    /// `Type\Atomic\TDependentGetType`. `var_id` is the interned id of `$x`; a
    /// later `gettype($x) === "string"` / `switch (gettype($x))` narrows `$x`.
    TDependentGetType {
        var_id: crate::var_name::VarName,
    },

    // String subtypes
    TNonEmptyString,
    TNumericString,
    TNonEmptyNumericString,
    TLowercaseString,
    TNonEmptyLowercaseString,
    TTruthyString,
    /// Psalm's `TCallableString` (extends TNonFalsyString): a callable-string
    /// docblock type — a non-falsy string naming a callable.
    TCallableString,
    TClassString {
        as_type: Option<Box<TAtomic>>,
    },

    // Int subtypes
    //
    // `positive-int`, `negative-int`, `non-negative-int` and `non-positive-int`
    // are all represented as `TIntRange` (mirroring Psalm, which lowers every
    // bounded int keyword to a single `TIntRange` atomic):
    //   positive-int     => TIntRange { min: Some(1),  max: None }
    //   negative-int     => TIntRange { min: None,     max: Some(-1) }
    //   non-negative-int => TIntRange { min: Some(0),  max: None }
    //   non-positive-int => TIntRange { min: None,     max: Some(0) }
    TIntRange {
        min: Option<i64>,
        max: Option<i64>,
    },

    // Array types (key difference from Hakana - supports PHP autovivification)
    /// Unified array type. A single variant models every array sort that Psalm
    /// splits across `TArray`/`TNonEmptyArray`/`TList`/`TNonEmptyList`/`TKeyedArray`:
    ///
    /// - `array<K, V>`           — `known_values` empty, `params` `Some((K, V))`, `is_list` false, `is_nonempty` false
    /// - `non-empty-array<K, V>` — as above with `is_nonempty` true
    /// - `list<V>`               — `known_values` empty, `params` `Some((int, V))`, `is_list` true, `is_nonempty` false
    /// - `non-empty-list<V>`     — as above with `is_nonempty` true
    /// - `array{foo: T, bar?: U}`— `known_values` populated, `params` `None` (sealed)
    /// - `list{T, U}`            — `known_values` keyed `0..n`, `is_list` true
    /// - `[]` (empty array)      — `known_values` empty, `params` `None`, `is_nonempty` false
    ///
    /// `params` is the typed fallback for keys outside `known_values`. `is_sealed`
    /// records whether additional keys are forbidden — these are independent:
    /// a sealed shape has no `params`, but an *unsealed* shape may also have no
    /// typed `params` (extra keys allowed, type unknown), matching pzoom's prior
    /// `TKeyedArray { sealed, fallback_* }`. Each known entry carries its own
    /// possibly-undefined flag (Hakana's dict `known_items`). Construct via
    /// [`TAtomic::array`], [`TAtomic::non_empty_array`], [`TAtomic::list`],
    /// [`TAtomic::non_empty_list`] or [`TAtomic::keyed_array`], which normalise
    /// `is_list`/`is_nonempty`.
    TArray {
        /// Known entries; `bool` is `possibly_undefined`. Behind `Arc` so cloning
        /// a shape is a refcount bump (copy-on-write via `Arc::make_mut`).
        known_values: std::sync::Arc<FxHashMap<ArrayKey, (bool, TUnion)>>,
        /// Typed fallback `(key, value)` params for keys outside `known_values`,
        /// or `None` when there is no typed fallback.
        params: Option<Box<(TUnion, TUnion)>>,
        /// Sequential integer keys from 0 (Psalm's `TKeyedArray::$is_list`).
        is_list: bool,
        /// Guaranteed to hold at least one element.
        is_nonempty: bool,
        /// No keys beyond `known_values` are allowed (Psalm's sealed shape /
        /// `fallback_params === null`). Implies `params` is `None`.
        is_sealed: bool,
        /// This array is known to be a valid `callable` (Psalm's
        /// `TCallableKeyedArray` / `TCallableInterface`): a `callable-array`
        /// shape `[class-string|object, non-empty-string]`. Lets the combiner
        /// absorb it into `callable`, like Psalm.
        #[serde(default)]
        is_callable: bool,
    },
    /// `class-string-map<T as Foo, T>` — an array whose value type is a
    /// function of its `class-string` key (Psalm's `Type\Atomic\TClassStringMap`).
    /// `param_name` is the placeholder template name introduced by the first
    /// param, `as_type` its optional named-object upper bound, and
    /// `value_param` the value type (typically referencing the placeholder as a
    /// `TTemplateParam` whose defining entity is `class-string-map`).
    TClassStringMap {
        param_name: StrId,
        as_type: Option<Box<TAtomic>>,
        value_param: Box<TUnion>,
    },

    // Object types
    TNamedObject {
        name: StrId,
        /// Generic type parameters
        type_params: Option<Vec<TUnion>>,
        /// True when this represents the late-static-bound type (`static`/`$this`).
        /// `name` holds the concrete class; `is_static` marks that it should be
        /// re-resolved to the runtime class at each use site. Mirrors Hakana's
        /// `TNamedObject::is_this`.
        is_static: bool,
        /// True when `type_params` were remapped through an `@extends`/`@implements`
        /// clause and should not be re-inferred. Mirrors Hakana's
        /// `TNamedObject::remapped_params`.
        remapped_params: bool,
    },
    TObjectIntersection {
        types: Vec<TAtomic>,
    },
    /// `object{foo: int, bar?: string}` — an object with a known set of
    /// properties (Psalm's `Type\Atomic\TObjectWithProperties`). Unlike a keyed
    /// array these are object instances, so they are assignable to `object` and
    /// only coercible from a bare `object`.
    TObjectWithProperties {
        properties: FxHashMap<ArrayKey, TUnion>,
        /// `stringable-object`: an object guaranteed to have `__toString`
        /// (Psalm models this as a methods-only TObjectWithProperties with
        /// `is_stringable_object_only`).
        #[serde(default)]
        is_stringable: bool,
        /// `callable-object`: an object known to be invokable (Psalm's
        /// TCallableObject) — the object half left when `is_array` is
        /// subtracted from a `callable`.
        #[serde(default)]
        is_invokable: bool,
    },
    TObject,
    TClosedResource,
    TResource,

    // Callable types
    TCallable {
        params: Option<Vec<FunctionLikeParameter>>,
        return_type: Option<Box<TUnion>>,
        is_pure: Option<bool>,
    },
    TClosure {
        params: Option<Vec<FunctionLikeParameter>>,
        return_type: Option<Box<TUnion>>,
        is_pure: Option<bool>,
    },

    // Special types
    TMixed,
    TNonEmptyMixed,
    /// A mixed created by reconciling `isset($arr[$key])` on an unknown slot
    /// inside a loop (Psalm's `TMixed::$from_loop_isset`, Hakana's
    /// `TMixedFromLoopIsset`). Behaves as mixed everywhere, except the type
    /// combiner drops it when any concrete type is present — so loop-fixpoint
    /// placeholder mixeds don't pollute the converged type.
    TMixedFromLoopIsset,
    TNothing, // Never/bottom type
    TVoid,
    TIterable {
        key_type: Box<TUnion>,
        value_type: Box<TUnion>,
    },

    // Template/generic types
    TTemplateParam {
        name: StrId,
        defining_entity: GenericParent,
        as_type: Box<TUnion>,
    },
    TTemplateParamClass {
        name: StrId,
        defining_entity: GenericParent,
        as_type: Box<TAtomic>,
    },

    // Enum types
    TEnum {
        name: StrId,
    },
    TEnumCase {
        enum_name: StrId,
        case_name: StrId,
    },

    // Array key type (int|string)
    TArrayKey,

    // Scalar type (int|float|string|bool)
    TScalar,
    /// Psalm's `TNonEmptyScalar`: any scalar except the empty/falsy ones
    /// ('', '0', 0, 0.0, false). Produced by truthy/`!empty` narrowing of
    /// `scalar`; always truthy.
    TNonEmptyScalar,

    // Numeric type (int|float)
    TNumeric,

    /// A conditional type `(<cond> ? if : else)` (Psalm's `Type\Atomic\TConditional`).
    /// Carried on a function's return type rather than in storage; evaluated at the
    /// call site against the argument/template the condition tests.
    TConditional(Box<ConditionalReturnType>),

    /// `key-of<T>` where `T` is an unresolved template parameter (Psalm's
    /// `Type\Atomic\TTemplateKeyOf`). Kept deferred so a concrete key cannot satisfy
    /// it; resolved to the keys of the bound replacement during template substitution.
    TTemplateKeyOf {
        param_name: StrId,
        defining_entity: GenericParent,
        as_type: Box<TUnion>,
    },
    /// `value-of<T>` where `T` is an unresolved template parameter (Psalm's
    /// `Type\Atomic\TTemplateValueOf`).
    TTemplateValueOf {
        param_name: StrId,
        defining_entity: GenericParent,
        as_type: Box<TUnion>,
    },

    /// A type variable (Hakana's `TTypeVariable`, mirroring the Hack
    /// typechecker): a placeholder (``_N`) whose constraints accumulate in
    /// `FunctionAnalysisData::type_variable_bounds` while the function body is
    /// checked and are reconciled at the end of the function.
    TTypeVariable {
        name: String,
    },

    /// `properties-of<C>` for a concrete class `C` (Psalm's `Type\Atomic\TPropertiesOf`).
    /// Expanded to a keyed array of the class's properties (filtered by visibility) by
    /// the type expander.
    TPropertiesOf {
        classlike_name: StrId,
        visibility_filter: PropertiesOfVisibility,
    },
    /// `properties-of<T>` where `T` is an unresolved template parameter (Psalm's
    /// `Type\Atomic\TTemplatePropertiesOf`).
    TTemplatePropertiesOf {
        param_name: StrId,
        defining_entity: GenericParent,
        visibility_filter: PropertiesOfVisibility,
    },
}

/// Visibility filter for `properties-of<T>` and its public/protected/private variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertiesOfVisibility {
    All,
    Public,
    Protected,
    Private,
}

impl PropertiesOfVisibility {
    /// The docblock utility-type name this filter corresponds to.
    pub fn utility_name(self) -> &'static str {
        match self {
            PropertiesOfVisibility::All => "properties-of",
            PropertiesOfVisibility::Public => "public-properties-of",
            PropertiesOfVisibility::Protected => "protected-properties-of",
            PropertiesOfVisibility::Private => "private-properties-of",
        }
    }
}

/// A conditional type `(<template> is <conditional_type> ? if_true : if_false)`
/// — Psalm's `Type\Atomic\TConditional`. The subject is always a template:
/// a declared one (`TFlags`), one generated from a `$param` reference (kept
/// under its `$name`), or a synthetic (`TFunctionArgCount`,
/// `PHP_MAJOR_VERSION`, `PHP_VERSION_ID`). Template replacement resolves it
/// (Psalm's TemplateInferredTypeReplacer::replaceConditional).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionalReturnType {
    pub param_name: StrId,
    pub defining_entity: GenericParent,
    /// The subject template's declared bound.
    pub as_type: TUnion,
    /// The type the subject is tested against.
    pub conditional_type: TUnion,
    pub if_true_type: TUnion,
    pub if_false_type: TUnion,
}

/// Whether `known_values` form a valid list: integer keys `0..n` with a
/// possibly-undefined "tail" (once one entry is possibly-undefined, every later
/// one is too). Mirrors the invariant Psalm enforces in `TKeyedArray`.
pub fn known_values_form_list(known_values: &FxHashMap<ArrayKey, (bool, TUnion)>) -> bool {
    let mut had_possibly_undefined = false;
    for index in 0..known_values.len() as i64 {
        let Some((possibly_undefined, _)) = known_values.get(&ArrayKey::Int(index)) else {
            return false;
        };
        if had_possibly_undefined && !*possibly_undefined {
            return false;
        }
        if *possibly_undefined {
            had_possibly_undefined = true;
        }
    }
    true
}

/// Whether a shape's known entries guarantee at least one element (some entry is
/// always-defined and not `never`).
pub fn array_known_values_nonempty(known_values: &FxHashMap<ArrayKey, (bool, TUnion)>) -> bool {
    known_values
        .values()
        .any(|(possibly_undefined, value)| !*possibly_undefined && !value.is_nothing())
}

/// Key type for keyed arrays (shapes).
///
/// `ClassString` is a string key that originated from a `Foo::class`
/// expression (Psalm tracks this via `TKeyedArray::$class_strings`). It is the
/// *same array key* as the plain string `"Foo"` — PHP coerces `Foo::class` to
/// that string — so it hashes and compares equal to `String("Foo")`; the
/// distinction only affects how the key is rendered and how it iterates (as a
/// `class-string` rather than a literal string).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ArrayKey {
    Int(i64),
    String(String),
    ClassString(String),
}

impl ArrayKey {
    /// The string value of a string/class-string key, or `None` for an int key.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ArrayKey::Int(_) => None,
            ArrayKey::String(value) | ArrayKey::ClassString(value) => Some(value),
        }
    }
}

impl PartialEq for ArrayKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ArrayKey::Int(a), ArrayKey::Int(b)) => a == b,
            (
                ArrayKey::String(a) | ArrayKey::ClassString(a),
                ArrayKey::String(b) | ArrayKey::ClassString(b),
            ) => a == b,
            _ => false,
        }
    }
}

impl Eq for ArrayKey {}

impl std::hash::Hash for ArrayKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ArrayKey::Int(value) => {
                0u8.hash(state);
                value.hash(state);
            }
            // A class-string key hashes as its plain string so it collides
            // with `String(value)` in the property map.
            ArrayKey::String(value) | ArrayKey::ClassString(value) => {
                1u8.hash(state);
                value.hash(state);
            }
        }
    }
}

impl Ord for ArrayKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (ArrayKey::Int(a), ArrayKey::Int(b)) => a.cmp(b),
            // Int keys sort before string keys (the previous derived order).
            (ArrayKey::Int(_), _) => std::cmp::Ordering::Less,
            (_, ArrayKey::Int(_)) => std::cmp::Ordering::Greater,
            (
                ArrayKey::String(a) | ArrayKey::ClassString(a),
                ArrayKey::String(b) | ArrayKey::ClassString(b),
            ) => a.cmp(b),
        }
    }
}

impl PartialOrd for ArrayKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Parameter for callable/closure types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionLikeParameter {
    pub name: Option<StrId>,
    pub param_type: TUnion,
    pub is_optional: bool,
    pub is_variadic: bool,
    pub by_ref: bool,
}

/// PHP `addcslashes($value, "\0..\37\\\"")`, as Psalm escapes literal string
/// values for display: C-style escapes for the common control characters,
/// three-digit octal for the rest, and escaped backslash/double-quote.
fn addcslashes_control(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\{:03o}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Psalm's `Config::$max_string_length` default: literal strings at or above
/// this length degrade to non-empty-/non-falsy-string (configurable in
/// psalm.xml via `maxStringLength`).
pub const DEFAULT_MAX_STRING_LENGTH: usize = 1000;

impl TAtomic {
    /// Psalm's `Type::getAtomicStringFromLiteral`: a literal string type, unless
    /// the value is at or over the configured length limit, in which case it
    /// degrades to `non-empty-string` (`'0'`) or `non-falsy-string`.
    pub fn string_from_literal(value: String, max_string_length: usize) -> TAtomic {
        if value.is_empty() || value.len() < max_string_length {
            TAtomic::TLiteralString { value }
        } else if value == "0" {
            TAtomic::TNonEmptyString
        } else {
            TAtomic::TTruthyString
        }
    }

    // ---- Unified array constructors (normalise `is_list` / `is_nonempty`) ----

    /// `array<K, V>` — a possibly-empty generic array.
    #[inline]
    pub fn array(key_type: TUnion, value_type: TUnion) -> Self {
        TAtomic::TArray {
            known_values: std::sync::Arc::new(FxHashMap::default()),
            params: Some(Box::new((key_type, value_type))),
            is_list: false,
            is_nonempty: false,
            is_sealed: false,
            is_callable: false,
        }
    }

    /// `non-empty-array<K, V>`.
    #[inline]
    pub fn non_empty_array(key_type: TUnion, value_type: TUnion) -> Self {
        TAtomic::TArray {
            known_values: std::sync::Arc::new(FxHashMap::default()),
            params: Some(Box::new((key_type, value_type))),
            is_list: false,
            is_nonempty: true,
            is_sealed: false,
            is_callable: false,
        }
    }

    /// `list<V>` — a possibly-empty list (integer keys from 0).
    #[inline]
    pub fn list(value_type: TUnion) -> Self {
        TAtomic::TArray {
            known_values: std::sync::Arc::new(FxHashMap::default()),
            params: Some(Box::new((TUnion::new(TAtomic::TInt), value_type))),
            is_list: true,
            is_nonempty: false,
            is_sealed: false,
            is_callable: false,
        }
    }

    /// `non-empty-list<V>`.
    #[inline]
    pub fn non_empty_list(value_type: TUnion) -> Self {
        TAtomic::TArray {
            known_values: std::sync::Arc::new(FxHashMap::default()),
            params: Some(Box::new((TUnion::new(TAtomic::TInt), value_type))),
            is_list: true,
            is_nonempty: true,
            is_sealed: false,
            is_callable: false,
        }
    }

    /// The empty array literal `[]` — pzoom models it as `array<never, never>`
    /// (matching the previous `TArray { never, never }`), not a list.
    #[inline]
    pub fn empty_array() -> Self {
        TAtomic::array(
            TUnion::new(TAtomic::TNothing),
            TUnion::new(TAtomic::TNothing),
        )
    }

    /// General shape constructor. `sealed` drops the fallback; otherwise
    /// `fallback_key`/`fallback_value` (when both present) become `params`.
    /// Normalises `is_list` (sequential int keys + possibly-undefined tail) and
    /// `is_nonempty` (any always-defined entry), like Psalm's `TKeyedArray`.
    #[inline]
    pub fn keyed_array(
        known_values: FxHashMap<ArrayKey, (bool, TUnion)>,
        is_list: bool,
        sealed: bool,
        fallback_key: Option<TUnion>,
        fallback_value: Option<TUnion>,
    ) -> Self {
        let params = if sealed {
            None
        } else {
            match (fallback_key, fallback_value) {
                (Some(k), Some(v)) => Some(Box::new((k, v))),
                _ => None,
            }
        };
        let is_list = is_list && known_values_form_list(&known_values);
        let is_nonempty = array_known_values_nonempty(&known_values);
        TAtomic::TArray {
            known_values: std::sync::Arc::new(known_values),
            params,
            is_list,
            is_nonempty,
            is_sealed: sealed,
            is_callable: false,
        }
    }

    /// Build a shape from an already-built `Arc`, normalising the flags.
    #[inline]
    pub fn keyed_array_arc(
        known_values: std::sync::Arc<FxHashMap<ArrayKey, (bool, TUnion)>>,
        is_list: bool,
        sealed: bool,
        params: Option<Box<(TUnion, TUnion)>>,
    ) -> Self {
        let is_list = is_list && known_values_form_list(&known_values);
        let is_nonempty = array_known_values_nonempty(&known_values);
        TAtomic::TArray {
            known_values,
            params,
            is_list,
            is_nonempty,
            is_sealed: sealed,
            is_callable: false,
        }
    }

    /// Rebuild this `TArray` with new entries and fallback `params`, preserving
    /// its classification flags (`is_list` / `is_nonempty` / `is_sealed` /
    /// `is_callable`). For transforms — template replacement, type expansion —
    /// that rewrite the nested unions but keep the shape's kind and so must NOT
    /// re-normalise the flags (which would, e.g., drop `is_nonempty` from a
    /// generic `non-empty-array<K, V>`). Returns `self.clone()` for a non-array
    /// atomic.
    #[inline]
    pub fn rebuilt_array(
        &self,
        known_values: std::sync::Arc<FxHashMap<ArrayKey, (bool, TUnion)>>,
        params: Option<Box<(TUnion, TUnion)>>,
    ) -> Self {
        match self {
            TAtomic::TArray {
                is_list,
                is_nonempty,
                is_sealed,
                is_callable,
                ..
            } => TAtomic::TArray {
                known_values,
                params,
                is_list: *is_list,
                is_nonempty: *is_nonempty,
                is_sealed: *is_sealed,
                is_callable: *is_callable,
            },
            _ => self.clone(),
        }
    }

    /// A `callable-array` shape — Psalm's `TCallableKeyedArray`. Identical to
    /// [`TAtomic::keyed_array`] but flags the result `is_callable` so the
    /// combiner absorbs it into `callable`.
    #[inline]
    pub fn callable_array(
        known_values: FxHashMap<ArrayKey, (bool, TUnion)>,
        is_list: bool,
        sealed: bool,
        fallback_key: Option<TUnion>,
        fallback_value: Option<TUnion>,
    ) -> Self {
        let mut atomic =
            TAtomic::keyed_array(known_values, is_list, sealed, fallback_key, fallback_value);
        if let TAtomic::TArray { is_callable, .. } = &mut atomic {
            *is_callable = true;
        }
        atomic
    }

    // ---- Unified array accessors ----

    /// True for the unified array atomic (every array sort).
    #[inline]
    pub fn is_array(&self) -> bool {
        matches!(self, TAtomic::TArray { .. })
    }

    /// The fallback `(key, value)` params, or `None` when sealed / not an array.
    #[inline]
    pub fn array_params(&self) -> Option<(&TUnion, &TUnion)> {
        match self {
            TAtomic::TArray { params, .. } => params.as_deref().map(|(k, v)| (k, v)),
            _ => None,
        }
    }

    /// The known entries map (`bool` is possibly-undefined), or `None`.
    #[inline]
    pub fn array_known_values(
        &self,
    ) -> Option<&std::sync::Arc<FxHashMap<ArrayKey, (bool, TUnion)>>> {
        match self {
            TAtomic::TArray { known_values, .. } => Some(known_values),
            _ => None,
        }
    }

    /// Whether this array is a list (sequential int keys from 0).
    #[inline]
    pub fn array_is_list(&self) -> bool {
        matches!(self, TAtomic::TArray { is_list: true, .. })
    }

    /// Whether this array is guaranteed to hold at least one element.
    #[inline]
    pub fn array_is_nonempty(&self) -> bool {
        matches!(
            self,
            TAtomic::TArray {
                is_nonempty: true,
                ..
            }
        )
    }

    /// Whether this array is sealed (no keys allowed beyond `known_values`).
    #[inline]
    pub fn array_is_sealed(&self) -> bool {
        matches!(
            self,
            TAtomic::TArray {
                is_sealed: true,
                ..
            }
        )
    }

    /// True when this is a *generic* array/list — an array with no known entries
    /// (just fallback params), e.g. `array<K,V>` or `list<V>`.
    #[inline]
    pub fn is_generic_array(&self) -> bool {
        matches!(self, TAtomic::TArray { known_values, .. } if known_values.is_empty())
    }

    /// The guaranteed minimum number of entries for a keyed array, mirroring
    /// Psalm's `TKeyedArray::getMinCount()`. Returns `None` for atomics that are
    /// not keyed arrays.
    ///
    /// For a list this is the length of the leading run of always-defined entries;
    /// for a shape it is the count of properties that are neither possibly-undefined
    /// nor `never`.
    pub fn get_min_count(&self) -> Option<usize> {
        let TAtomic::TArray {
            known_values,
            is_list,
            ..
        } = self
        else {
            return None;
        };

        // A generic array/list (no known entries) has no statically-known min.
        if known_values.is_empty() {
            return None;
        }

        if *is_list {
            let mut min_count = 0usize;
            while let Some((possibly_undefined, value)) =
                known_values.get(&ArrayKey::Int(min_count as i64))
            {
                if *possibly_undefined || value.is_nothing() {
                    break;
                }
                min_count += 1;
            }
            return Some(min_count);
        }

        Some(
            known_values
                .values()
                .filter(|(possibly_undefined, value)| !*possibly_undefined && !value.is_nothing())
                .count(),
        )
    }

    /// The maximum number of entries for a keyed array, mirroring Psalm's
    /// `TKeyedArray::getMaxCount()`. Returns `None` when the shape is unsealed (can
    /// hold extra keys) or when the atomic is not a keyed array.
    pub fn get_max_count(&self) -> Option<usize> {
        let TAtomic::TArray {
            known_values,
            is_sealed,
            ..
        } = self
        else {
            return None;
        };

        // Generic (no known entries) or unsealed shapes have no statically-known max.
        if known_values.is_empty() || !*is_sealed {
            return None;
        }

        Some(
            known_values
                .values()
                .filter(|(_, value)| !value.is_nothing())
                .count(),
        )
    }

    /// Construct a plain named object (no type parameters, not late-static-bound).
    #[inline]
    pub fn named_object(name: StrId) -> Self {
        TAtomic::TNamedObject {
            name,
            type_params: None,
            is_static: false,
            remapped_params: false,
        }
    }

    /// Construct a generic named object with the given type parameters.
    #[inline]
    pub fn named_object_with_params(name: StrId, type_params: Option<Vec<TUnion>>) -> Self {
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static: false,
            remapped_params: false,
        }
    }

    /// Returns true if this type is nullable (can be null).
    pub fn is_nullable(&self) -> bool {
        matches!(self, TAtomic::TNull)
    }

    /// For a `class-string-map`, the `class-string` key type used when the map
    /// is treated as a plain array — Psalm's `TClassStringMap::getStandinKeyParam()`:
    /// `class<param_name:class-string-map>` bounded by `as_type` (or `object`).
    /// Returns `None` for every other atomic.
    pub fn get_class_string_map_standin_key_param(&self) -> Option<TAtomic> {
        let TAtomic::TClassStringMap {
            param_name,
            as_type,
            ..
        } = self
        else {
            return None;
        };

        Some(TAtomic::TTemplateParamClass {
            name: *param_name,
            // Psalm's synthetic `class-string-map` defining class — a
            // type-level definition, not a real class-like or function-like.
            defining_entity: GenericParent::TypeDefinition(StrId::CLASS_STRING_MAP),
            as_type: Box::new(as_type.as_deref().cloned().unwrap_or(TAtomic::TObject)),
        })
    }

    /// For a `class-string-map`, the equivalent generic `array` atomic Psalm's
    /// comparators substitute before comparing
    /// (`new TArray([getStandinKeyParam(), value_param])`).
    pub fn get_class_string_map_as_array(&self) -> Option<TAtomic> {
        let TAtomic::TClassStringMap { value_param, .. } = self else {
            return None;
        };

        Some(TAtomic::array(
            TUnion::new(
                self.get_class_string_map_standin_key_param()
                    .expect("checked TClassStringMap above"),
            ),
            (**value_param).clone(),
        ))
    }

    /// For the dependent `get_class`/`gettype` atomics (Psalm's `TDependentGetClass`
    /// / `TDependentGetType`, both `TString` subtypes), the plain string-ish type
    /// they stand in for. Lets type operations that have not been taught about the
    /// dependent variants fall back to the supertype, mirroring Psalm's class
    /// inheritance. Returns `None` for every other atomic.
    pub fn dependent_string_equivalent(&self) -> Option<TAtomic> {
        match self {
            TAtomic::TDependentGetClass { as_type, .. } => {
                let inner = as_type
                    .get_single()
                    .filter(|a| matches!(a, TAtomic::TNamedObject { .. }))
                    .cloned()
                    .map(Box::new);
                Some(TAtomic::TClassString { as_type: inner })
            }
            TAtomic::TDependentGetType { .. } => Some(TAtomic::TString),
            _ => None,
        }
    }

    /// Returns true if this is a literal type.
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralFloat { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TTrue
                | TAtomic::TFalse
        )
    }

    /// Returns true if this type can be falsy.
    /// Whether this atomic *is* (or can be) the boolean `false`, mirroring Psalm's
    /// `Union::isFalsable` (`isset($types['false'])` plus falsable template bounds).
    ///
    /// This is deliberately **not** "could hold a falsy value" — `0`, `""`, `[]`, etc.
    /// are falsy but not falsable. Use [`Self::is_falsy`] for the falsy notion.
    pub fn is_falsable(&self) -> bool {
        match self {
            TAtomic::TFalse => true,
            TAtomic::TTemplateParam { as_type, .. } => as_type.is_falsable(),
            TAtomic::TTemplateParamClass { as_type, .. } => as_type.is_falsable(),
            _ => false,
        }
    }

    /// Returns true if this type is always falsy.
    pub fn is_falsy(&self) -> bool {
        match self {
            TAtomic::TFalse | TAtomic::TNull => true,
            TAtomic::TLiteralInt { value: 0 } => true,
            TAtomic::TLiteralFloat { value } => *value == 0.0,
            TAtomic::TLiteralString { value } => value.is_empty() || value == "0",
            _ => false,
        }
    }

    /// Returns true if this type is always truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            TAtomic::TTrue => true,
            TAtomic::TNonEmptyScalar => true,
            TAtomic::TTruthyString
            | TAtomic::TCallableString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TDependentGetClass { .. }
            | TAtomic::TTemplateParamClass { .. } => true,
            TAtomic::TNonEmptyMixed => true,
            TAtomic::TLiteralInt { value } => *value != 0,
            TAtomic::TLiteralFloat { value } => *value != 0.0,
            TAtomic::TLiteralString { value } => {
                value != NON_SPECIFIC_LITERAL_STRING_VALUE && !value.is_empty() && value != "0"
            }
            // An int range that cannot include 0 (wholly positive or wholly
            // negative) is always truthy. Covers `positive-int`/`negative-int`.
            TAtomic::TIntRange { min, max } => {
                min.is_some_and(|m| m > 0) || max.is_some_and(|m| m < 0)
            }
            TAtomic::TNamedObject { name, .. } => {
                *name != StrId::SIMPLE_XML_ELEMENT && *name != StrId::SIMPLE_XMLITERATOR
            }
            TAtomic::TObject => true,
            TAtomic::TObjectIntersection { .. } => true,
            TAtomic::TResource => true,
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
            TAtomic::TArray {
                known_values,
                is_nonempty,
                ..
            } => {
                *is_nonempty
                    || known_values
                        .values()
                        .any(|(possibly_undefined, _)| !*possibly_undefined)
            }
            _ => false,
        }
    }

    /// Returns a human-readable type identifier, resolving class names through an interner
    /// when available.
    pub fn get_id(&self, interner: Option<&Interner>) -> String {
        fn strid_to_string(id: StrId, interner: Option<&Interner>) -> String {
            if let Some(interner) = interner {
                return interner.lookup(id).to_string();
            }

            PRELOADED_STRINGS
                .get(id.0 as usize)
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("@{}", id.0))
        }

        // pzoom renders the defining entity bare (Psalm's plain
        // `$defining_class` strings, no Hakana `fn-` prefix) so template ids
        // in issue text stay stable.
        fn generic_parent_to_string(parent: &GenericParent, interner: Option<&Interner>) -> String {
            match parent {
                GenericParent::ClassLike(id)
                | GenericParent::FunctionLike(id)
                | GenericParent::TypeDefinition(id) => strid_to_string(*id, interner),
            }
        }

        match self {
            TAtomic::TInt => "int".to_string(),
            TAtomic::TNonspecificLiteralInt => "literal-int".to_string(),
            TAtomic::TFloat => "float".to_string(),
            TAtomic::TString => "string".to_string(),
            TAtomic::TBool => "bool".to_string(),
            TAtomic::TTrue => "true".to_string(),
            TAtomic::TFalse => "false".to_string(),
            TAtomic::TNull => "null".to_string(),
            TAtomic::TLiteralInt { value } => format!("{}", value),
            TAtomic::TLiteralFloat { value } => format!("{}", value),
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    "literal-string".to_string()
                } else {
                    // Psalm `TLiteralString::getId`: quote control characters,
                    // backslashes and double quotes; truncate long values.
                    let escaped = addcslashes_control(value);
                    if value.chars().count() > 80 {
                        format!("'{}...'", escaped.chars().take(80).collect::<String>())
                    } else {
                        format!("'{}'", escaped)
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => "mixed".to_string(),
            TAtomic::TNothing => "never".to_string(),
            TAtomic::TVoid => "void".to_string(),
            TAtomic::TArray {
                known_values,
                params,
                is_list,
                is_nonempty,
                is_sealed: _,
                is_callable: _,
            } => {
                // Render every array sort like Psalm: `array<K, V>` /
                // `non-empty-list<V>` for generic arrays, and
                // `array{foo: int, bar?: string}` / `list{int, string}` for
                // shapes, with an unsealed fallback as `, ...<K, V>` (`, ...<V>`
                // for lists) inside the braces.
                if known_values.is_empty() {
                    // No known items: a generic list/array, or the empty array.
                    return match params.as_deref() {
                        Some((key, value)) => {
                            if *is_list {
                                let prefix = if *is_nonempty {
                                    "non-empty-list"
                                } else {
                                    "list"
                                };
                                format!("{}<{}>", prefix, value.get_id(interner))
                            } else {
                                let prefix = if *is_nonempty {
                                    "non-empty-array"
                                } else {
                                    "array"
                                };
                                format!(
                                    "{}<{}, {}>",
                                    prefix,
                                    key.get_id(interner),
                                    value.get_id(interner)
                                )
                            }
                        }
                        None => "array<never, never>".to_string(),
                    };
                }

                let mut int_entries: Vec<(i64, bool, &TUnion)> = Vec::new();
                // (key, is_class_string, possibly_undefined, value)
                let mut string_entries: Vec<(&str, bool, bool, &TUnion)> = Vec::new();
                for (key, (possibly_undefined, value)) in known_values.iter() {
                    match key {
                        ArrayKey::Int(i) => int_entries.push((*i, *possibly_undefined, value)),
                        ArrayKey::String(s) => {
                            string_entries.push((s.as_str(), false, *possibly_undefined, value))
                        }
                        ArrayKey::ClassString(s) => {
                            string_entries.push((s.as_str(), true, *possibly_undefined, value))
                        }
                    }
                }
                int_entries.sort_by_key(|(i, _, _)| *i);

                // Psalm uses positional list syntax (`list{int, string}`) only when
                // every element is required; an optional element forces explicit keys.
                let all_required = known_values
                    .values()
                    .all(|(possibly_undefined, _)| !*possibly_undefined);
                let use_list_syntax = *is_list && all_required;

                let mut entries: Vec<String> = Vec::new();
                if use_list_syntax {
                    for (_, _, value) in &int_entries {
                        entries.push(value.get_id(interner));
                    }
                } else {
                    for (key, possibly_undefined, value) in &int_entries {
                        let optional = if *possibly_undefined { "?" } else { "" };
                        entries.push(format!("{}{}: {}", key, optional, value.get_id(interner)));
                    }
                    for (key, is_class_string, possibly_undefined, value) in &string_entries {
                        let optional = if *possibly_undefined { "?" } else { "" };
                        // A class-string key renders as `Foo::class` (Psalm's
                        // TKeyedArray::getId), not the bare class name.
                        if *is_class_string {
                            entries.push(format!(
                                "{}::class{}: {}",
                                key,
                                optional,
                                value.get_id(interner)
                            ));
                        } else {
                            entries.push(format!(
                                "{}{}: {}",
                                key,
                                optional,
                                value.get_id(interner)
                            ));
                        }
                    }
                    // Psalm sorts non-list property strings for a stable id.
                    if !*is_list {
                        entries.sort();
                    }
                }

                let params_part = match params.as_deref() {
                    Some((key, value)) => {
                        if *is_list {
                            format!(", ...<{}>", value.get_id(interner))
                        } else {
                            format!(
                                ", ...<{}, {}>",
                                key.get_id(interner),
                                value.get_id(interner)
                            )
                        }
                    }
                    None => String::new(),
                };

                let prefix = if *is_list { "list" } else { "array" };
                format!("{}{{{}{}}}", prefix, entries.join(", "), params_part)
            }
            TAtomic::TObject => "object".to_string(),
            TAtomic::TNamedObject {
                name,
                type_params,
                is_static,
                ..
            } => {
                // The late-static-bound type displays as `static` (the concrete class
                // in `name` is only the resolution target), matching Psalm.
                let mut id = if *is_static {
                    "static".to_string()
                } else {
                    strid_to_string(*name, interner)
                };
                if let Some(type_params) = type_params {
                    let params = type_params
                        .iter()
                        .map(|p| p.get_id(interner))
                        .collect::<Vec<_>>()
                        .join(", ");
                    id.push('<');
                    id.push_str(&params);
                    id.push('>');
                }
                id
            }
            TAtomic::TObjectIntersection { types } => {
                // Psalm renders intersections in declaration order, which is
                // stable for it (single-process array order). pzoom's member
                // order depends on process-nondeterministic StrId assignment,
                // so the rendered ids are sorted for deterministic output —
                // mirroring what Union::getId does for union members.
                let mut type_ids: Vec<String> = Vec::with_capacity(types.len());
                for atomic in types {
                    let type_id = atomic.get_id(interner);
                    if !type_ids.contains(&type_id) {
                        type_ids.push(type_id);
                    }
                }
                type_ids.sort_unstable();
                type_ids.join("&")
            }
            TAtomic::TObjectWithProperties {
                is_stringable: true,
                ..
            } => "stringable-object".to_string(),
            TAtomic::TObjectWithProperties {
                is_invokable: true, ..
            } => "callable-object".to_string(),
            TAtomic::TObjectWithProperties { properties, .. } => {
                let mut entries = properties
                    .iter()
                    .map(|(key, value_type)| {
                        let key_str = match key {
                            ArrayKey::Int(i) => i.to_string(),
                            ArrayKey::String(s) | ArrayKey::ClassString(s) => s.clone(),
                        };
                        let optional = if value_type.possibly_undefined {
                            "?"
                        } else {
                            ""
                        };
                        format!("{}{}: {}", key_str, optional, value_type.get_id(interner))
                    })
                    .collect::<Vec<_>>();
                entries.sort();
                format!("object{{{}}}", entries.join(", "))
            }
            TAtomic::TEnum { name } => strid_to_string(*name, interner),
            TAtomic::TEnumCase {
                enum_name,
                case_name,
            } => format!(
                "{}::{}",
                strid_to_string(*enum_name, interner),
                strid_to_string(*case_name, interner)
            ),
            TAtomic::TCallable {
                params,
                return_type,
                is_pure,
            } => {
                let callable_prefix = match is_pure {
                    Some(true) => "pure-callable",
                    Some(false) => "impure-callable",
                    None => "callable",
                };
                if params.is_none() && return_type.is_none() {
                    callable_prefix.to_string()
                } else {
                    let params_str = params
                        .as_ref()
                        .map(|params| {
                            params
                                .iter()
                                .map(|param| {
                                    let mut p = param.param_type.get_id(interner);
                                    if param.is_variadic {
                                        p = format!("...{}", p);
                                    }
                                    if param.is_optional {
                                        p.push('=');
                                    }
                                    p
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    // Psalm wraps a multi-atomic return type in parentheses,
                    // e.g. `callable():(A|B)`.
                    let return_str = return_type
                        .as_ref()
                        .map(|t| {
                            let id = t.get_id(interner);
                            if t.types.len() > 1 {
                                format!("({})", id)
                            } else {
                                id
                            }
                        })
                        .unwrap_or_else(|| "mixed".to_string());
                    format!("{}({}):{}", callable_prefix, params_str, return_str)
                }
            }
            TAtomic::TClosure {
                params,
                return_type,
                is_pure,
            } => {
                let closure_prefix = match is_pure {
                    Some(true) => "pure-Closure",
                    Some(false) => "impure-Closure",
                    None => "Closure",
                };
                if params.is_none() && return_type.is_none() {
                    "Closure".to_string()
                } else {
                    let params_str = params
                        .as_ref()
                        .map(|params| {
                            params
                                .iter()
                                .map(|param| {
                                    let mut p = param.param_type.get_id(interner);
                                    if param.is_variadic {
                                        p = format!("...{}", p);
                                    }
                                    if param.is_optional {
                                        p.push('=');
                                    }
                                    p
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    // Psalm wraps a multi-atomic return type in parentheses,
                    // e.g. `callable():(A|B)`.
                    let return_str = return_type
                        .as_ref()
                        .map(|t| {
                            let id = t.get_id(interner);
                            if t.types.len() > 1 {
                                format!("({})", id)
                            } else {
                                id
                            }
                        })
                        .unwrap_or_else(|| "mixed".to_string());
                    format!("{}({}):{}", closure_prefix, params_str, return_str)
                }
            }
            TAtomic::TIterable {
                key_type,
                value_type,
            } => format!(
                "iterable<{}, {}>",
                key_type.get_id(interner),
                value_type.get_id(interner)
            ),
            TAtomic::TResource => "resource".to_string(),
            TAtomic::TClosedResource => "closed-resource".to_string(),
            TAtomic::TClassStringMap {
                param_name,
                as_type,
                value_param,
            } => {
                // Psalm's TClassStringMap::getId:
                // `class-string-map<T as Foo, T>` (the bound defaults to `object`).
                format!(
                    "class-string-map<{} as {}, {}>",
                    strid_to_string(*param_name, interner),
                    as_type
                        .as_ref()
                        .map(|a| a.get_id(interner))
                        .unwrap_or_else(|| "object".to_string()),
                    value_param.get_id(interner)
                )
            }
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => {
                // Psalm's exact getId: `Name:DefiningClass as <constraint>`.
                format!(
                    "{}:{} as {}",
                    strid_to_string(*name, interner),
                    generic_parent_to_string(defining_entity, interner),
                    as_type.get_id(interner)
                )
            }
            TAtomic::TTemplateParamClass {
                name,
                defining_entity,
                ..
            } => {
                format!(
                    "class<{}:{}>",
                    strid_to_string(*name, interner),
                    generic_parent_to_string(defining_entity, interner)
                )
            }
            TAtomic::TArrayKey => "array-key".to_string(),
            TAtomic::TScalar => "scalar".to_string(),
            TAtomic::TNonEmptyScalar => "non-empty-scalar".to_string(),
            TAtomic::TNumeric => "numeric".to_string(),
            TAtomic::TConditional(conditional) => {
                // Psalm: `(subject is conditional_type ? if_true : if_false)`.
                format!(
                    "({} is {} ? {} : {})",
                    strid_to_string(conditional.param_name, interner),
                    conditional.conditional_type.get_id(interner),
                    conditional.if_true_type.get_id(interner),
                    conditional.if_false_type.get_id(interner)
                )
            }
            TAtomic::TTemplateKeyOf { param_name, .. } => {
                format!("key-of<{}>", strid_to_string(*param_name, interner))
            }
            TAtomic::TTemplateValueOf { param_name, .. } => {
                format!("value-of<{}>", strid_to_string(*param_name, interner))
            }
            TAtomic::TTypeVariable { name } => name.clone(),
            TAtomic::TPropertiesOf {
                classlike_name,
                visibility_filter,
            } => {
                format!(
                    "{}<{}>",
                    visibility_filter.utility_name(),
                    strid_to_string(*classlike_name, interner)
                )
            }
            TAtomic::TTemplatePropertiesOf {
                param_name,
                visibility_filter,
                ..
            } => {
                format!(
                    "{}<{}>",
                    visibility_filter.utility_name(),
                    strid_to_string(*param_name, interner)
                )
            }
            TAtomic::TNonEmptyString => "non-empty-string".to_string(),
            TAtomic::TNumericString => "numeric-string".to_string(),
            TAtomic::TNonEmptyNumericString => "non-empty-numeric-string".to_string(),
            TAtomic::TLowercaseString => "lowercase-string".to_string(),
            TAtomic::TCallableString => "callable-string".to_string(),
            TAtomic::TNonEmptyLowercaseString => "non-empty-lowercase-string".to_string(),
            TAtomic::TTruthyString => "non-falsy-string".to_string(),
            TAtomic::TClassString { as_type } => {
                if let Some(as_type) = as_type {
                    format!("class-string<{}>", as_type.get_id(interner))
                } else {
                    "class-string".to_string()
                }
            }
            TAtomic::TLiteralClassString { name } => format!("{}::class", name),
            // Dependent types are string-valued; they display as their underlying
            // string-ish type (the dependency on the variable is internal state).
            TAtomic::TDependentGetClass { as_type, .. } => {
                format!("class-string<{}>", as_type.get_id(interner))
            }
            TAtomic::TDependentGetType { .. } => "string".to_string(),
            TAtomic::TIntRange { min, max } => {
                let min = min.map_or_else(|| "min".to_string(), |v| v.to_string());
                let max = max.map_or_else(|| "max".to_string(), |v| v.to_string());
                format!("int<{}, {}>", min, max)
            }
            TAtomic::TNonEmptyMixed => "non-empty-mixed".to_string(),
        }
    }
}
