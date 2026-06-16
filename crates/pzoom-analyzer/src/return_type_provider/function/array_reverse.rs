//! `array_reverse` return-type provider (Psalm's
//! `ArrayReverseReturnTypeProvider`).
//!
//! A generic array input reverses to itself — crucially preserving
//! non-emptiness, which the stub's conditional return cannot express. Lists
//! reverse to lists of the same value type when keys are not preserved; keyed
//! shapes simplify to their generic list form. Anything else falls through to
//! the stub.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArrayReverseReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayReverseReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_reverse"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let first_pos = event.arg_positions.first().copied()?;
        let first_arg_type = analysis_data.expr_types.get(&first_pos).cloned()?;

        if first_arg_type.types.len() != 1 {
            return None;
        }

        // `$preserve_keys` defaults to false; a non-false second argument
        // falls through to the stub's conditional.
        let preserve_keys_is_false = match event.arg_positions.get(1) {
            None => true,
            Some(second_pos) => analysis_data
                .expr_types
                .get(&*second_pos)
                .cloned()
                .is_some_and(|second_type| {
                    second_type.is_single()
                        && matches!(second_type.get_single(), Some(TAtomic::TFalse))
                }),
        };

        match first_arg_type.types.first()? {
            // Psalm returns the input type unchanged for generic arrays,
            // whatever the key-preservation flag.
            TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. } => {
                Some((*first_arg_type).clone())
            }
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } if preserve_keys_is_false => {
                Some((*first_arg_type).clone())
            }
            TAtomic::TKeyedArray {
                properties,
                is_list: true,
                fallback_value_type,
                ..
            } if preserve_keys_is_false => {
                let mut value_type: Option<TUnion> = None;
                for property_type in properties.values() {
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, property_type, false),
                        None => property_type.clone(),
                    });
                }
                if let Some(fallback) = fallback_value_type {
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, fallback, false),
                        None => (**fallback).clone(),
                    });
                }
                let value_type = value_type?;

                let non_empty = properties
                    .values()
                    .any(|property_type| !property_type.possibly_undefined);

                Some(TUnion::new(if non_empty {
                    TAtomic::TNonEmptyList {
                        value_type: Box::new(value_type),
                    }
                } else {
                    TAtomic::TList {
                        value_type: Box::new(value_type),
                    }
                }))
            }
            _ => None,
        }
    }
}
