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
        Hint::Array(_) => TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed())),
        Hint::Callable(_) => TUnion::new(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        }),
        // A native `static` return type is the late-static-bound type: keep the
        // concrete declaring class in `name` for member resolution, and flag it as
        // static so it is re-resolved at each call site. Mirrors Hakana's is_this.
        Hint::Static(_) => TUnion::new(TAtomic::TNamedObject {
            name: self_class.unwrap_or(StrId::STATIC),
            type_params: None,
            is_static: self_class.is_some(),
            remapped_params: false,
        }),
        Hint::Self_(_) => TUnion::new(TAtomic::TNamedObject {
            name: self_class.unwrap_or(StrId::SELF),
            type_params: None,
            is_static: false,
            remapped_params: false,
        }),
        Hint::Parent(_) => TUnion::new(TAtomic::TNamedObject {
            name: parent_class.unwrap_or(StrId::PARENT),
            type_params: None,
            is_static: false,
            remapped_params: false,
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
        // Psalm's Type::getIterable(): iterable<mixed, mixed>.
        Hint::Iterable(_) => TUnion::new(TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        }),
        Hint::Nullable(nullable) => {
            let mut inner = resolve_hint(
                nullable.hint,
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
                union.left,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            let right = resolve_hint(
                union.right,
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
                intersection.left,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );
            let right = resolve_hint(
                intersection.right,
                interner,
                current_namespace,
                self_class,
                parent_class,
                use_aliases,
                resolved_names,
            );

            // An intersection over union sides distributes into disjunctive
            // normal form: `A & (B|C)` = `(A&B) | (A&C)`. This both resolves
            // the type correctly and preserves the intersection structure so
            // version gating (DNF requires PHP 8.2) can see it.
            let mut result_atomics = Vec::new();
            for left_atomic in &left.types {
                for right_atomic in &right.types {
                    let mut parts = Vec::new();
                    push_intersection_part(left_atomic.clone(), &mut parts);
                    push_intersection_part(right_atomic.clone(), &mut parts);
                    if parts.len() == 1 {
                        result_atomics.push(parts.pop().unwrap());
                    } else {
                        result_atomics.push(TAtomic::TObjectIntersection { types: parts });
                    }
                }
            }

            TUnion::from_types(result_atomics)
        }
        Hint::Parenthesized(paren) => resolve_hint(
            paren.hint,
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
            is_static: false,
            remapped_params: false,
        }),
        "parent" if parent_class.is_some() => TUnion::new(TAtomic::TNamedObject {
            name: parent_class.unwrap(),
            type_params: None,
            is_static: false,
            remapped_params: false,
        }),
        // PHP signatures only recognize the canonical scalar keywords;
        // `boolean`/`integer`/`double`/`real` are CLASS references there
        // (Psalm reports UndefinedClass). Docblocks keep the loose aliases.
        "int" => TUnion::int(),
        "float" => TUnion::float(),
        "string" => TUnion::string(),
        "bool" => TUnion::bool(),
        "array" => TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed())),
        "object" => TUnion::new(TAtomic::TObject),
        "mixed" => TUnion::mixed(),
        "void" => TUnion::void(),
        "null" => TUnion::null(),
        "true" => TUnion::new(TAtomic::TTrue),
        "false" => TUnion::new(TAtomic::TFalse),
        "never" | "no-return" | "never-return" => TUnion::nothing(),
        // Psalm's Type::getIterable(): iterable<mixed, mixed> — the key is
        // mixed, not array-key (objects can yield arbitrary keys).
        "iterable" => TUnion::new(TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        }),
        "callable" => TUnion::new(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        }),
        // `resource`/`scalar`/`numeric` are docblock pseudo-types only: in a
        // native signature they are class references (Psalm resolves them as
        // classes — `Scalar $x` hitting Psalm\Type\Atomic\Scalar and
        // `Resource $r` hitting a Foo\Resource class are real code;
        // lowercase spellings report InvalidClass/UndefinedClass).
        "array-key" => TUnion::array_key(),
        _ => {
            // It's a class name - resolve it
            let resolved_name = resolved_names
                .and_then(|names| names.get(&{ ident.span().start.offset }).copied())
                .unwrap_or_else(|| {
                    resolve_class_name(ident, interner, current_namespace, use_aliases)
                });
            // A hint resolving to the bare global name `resource` parses as
            // the reserved pseudo-type (Psalm's TypeHintResolver goes through
            // Type::parseString, where `resource` is a reserved word); the
            // analyzer reports ReservedWord for it. Namespaced/aliased
            // Resource names stay class references.
            if interner
                .lookup(resolved_name)
                .eq_ignore_ascii_case("resource")
            {
                return TUnion::new(TAtomic::TResource);
            }
            TUnion::new(TAtomic::TNamedObject {
                name: resolved_name,
                type_params: None,
                is_static: false,
                remapped_params: false,
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

    // Read-only resolution: every fully-qualified name reachable here was
    // already interned by the scan-time `resolve_names` pass, so `find`
    // succeeds. `StrId::EMPTY` is only returned for a name that was never
    // interned (a non-existent class), which the analyzer treats as undefined.
    if ident.is_fully_qualified() {
        // Fully qualified - strip the leading backslash
        let stripped = name.strip_prefix('\\').unwrap_or(name);
        return interner.find(stripped).unwrap_or(StrId::EMPTY);
    }

    let (first_segment, remainder) = match name.split_once('\\') {
        Some((first, rest)) => (first, Some(rest)),
        None => (name, None),
    };

    if let Some(use_aliases) = use_aliases
        && let Some(alias_target) = use_aliases.get(&first_segment.to_ascii_lowercase())
    {
        if let Some(remainder) = remainder {
            let target = interner.lookup(*alias_target);
            return interner
                .find(&format!("{}\\{}", target, remainder))
                .unwrap_or(StrId::EMPTY);
        }

        return *alias_target;
    }

    if let Some(ns) = current_namespace {
        // Unqualified/qualified in a namespace - prepend namespace
        let ns_str = interner.lookup(ns);
        let full_name = format!("{}\\{}", ns_str, name);
        return interner.find(&full_name).unwrap_or(StrId::EMPTY);
    }

    // Global namespace
    interner.find(name).unwrap_or(StrId::EMPTY)
}
