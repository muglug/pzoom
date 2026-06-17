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
            return Some(TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            }));
        }

        // Reduce the input to a generic (key, value, non-empty) view; Psalm
        // calls TKeyedArray::getGenericArrayType for shapes.
        let (key_type, value_type, non_empty) = match first_arg_type.types.first()? {
            TAtomic::TArray {
                key_type,
                value_type,
            } => ((**key_type).clone(), (**value_type).clone(), false),
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => ((**key_type).clone(), (**value_type).clone(), true),
            TAtomic::TList { value_type } => (TUnion::int(), (**value_type).clone(), false),
            TAtomic::TNonEmptyList { value_type } => (TUnion::int(), (**value_type).clone(), true),
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                let mut key_type: Option<TUnion> = None;
                let mut value_type: Option<TUnion> = None;
                for (key, prop_type) in properties.iter() {
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
                if let (Some(fallback_key), Some(fallback_value)) =
                    (fallback_key_type, fallback_value_type)
                {
                    key_type = Some(match key_type {
                        Some(existing) => combine_union_types(&existing, fallback_key, false),
                        None => (**fallback_key).clone(),
                    });
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, fallback_value, false),
                        None => (**fallback_value).clone(),
                    });
                }
                let non_empty = properties.values().any(|prop| !prop.possibly_undefined);
                (key_type?, value_type?, non_empty)
            }
            _ => {
                return Some(TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                }));
            }
        };

        // Psalm: a string-bearing key type keeps the generic array; an
        // int-only key type degrades to a list of the value-kind.
        let has_string_key = key_type.has_string();

        if has_string_key {
            let atomic = if non_empty {
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                }
            } else {
                TAtomic::TArray {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                }
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
        Some(TUnion::new(TAtomic::TList {
            value_type: Box::new(list_value),
        }))
    }
}
