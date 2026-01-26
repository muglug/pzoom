//! Atomic types - the building blocks of the type system.
//!
//! Modeled after Psalm's `Type\Atomic` hierarchy.

use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::TUnion;

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
    TLiteralInt { value: i64 },
    TLiteralFloat { value: f64 },
    TLiteralString { value: String },
    TLiteralClassString { name: String },

    // String subtypes
    TNonEmptyString,
    TNumericString,
    TNonEmptyNumericString,
    TLowercaseString,
    TNonEmptyLowercaseString,
    TTruthyString,
    TClassString { as_type: Option<Box<TAtomic>> },

    // Int subtypes
    TPositiveInt,
    TNegativeInt,
    TIntRange { min: Option<i64>, max: Option<i64> },

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
    TObject,
    TClosedResource,
    TResource,

    // Callable types
    TCallable {
        params: Option<Vec<FunctionLikeParameter>>,
        return_type: Option<Box<TUnion>>,
    },
    TClosure {
        params: Option<Vec<FunctionLikeParameter>>,
        return_type: Option<Box<TUnion>>,
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
            TAtomic::TBool
            | TAtomic::TFalse
            | TAtomic::TNull
            | TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TLiteralInt { value: 0 }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TArray { .. } => true,
            TAtomic::TLiteralFloat { value } => *value == 0.0,
            _ => false,
        }
    }

    /// Returns true if this type is always falsy.
    pub fn is_falsy(&self) -> bool {
        match self {
            TAtomic::TFalse | TAtomic::TNull => true,
            TAtomic::TLiteralInt { value: 0 } => true,
            TAtomic::TLiteralFloat { value } => *value == 0.0,
            TAtomic::TLiteralString { value } => value.is_empty(),
            _ => false,
        }
    }

    /// Returns true if this type is always truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            TAtomic::TTrue => true,
            TAtomic::TNonEmptyString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString => true,
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => true,
            TAtomic::TNonEmptyMixed => true,
            TAtomic::TLiteralInt { value } => *value != 0,
            TAtomic::TLiteralFloat { value } => *value != 0.0,
            TAtomic::TLiteralString { value } => !value.is_empty(),
            TAtomic::TPositiveInt | TAtomic::TNegativeInt => true,
            TAtomic::TNamedObject { .. } | TAtomic::TObject => true,
            TAtomic::TResource => true,
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
            TAtomic::TKeyedArray { properties, .. } if !properties.is_empty() => true,
            _ => false,
        }
    }

    /// Returns a human-readable type identifier.
    pub fn get_id(&self) -> String {
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
            TAtomic::TLiteralString { .. } => "literal-string".to_string(),
            TAtomic::TMixed => "mixed".to_string(),
            TAtomic::TNothing => "never".to_string(),
            TAtomic::TVoid => "void".to_string(),
            TAtomic::TArray { .. } => "array".to_string(),
            TAtomic::TKeyedArray { is_list: true, .. } => "list".to_string(),
            TAtomic::TKeyedArray { .. } => "array".to_string(),
            TAtomic::TObject => "object".to_string(),
            TAtomic::TNamedObject { .. } => "object".to_string(),
            TAtomic::TEnum { .. } => "enum".to_string(),
            TAtomic::TEnumCase { .. } => "enum-case".to_string(),
            TAtomic::TCallable { .. } => "callable".to_string(),
            TAtomic::TClosure { .. } => "Closure".to_string(),
            TAtomic::TIterable { .. } => "iterable".to_string(),
            TAtomic::TResource => "resource".to_string(),
            TAtomic::TClosedResource => "closed-resource".to_string(),
            TAtomic::TNonEmptyArray { .. } => "non-empty-array".to_string(),
            TAtomic::TList { .. } => "list".to_string(),
            TAtomic::TNonEmptyList { .. } => "non-empty-list".to_string(),
            TAtomic::TTemplateParam { .. } => "template".to_string(),
            TAtomic::TTemplateParamClass { .. } => "template-class".to_string(),
            TAtomic::TArrayKey => "array-key".to_string(),
            TAtomic::TScalar => "scalar".to_string(),
            TAtomic::TNumeric => "numeric".to_string(),
            TAtomic::TNonEmptyString => "non-empty-string".to_string(),
            TAtomic::TNumericString => "numeric-string".to_string(),
            TAtomic::TNonEmptyNumericString => "non-empty-numeric-string".to_string(),
            TAtomic::TLowercaseString => "lowercase-string".to_string(),
            TAtomic::TNonEmptyLowercaseString => "non-empty-lowercase-string".to_string(),
            TAtomic::TTruthyString => "truthy-string".to_string(),
            TAtomic::TClassString { .. } => "class-string".to_string(),
            TAtomic::TLiteralClassString { .. } => "class-string".to_string(),
            TAtomic::TPositiveInt => "positive-int".to_string(),
            TAtomic::TNegativeInt => "negative-int".to_string(),
            TAtomic::TIntRange { .. } => "int".to_string(),
            TAtomic::TNonEmptyMixed => "non-empty-mixed".to_string(),
        }
    }
}
