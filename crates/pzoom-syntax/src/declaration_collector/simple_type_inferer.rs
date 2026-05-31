//! Simple type inference for constant expressions.
//!
//! Mirrors Hakana's `code_info_builder/simple_type_inferer.rs` and Psalm's
//! `SimpleTypeInferer`: infer a type from a *simple* constant expression
//! (literals, arrays/shapes of literals, `::class`, unary minus, …) during
//! scanning, without running the full analyzer. Used for constant values,
//! enum case values and parameter defaults. The entry point is [`infer`].

use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use rustc_hash::FxHashMap;

use pzoom_code_info::class_constant_info::ClassConstantInfo;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::{Interner, StrId};

/// Infer a parameter default's type: a simple constant expression, or a
/// `self::CONST` / `static::CONST` reference resolved against `class_constants`.
pub(crate) fn infer_param_default_type(
    expr: &Expression<'_>,
    interner: &Interner,
    self_class: Option<StrId>,
    class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
) -> Option<TUnion> {
    if let Some(inferred) = infer(expr) {
        return Some(inferred);
    }

    let class_constants = class_constants?;

    let Expression::Access(Access::ClassConstant(class_constant_access)) = expr.unparenthesized()
    else {
        return None;
    };

    let ClassLikeConstantSelector::Identifier(constant_name) = &class_constant_access.constant
    else {
        return None;
    };

    let is_current_class_reference = match class_constant_access.class.unparenthesized() {
        Expression::Self_(_) | Expression::Static(_) => true,
        Expression::Identifier(identifier) => self_class.is_some_and(|class_id| {
            let declared_class_name = interner.lookup(class_id);
            let declared_short_name = declared_class_name
                .rsplit('\\')
                .next()
                .unwrap_or(declared_class_name.as_ref());

            identifier
                .value()
                .eq_ignore_ascii_case(declared_class_name.as_ref())
                || identifier.value().eq_ignore_ascii_case(declared_short_name)
                || identifier
                    .value()
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(declared_class_name.trim_start_matches('\\').as_ref())
        }),
        _ => false,
    };

    if !is_current_class_reference {
        return None;
    }

    let const_name = interner.intern(constant_name.value);
    class_constants
        .get(&const_name)
        .map(|const_info| const_info.constant_type.clone())
}

/// Infer the type of a simple constant expression. Returns `None` when the
/// expression isn't a simple constant pzoom can evaluate at scan time.
pub(crate) fn infer(expr: &Expression<'_>) -> Option<TUnion> {
    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => infer(parenthesized.expression),
        Expression::Binary(binary) => match binary.operator {
            BinaryOperator::StringConcat(_) => Some(TUnion::string()),
            BinaryOperator::Division(_) => Some(TUnion::float()),
            BinaryOperator::Addition(_)
            | BinaryOperator::Subtraction(_)
            | BinaryOperator::Multiplication(_)
            | BinaryOperator::Modulo(_)
            | BinaryOperator::Exponentiation(_)
            | BinaryOperator::BitwiseAnd(_)
            | BinaryOperator::BitwiseOr(_)
            | BinaryOperator::BitwiseXor(_)
            | BinaryOperator::LeftShift(_)
            | BinaryOperator::RightShift(_) => Some(TUnion::int()),
            BinaryOperator::Equal(_)
            | BinaryOperator::NotEqual(_)
            | BinaryOperator::Identical(_)
            | BinaryOperator::NotIdentical(_)
            | BinaryOperator::AngledNotEqual(_)
            | BinaryOperator::LessThan(_)
            | BinaryOperator::LessThanOrEqual(_)
            | BinaryOperator::GreaterThan(_)
            | BinaryOperator::GreaterThanOrEqual(_)
            | BinaryOperator::Spaceship(_)
            | BinaryOperator::And(_)
            | BinaryOperator::Or(_)
            | BinaryOperator::LowAnd(_)
            | BinaryOperator::LowOr(_)
            | BinaryOperator::LowXor(_)
            | BinaryOperator::Instanceof(_) => Some(TUnion::bool()),
            BinaryOperator::NullCoalesce(_) => {
                infer(binary.lhs).or_else(|| infer(binary.rhs))
            }
        },
        Expression::UnaryPrefix(unary) => match &unary.operator {
            UnaryPrefixOperator::Plus(_) => infer(unary.operand),
            UnaryPrefixOperator::Negation(_) => {
                let operand_type = infer(unary.operand)?;
                Some(negate_simple_union(operand_type))
            }
            _ => None,
        },
        Expression::Literal(Literal::Null(_)) => Some(TUnion::null()),
        Expression::Literal(Literal::True(_)) => Some(TUnion::new(TAtomic::TTrue)),
        Expression::Literal(Literal::False(_)) => Some(TUnion::new(TAtomic::TFalse)),
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(|value| TUnion::new(TAtomic::TLiteralInt { value }))
            .or_else(|| Some(TUnion::int())),
        Expression::Literal(Literal::Float(float_lit)) => {
            Some(TUnion::new(TAtomic::TLiteralFloat {
                value: float_lit.value.into_inner(),
            }))
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| {
                TUnion::new(TAtomic::TLiteralString {
                    value: value.to_string(),
                })
            })
            .or_else(|| Some(TUnion::string())),
        Expression::Access(Access::ClassConstant(class_constant_access)) => {
            let ClassLikeConstantSelector::Identifier(constant_name) =
                &class_constant_access.constant
            else {
                return None;
            };

            if !constant_name.value.eq_ignore_ascii_case("class") {
                return None;
            }

            let Expression::Identifier(class_identifier) =
                class_constant_access.class.unparenthesized()
            else {
                return None;
            };

            Some(TUnion::new(TAtomic::TLiteralString {
                value: class_identifier.value().trim_start_matches('\\').to_string(),
            }))
        }
        Expression::Array(array) => infer_simple_array_type(array.elements.iter()),
        Expression::LegacyArray(array) => infer_simple_array_type(array.elements.iter()),
        _ => None,
    }
}

