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
    TPositiveInt,
    TNegativeInt,
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
    },
    TObjectIntersection {
        types: Vec<TAtomic>,
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
    /// Returns true if this type is nullable (can be null).
    pub fn is_nullable(&self) -> bool {
        matches!(self, TAtomic::TNull)
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
    pub fn is_falsable(&self) -> bool {
        match self {
            TAtomic::TMixed
            | TAtomic::TBool
            | TAtomic::TFalse
            | TAtomic::TNull
            | TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TScalar
            | TAtomic::TNumeric
            | TAtomic::TArrayKey
            | TAtomic::TLiteralInt { value: 0 }
            | TAtomic::TArray { .. }
            | TAtomic::TList { .. } => true,
            TAtomic::TLiteralFloat { value } => *value == 0.0,
            TAtomic::TLiteralString { value } => value.is_empty() || value == "0",
            TAtomic::TKeyedArray { properties, .. } => properties
                .values()
                .all(|value_type| value_type.possibly_undefined),
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
            | TAtomic::TTemplateParamClass { .. } => true,
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => true,
            TAtomic::TNonEmptyMixed => true,
            TAtomic::TLiteralInt { value } => *value != 0,
            TAtomic::TLiteralFloat { value } => *value != 0.0,
            TAtomic::TLiteralString { value } => {
                value != NON_SPECIFIC_LITERAL_STRING_VALUE && !value.is_empty() && value != "0"
            }
            TAtomic::TPositiveInt | TAtomic::TNegativeInt => true,
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
            TAtomic::TKeyedArray { is_list: true, .. } => "list".to_string(),
            TAtomic::TKeyedArray { .. } => "array".to_string(),
            TAtomic::TObject => "object".to_string(),
            TAtomic::TNamedObject { name, type_params } => {
                let mut id = strid_to_string(*name, interner);
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
                let callable_prefix = if matches!(is_pure, Some(true)) {
                    "pure-callable"
                } else {
                    "callable"
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
                    let return_str = return_type
                        .as_ref()
                        .map(|t| t.get_id(interner))
                        .unwrap_or_else(|| "mixed".to_string());
                    format!("{}({}):{}", callable_prefix, params_str, return_str)
                }
            }
            TAtomic::TClosure {
                params,
                return_type,
                is_pure,
            } => {
                let closure_prefix = if matches!(is_pure, Some(true)) {
                    "pure-function"
                } else {
                    "function"
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
                    let return_str = return_type
                        .as_ref()
                        .map(|t| t.get_id(interner))
                        .unwrap_or_else(|| "mixed".to_string());
                    format!("({}({}):{})", closure_prefix, params_str, return_str)
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
                ..
            } => {
                format!(
                    "{}:{}",
                    strid_to_string(*name, interner),
                    strid_to_string(*defining_entity, interner)
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
            TAtomic::TPositiveInt => "positive-int".to_string(),
            TAtomic::TNegativeInt => "negative-int".to_string(),
            TAtomic::TIntRange { min, max } => {
                let min = min.map_or_else(|| "min".to_string(), |v| v.to_string());
                let max = max.map_or_else(|| "max".to_string(), |v| v.to_string());
                format!("int<{}, {}>", min, max)
            }
            TAtomic::TNonEmptyMixed => "non-empty-mixed".to_string(),
        }
    }
}
