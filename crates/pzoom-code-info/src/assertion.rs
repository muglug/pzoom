//! Assertion types for type narrowing.
//!
//! Assertions represent type guards that narrow the type of a variable based on
//! conditions like `if ($x instanceof Foo)` or `if (is_string($x))`.

use std::hash::{Hash, Hasher};

use crate::t_atomic::{ArrayKey, TAtomic};
use crate::t_union::TUnion;

/// Represents an assertion about a variable's type.
///
/// Assertions are used to narrow types in conditional branches. For example,
/// after `if ($x instanceof Foo)`, we know `$x` is of type `Foo` in the true branch.
#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    /// Any type (no constraint).
    Any,

    /// Variable is a specific type (e.g., `is_string($x)`).
    IsType(TAtomic),

    /// Variable is NOT a specific type (e.g., `!is_string($x)`).
    IsNotType(TAtomic),

    /// Variable is falsy (null, false, 0, "", []).
    Falsy,

    /// Variable is truthy (not null, false, 0, "", []).
    Truthy,

    /// Variable is equal to a specific value (e.g., `$x === 5`).
    IsEqual(TAtomic),

    /// Variable is not equal to a specific value (e.g., `$x !== 5`).
    IsNotEqual(TAtomic),

    /// Variable is set via equality check in isset context.
    IsEqualIsset,

    /// Variable is set (isset($x)).
    IsIsset,

    /// Variable is not set (!isset($x)).
    IsNotIsset,

    /// Variable has string array access.
    HasStringArrayAccess,

    /// Variable has int or string array access.
    HasIntOrStringArrayAccess,

    /// Array key exists.
    ArrayKeyExists,

    /// Array key does not exist.
    ArrayKeyDoesNotExist,

    /// Variable is in an array of values (in_array($x, [...])).
    InArray(TUnion),

    /// Variable is not in an array of values.
    NotInArray(TUnion),

    /// Array has a specific key (array_key_exists('key', $arr)).
    HasArrayKey(ArrayKey),

    /// Array does not have a specific key.
    DoesNotHaveArrayKey(ArrayKey),

    /// Array has a non-null entry for a specific key.
    HasNonnullEntryForKey(ArrayKey),

    /// Array does not have a non-null entry for a specific key.
    DoesNotHaveNonnullEntryForKey(ArrayKey),

    /// Variable is a non-empty countable (count($x) > 0).
    /// The boolean indicates whether this assertion is negatable.
    NonEmptyCountable(bool),

    /// Variable is an empty countable (count($x) === 0).
    EmptyCountable,

    /// Variable has an exact count (count($x) === n).
    HasExactCount(usize),

    /// Variable does not have an exact count (count($x) !== n).
    DoesNotHaveExactCount(usize),
}

