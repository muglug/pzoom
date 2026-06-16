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

use pzoom_code_info::class_constant_info::{ClassConstantInfo, UnresolvedConstExpr};
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
    infer_in_class(expr, None)
}

/// [`infer`] with a class context, so `self::class` (a literal string Psalm's
/// SimpleTypeInferer resolves during scanning) evaluates inside class
/// constants like CodeLocation::PROPERTY_KEYS_FOR_UNSERIALIZE.
/// Context for simple inference: the enclosing class name (for
/// `self::class`) and an optional resolver mapping unqualified class
/// identifiers to FQCNs through the file's use/namespace state (Psalm's
/// SimpleTypeInferer resolves `Foo::class` with the file aliases).
pub(crate) struct InferClassContext<'a> {
    pub self_class: Option<&'a str>,
    /// The enclosing class's resolved parent FQCN, for `parent::CONST` in
    /// constant initializers.
    pub parent_class: Option<&'a str>,
    pub class_resolver: Option<&'a dyn Fn(&str) -> String>,
    /// Spans of array elements whose implicit key would overflow i64 (a PHP
    /// fatal; Psalm reports InvalidArrayOffset). Recorded for the caller to
    /// surface.
    pub key_overflow_sink: Option<&'a std::cell::RefCell<Vec<(u32, u32)>>>,
    /// Resolves `Enum::CASE->name` / `->value` against already-scanned enum
    /// declarations: (resolved class name, case name, wants_name) -> literal.
    pub enum_case_resolver: Option<&'a dyn Fn(&str, &str, bool) -> Option<TUnion>>,
    /// Resolves a bare global constant name against constants already
    /// collected in this file (Psalm's SimpleTypeInferer receives the file's
    /// `$existing_constants`).
    pub global_constant_resolver: Option<&'a dyn Fn(&str) -> Option<TUnion>>,
}

pub fn infer_in_class(expr: &Expression<'_>, self_class: Option<&str>) -> Option<TUnion> {
    infer_with_context(
        expr,
        &InferClassContext {
            self_class,
            parent_class: None,
            class_resolver: None,
            key_overflow_sink: None,
            enum_case_resolver: None,
            global_constant_resolver: None,
        },
    )
}

