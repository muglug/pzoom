//! Port of Psalm's `ArrayUArrayParamsProvider`.
//!
//! `array_diff_ukey` and friends take a variable number of arrays followed by
//! one or two comparison callbacks, so the parameter list depends on the
//! call's argument count. pzoom's stubs model the tail as a plain variadic;
//! this provider rebuilds the per-call list so a misplaced array or callback
//! is flagged at the right position.

use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_str::StrId;
use pzoom_code_info::t_atomic::FunctionLikeParameter;
use pzoom_code_info::{TAtomic, TUnion};

use crate::function_analysis_data::FunctionAnalysisData;

use super::{FunctionParamsProvider, FunctionParamsProviderEvent, FunctionParamsProviderResult};

pub(super) struct ArrayUArrayParamsProvider;

impl FunctionParamsProvider for ArrayUArrayParamsProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &[
            "array_diff_ukey",
            "array_diff_uassoc",
            "array_intersect_ukey",
            "array_intersect_uassoc",
            "array_udiff_uassoc",
            "array_uintersect_uassoc",
            "array_udiff",
            "array_udiff_assoc",
            "array_uintersect",
            "array_uintersect_assoc",
        ]
    }

    fn get_function_params(
        &self,
        event: &FunctionParamsProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<FunctionParamsProviderResult> {
        let normalized = event.function_id.trim_start_matches('\\');
        // (trailing callback count, whether the first callback compares values)
        let (callback_count, first_callback_is_value) = match normalized {
            "array_diff_ukey" | "array_diff_uassoc" | "array_intersect_ukey"
            | "array_intersect_uassoc" => (1usize, false),
            "array_udiff_uassoc" | "array_uintersect_uassoc" => (2, true),
            "array_udiff" | "array_udiff_assoc" | "array_uintersect"
            | "array_uintersect_assoc" => (1, true),
            _ => return None,
        };

        // The callback shape mirrors Psalm's callmap entry for
        // array_udiff_uassoc: callable(mixed, mixed): int.
        let comparison_callback_type = TUnion::new(TAtomic::TCallable {
            params: Some(vec![
                FunctionLikeParameter {
                    name: None,
                    param_type: TUnion::mixed(),
                    is_optional: false,
                    is_variadic: false,
                    by_ref: false,
                },
                FunctionLikeParameter {
                    name: None,
                    param_type: TUnion::mixed(),
                    is_optional: false,
                    is_variadic: false,
                    by_ref: false,
                },
            ]),
            return_type: Some(Box::new(TUnion::int())),
            is_pure: None,
        });
        let make_callback_param = |name: &str| ParamInfo {
            name: event.analyzer.interner.intern(name),
            param_type: Some(comparison_callback_type.clone()),
            ..Default::default()
        };
        let value_callback = make_callback_param("$value_compare_func");
        let key_callback = make_callback_param("$key_compare_func");
        let array_param = ParamInfo {
            name: StrId::ARRAY_VAR,
            signature_type: Some(TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            })),
            ..Default::default()
        };

        let array_count = event.args.len().saturating_sub(callback_count).max(1);

        let mut params = Vec::with_capacity(array_count + callback_count);
        for _ in 0..array_count {
            params.push(array_param.clone());
        }
        if callback_count == 2 {
            params.push(value_callback);
            params.push(key_callback);
        } else if first_callback_is_value {
            params.push(value_callback);
        } else {
            params.push(key_callback);
        }

        Some(FunctionParamsProviderResult::Params(params))
    }
}
