//! Scope-related structures for analysis.

pub mod if_conditional_scope;
pub mod if_scope;
pub mod loop_scope;
pub mod switch_scope;

pub use if_conditional_scope::IfConditionalScope;
pub use if_scope::IfScope;
pub use loop_scope::LoopScope;
pub use switch_scope::SwitchScope;

use pzoom_str::{Interner, StrId};

/// Check if a variable name has another variable name as its root.
///
/// For example, `$a['foo']` has root `$a`.
pub fn var_has_root(var_name: &str, root: StrId, interner: &Interner) -> bool {
    let root_str = interner.lookup(root);
    var_name.starts_with(&*root_str)
        && (var_name.len() == root_str.len()
            || var_name
                .chars()
                .nth(root_str.len())
                .map_or(false, |c| c == '[' || c == '-'))
}