pub(crate) fn infer_with_context(
    expr: &Expression<'_>,
    infer_context: &InferClassContext<'_>,
) -> Option<TUnion> {
    let self_class = infer_context.self_class;
    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => {
            infer_with_context(parenthesized.expression, infer_context)
        }
        // A heredoc/nowdoc with only literal parts is a literal string
        // (Psalm's SimpleTypeInferer resolves these for constants).
        Expression::CompositeString(mago_syntax::ast::ast::string::CompositeString::Document(
            document,
        )) => {
            let mut value = String::new();
            for part in document.parts.iter() {
                match part {
                    mago_syntax::ast::ast::string::StringPart::Literal(literal) => {
                        value.push_str(literal.value);
                    }
                    _ => return Some(TUnion::string()),
                }
            }
            Some(TUnion::new(TAtomic::string_from_literal(
                value,
                pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
            )))
        }
        // Psalm's SimpleTypeInferer: __DIR__/__FILE__ are non-empty strings,
        // __LINE__ is int<1, max>, the name constants are strings.
        Expression::MagicConstant(magic_constant) => {
            use mago_syntax::ast::ast::magic_constant::MagicConstant;
            Some(match magic_constant {
                MagicConstant::Directory(_) | MagicConstant::File(_) => {
                    TUnion::new(TAtomic::TNonEmptyString)
                }
                MagicConstant::Line(_) => TUnion::new(TAtomic::TIntRange {
                    min: Some(1),
                    max: None,
                }),
                _ => TUnion::string(),
            })
        }
        Expression::Binary(binary) => match binary.operator {
            BinaryOperator::StringConcat(_) => {
                // Psalm's SimpleTypeInferer concatenates literal pieces into a
                // literal string (constant array keys depend on this).
                let literal_piece = |union: &TUnion| -> Option<String> {
                    match union.get_single()? {
                        TAtomic::TLiteralString { value } => Some(value.clone()),
                        // `self::class . '...'` concatenates the class name.
                        TAtomic::TLiteralClassString { name } => Some(name.clone()),
                        _ => None,
                    }
                };
                // An operand that can't be inferred yet (a cross-class
                // constant) defers the whole expression to late resolution.
                let lhs_type = infer_with_context(binary.lhs, infer_context)?;
                let rhs_type = infer_with_context(binary.rhs, infer_context)?;
                if let (Some(lhs_value), Some(rhs_value)) =
                    (literal_piece(&lhs_type), literal_piece(&rhs_type))
                {
                    return Some(TUnion::new(TAtomic::string_from_literal(
                        format!("{lhs_value}{rhs_value}"),
                        pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
                    )));
                }
                Some(TUnion::string())
            }
            BinaryOperator::Division(_) => {
                // Psalm divides int literals exactly: an even division stays
                // an int literal (JIT_BUFFER/1024/1024 = 64), otherwise float.
                if let Some(lhs_type) = infer_with_context(binary.lhs, infer_context)
                    && let Some(rhs_type) = infer_with_context(binary.rhs, infer_context)
                    && let Some(TAtomic::TLiteralInt { value: lhs_value }) = lhs_type.get_single()
                    && let Some(TAtomic::TLiteralInt { value: rhs_value }) = rhs_type.get_single()
                    && *rhs_value != 0
                    && lhs_value % rhs_value == 0
                {
                    return Some(TUnion::new(TAtomic::TLiteralInt {
                        value: lhs_value / rhs_value,
                    }));
                }
                Some(TUnion::float())
            }
            BinaryOperator::Addition(_)
            | BinaryOperator::Subtraction(_)
            | BinaryOperator::Multiplication(_)
            | BinaryOperator::Modulo(_)
            | BinaryOperator::Exponentiation(_)
            | BinaryOperator::BitwiseAnd(_)
            | BinaryOperator::BitwiseOr(_)
            | BinaryOperator::BitwiseXor(_)
            | BinaryOperator::LeftShift(_)
            | BinaryOperator::RightShift(_) => {
                // Psalm computes literal int arithmetic exactly in constant
                // initializers (128 * 1024 * 1024 stays a literal). An operand
                // that can't be inferred yet (a cross-class constant) defers
                // the whole expression to the late-resolution IR.
                let lhs_type = infer_with_context(binary.lhs, infer_context)?;
                let rhs_type = infer_with_context(binary.rhs, infer_context)?;
                // PHP's `+` on arrays keeps the left operand's keys.
                if matches!(binary.operator, BinaryOperator::Addition(_))
                    && let (
                        Some(TAtomic::TKeyedArray {
                            properties: lhs_properties,
                            is_list: lhs_is_list,
                            sealed,
                            fallback_key_type,
                            fallback_value_type,
                        }),
                        Some(TAtomic::TKeyedArray {
                            properties: rhs_properties,
                            is_list: rhs_is_list,
                            ..
                        }),
                    ) = (lhs_type.get_single(), rhs_type.get_single())
                {
                    let mut properties = (**lhs_properties).clone();
                    for (key, value) in rhs_properties.iter() {
                        properties
                            .entry(key.clone())
                            .or_insert_with(|| value.clone());
                    }
                    return Some(TUnion::new(TAtomic::TKeyedArray {
                        properties: std::sync::Arc::new(properties),
                        is_list: *lhs_is_list && *rhs_is_list,
                        sealed: *sealed,
                        fallback_key_type: fallback_key_type.clone(),
                        fallback_value_type: fallback_value_type.clone(),
                    }));
                }
                if let Some(TAtomic::TLiteralInt { value: lhs }) = lhs_type.get_single()
                    && let Some(TAtomic::TLiteralInt { value: rhs }) = rhs_type.get_single()
                {
                    let computed = match binary.operator {
                        BinaryOperator::Addition(_) => lhs.checked_add(*rhs),
                        BinaryOperator::Subtraction(_) => lhs.checked_sub(*rhs),
                        BinaryOperator::Multiplication(_) => lhs.checked_mul(*rhs),
                        BinaryOperator::Modulo(_) => {
                            if *rhs != 0 {
                                lhs.checked_rem(*rhs)
                            } else {
                                None
                            }
                        }
                        BinaryOperator::BitwiseAnd(_) => Some(lhs & rhs),
                        BinaryOperator::BitwiseOr(_) => Some(lhs | rhs),
                        BinaryOperator::BitwiseXor(_) => Some(lhs ^ rhs),
                        BinaryOperator::LeftShift(_) => u32::try_from(*rhs)
                            .ok()
                            .and_then(|shift| lhs.checked_shl(shift)),
                        BinaryOperator::RightShift(_) => u32::try_from(*rhs)
                            .ok()
                            .and_then(|shift| lhs.checked_shr(shift)),
                        _ => None,
                    };
                    if let Some(value) = computed {
                        return Some(TUnion::new(TAtomic::TLiteralInt { value }));
                    }
                }
                Some(TUnion::int())
            }
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
            BinaryOperator::NullCoalesce(_) => infer_with_context(binary.lhs, infer_context)
                .or_else(|| infer_with_context(binary.rhs, infer_context)),
        },
        Expression::UnaryPrefix(unary) => match &unary.operator {
            UnaryPrefixOperator::Plus(_) => infer_with_context(unary.operand, infer_context),
            UnaryPrefixOperator::Negation(_) => {
                let operand_type = infer_with_context(unary.operand, infer_context)?;
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
        Expression::Literal(Literal::String(string_lit)) => {
            // mago's cooked value drops the backslash of unrecognized escape
            // sequences ("Functions\Module" must keep its backslash, as PHP
            // does); re-cook from the raw text.
            let value = super::php_unescape_string_literal(string_lit);
            // Scan-time inference has no Config; Psalm's default
            // maxStringLength applies.
            Some(TUnion::new(TAtomic::string_from_literal(
                value,
                pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
            )))
        }
        // `Enum::CASE->name` / `Enum::CASE->value` in a constant initializer
        // (Psalm's ConstantTypeResolver EnumNameFetch/EnumValueFetch).
        Expression::Access(Access::Property(property_access)) => {
            let resolver = infer_context.enum_case_resolver?;
            let mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector::Identifier(
                property_name,
            ) = &property_access.property
            else {
                return None;
            };
            let wants_name = match property_name.value {
                "name" => true,
                "value" => false,
                _ => return None,
            };
            let Expression::Access(Access::ClassConstant(case_access)) =
                property_access.object.unparenthesized()
            else {
                return None;
            };
            let ClassLikeConstantSelector::Identifier(case_name) = &case_access.constant else {
                return None;
            };
            let class_name = match case_access.class.unparenthesized() {
                Expression::Identifier(class_identifier) => {
                    let raw = class_identifier.value();
                    if let Some(stripped) = raw.strip_prefix('\\') {
                        stripped.to_string()
                    } else if let Some(class_resolver) = infer_context.class_resolver {
                        class_resolver(raw)
                    } else {
                        raw.to_string()
                    }
                }
                Expression::Self_(_) | Expression::Static(_) => self_class?.to_string(),
                _ => return None,
            };
            resolver(&class_name, case_name.value, wants_name)
        }
        Expression::Access(Access::ClassConstant(class_constant_access)) => {
            let ClassLikeConstantSelector::Identifier(constant_name) =
                &class_constant_access.constant
            else {
                return None;
            };

            if !constant_name.value.eq_ignore_ascii_case("class") {
                return None;
            }

            let class_name = match class_constant_access.class.unparenthesized() {
                Expression::Identifier(class_identifier) => {
                    let raw = class_identifier.value();
                    if let Some(stripped) = raw.strip_prefix('\\') {
                        stripped.to_string()
                    } else if let Some(resolver) = infer_context.class_resolver {
                        resolver(raw)
                    } else {
                        raw.to_string()
                    }
                }
                Expression::Self_(_) => self_class?.to_string(),
                _ => return None,
            };

            // Psalm types `X::class` as a literal CLASS string, so the
            // constant satisfies `class-string` parameters.
            Some(TUnion::new(TAtomic::TLiteralClassString {
                name: class_name,
            }))
        }
        Expression::Array(array) => infer_simple_array_type(array.elements.iter(), infer_context),
        Expression::LegacyArray(array) => {
            infer_simple_array_type(array.elements.iter(), infer_context)
        }
        // A bare global constant: same-file constants resolve through the
        // context (Psalm's SimpleTypeInferer `$existing_constants`), runtime
        // constants through the shared table.
        Expression::ConstantAccess(constant_access) => {
            let raw = constant_access.name.value().trim_start_matches('\\');
            if let Some(resolver) = infer_context.global_constant_resolver
                && let Some(resolved) = resolver(raw)
            {
                return Some(resolved);
            }
            pzoom_code_info::runtime_constants::runtime_global_constant_type(
                &raw.to_ascii_lowercase(),
            )
        }
        _ => None,
    }
}

fn infer_simple_array_type<'a>(
    elements: impl Iterator<Item = &'a ArrayElement<'a>>,
    infer_context: &InferClassContext<'_>,
) -> Option<TUnion> {
    let mut properties = FxHashMap::default();
    let mut next_int_key = 0i64;
    let mut is_list = true;

    let mut key_overflowed = false;
    for element in elements {
        match element {
            ArrayElement::KeyValue(kv) => {
                // An explicit int key past PHP_INT_MAX is the same fatal as
                // overflowing the auto-increment (Psalm: InvalidArrayOffset).
                if let Expression::Literal(Literal::Integer(int_literal)) = kv.key.unparenthesized()
                    && int_literal
                        .value
                        .is_none_or(|value| i64::try_from(value).is_err())
                {
                    if let Some(sink) = infer_context.key_overflow_sink {
                        let span = mago_span::HasSpan::span(kv.key);
                        sink.borrow_mut().push((span.start.offset, span.end.offset));
                    }
                    return None;
                }
                let key_type = infer_with_context(kv.key, infer_context)?;
                let value_type = infer_with_context(kv.value, infer_context)?;
                let key = simple_union_to_array_key(&key_type)?;

                if !matches!(key, ArrayKey::Int(value) if value == next_int_key) {
                    is_list = false;
                }

                if let ArrayKey::Int(value) = key {
                    match value.checked_add(1) {
                        Some(next) => next_int_key = next,
                        None => key_overflowed = true,
                    }
                    properties.insert(ArrayKey::Int(value), value_type);
                } else {
                    properties.insert(key, value_type);
                }
            }
            ArrayElement::Value(value) => {
                if key_overflowed {
                    // Auto-incrementing past PHP_INT_MAX is a fatal error.
                    if let Some(sink) = infer_context.key_overflow_sink {
                        let span = mago_span::HasSpan::span(value);
                        sink.borrow_mut().push((span.start.offset, span.end.offset));
                    }
                    return None;
                }
                let value_type = infer_with_context(value.value, infer_context)?;
                properties.insert(ArrayKey::Int(next_int_key), value_type);
                next_int_key += 1;
            }
            ArrayElement::Missing(_) => {}
            ArrayElement::Variadic(variadic) => {
                // An unpacked array re-indexes its int keys: past
                // PHP_INT_MAX that's the same fatal as a plain value.
                if key_overflowed && let Some(sink) = infer_context.key_overflow_sink {
                    let span = mago_span::HasSpan::span(variadic);
                    sink.borrow_mut().push((span.start.offset, span.end.offset));
                }
                return None;
            }
        }
    }

    if properties.is_empty() {
        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::nothing()),
            value_type: Box::new(TUnion::nothing()),
        }));
    }

    Some(TUnion::new(TAtomic::TKeyedArray {
        properties: std::sync::Arc::new(properties),
        is_list,
        sealed: true,
        fallback_key_type: None,
        fallback_value_type: None,
    }))
}

