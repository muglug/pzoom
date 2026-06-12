//! Class constant information.
//!
//! Mirrors Hakana's `class_constant_info.rs`. Split out of
//! [`crate::class_like_info`].

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::TUnion;
use crate::member_visibility::Visibility;

/// Information about a class constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassConstantInfo {
    pub name: StrId,
    pub declaring_class: StrId,
    pub constant_type: TUnion,
    pub visibility: Visibility,
    pub is_final: bool,
    pub is_deprecated: bool,
    pub start_offset: u32,
    /// Initializer pieces scan-time inference couldn't evaluate (cross-class
    /// constant references) — Psalm's `UnresolvedConstantComponent`, resolved
    /// by the populator once every class is known (`ConstantTypeResolver`).
    pub unresolved_initializer: Option<UnresolvedConstExpr>,
    /// For an enum case constant: the backed value (`case K3 = 1` stores 1),
    /// powering `E::CASE->value` literal resolution.
    #[serde(default)]
    pub enum_case_value: Option<TUnion>,
    /// References the initializer failed to resolve against the populated
    /// codebase (Psalm reports these when analyzing the assignment).
    #[serde(default)]
    pub resolution_failures: Vec<ConstResolutionFailure>,
    /// The initializer references itself through other constants (Psalm's
    /// ConstantTypeResolver throws CircularReferenceException; the analyzer
    /// reports CircularReference).
    #[serde(default)]
    pub circular: bool,
    /// The DECLARED type: a `@var` docblock or native `const type X` hint
    /// (Psalm's `ClassConstantStorage::$type`, as opposed to the inferred
    /// value type). Used for override covariance checks.
    #[serde(default)]
    pub declared_type: Option<TUnion>,
    /// Whether the constant was declared with a native type hint
    /// (`const int B = 0;`) — Psalm's `$stmt->type !== null`, which drives
    /// MissingClassConstType on PHP >= 8.3.
    #[serde(default)]
    pub has_type_hint: bool,
}

/// A reference a constant initializer could not resolve.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ConstResolutionFailure {
    /// `MissingClass::CONST` — the class is unknown.
    MissingClass(StrId),
    /// `Known::MISSING` — the class exists, the constant doesn't.
    MissingClassConstant(StrId, StrId),
    /// A bare `MISSING` global constant.
    MissingGlobalConstant(StrId),
}

/// A constant initializer expression deferred to post-scan resolution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UnresolvedConstExpr {
    /// A component scan-time inference already evaluated.
    Resolved(TUnion),
    /// `Other::CONST`, with the class resolved to a FQCN at collect time.
    ClassConstant { class: StrId, constant: StrId },
    /// An array literal; a `None` key takes the next list index.
    ArrayLiteral(Vec<UnresolvedArrayEntry>),
    /// String concatenation.
    Concat(Box<UnresolvedConstExpr>, Box<UnresolvedConstExpr>),
    /// `EXPR[key]` offset fetch (Psalm's ArrayOffsetFetch component).
    ArrayAccess {
        array: Box<UnresolvedConstExpr>,
        key: Box<UnresolvedConstExpr>,
    },
    /// `EXPR + EXPR` — for arrays, union with left precedence.
    Plus(Box<UnresolvedConstExpr>, Box<UnresolvedConstExpr>),
    /// A global constant reference (`JSON_PRETTY_PRINT`), resolved against
    /// the populated codebase constants.
    GlobalConstant(StrId),
    /// An int-producing binary operation over late-resolved operands.
    IntOp {
        op: UnresolvedIntOp,
        lhs: Box<UnresolvedConstExpr>,
        rhs: Box<UnresolvedConstExpr>,
    },
    /// `Other::CASE->value` / `->name` (Psalm's UnresolvedConstant
    /// EnumValueFetch / EnumNameFetch components).
    EnumCasePropertyFetch {
        class: StrId,
        case: StrId,
        fetch_name: bool,
    },
    /// `COND ? IF : ELSE` (Psalm's UnresolvedTernary; a `None` if-branch is
    /// the short `?:` form).
    Ternary {
        cond: Box<UnresolvedConstExpr>,
        if_branch: Option<Box<UnresolvedConstExpr>>,
        else_branch: Box<UnresolvedConstExpr>,
    },
}

/// Operator for [`UnresolvedConstExpr::IntOp`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum UnresolvedIntOp {
    Sub,
    Mul,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnresolvedArrayEntry {
    pub key: Option<UnresolvedConstExpr>,
    pub value: UnresolvedConstExpr,
    /// `...EXPR` spread element: the value's array entries are inlined
    /// (string keys kept, int keys renumbered).
    #[serde(default)]
    pub is_spread: bool,
}
