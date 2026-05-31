//! PDOStatement::fetchAll return-type provider (mirrors Psalm's
//! PdoStatementReturnTypeProvider).

use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::StrId;

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent};

pub(super) struct PdoStatementReturnTypeProvider;

impl MethodReturnTypeProvider for PdoStatementReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["PDOStatement"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("fetchAll") {
            return None;
        }

        let mode = event
            .arg_positions
            .first()
            .and_then(|p| event.analysis_data.get_expr_type(*p))
            .and_then(|t| match t.get_single() {
                Some(TAtomic::TLiteralInt { value }) => Some(*value),
                _ => None,
            })?;

        let scalar_or_null = || TUnion::from_types(vec![TAtomic::TScalar, TAtomic::TNull]);
        let list_of = |value: TUnion| {
            TUnion::new(TAtomic::TList {
                value_type: Box::new(value),
            })
        };
        let array_of = |key: TUnion, value: TUnion| {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(key),
                value_type: Box::new(value),
            })
        };

        let result = match mode {
            // FETCH_ASSOC
            2 => list_of(array_of(TUnion::string(), scalar_or_null())),
            // FETCH_NUM
            3 => list_of(list_of(scalar_or_null())),
            // FETCH_BOTH
            4 => list_of(array_of(TUnion::array_key(), scalar_or_null())),
            // FETCH_OBJ
            5 => list_of(TUnion::new(TAtomic::named_object(StrId::STDCLASS))),
            // FETCH_BOUND
            6 => list_of(TUnion::bool()),
            // FETCH_COLUMN
            7 => list_of(scalar_or_null()),
            // FETCH_CLASS (optionally with a class-name second argument)
            8 => {
                let element = pdo_fetch_class_name(event)
                    .map(TAtomic::named_object)
                    .unwrap_or(TAtomic::TObject);
                list_of(TUnion::new(element))
            }
            // FETCH_NAMED
            11 => {
                let inner = TUnion::from_types(vec![
                    TAtomic::TScalar,
                    TAtomic::TNull,
                    TAtomic::TList {
                        value_type: Box::new(scalar_or_null()),
                    },
                ]);
                list_of(array_of(TUnion::string(), inner))
            }
            // FETCH_KEY_PAIR
            12 => array_of(TUnion::array_key(), scalar_or_null()),
            _ => return None,
        };

        Some(result)
    }
}

/// Extract the class name from `fetchAll`'s second argument (`SomeClass::class`).
fn pdo_fetch_class_name(event: &MethodReturnTypeProviderEvent<'_, '_>) -> Option<StrId> {
    let arg_type = event
        .analysis_data
        .get_expr_type(*event.arg_positions.get(1)?)?;
    match arg_type.get_single() {
        Some(TAtomic::TClassString {
            as_type: Some(as_type),
        }) => match as_type.as_ref() {
            TAtomic::TNamedObject { name, .. } => Some(*name),
            _ => None,
        },
        Some(TAtomic::TLiteralClassString { name }) => Some(event.analyzer.interner.intern(name)),
        Some(TAtomic::TLiteralString { value }) => Some(event.analyzer.interner.intern(value)),
        _ => None,
    }
}