/// Build Psalm's `UnresolvedConstantComponent` analog for a constant
/// initializer the simple inferer couldn't evaluate: cross-class constant
/// references (resolved to FQCNs here, while the file's aliases are at
/// hand) inside literals/arrays/concats. The populator resolves the IR
/// once every class is known (Psalm's ConstantTypeResolver).
pub(crate) fn build_unresolved_const_expr(
    expr: &Expression<'_>,
    infer_context: &InferClassContext<'_>,
    interner: &dyn Fn(&str) -> StrId,
) -> Option<UnresolvedConstExpr> {
    if let Some(resolved) = infer_with_context(expr, infer_context) {
        return Some(UnresolvedConstExpr::Resolved(resolved));
    }

    match expr.unparenthesized() {
        Expression::Access(Access::ClassConstant(class_constant_access)) => {
            let ClassLikeConstantSelector::Identifier(constant_name) =
                &class_constant_access.constant
            else {
                return None;
            };

            let class_name = match class_constant_access.class.unparenthesized() {
                Expression::Identifier(class_identifier) => {
                    let raw = class_identifier.value();
                    if let Some(stripped) = raw.strip_prefix('\\') {
                        stripped.to_string()
                    } else if let Some(resolver) = infer_context.class_resolver {
                        resolver(raw)
                    } else {
                        raw.to_string()
                    }
                }
                Expression::Self_(_) | Expression::Static(_) => {
                    infer_context.self_class?.to_string()
                }
                Expression::Parent(_) => infer_context.parent_class?.to_string(),
                _ => return None,
            };

            Some(UnresolvedConstExpr::ClassConstant {
                class: interner(class_name.trim_start_matches('\\')),
                constant: interner(constant_name.value),
            })
        }
        // `self::KEYS['hi']` — Psalm's UnresolvedConstant ArrayOffsetFetch.
        Expression::ArrayAccess(array_access) => Some(UnresolvedConstExpr::ArrayAccess {
            array: Box::new(build_unresolved_const_expr(
                array_access.array,
                infer_context,
                interner,
            )?),
            key: Box::new(build_unresolved_const_expr(
                array_access.index,
                infer_context,
                interner,
            )?),
        }),
        Expression::Array(array) => {
            build_unresolved_array(array.elements.iter(), infer_context, interner)
        }
        Expression::LegacyArray(array) => {
            build_unresolved_array(array.elements.iter(), infer_context, interner)
        }
        Expression::Binary(binary)
            if matches!(binary.operator, BinaryOperator::StringConcat(_)) =>
        {
            Some(UnresolvedConstExpr::Concat(
                Box::new(build_unresolved_const_expr(
                    binary.lhs,
                    infer_context,
                    interner,
                )?),
                Box::new(build_unresolved_const_expr(
                    binary.rhs,
                    infer_context,
                    interner,
                )?),
            ))
        }
        // `parent::ARR + [...]` — array union with left precedence.
        Expression::Binary(binary) if matches!(binary.operator, BinaryOperator::Addition(_)) => {
            Some(UnresolvedConstExpr::Plus(
                Box::new(build_unresolved_const_expr(
                    binary.lhs,
                    infer_context,
                    interner,
                )?),
                Box::new(build_unresolved_const_expr(
                    binary.rhs,
                    infer_context,
                    interner,
                )?),
            ))
        }
        // Int arithmetic over late-resolved operands
        // (`JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES`).
        Expression::Binary(binary) => {
            use pzoom_code_info::class_constant_info::UnresolvedIntOp;
            let op = match binary.operator {
                BinaryOperator::Subtraction(_) => UnresolvedIntOp::Sub,
                BinaryOperator::Multiplication(_) => UnresolvedIntOp::Mul,
                BinaryOperator::Modulo(_) => UnresolvedIntOp::Mod,
                BinaryOperator::BitwiseAnd(_) => UnresolvedIntOp::BitAnd,
                BinaryOperator::BitwiseOr(_) => UnresolvedIntOp::BitOr,
                BinaryOperator::BitwiseXor(_) => UnresolvedIntOp::BitXor,
                BinaryOperator::LeftShift(_) => UnresolvedIntOp::Shl,
                BinaryOperator::RightShift(_) => UnresolvedIntOp::Shr,
                _ => return None,
            };
            Some(UnresolvedConstExpr::IntOp {
                op,
                lhs: Box::new(build_unresolved_const_expr(
                    binary.lhs,
                    infer_context,
                    interner,
                )?),
                rhs: Box::new(build_unresolved_const_expr(
                    binary.rhs,
                    infer_context,
                    interner,
                )?),
            })
        }
        // `COND ? IF : ELSE` — Psalm's ExpressionResolver builds an
        // UnresolvedTernary, evaluated once the condition's constants resolve.
        Expression::Conditional(conditional) => Some(UnresolvedConstExpr::Ternary {
            cond: Box::new(build_unresolved_const_expr(
                conditional.condition,
                infer_context,
                interner,
            )?),
            if_branch: match conditional.then {
                Some(then_expr) => Some(Box::new(build_unresolved_const_expr(
                    then_expr,
                    infer_context,
                    interner,
                )?)),
                None => None,
            },
            else_branch: Box::new(build_unresolved_const_expr(
                conditional.r#else,
                infer_context,
                interner,
            )?),
        }),
        // A bare global constant reference (`JSON_PRETTY_PRINT`).
        Expression::ConstantAccess(constant_access) => {
            let raw = constant_access.name.value();
            Some(UnresolvedConstExpr::GlobalConstant(interner(
                raw.trim_start_matches('\\'),
            )))
        }
        // `Other::CASE->value` / `->name` (Psalm's EnumValueFetch /
        // EnumNameFetch) — deferred when the enum isn't collected yet.
        Expression::Access(Access::Property(property_access)) => {
            let mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector::Identifier(
                property_name,
            ) = &property_access.property
            else {
                return None;
            };
            let fetch_name = match property_name.value {
                "name" => true,
                "value" => false,
                _ => return None,
            };
            let Expression::Access(Access::ClassConstant(case_access)) =
                property_access.object.unparenthesized()
            else {
                return None;
            };
            let ClassLikeConstantSelector::Identifier(case_name) = &case_access.constant else {
                return None;
            };
            let class_name = match case_access.class.unparenthesized() {
                Expression::Identifier(class_identifier) => {
                    let raw = class_identifier.value();
                    if let Some(stripped) = raw.strip_prefix('\\') {
                        stripped.to_string()
                    } else if let Some(class_resolver) = infer_context.class_resolver {
                        class_resolver(raw)
                    } else {
                        raw.to_string()
                    }
                }
                Expression::Self_(_) | Expression::Static(_) => {
                    infer_context.self_class?.to_string()
                }
                _ => return None,
            };
            Some(UnresolvedConstExpr::EnumCasePropertyFetch {
                class: interner(class_name.trim_start_matches('\\')),
                case: interner(case_name.value),
                fetch_name,
            })
        }
        _ => None,
    }
}

