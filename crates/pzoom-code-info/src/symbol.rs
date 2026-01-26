//! Symbol kinds and references.

use serde::{Deserialize, Serialize};

/// The kind of symbol being referenced or defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Class,
    Interface,
    Trait,
    Enum,
    Function,
    Method,
    Property,
    ClassConstant,
    GlobalConstant,
    Variable,
    Parameter,
    TypeAlias,
}
