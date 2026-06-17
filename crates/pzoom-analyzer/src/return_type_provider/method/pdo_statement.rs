//! PDOStatement::fetch/fetchAll return-type provider (mirrors Psalm's
//! PdoStatementReturnTypeProvider).

use mago_syntax::ast::ast::argument::Argument;
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
        if event.method_name.eq_ignore_ascii_case("fetch") {
            return handle_fetch(event);
        }

        if !event.method_name.eq_ignore_ascii_case("fetchAll") {
            return None;
        }

        let mode = event
            .arg_positions
            .first()
            .and_then(|p| event.analysis_data.expr_types.get(&*p).cloned())
            .and_then(|t| match t.get_single() {
                Some(TAtomic::TLiteralInt { value }) => Some(*value),
                _ => None,
            })?;

        let scalar_or_null = || TUnion::from_types(vec![TAtomic::TScalar, TAtomic::TNull]);
        let list_of = |value: TUnion| TUnion::new(TAtomic::list(value));
        let array_of = |key: TUnion, value: TUnion| TUnion::new(TAtomic::array(key, value));

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
                    TAtomic::list(scalar_or_null()),
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

/// `PDOStatement::fetch(mode)` (Psalm's `handleFetch`): the first positional
/// argument — or the named `mode:` argument — selects the row shape.
fn handle_fetch(event: &MethodReturnTypeProviderEvent<'_, '_>) -> Option<TUnion> {
    let mut fetch_mode = 0;
    for (arg, arg_pos) in event.args.iter().zip(event.arg_positions) {
        if let Argument::Named(named_arg) = arg
            && named_arg.name.value != "mode"
        {
            continue;
        }
        if let Some(arg_type) = event.analysis_data.expr_types.get(&*arg_pos).cloned()
            && let Some(TAtomic::TLiteralInt { value }) = arg_type.get_single()
        {
            fetch_mode = *value;
        }
        break;
    }

    let scalar_or_null = || TUnion::from_types(vec![TAtomic::TScalar, TAtomic::TNull]);
    let array_of = |key: TUnion, value: TUnion| TUnion::new(TAtomic::array(key, value));
    let or_false = |atomic: TAtomic| TUnion::from_types(vec![atomic, TAtomic::TFalse]);

    let result = match fetch_mode {
        // FETCH_LAZY
        1 => or_false(TAtomic::TObject),
        // FETCH_ASSOC
        2 => or_false(TAtomic::array(TUnion::string(), scalar_or_null())),
        // FETCH_NUM
        3 => or_false(TAtomic::list(scalar_or_null())),
        // FETCH_BOTH
        4 => or_false(TAtomic::array(TUnion::array_key(), scalar_or_null())),
        // FETCH_OBJ
        5 => or_false(TAtomic::named_object(StrId::STDCLASS)),
        // FETCH_BOUND
        6 => TUnion::bool(),
        // FETCH_COLUMN
        7 => TUnion::from_types(vec![TAtomic::TScalar, TAtomic::TNull, TAtomic::TFalse]),
        // FETCH_CLASS
        8 => or_false(TAtomic::TObject),
        // FETCH_NAMED
        11 => or_false(TAtomic::array(
            TUnion::string(),
            TUnion::from_types(vec![
                TAtomic::TScalar,
                TAtomic::TNull,
                TAtomic::list(scalar_or_null()),
            ]),
        )),
        // FETCH_KEY_PAIR
        12 => array_of(TUnion::array_key(), scalar_or_null()),
        _ => return None,
    };

    Some(result)
}

/// Extract the class name from `fetchAll`'s second argument (`SomeClass::class`).
fn pdo_fetch_class_name(event: &MethodReturnTypeProviderEvent<'_, '_>) -> Option<StrId> {
    let arg_type = event
        .analysis_data
        .expr_types
        .get(&*event.arg_positions.get(1)?)
        .cloned()?;
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
