//! Scope-related structures for analysis.

pub mod finally_scope;
pub mod if_conditional_scope;
pub mod if_scope;
pub mod loop_scope;
pub mod switch_scope;

pub use finally_scope::FinallyScope;
pub use if_conditional_scope::IfConditionalScope;
pub use if_scope::IfScope;
pub use loop_scope::LoopScope;
pub use switch_scope::SwitchScope;

/// Check if a variable name has another variable name as its root.
///
/// For example, `$a['foo']` has root `$a`.
pub fn var_has_root(var_name: &str, root: &str) -> bool {
    var_name.starts_with(root)
        && (var_name.len() == root.len()
            || var_name
                .chars()
                .nth(root.len())
                .map_or(false, |c| c == '[' || c == '-'))
}