impl Assertion {
    /// Converts the assertion to a string representation.
    pub fn to_string(&self) -> String {
        match self {
            Assertion::Any => "any".to_string(),
            Assertion::Falsy => "falsy".to_string(),
            Assertion::Truthy => "truthy".to_string(),
            Assertion::IsType(atomic) => atomic.get_id(None),
            Assertion::IsNotType(atomic) => format!("!{}", atomic.get_id(None)),
            Assertion::IsEqual(atomic) => format!("={}", atomic.get_id(None)),
            Assertion::IsNotEqual(atomic) => format!("!={}", atomic.get_id(None)),
            Assertion::IsEqualIsset => "=isset".to_string(),
            Assertion::IsIsset => "isset".to_string(),
            Assertion::IsNotIsset => "!isset".to_string(),
            Assertion::HasStringArrayAccess => "=string-array-access".to_string(),
            Assertion::HasIntOrStringArrayAccess => "=int-or-string-array-access".to_string(),
            Assertion::ArrayKeyExists => "array-key-exists".to_string(),
            Assertion::ArrayKeyDoesNotExist => "!array-key-exists".to_string(),
            Assertion::HasArrayKey(key) => format!("=has-array-key-{}", key.to_string()),
            Assertion::DoesNotHaveArrayKey(key) => format!("!=has-array-key-{}", key.to_string()),
            Assertion::HasNonnullEntryForKey(key) => {
                format!("=has-nonnull-entry-for-{}", key.to_string())
            }
            Assertion::DoesNotHaveNonnullEntryForKey(key) => {
                format!("!=has-nonnull-entry-for-{}", key.to_string())
            }
            Assertion::InArray(union) => format!("=in-array-{}", union.get_id(None)),
            Assertion::NotInArray(union) => format!("!=in-array-{}", union.get_id(None)),
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    "non-empty-countable".to_string()
                } else {
                    "=non-empty-countable".to_string()
                }
            }
            Assertion::EmptyCountable => "empty-countable".to_string(),
            Assertion::HasExactCount(n) => format!("has-exactly-{}", n),
            Assertion::DoesNotHaveExactCount(n) => format!("!has-exactly-{}", n),
        }
    }

    /// Computes a hash of the assertion for use in clause lookups.
    pub fn to_hash(&self) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        self.to_string().hash(&mut hasher);
        hasher.finish()
    }

    /// Returns true if this assertion represents a negation.
    pub fn has_negation(&self) -> bool {
        matches!(
            self,
            Assertion::Falsy
                | Assertion::IsNotType(_)
                | Assertion::IsNotEqual(_)
                | Assertion::IsNotIsset
                | Assertion::NotInArray(_)
                | Assertion::ArrayKeyDoesNotExist
                | Assertion::DoesNotHaveArrayKey(_)
                | Assertion::DoesNotHaveExactCount(_)
                | Assertion::DoesNotHaveNonnullEntryForKey(_)
                | Assertion::EmptyCountable
        )
    }

    /// Returns true if this assertion involves isset.
    pub fn has_isset(&self) -> bool {
        matches!(
            self,
            Assertion::IsIsset
                | Assertion::ArrayKeyExists
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqualIsset
        )
    }

    /// Returns true if this assertion involves non-isset equality.
    pub fn has_non_isset_equality(&self) -> bool {
        matches!(
            self,
            Assertion::InArray(_)
                | Assertion::HasIntOrStringArrayAccess
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqual(_)
        )
    }

    /// Returns true if this assertion involves equality.
    pub fn has_equality(&self) -> bool {
        matches!(
            self,
            Assertion::InArray(_)
                | Assertion::HasIntOrStringArrayAccess
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqualIsset
                | Assertion::IsEqual(_)
                | Assertion::IsNotEqual(_)
        )
    }

    /// Returns true if this assertion involves a literal string or int.
    pub fn has_literal_string_or_int(&self) -> bool {
        match self {
            Assertion::IsEqual(atomic)
            | Assertion::IsNotEqual(atomic)
            | Assertion::IsType(atomic)
            | Assertion::IsNotType(atomic) => {
                matches!(
                    atomic,
                    TAtomic::TLiteralInt { .. } | TAtomic::TLiteralString { .. }
                )
            }
            _ => false,
        }
    }

    /// Returns the atomic type associated with this assertion, if any.
    pub fn get_type(&self) -> Option<&TAtomic> {
        match self {
            Assertion::IsEqual(atomic)
            | Assertion::IsNotEqual(atomic)
            | Assertion::IsType(atomic)
            | Assertion::IsNotType(atomic) => Some(atomic),
            _ => None,
        }
    }

    /// Returns true if this assertion is the negation of another.
    pub fn is_negation_of(&self, other: &Assertion) -> bool {
        match self {
            Assertion::Any => false,
            Assertion::Falsy => matches!(other, Assertion::Truthy),
            Assertion::Truthy => matches!(other, Assertion::Falsy),
            Assertion::IsType(atomic) => match other {
                Assertion::IsNotType(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsNotType(atomic) => match other {
                Assertion::IsType(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsEqual(atomic) => match other {
                Assertion::IsNotEqual(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsNotEqual(atomic) => match other {
                Assertion::IsEqual(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsEqualIsset => false,
            Assertion::IsIsset => matches!(other, Assertion::IsNotIsset),
            Assertion::IsNotIsset => matches!(other, Assertion::IsIsset),
            Assertion::HasStringArrayAccess => false,
            Assertion::HasIntOrStringArrayAccess => false,
            Assertion::ArrayKeyExists => matches!(other, Assertion::ArrayKeyDoesNotExist),
            Assertion::ArrayKeyDoesNotExist => matches!(other, Assertion::ArrayKeyExists),
            Assertion::HasArrayKey(key) => match other {
                Assertion::DoesNotHaveArrayKey(other_key) => other_key == key,
                _ => false,
            },
            Assertion::DoesNotHaveArrayKey(key) => match other {
                Assertion::HasArrayKey(other_key) => other_key == key,
                _ => false,
            },
            Assertion::HasNonnullEntryForKey(key) => match other {
                Assertion::DoesNotHaveNonnullEntryForKey(other_key) => other_key == key,
                _ => false,
            },
            Assertion::DoesNotHaveNonnullEntryForKey(key) => match other {
                Assertion::HasNonnullEntryForKey(other_key) => other_key == key,
                _ => false,
            },
            Assertion::InArray(union) => match other {
                Assertion::NotInArray(other_union) => other_union == union,
                _ => false,
            },
            Assertion::NotInArray(union) => match other {
                Assertion::InArray(other_union) => other_union == union,
                _ => false,
            },
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    matches!(other, Assertion::EmptyCountable)
                } else {
                    false
                }
            }
            Assertion::EmptyCountable => matches!(other, Assertion::NonEmptyCountable(true)),
            Assertion::HasExactCount(n) => match other {
                Assertion::DoesNotHaveExactCount(other_n) => other_n == n,
                _ => false,
            },
            Assertion::DoesNotHaveExactCount(n) => match other {
                Assertion::HasExactCount(other_n) => other_n == n,
                _ => false,
            },
        }
    }

    /// Returns the negation of this assertion.
    pub fn get_negation(&self) -> Self {
        match self {
            Assertion::Any => Assertion::Any,
            Assertion::Falsy => Assertion::Truthy,
            Assertion::IsType(atomic) => Assertion::IsNotType(atomic.clone()),
            Assertion::IsNotType(atomic) => Assertion::IsType(atomic.clone()),
            Assertion::Truthy => Assertion::Falsy,
            Assertion::IsEqual(atomic) => Assertion::IsNotEqual(atomic.clone()),
            Assertion::IsNotEqual(atomic) => Assertion::IsEqual(atomic.clone()),
            Assertion::IsIsset => Assertion::IsNotIsset,
            Assertion::IsNotIsset => Assertion::IsIsset,
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    Assertion::EmptyCountable
                } else {
                    Assertion::Any
                }
            }
            Assertion::EmptyCountable => Assertion::NonEmptyCountable(true),
            Assertion::ArrayKeyExists => Assertion::ArrayKeyDoesNotExist,
            Assertion::ArrayKeyDoesNotExist => Assertion::ArrayKeyExists,
            Assertion::InArray(union) => Assertion::NotInArray(union.clone()),
            Assertion::NotInArray(union) => Assertion::InArray(union.clone()),
            Assertion::HasExactCount(size) => Assertion::DoesNotHaveExactCount(*size),
            Assertion::DoesNotHaveExactCount(size) => Assertion::HasExactCount(*size),
            Assertion::HasArrayKey(key) => Assertion::DoesNotHaveArrayKey(key.clone()),
            Assertion::DoesNotHaveArrayKey(key) => Assertion::HasArrayKey(key.clone()),
            Assertion::HasNonnullEntryForKey(key) => {
                Assertion::DoesNotHaveNonnullEntryForKey(key.clone())
            }
            Assertion::DoesNotHaveNonnullEntryForKey(key) => {
                Assertion::HasNonnullEntryForKey(key.clone())
            }
            // These are generated within the reconciler, so their negations are meaningless.
            Assertion::HasStringArrayAccess => Assertion::Any,
            Assertion::HasIntOrStringArrayAccess => Assertion::Any,
            Assertion::IsEqualIsset => Assertion::Any,
        }
    }
}

impl ArrayKey {
    /// Converts the array key to a string representation.
    pub fn to_string(&self) -> String {
        match self {
            ArrayKey::Int(i) => i.to_string(),
            ArrayKey::String(s) => format!("'{}'", s),
        }
    }
}
