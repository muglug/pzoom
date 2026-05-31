//! Context threaded through docblock type parsing.
//!
//! Mirrors Hakana's `code_info::type_resolution::TypeResolutionContext` (and the
//! `$template_type_map` Psalm's `TypeParser` receives). It lets the parser
//! recognise in-scope template parameters while building a type, so utility
//! types like `key-of<T>`, `value-of<T>`, `properties-of<T>` and
//! `int-mask-of<T>` resolve to their deferred (template) forms inline — instead
//! of being patched up in a separate post-parse pass.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::TUnion;

/// A single in-scope template parameter: its name, the entity that defines it,
/// and its upper-bound (`as`) type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateBinding {
    pub name: StrId,
    pub defining_entity: StrId,
    pub as_type: TUnion,
}

/// The set of template parameters in scope while a type string is parsed.
///
/// Mirrors Hakana's `TypeResolutionContext`; pzoom only needs the template map
/// for now (the `template_supers` list can be added if/when bounded-super
/// templates are modelled).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TypeResolutionContext {
    pub template_type_map: Vec<TemplateBinding>,
    /// Names of the enclosing function's parameters, as written (including the
    /// leading `$`, matching `ParamInfo.name`). Mirrors the param references
    /// Psalm's `getConditionalSanitizedTypeTokens` resolves: it lets the parser
    /// recognise `$param` in a conditional condition (`($param is T ? A : B)`)
    /// instead of treating the leading space as a stray callable-param marker.
    pub param_names: Vec<StrId>,
}

impl TypeResolutionContext {
    pub fn new() -> Self {
        Self {
            template_type_map: Vec::new(),
            param_names: Vec::new(),
        }
    }

    /// Look up a template parameter by name, returning its binding if `name`
    /// refers to an in-scope template.
    pub fn get_template(&self, name: StrId) -> Option<&TemplateBinding> {
        self.template_type_map
            .iter()
            .find(|binding| binding.name == name)
    }

    /// Whether `name` is one of the enclosing function's parameter names.
    pub fn is_param(&self, name: StrId) -> bool {
        self.param_names.contains(&name)
    }

    pub fn is_empty(&self) -> bool {
        self.template_type_map.is_empty() && self.param_names.is_empty()
    }
}
