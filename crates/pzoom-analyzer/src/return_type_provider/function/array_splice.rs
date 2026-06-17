//! `array_splice` return-type provider (Psalm's
//! `ArraySpliceReturnTypeProvider`).
//!
//! Psalm types the extracted slice from the input array: string-keyed inputs
//! keep their generic array type (including non-emptiness), int-keyed inputs
//! become lists of the matching value type.

use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct ArraySpliceReturnTypeProvider;

impl FunctionReturnTypeProvider for ArraySpliceReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_splice"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let first_pos = event.arg_positions.first().copied()?;
        let first_arg_type = analysis_data.expr_types.get(&first_pos).cloned()?;

        if first_arg_type.types.len() != 1 {
            return Some(TUnion::new(TAtomic::array(
                TUnion::array_key(),
                TUnion::mixed(),
            )));
        }

        // Reduce the input to a generic (key, value, non-empty) view; Psalm
        // calls TKeyedArray::getGenericArrayType for shapes.
        let (key_type, value_type, non_empty) = match first_arg_type.types.first()? {
            // A generic list (former TList/TNonEmptyList): empty known_values,
            // list-typed fallback; the key is `int`.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                is_list: true,
                is_nonempty,
                ..
            } if known_values.is_empty() => (TUnion::int(), params.1.clone(), *is_nonempty),
            // A generic array (former TArray/TNonEmptyArray): empty
            // known_values, typed fallback, not a list.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                is_nonempty,
                ..
            } if known_values.is_empty() => (params.0.clone(), params.1.clone(), *is_nonempty),
            // A keyed-array shape (former TKeyedArray) — including the empty
            // array `[]`, which has empty `known_values` and no typed fallback.
            // An empty shape yields no value type, so `value_type?` propagates
            // `None` and the provider defers to the stub, as before.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                let mut key_type: Option<TUnion> = None;
                let mut value_type: Option<TUnion> = None;
                for (key, (_possibly_undefined, prop_type)) in known_values.iter() {
                    let key_atomic = match key {
                        pzoom_code_info::ArrayKey::Int(value) => {
                            TAtomic::TLiteralInt { value: *value }
                        }
                        pzoom_code_info::ArrayKey::String(value)
                        | pzoom_code_info::ArrayKey::ClassString(value) => {
                            TAtomic::TLiteralString {
                                value: value.clone(),
                            }
                        }
                    };
                    let key_union = TUnion::new(key_atomic);
                    key_type = Some(match key_type {
                        Some(existing) => combine_union_types(&existing, &key_union, false),
                        None => key_union,
                    });
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, prop_type, false),
                        None => prop_type.clone(),
                    });
                }
                if let Some((fallback_key, fallback_value)) = params.as_deref() {
                    key_type = Some(match key_type {
                        Some(existing) => combine_union_types(&existing, fallback_key, false),
                        None => fallback_key.clone(),
                    });
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, fallback_value, false),
                        None => fallback_value.clone(),
                    });
                }
                let non_empty = known_values
                    .values()
                    .any(|(possibly_undefined, _)| !*possibly_undefined);
                (key_type?, value_type?, non_empty)
            }
            _ => {
                return Some(TUnion::new(TAtomic::array(
                    TUnion::array_key(),
                    TUnion::mixed(),
                )));
            }
        };

        // Psalm: a string-bearing key type keeps the generic array; an
        // int-only key type degrades to a list of the value-kind.
        let has_string_key = key_type.has_string();

        if has_string_key {
            let atomic = if non_empty {
                TAtomic::non_empty_array(key_type, value_type)
            } else {
                TAtomic::array(key_type, value_type)
            };
            return Some(TUnion::new(atomic));
        }

        let all_string = value_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
            )
        });
        let all_int = value_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
            )
        });
        let list_value = if all_string {
            TUnion::string()
        } else if all_int {
            TUnion::int()
        } else {
            TUnion::mixed()
        };
        Some(TUnion::new(TAtomic::list(list_value)))
    }
}
