//! Type hint resolution - converts mago AST type hints to pzoom types.

use mago_span::HasSpan;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::type_hint::Hint;
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

use crate::ResolvedNames;

/// Resolve a mago Hint to a pzoom TUnion.
pub fn resolve_hint(
    hint: &Hint<'_>,
    interner: &Interner,
    current_namespace: Option<StrId>,
    self_class: Option<StrId>,
    parent_class: Option<StrId>,
    use_aliases: Option<&FxHashMap<String, StrId>>,
    resolved_names: Option<&ResolvedNames>,
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
            is_pure: None,
        }),
        Hint::Static(_) => TUnion::new(TAtomic::TNamedObject {
            name: self_class.unwrap_or(StrId::STATIC),
            type_params: None,
        }),
        Hint::Self_(_) => TUnion::new(TAtomic::TNamedObject {
            name: self_class.unwrap_or(StrId::SELF),
            type_params: None,
        }),
        Hint::Parent(_) => TUnion::new(TAtomic::TNamedObject {
            name: parent_class.unwrap_or(StrId::PARENT),
            type_params: None,
        }),
        Hint::Identifier(ident) => resolve_identifier_hint(
            ident,
            interner,
            current_namespace,
            self_class,
            parent_class,
            use_aliases,
            resolved_names,
        ),
        Hint::Void(_) => TUnion::void(),
        Hint::Never(_) => TUnion::nothing(),
        Hint::Float(_) => TUnion::float(),
        Hint::Bool(_) => TUnion::bool(),
        Hint::Integer(_) => TUnion::int(),
        Hint::String(_) => TUnion::string(),
        Hint::Object(_) => TUnion::new(TAtomic::TObject),
        Hint::Mixed(_) => TUnion::mixed(),
        Hint::Iterable(_) => TUnion::new(TAtomic::TIterable {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        Hint::Nullable(nullable) => {
            let mut inner = resolve_hint(
                &nullable.hint,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            inner.add_type(TAtomic::TNull);
            inner
        }
        Hint::Union(union) => {
            let left = resolve_hint(
                &union.left,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            let right = resolve_hint(
                &union.right,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            let mut types = left.types;
            types.extend(right.types);
            TUnion::from_types(types)
        }
        Hint::Intersection(intersection) => {
            let left = resolve_hint(
                &intersection.left,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            let right = resolve_hint(
                &intersection.right,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );

            let (Some(left_atomic), Some(right_atomic)) =
                (left.get_single().cloned(), right.get_single().cloned())
            else {
                return left;
            };

            let mut parts = Vec::new();
            push_intersection_part(left_atomic, &mut parts);
            push_intersection_part(right_atomic, &mut parts);

            if parts.len() == 1 {
                TUnion::new(parts.pop().unwrap())
            } else {
                TUnion::new(TAtomic::TObjectIntersection { types: parts })
            }
        }
        Hint::Parenthesized(paren) => resolve_hint(
            &paren.hint,
            interner,
            current_namespace,
            self_class,
            parent_class,
            use_aliases,
            resolved_names,
        ),
    }
}

fn push_intersection_part(atomic: TAtomic, parts: &mut Vec<TAtomic>) {
    match atomic {
        TAtomic::TObjectIntersection { types } => {
            for part in types {
                if !parts.contains(&part) {
                    parts.push(part);
                }
            }
        }
        _ => {
            if !parts.contains(&atomic) {
                parts.push(atomic);
            }
        }
    }
}

/// Resolve an identifier to a type (handles built-in types and class names).
fn resolve_identifier_hint(
    ident: &Identifier<'_>,
    interner: &Interner,
    current_namespace: Option<StrId>,
    self_class: Option<StrId>,
    parent_class: Option<StrId>,
    use_aliases: Option<&FxHashMap<String, StrId>>,
    resolved_names: Option<&ResolvedNames>,
) -> TUnion {
    let name = ident.value();

    // Check for built-in type names (case-insensitive)
    match name.to_lowercase().as_str() {
        "self" | "static" if self_class.is_some() => TUnion::new(TAtomic::TNamedObject {
            name: self_class.unwrap(),
            type_params: None,
        }),
        "parent" if parent_class.is_some() => TUnion::new(TAtomic::TNamedObject {
            name: parent_class.unwrap(),
            type_params: None,
        }),
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
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        "callable" => TUnion::new(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        }),
        "resource" => TUnion::new(TAtomic::TResource),
        "scalar" => TUnion::new(TAtomic::TScalar),
        "numeric" => TUnion::new(TAtomic::TNumeric),
        "array-key" => TUnion::array_key(),
        _ => {
            // It's a class name - resolve it
            let resolved_name = resolved_names
                .and_then(|names| names.get(&(ident.span().start.offset as u32)).copied())
                .unwrap_or_else(|| {
                    resolve_class_name(ident, interner, current_namespace, use_aliases)
                });
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
    interner: &Interner,
    current_namespace: Option<StrId>,
    use_aliases: Option<&FxHashMap<String, StrId>>,
) -> StrId {
    let name = ident.value();

    if ident.is_fully_qualified() {
        // Fully qualified - strip the leading backslash
        let stripped = name.strip_prefix('\\').unwrap_or(name);
        return interner.intern(stripped);
    }

    let (first_segment, remainder) = match name.split_once('\\') {
        Some((first, rest)) => (first, Some(rest)),
        None => (name, None),
    };

    if let Some(use_aliases) = use_aliases {
        if let Some(alias_target) = use_aliases.get(&first_segment.to_ascii_lowercase()) {
            if let Some(remainder) = remainder {
                let target = interner.lookup(*alias_target);
                return interner.intern(&format!("{}\\{}", target, remainder));
            }

            return *alias_target;
        }
    }

    if let Some(ns) = current_namespace {
        // Unqualified/qualified in a namespace - prepend namespace
        let ns_str = interner.lookup(ns);
        let full_name = format!("{}\\{}", ns_str, name);
        return interner.intern(&full_name);
    }

    // Global namespace
    interner.intern(name)
}
