//! `parse_url` return-type provider (Psalm's `ParseUrlReturnTypeProvider`).
//!
//! With no component argument (or an explicit default `-1`), the result is
//! the URL-parts shape `array{scheme?: string, ..., port?: int}|false`. A
//! known string-component constant yields `string|false|null`, `PHP_URL_PORT`
//! yields `int|false|null`, and any other non-default component falls back to
//! `string|int|null`.

use rustc_hash::FxHashMap;

use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ParseUrlReturnTypeProvider;

/// PHP_URL_SCHEME, PHP_URL_USER, PHP_URL_PASS, PHP_URL_HOST, PHP_URL_PATH,
/// PHP_URL_QUERY, PHP_URL_FRAGMENT
const STRING_COMPONENTS: &[i64] = &[0, 2, 3, 1, 5, 6, 7];
/// PHP_URL_PORT
const INT_COMPONENTS: &[i64] = &[4];

fn union_is_int_literals_in(union: &TUnion, allowed: &[i64]) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(
            |atomic| matches!(atomic, TAtomic::TLiteralInt { value } if allowed.contains(value)),
        )
}

impl FunctionReturnTypeProvider for ParseUrlReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["parse_url"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        if let Some(component_pos) = event.arg_positions.get(1) {
            let mut is_default_component = false;

            if let Some(component_type) = analysis_data.expr_types.get(&*component_pos).cloned()
                && !component_type.is_mixed()
            {
                if union_is_int_literals_in(&component_type, STRING_COMPONENTS) {
                    let mut result =
                        TUnion::from_types(vec![TAtomic::TString, TAtomic::TFalse, TAtomic::TNull]);
                    result.ignore_nullable_issues = true;
                    result.ignore_falsable_issues = true;
                    return Some(result);
                }

                if union_is_int_literals_in(&component_type, INT_COMPONENTS) {
                    let mut result =
                        TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFalse, TAtomic::TNull]);
                    result.ignore_nullable_issues = true;
                    result.ignore_falsable_issues = true;
                    return Some(result);
                }

                if component_type.types.len() == 1
                    && let Some(TAtomic::TLiteralInt { value }) = component_type.get_single()
                {
                    is_default_component = *value <= -1;
                }
            }

            if !is_default_component {
                let mut result =
                    TUnion::from_types(vec![TAtomic::TString, TAtomic::TInt, TAtomic::TNull]);
                result.ignore_nullable_issues = true;
                return Some(result);
            }
        }

        let mut properties = FxHashMap::default();
        for key in [
            "scheme", "user", "pass", "host", "path", "query", "fragment",
        ] {
            let mut component = TUnion::string();
            component.possibly_undefined = true;
            properties.insert(ArrayKey::String(key.to_string()), component);
        }
        let mut port = TUnion::int();
        port.possibly_undefined = true;
        properties.insert(ArrayKey::String("port".to_string()), port);

        let mut result = TUnion::from_types(vec![
            TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(properties),
                is_list: false,
                sealed: true,
                fallback_key_type: None,
                fallback_value_type: None,
            },
            TAtomic::TFalse,
        ]);
        result.ignore_falsable_issues = true;
        Some(result)
    }
}
