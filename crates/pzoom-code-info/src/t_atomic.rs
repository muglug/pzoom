//! Atomic types - the building blocks of the type system.
//!
//! Modeled after Psalm's `Type\Atomic` hierarchy.

use pzoom_str::{Interner, PRELOADED_STRINGS, StrId};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::TUnion;

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
        var_id: StrId,
        as_type: Box<TUnion>,
    },
    /// The result type of `gettype($x)` where `$x` is a variable. Mirrors Psalm's
    /// `Type\Atomic\TDependentGetType`. `var_id` is the interned id of `$x`; a
    /// later `gettype($x) === "string"` / `switch (gettype($x))` narrows `$x`.
    TDependentGetType {
        var_id: StrId,
    },

    // String subtypes
    TNonEmptyString,
    TNumericString,
    TNonEmptyNumericString,
    TLowercaseString,
    TNonEmptyLowercaseString,
    TTruthyString,
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
    TArray {
        key_type: Box<TUnion>,
        value_type: Box<TUnion>,
    },
    TNonEmptyArray {
        key_type: Box<TUnion>,
        value_type: Box<TUnion>,
    },
    TList {
        value_type: Box<TUnion>,
    },
    TNonEmptyList {
        value_type: Box<TUnion>,
    },
    /// Keyed array / shape type - array with known keys and value types
    TKeyedArray {
        properties: FxHashMap<ArrayKey, TUnion>,
        /// If true, this is a list (sequential integer keys starting from 0)
        is_list: bool,
        /// Whether the shape is sealed (no additional keys allowed)
        sealed: bool,
        /// Fallback type for unknown keys when not sealed
        fallback_key_type: Option<Box<TUnion>>,
        fallback_value_type: Option<Box<TUnion>>,
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
    TNothing, // Never/bottom type
    TVoid,
    TIterable {
        key_type: Box<TUnion>,
        value_type: Box<TUnion>,
    },

    // Template/generic types
    TTemplateParam {
        name: StrId,
        defining_entity: StrId,
        as_type: Box<TUnion>,
    },
    TTemplateParamClass {
        name: StrId,
        defining_entity: StrId,
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
        defining_entity: StrId,
        as_type: Box<TUnion>,
    },
    /// `value-of<T>` where `T` is an unresolved template parameter (Psalm's
    /// `Type\Atomic\TTemplateValueOf`).
    TTemplateValueOf {
        param_name: StrId,
        defining_entity: StrId,
        as_type: Box<TUnion>,
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
        defining_entity: StrId,
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

/// A conditional return type `(<condition> ? if_true : if_false)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionalReturnType {
    pub condition: ConditionalReturnCondition,
    pub if_true_type: TUnion,
    pub if_false_type: TUnion,
}

/// The condition controlling a conditional return type branch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConditionalReturnCondition {
    /// Template condition, e.g. `TType is 'array'`.
    TemplateIs {
        template_name: StrId,
        asserted_type: TUnion,
    },
    /// Parameter condition, e.g. `$name is class-string`, evaluated against the
    /// argument's type at the call site.
    ParamIs {
        param_id: StrId,
        asserted_type: TUnion,
    },
    /// Argument-count condition, e.g. `func_num_args() is 1`.
    FuncNumArgsIs { count: usize },
}

/// Key type for keyed arrays (shapes).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ArrayKey {
    Int(i64),
    String(String),
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

impl TAtomic {
    /// The guaranteed minimum number of entries for a keyed array, mirroring
    /// Psalm's `TKeyedArray::getMinCount()`. Returns `None` for atomics that are
    /// not keyed arrays.
    ///
    /// For a list this is the length of the leading run of always-defined entries;
    /// for a shape it is the count of properties that are neither possibly-undefined
    /// nor `never`.
    pub fn get_min_count(&self) -> Option<usize> {
        let TAtomic::TKeyedArray {
            properties,
            is_list,
            ..
        } = self
        else {
            return None;
        };

        if *is_list {
            let mut min_count = 0usize;
            while let Some(property) = properties.get(&ArrayKey::Int(min_count as i64)) {
                if property.possibly_undefined || property.is_nothing() {
                    break;
                }
                min_count += 1;
            }
            return Some(min_count);
        }

        Some(
            properties
                .values()
                .filter(|property| !property.possibly_undefined && !property.is_nothing())
                .count(),
        )
    }

