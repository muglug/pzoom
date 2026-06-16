//! Psalm's PropertyMap (`dictionaries/PropertyMap.php`, applied in
//! `ClassLikeNodeScanner::leaveNode` / `Codebase\Reflection`): hardcoded
//! property types for well-known extension and vendor classes, overriding
//! whatever the scan produced. The canonical example is
//! `PhpParser\Node\Expr\Array_::$items => array<int, ArrayItem|null>` — the
//! vendor docblock says `ArrayItem[]`, but items are nullable in list
//! syntax, and Psalm's own code guards `if ($item)` accordingly.
//!
//! `dictionaries/property_map.json` is exported from Psalm's artifact by
//! `tools/export_property_map.php` (re-runnable; Psalm regenerates its map
//! with `bin/update-property-map.php` + `ManualPropertyMap.php` overrides).

use std::sync::OnceLock;

use pzoom_code_info::TUnion;
use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::member_visibility::Visibility;
use pzoom_code_info::property_info::PropertyInfo;
use pzoom_str::ThreadedInterner;
use rustc_hash::FxHashMap;

static PROPERTY_MAP_JSON: &str = include_str!("../../../dictionaries/property_map.json");

static PROPERTY_MAP: OnceLock<FxHashMap<String, Vec<(String, String)>>> = OnceLock::new();

fn property_map() -> &'static FxHashMap<String, Vec<(String, String)>> {
    PROPERTY_MAP.get_or_init(|| {
        let parsed: FxHashMap<String, FxHashMap<String, String>> =
            serde_json::from_str(PROPERTY_MAP_JSON).expect("property_map.json must parse");
        parsed
            .into_iter()
            .map(|(class, props)| {
                let mut props: Vec<(String, String)> = props.into_iter().collect();
                props.sort();
                (class, props)
            })
            .collect()
    })
}

/// Psalm's `ClassLikeNodeScanner::leaveNode` PropertyMap application: each
/// mapped property's type string is parsed (names are fully qualified, as
/// with Psalm's `Type::parseString`) and overrides the scanned property,
/// creating it when absent.
pub(crate) fn apply_property_map(class_info: &mut ClassLikeInfo, interner: &ThreadedInterner) {
    let fq_lower = interner.lookup(class_info.name).to_lowercase();
    let Some(mapped_properties) = property_map().get(&fq_lower) else {
        return;
    };

    for (property_name, type_string) in mapped_properties {
        let Ok(mut property_type) =
            crate::docblock::parse_type_string(type_string, interner.parent_ref())
        else {
            continue;
        };

        // Psalm hard-codes `DateInterval::$days`'s falsable leniency.
        if fq_lower == "dateinterval" && property_name == "days" {
            property_type.ignore_falsable_issues = true;
        }

        let property_name_id = interner.intern(property_name);

        match class_info.properties.get(&property_name_id) {
            Some(existing) => {
                let mut updated = (**existing).clone();
                updated.property_type = Some(property_type);
                class_info
                    .properties
                    .insert(property_name_id, std::sync::Arc::new(updated));
            }
            None => {
                class_info.properties.insert(
                    property_name_id,
                    std::sync::Arc::new(PropertyInfo {
                        name: property_name_id,
                        declaring_class: class_info.name,
                        property_type: Some(property_type),
                        signature_type: None,
                        visibility: Visibility::Public,
                        is_static: false,
                        is_readonly: false,
                        is_readonly_native: false,
                        readonly_allow_private_mutation: false,
                        has_default: false,
                        is_promoted: false,
                        is_hooked: false,
                        is_deprecated: false,
                        // Psalm's mapped properties carry no location and
                        // are skipped by the initialization checks.
                        location_free: true,
                        marked_initialized: false,
                        internal: Vec::new(),
                        description: None,
                        start_offset: class_info.start_offset,
                    }),
                );
            }
        }
    }
}

#[allow(dead_code)]
fn _assert_types(_: Option<TUnion>) {}
