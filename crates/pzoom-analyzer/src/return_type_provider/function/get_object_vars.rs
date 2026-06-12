//! `get_object_vars()` return-type provider.
//!
//! Mirrors Psalm's GetObjectVarsReturnTypeProvider: a single named-object
//! argument yields a keyed shape of the properties visible from the calling
//! context, with a `string => mixed` fallback unless the class is final
//! (subclasses may add properties).

use pzoom_code_info::member_visibility::Visibility;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion};
use rustc_hash::FxHashMap;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) struct GetObjectVarsReturnTypeProvider;

impl FunctionReturnTypeProvider for GetObjectVarsReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["get_object_vars"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let first_pos = event.arg_positions.first().copied()?;
        let first_arg_type = analysis_data.expr_types.get(&first_pos).cloned()?;

        let fallback = || {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::string()),
                value_type: Box::new(TUnion::mixed()),
            })
        };

        match first_arg_type.get_single()? {
            TAtomic::TObjectWithProperties { properties, .. } => {
                if properties.is_empty() {
                    return Some(fallback());
                }
                Some(TUnion::new(TAtomic::TKeyedArray {
                    properties: std::sync::Arc::new(properties.clone()),
                    is_list: false,
                    sealed: true,
                    fallback_key_type: None,
                    fallback_value_type: None,
                }))
            }
            TAtomic::TNamedObject { name, .. } => {
                let class_name = event.analyzer.interner.lookup(*name);
                if class_name.eq_ignore_ascii_case("stdClass") {
                    return Some(fallback());
                }
                let class_info = event.analyzer.codebase.get_class(*name)?;

                let mut properties: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
                for (prop_id, prop_info) in &class_info.properties {
                    if prop_info.is_static {
                        continue;
                    }
                    if !property_visible_from_context(event.analyzer, class_info, *prop_id, prop_info.visibility) {
                        continue;
                    }
                    let prop_name = event.analyzer.interner.lookup(*prop_id);
                    properties.insert(
                        ArrayKey::String(prop_name.trim_start_matches('$').to_string()),
                        prop_info.get_type().cloned().unwrap_or_else(TUnion::mixed),
                    );
                }

                if properties.is_empty() {
                    if class_info.is_final {
                        return Some(TUnion::new(TAtomic::TArray {
                            key_type: Box::new(TUnion::nothing()),
                            value_type: Box::new(TUnion::nothing()),
                        }));
                    }
                    return Some(fallback());
                }

                // A non-final class may gain properties in subclasses, so the
                // shape stays open (Psalm's string => mixed fallback params).
                let open = !class_info.is_final;
                Some(TUnion::new(TAtomic::TKeyedArray {
                    properties: std::sync::Arc::new(properties),
                    is_list: false,
                    sealed: !open,
                    fallback_key_type: open.then(|| Box::new(TUnion::string())),
                    fallback_value_type: open.then(|| Box::new(TUnion::mixed())),
                }))
            }
            _ => None,
        }
    }
}

/// Whether the property is visible from the analyzer's current class context
/// (Psalm's ClassAnalyzer::checkPropertyVisibility, without issue emission).
fn property_visible_from_context(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    prop_id: pzoom_str::StrId,
    visibility: Visibility,
) -> bool {
    let calling_class = analyzer.get_declaring_class();
    let declaring = class_info
        .declaring_property_ids
        .get(&prop_id)
        .copied()
        .unwrap_or(class_info.name);
    match visibility {
        Visibility::Public => true,
        Visibility::Private => calling_class == Some(declaring),
        Visibility::Protected => calling_class.is_some_and(|caller| {
            caller == declaring
                || analyzer
                    .codebase
                    .get_class(caller)
                    .is_some_and(|info| info.all_parent_classes.contains(&declaring))
                || analyzer
                    .codebase
                    .get_class(declaring)
                    .is_some_and(|info| info.all_parent_classes.contains(&caller))
        }),
    }
}