    /// The maximum number of entries for a keyed array, mirroring Psalm's
    /// `TKeyedArray::getMaxCount()`. Returns `None` when the shape is unsealed (can
    /// hold extra keys) or when the atomic is not a keyed array.
    pub fn get_max_count(&self) -> Option<usize> {
        let TAtomic::TKeyedArray {
            properties,
            sealed,
            fallback_key_type,
            fallback_value_type,
            ..
        } = self
        else {
            return None;
        };

        if !*sealed || fallback_key_type.is_some() || fallback_value_type.is_some() {
            return None;
        }

        Some(
            properties
                .values()
                .filter(|property| !property.is_nothing())
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
            TAtomic::TTemplateParam { as_type, .. } => as_type.is_falsable,
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
            TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TDependentGetClass { .. }
            | TAtomic::TTemplateParamClass { .. } => true,
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => true,
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
            TAtomic::TKeyedArray { properties, .. } => properties
                .values()
                .any(|value_type| !value_type.possibly_undefined),
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

        match self {
            TAtomic::TInt => "int".to_string(),
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
                    format!("string({})", value)
                }
            }
            TAtomic::TMixed => "mixed".to_string(),
            TAtomic::TNothing => "never".to_string(),
            TAtomic::TVoid => "void".to_string(),
            TAtomic::TArray {
                key_type,
                value_type,
            } => format!(
                "array<{}, {}>",
                key_type.get_id(interner),
                value_type.get_id(interner)
            ),
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                // Render the shape like Psalm's `TKeyedArray::getId`:
                // `array{foo: int, bar?: string}` / `list{int, string}`, with an
                // unsealed fallback as `, ...<K, V>` (`, ...<V>` for lists) inside
                // the braces.
                let has_fallback = !*sealed && fallback_value_type.is_some();

                let mut int_entries: Vec<(i64, &TUnion)> = Vec::new();
                let mut string_entries: Vec<(&str, &TUnion)> = Vec::new();
                for (key, value) in properties {
                    match key {
                        ArrayKey::Int(i) => int_entries.push((*i, value)),
                        ArrayKey::String(s) => string_entries.push((s.as_str(), value)),
                    }
                }
                int_entries.sort_by_key(|(i, _)| *i);

                if properties.is_empty() {
                    // No known items: a generic list/array, or the empty array.
                    if let Some(fallback_value) = fallback_value_type.as_ref().filter(|_| has_fallback)
                    {
                        return if *is_list {
                            format!("list<{}>", fallback_value.get_id(interner))
                        } else {
                            let fallback_key = fallback_key_type
                                .as_ref()
                                .map(|k| k.get_id(interner))
                                .unwrap_or_else(|| "array-key".to_string());
                            format!("array<{}, {}>", fallback_key, fallback_value.get_id(interner))
                        };
                    }
                    return "array<never, never>".to_string();
                }

                // Psalm uses positional list syntax (`list{int, string}`) only when
                // every element is required; an optional element forces explicit keys.
                let all_required = properties.values().all(|value| !value.possibly_undefined);
                let use_list_syntax = *is_list && all_required;

                let mut entries: Vec<String> = Vec::new();
                if use_list_syntax {
                    for (_, value) in &int_entries {
                        entries.push(value.get_id(interner));
                    }
                } else {
                    for (key, value) in &int_entries {
                        let optional = if value.possibly_undefined { "?" } else { "" };
                        entries.push(format!("{}{}: {}", key, optional, value.get_id(interner)));
                    }
                    for (key, value) in &string_entries {
                        let optional = if value.possibly_undefined { "?" } else { "" };
                        entries.push(format!("{}{}: {}", key, optional, value.get_id(interner)));
                    }
                    // Psalm sorts non-list property strings for a stable id.
                    if !*is_list {
                        entries.sort();
                    }
                }

                let params_part = if has_fallback {
                    let fallback_value = fallback_value_type.as_ref().unwrap().get_id(interner);
                    if *is_list {
                        format!(", ...<{}>", fallback_value)
                    } else {
                        let fallback_key = fallback_key_type
                            .as_ref()
                            .map(|k| k.get_id(interner))
                            .unwrap_or_else(|| "array-key".to_string());
                        format!(", ...<{}, {}>", fallback_key, fallback_value)
                    }
                } else {
                    String::new()
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
                let mut type_ids = types
                    .iter()
                    .map(|atomic| atomic.get_id(interner))
                    .collect::<Vec<_>>();
                type_ids.sort();
                type_ids.dedup();
                type_ids.join("&")
            }
            TAtomic::TObjectWithProperties { properties } => {
                let mut entries = properties
                    .iter()
                    .map(|(key, value_type)| {
                        let key_str = match key {
                            ArrayKey::Int(i) => i.to_string(),
                            ArrayKey::String(s) => s.clone(),
                        };
                        let optional = if value_type.possibly_undefined { "?" } else { "" };
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
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => format!(
                "non-empty-array<{}, {}>",
                key_type.get_id(interner),
                value_type.get_id(interner)
            ),
            TAtomic::TList { value_type } => {
                format!("list<{}>", value_type.get_id(interner))
            }
            TAtomic::TNonEmptyList { value_type } => {
                format!("non-empty-list<{}>", value_type.get_id(interner))
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
                    strid_to_string(*defining_entity, interner),
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
                    strid_to_string(*defining_entity, interner)
                )
            }
            TAtomic::TArrayKey => "array-key".to_string(),
            TAtomic::TScalar => "scalar".to_string(),
            TAtomic::TNumeric => "numeric".to_string(),
            TAtomic::TConditional(conditional) => {
                // Psalm: `(subject is conditional_type ? if_true : if_false)`.
                let (subject, asserted) = match &conditional.condition {
                    ConditionalReturnCondition::TemplateIs {
                        template_name,
                        asserted_type,
                    } => (
                        strid_to_string(*template_name, interner),
                        asserted_type.get_id(interner),
                    ),
                    ConditionalReturnCondition::ParamIs {
                        param_id,
                        asserted_type,
                    } => (
                        strid_to_string(*param_id, interner),
                        asserted_type.get_id(interner),
                    ),
                    ConditionalReturnCondition::FuncNumArgsIs { count } => {
                        ("func_num_args()".to_string(), count.to_string())
                    }
                };
                format!(
                    "({} is {} ? {} : {})",
                    subject,
                    asserted,
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
            TAtomic::TNonEmptyLowercaseString => "non-empty-lowercase-string".to_string(),
            TAtomic::TTruthyString => "truthy-string".to_string(),
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
