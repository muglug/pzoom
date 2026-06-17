//! `"array_map"` return-type provider.

use pzoom_code_info::{TAtomic, TUnion};
use rustc_hash::FxHashMap;

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
pub(super) struct ArrayMapReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayMapReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_map"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_map_return_type(
            event.analyzer,
            event.args,
            event.arg_positions,
            analysis_data,
            event.context,
        )
    }
}

fn infer_array_map_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    if args.len() < 2 || arg_positions.len() < 2 {
        return None;
    }

    let callback_type = analysis_data.expr_types.get(&arg_positions[0]).cloned()?;

    // Psalm's null-callback (zip) path is only precise for sealed keyed-shape
    // arguments; everything else — including spreads — returns the plain
    // possibly-empty array (Type::getArray()).
    if callback_type.is_null() {
        return Some(TUnion::new(TAtomic::array(
            TUnion::array_key(),
            TUnion::mixed(),
        )));
    }

    let mut input_array_infos = Vec::new();
    let mut callback_input_types = Vec::new();
    let mut first_array_type = None;
    for arg_pos in arg_positions.iter().skip(1) {
        let array_type = analysis_data.expr_types.get(&*arg_pos).cloned()?;
        if first_array_type.is_none() {
            first_array_type = Some(array_type.clone());
        }
        let info = fca::extract_array_like_info_from_union(&array_type)?;
        callback_input_types.push(info.value_type.clone());
        input_array_infos.push(info);
    }

    let mut callback_return_type = fca::infer_array_map_callable_return_type(
        analyzer,
        &callback_type,
        &callback_input_types,
        context,
    )
    .unwrap_or_else(TUnion::mixed);
    // A void callback produces null elements (Psalm converts a consumed void
    // value to null).
    if callback_return_type.is_void() {
        callback_return_type = TUnion::new(pzoom_code_info::TAtomic::TNull);
    }

    let first_info = input_array_infos.first()?;

    // Mirror Psalm's ArrayMapReturnTypeProvider: with a single array argument
    // that is a known keyed-array shape, preserve the shape and map each
    // property's type through the callback return type.
    if args.len() == 2 {
        if let Some(first_array_type) = &first_array_type {
            // A known keyed-array shape (former TKeyedArray): non-empty
            // known_values, or the empty array `[]` (empty known_values with no
            // typed fallback). A *generic* `array<...>`/`list<...>` (empty
            // known_values with a typed fallback) is not a shape and is handled
            // by the generic-array path below.
            if let Some(TAtomic::TArray {
                known_values,
                params,
                is_list,
                is_sealed,
                ..
            }) = first_array_type.get_single()
                && (!known_values.is_empty() || params.is_none())
            {
                let mut new_known_values: FxHashMap<_, (bool, TUnion)> = FxHashMap::default();
                for (key, (possibly_undefined, _prop)) in known_values.iter() {
                    new_known_values.insert(
                        key.clone(),
                        (*possibly_undefined, callback_return_type.clone()),
                    );
                }

                // Map the fallback value through the callback, preserving the
                // fallback key, only when a typed fallback is present.
                let (new_fallback_key, new_fallback_value) = match params.as_deref() {
                    Some((fk, _)) => (Some(fk.clone()), Some(callback_return_type.clone())),
                    None => (None, None),
                };

                return Some(TUnion::new(TAtomic::keyed_array(
                    new_known_values,
                    *is_list,
                    *is_sealed,
                    new_fallback_key,
                    new_fallback_value,
                )));
            }
        }
    }

    if args.len() == 2 {
        if first_info.is_list {
            let atomic = if first_info.is_non_empty {
                TAtomic::non_empty_list(callback_return_type)
            } else {
                TAtomic::list(callback_return_type)
            };
            return Some(TUnion::new(atomic));
        }

        let key_type = if first_info.key_type.is_nothing() {
            TUnion::array_key()
        } else {
            first_info.key_type.clone()
        };
        let atomic = if first_info.is_non_empty {
            TAtomic::non_empty_array(key_type, callback_return_type)
        } else {
            TAtomic::array(key_type, callback_return_type)
        };
        return Some(TUnion::new(atomic));
    }

    let all_non_empty = input_array_infos.iter().all(|info| info.is_non_empty);
    let atomic = if all_non_empty {
        TAtomic::non_empty_list(callback_return_type)
    } else {
        TAtomic::list(callback_return_type)
    };

    Some(TUnion::new(atomic))
}