fn build_unresolved_array<'a>(
    elements: impl Iterator<Item = &'a ArrayElement<'a>>,
    infer_context: &InferClassContext<'_>,
    interner: &dyn Fn(&str) -> StrId,
) -> Option<UnresolvedConstExpr> {
    let mut entries = Vec::new();
    for element in elements {
        match element {
            ArrayElement::KeyValue(kv) => {
                entries.push(pzoom_code_info::class_constant_info::UnresolvedArrayEntry {
                    key: Some(build_unresolved_const_expr(
                        kv.key,
                        infer_context,
                        interner,
                    )?),
                    value: build_unresolved_const_expr(kv.value, infer_context, interner)?,
                    is_spread: false,
                });
            }
            ArrayElement::Value(value) => {
                entries.push(pzoom_code_info::class_constant_info::UnresolvedArrayEntry {
                    key: None,
                    value: build_unresolved_const_expr(value.value, infer_context, interner)?,
                    is_spread: false,
                });
            }
            ArrayElement::Missing(_) => {}
            ArrayElement::Variadic(variadic) => {
                entries.push(pzoom_code_info::class_constant_info::UnresolvedArrayEntry {
                    key: None,
                    value: build_unresolved_const_expr(variadic.value, infer_context, interner)?,
                    is_spread: true,
                });
            }
        }
    }
    Some(UnresolvedConstExpr::ArrayLiteral(entries))
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
        // `X::class` keeps its class-string identity in the key (Psalm's
        // TKeyedArray::$class_strings) so iterating the shape yields a
        // class-string rather than a plain literal string.
        TAtomic::TLiteralClassString { name } => Some(ArrayKey::ClassString(name.clone())),
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
