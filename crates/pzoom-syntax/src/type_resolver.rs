//! Type hint resolution - converts mago AST type hints to pzoom types.

use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::type_hint::Hint;
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::{Interner, StrId};

/// Resolve a mago Hint to a pzoom TUnion.
pub fn resolve_hint(
    hint: &Hint<'_>,
    interner: &mut Interner,
    current_namespace: Option<StrId>,
) -> TUnion {
    match hint {
        Hint::Null(_) => TUnion::null(),
        Hint::True(_) => TUnion::new(TAtomic::TTrue),
        Hint::False(_) => TUnion::new(TAtomic::TFalse),
        Hint::Array(_) => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        Hint::Callable(_) => TUnion::new(TAtomic::TCallable {
            params: None,
            return_type: None,
        }),
        Hint::Static(_) => TUnion::new(TAtomic::TNamedObject {
            name: interner.intern("static"),
            type_params: None,
        }),
        Hint::Self_(_) => TUnion::new(TAtomic::TNamedObject {
            name: interner.intern("self"),
            type_params: None,
        }),
        Hint::Parent(_) => TUnion::new(TAtomic::TNamedObject {
            name: interner.intern("parent"),
            type_params: None,
        }),
        Hint::Identifier(ident) => resolve_identifier_hint(ident, interner, current_namespace),
        Hint::Void(_) => TUnion::void(),
        Hint::Never(_) => TUnion::nothing(),
        Hint::Float(_) => TUnion::float(),
        Hint::Bool(_) => TUnion::bool(),
        Hint::Integer(_) => TUnion::int(),
        Hint::String(_) => TUnion::string(),
        Hint::Object(_) => TUnion::new(TAtomic::TObject),
        Hint::Mixed(_) => TUnion::mixed(),
        Hint::Iterable(_) => TUnion::new(TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        }),
        Hint::Nullable(nullable) => {
            let mut inner = resolve_hint(&nullable.hint, interner, current_namespace);
            inner.add_type(TAtomic::TNull);
            inner
        }
        Hint::Union(union) => {
            let left = resolve_hint(&union.left, interner, current_namespace);
            let right = resolve_hint(&union.right, interner, current_namespace);
            let mut types = left.types;
            types.extend(right.types);
            TUnion::from_types(types)
        }
        Hint::Intersection(intersection) => {
            // For now, just return the first type
            // TODO: Proper intersection type support
            resolve_hint(&intersection.left, interner, current_namespace)
        }
        Hint::Parenthesized(paren) => resolve_hint(&paren.hint, interner, current_namespace),
    }
}

/// Resolve an identifier to a type (handles built-in types and class names).
fn resolve_identifier_hint(
    ident: &Identifier<'_>,
    interner: &mut Interner,
    current_namespace: Option<StrId>,
) -> TUnion {
    let name = ident.value();

    // Check for built-in type names (case-insensitive)
    match name.to_lowercase().as_str() {
        "int" | "integer" => TUnion::int(),
        "float" | "double" | "real" => TUnion::float(),
        "string" => TUnion::string(),
        "bool" | "boolean" => TUnion::bool(),
        "array" => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        "object" => TUnion::new(TAtomic::TObject),
        "mixed" => TUnion::mixed(),
        "void" => TUnion::void(),
        "null" => TUnion::null(),
        "true" => TUnion::new(TAtomic::TTrue),
        "false" => TUnion::new(TAtomic::TFalse),
        "never" | "no-return" | "never-return" => TUnion::nothing(),
        "iterable" => TUnion::new(TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        }),
        "callable" => TUnion::new(TAtomic::TCallable {
            params: None,
            return_type: None,
        }),
        "resource" => TUnion::new(TAtomic::TResource),
        "scalar" => TUnion::new(TAtomic::TScalar),
        "numeric" => TUnion::new(TAtomic::TNumeric),
        "array-key" => TUnion::array_key(),
        _ => {
            // It's a class name - resolve it
            let resolved_name = resolve_class_name(ident, interner, current_namespace);
            TUnion::new(TAtomic::TNamedObject {
                name: resolved_name,
                type_params: None,
            })
        }
    }
}

/// Resolve a class name, prepending the current namespace if needed.
fn resolve_class_name(
    ident: &Identifier<'_>,
    interner: &mut Interner,
    current_namespace: Option<StrId>,
) -> StrId {
    let name = ident.value();

    if ident.is_fully_qualified() {
        // Fully qualified - strip the leading backslash
        let stripped = name.strip_prefix('\\').unwrap_or(name);
        interner.intern(stripped)
    } else if let Some(ns) = current_namespace {
        // Unqualified in a namespace - prepend namespace
        let ns_str = interner.lookup(ns);
        let full_name = format!("{}\\{}", ns_str, name);
        interner.intern(&full_name)
    } else {
        // Global namespace
        interner.intern(name)
    }
}