fn infer_simple_array_type<'a>(
    elements: impl Iterator<Item = &'a ArrayElement<'a>>,
) -> Option<TUnion> {
    let mut properties = FxHashMap::default();
    let mut next_int_key = 0i64;
    let mut is_list = true;

    for element in elements {
        match element {
            ArrayElement::KeyValue(kv) => {
                let key_type = infer(kv.key)?;
                let value_type = infer(kv.value)?;
                let key = simple_union_to_array_key(&key_type)?;

                if !matches!(key, ArrayKey::Int(value) if value == next_int_key) {
                    is_list = false;
                }

                if let ArrayKey::Int(value) = key {
                    next_int_key = value + 1;
                    properties.insert(ArrayKey::Int(value), value_type);
                } else {
                    properties.insert(key, value_type);
                }
            }
            ArrayElement::Value(value) => {
                let value_type = infer(value.value)?;
                properties.insert(ArrayKey::Int(next_int_key), value_type);
                next_int_key += 1;
            }
            ArrayElement::Missing(_) => {}
            ArrayElement::Variadic(_) => return None,
        }
    }

    if properties.is_empty() {
        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::nothing()),
            value_type: Box::new(TUnion::nothing()),
        }));
    }

    Some(TUnion::new(TAtomic::TKeyedArray {
        properties,
        is_list,
        sealed: true,
        fallback_key_type: None,
        fallback_value_type: None,
    }))
}

fn simple_union_to_array_key(union: &TUnion) -> Option<ArrayKey> {
    let single = union.get_single()?;

    match single {
        TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => value
            .parse::<i64>()
            .ok()
            .map(ArrayKey::Int)
            .or_else(|| Some(ArrayKey::String(value.clone()))),
        TAtomic::TNull => Some(ArrayKey::String(String::new())),
        _ => None,
    }
}

fn negate_simple_union(t_union: TUnion) -> TUnion {
    if !t_union.is_single() {
        return t_union;
    }

    match t_union.get_single().cloned() {
        Some(TAtomic::TLiteralInt { value }) => TUnion::new(TAtomic::TLiteralInt { value: -value }),
        Some(TAtomic::TLiteralFloat { value }) => {
            TUnion::new(TAtomic::TLiteralFloat { value: -value })
        }
        Some(TAtomic::TInt) => TUnion::int(),
        Some(TAtomic::TFloat) => TUnion::float(),
        _ => t_union,
    }
}
