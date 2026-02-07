//! Template standin type replacement helpers.

use pzoom_code_info::TUnion;
use pzoom_str::StrId;
use rustc_hash::FxHashMap;

use crate::expr::call::function_call_analyzer;

/// Replaces template params in a union with inferred/default concrete types.
pub fn replace(
    union_type: &TUnion,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> TUnion {
    function_call_analyzer::substitute_templates_in_union(
        union_type,
        template_replacements,
        template_defaults,
    )
}
