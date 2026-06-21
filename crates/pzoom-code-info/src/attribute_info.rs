//! PHP attribute metadata: the constant values an attribute's arguments evaluate
//! to, and the per-declaration store keyed by resolved attribute-class name.

use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// A constant value — the kind of thing a PHP attribute argument (a constant
/// expression) evaluates to. Mirrors the literal [`crate::TAtomic`] variants the
/// scanner can resolve at scan time; anything it can't fold to a literal becomes
/// [`ConstValue::Unknown`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    /// `Foo::class` — the resolved class name.
    ClassString(StrId),
    /// An array literal's element values, in key order (keys are not retained).
    Array(Vec<ConstValue>),
    /// An argument the scanner doesn't fold to a constant (e.g. `Bar::SOME_CONST`).
    Unknown,
}

/// PHP attributes attached to a declaration, keyed by the **resolved**
/// attribute-class `StrId`. PHP attributes are repeatable, so the value holds one
/// entry per occurrence of that attribute, each entry the argument list of that
/// occurrence (`#[Foo('a'), Foo('b')]` → two entries). Empty argument lists (and
/// `#[Foo]` with no parens) are an empty inner `Vec`.
pub type AttributeMap = FxHashMap<StrId, Vec<Vec<ConstValue>>>;
