//! PHP attribute metadata: the per-declaration store of attributes, keyed by the
//! resolved attribute-class name, with each argument folded to the same constant
//! [`crate::TUnion`] the scanner infers for class constants / enum cases.

use crate::TUnion;
use pzoom_str::StrId;
use rustc_hash::FxHashMap;

/// PHP attributes attached to a declaration, keyed by the **resolved**
/// attribute-class `StrId`. PHP attributes are repeatable, so the value holds one
/// entry per occurrence of that attribute, each entry the argument list of that
/// occurrence (`#[Foo('a'), Foo('b')]` → two entries). Empty argument lists (and
/// `#[Foo]` with no parens) are an empty inner `Vec`.
///
/// Each argument is the constant [`TUnion`] the scanner folds it to (a literal
/// atomic — `TLiteralString`, `TLiteralInt`, `TLiteralClassString`, …), the same
/// inference used for class-constant and enum-case values; an argument that can't
/// be folded to a constant is `mixed`.
pub type AttributeMap = FxHashMap<StrId, Vec<Vec<TUnion>>>;
