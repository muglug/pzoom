//! Type string parser - converts Psalm/PHPDoc type syntax to TUnion.
//!
//! Based on Psalm's TypeParser.php. Uses a two-phase approach:
//! 1. Tokenize the type string
//! 2. Build a parse tree from tokens
//! 3. Convert parse tree to TUnion
//!
//! Supports:
//! - Scalar types: int, string, bool, float, null, void, mixed, never
//! - Union types: int|string
//! - Intersection types: A&B
//! - Nullable: ?string
//! - Arrays: array, int[], array<int, string>, list<T>
//! - Object types: ClassName, ClassName<T>
//! - Callable: callable(int): string, Closure
//! - Array shapes: array{foo: string, bar?: int}
//! - Literal types: 'literal', 123, true, false
//! - Special types: class-string<T>, key-of<T>, value-of<T>
//! - Class constants: MyClass::CONSTANT
//! - Int ranges: int<0, max>, positive-int, negative-int

use super::parse_tree::*;
use super::parse_tree_creator::ParseTreeCreator;
use super::type_tokenizer;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::Interner;
use rustc_hash::FxHashMap;

/// Parse a type string into a TUnion.
pub fn parse_type_string(type_str: &str, interner: &Interner) -> TUnion {
    let trimmed = type_str.trim();
    if trimmed.is_empty() {
        return TUnion::mixed();
    }

    // Tokenize
    let tokens = match type_tokenizer::tokenize(trimmed) {
        Ok(t) => t,
        Err(_) => return TUnion::mixed(),
    };

    // Handle single-token case
    if tokens.len() == 1 && !tokens[0].value.is_empty() {
        let token = &tokens[0];
        let fixed = type_tokenizer::fix_scalar_terms(&token.value);
        let mut union = match atomic_from_string(&fixed, None, interner) {
            Some(atomic) => TUnion::new(atomic),
            None => TUnion::mixed(),
        };
        union.from_docblock = !union.is_mixed();
        return union;
    }

    // Build parse tree
    let creator = ParseTreeCreator::new(tokens);
    let parse_tree = match creator.create() {
        Ok(tree) => tree,
        Err(_) => return TUnion::mixed(),
    };

    // Convert parse tree to TUnion
    let mut union = match get_type_from_tree(&parse_tree, interner) {
        TypeOrUnion::Union(u) => u,
        TypeOrUnion::Atomic(a) => TUnion::new(a),
    };
    union.from_docblock = !union.is_mixed();
    union
}

/// Result of parsing - either an atomic or union type.
enum TypeOrUnion {
    Atomic(TAtomic),
    Union(TUnion),
}

/// Convert a parse tree to a type.
fn get_type_from_tree(tree: &ParseTree, interner: &Interner) -> TypeOrUnion {
    match tree {
        ParseTree::Value(v) => {
            let fixed = type_tokenizer::fix_scalar_terms(&v.value);
            match atomic_from_string(&fixed, None, interner) {
                Some(a) => TypeOrUnion::Atomic(a),
                None => TypeOrUnion::Union(TUnion::mixed()),
            }
        }

        ParseTree::Generic(g) => {
            let generic_params: Vec<TUnion> = g
                .children
                .iter()
                .map(|c| match get_type_from_tree(c, interner) {
                    TypeOrUnion::Union(u) => u,
                    TypeOrUnion::Atomic(a) => TUnion::new(a),
                })
                .collect();

            match atomic_from_string(&g.value, Some(generic_params), interner) {
                Some(a) => TypeOrUnion::Atomic(a),
                None => TypeOrUnion::Union(TUnion::mixed()),
            }
        }

        ParseTree::Union(u) => {
            let mut types = Vec::new();
            for child in &u.children {
                match get_type_from_tree(child, interner) {
                    TypeOrUnion::Atomic(a) => types.push(a),
                    TypeOrUnion::Union(u) => types.extend(u.types),
                }
            }
            if types.is_empty() {
                TypeOrUnion::Union(TUnion::mixed())
            } else if types.len() == 1 {
                TypeOrUnion::Atomic(types.remove(0))
            } else {
                TypeOrUnion::Union(TUnion::from_types(types))
            }
        }

        ParseTree::Intersection(i) => {
            // For intersection types, we currently just take the first type
            // Full intersection support would need a TIntersection type
            if let Some(first) = i.children.first() {
                get_type_from_tree(first, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::Nullable(n) => {
            if let Some(child) = n.children.first() {
                let inner = match get_type_from_tree(child, interner) {
                    TypeOrUnion::Union(u) => u,
                    TypeOrUnion::Atomic(a) => TUnion::new(a),
                };
                let mut types = inner.types;
                types.push(TAtomic::TNull);
                TypeOrUnion::Union(TUnion::from_types(types))
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::KeyedArray(k) => {
            let (properties, is_list, sealed, fallback_key, fallback_value) =
                parse_keyed_array_children(&k.children, interner);

            TypeOrUnion::Atomic(TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type: fallback_key,
                fallback_value_type: fallback_value,
            })
        }

        ParseTree::KeyedArrayProperty(p) => {
            // This should be handled in keyed array context
            if let Some(child) = p.children.first() {
                get_type_from_tree(child, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::Callable(c) => {
            let (params, return_type) = parse_callable_children(&c.children, interner);
            let lowered = c.value.to_lowercase();
            let is_closure = lowered.contains("closure");
            let is_pure = lowered.contains("pure-");

            if is_closure {
                TypeOrUnion::Atomic(TAtomic::TClosure {
                    params: Some(params),
                    return_type,
                    is_pure: Some(is_pure),
                })
            } else {
                TypeOrUnion::Atomic(TAtomic::TCallable {
                    params: Some(params),
                    return_type,
                    is_pure: Some(is_pure),
                })
            }
        }

        ParseTree::CallableWithReturnType(crt) => {
            // First child is callable, second is return type
            if crt.children.len() >= 2 {
                let callable_result = get_type_from_tree(&crt.children[0], interner);
                let return_type = match get_type_from_tree(&crt.children[1], interner) {
                    TypeOrUnion::Union(u) => u,
                    TypeOrUnion::Atomic(a) => TUnion::new(a),
                };

                match callable_result {
                    TypeOrUnion::Atomic(TAtomic::TCallable {
                        params, is_pure, ..
                    }) => {
                        TypeOrUnion::Atomic(TAtomic::TCallable {
                            params,
                            return_type: Some(Box::new(return_type)),
                            is_pure,
                        })
                    }
                    TypeOrUnion::Atomic(TAtomic::TClosure {
                        params, is_pure, ..
                    }) => {
                        TypeOrUnion::Atomic(TAtomic::TClosure {
                            params,
                            return_type: Some(Box::new(return_type)),
                            is_pure,
                        })
                    }
                    other => other,
                }
            } else if let Some(first) = crt.children.first() {
                get_type_from_tree(first, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::CallableParam(cp) => {
            if let Some(child) = cp.children.first() {
                get_type_from_tree(child, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::Encapsulation(e) => {
            if let Some(child) = e.children.first() {
                get_type_from_tree(child, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::Root(r) => {
            if let Some(child) = r.children.first() {
                get_type_from_tree(child, interner)
            } else {
                TypeOrUnion::Union(TUnion::mixed())
            }
        }

        ParseTree::IndexedAccess(_) => {
            // T[K] - would need template resolution
            TypeOrUnion::Union(TUnion::mixed())
        }

        ParseTree::TemplateAs(_) | ParseTree::TemplateIs(_) | ParseTree::Conditional(_) => {
            // Template types need context resolution
            TypeOrUnion::Union(TUnion::mixed())
        }

        ParseTree::Method(_) | ParseTree::MethodWithReturnType(_) | ParseTree::MethodParam(_) => {
            // Method types are for @method annotations
            TypeOrUnion::Union(TUnion::mixed())
        }

        ParseTree::FieldEllipsis(_) => {
            // Should be handled in keyed array context
            TypeOrUnion::Union(TUnion::mixed())
        }
    }
}

/// Parse keyed array children into properties.
fn parse_keyed_array_children(
    children: &[ParseTree],
    interner: &Interner,
) -> (
    FxHashMap<ArrayKey, TUnion>,
    bool,
    bool,
    Option<Box<TUnion>>,
    Option<Box<TUnion>>,
) {
    let mut properties: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
    let mut is_list = true;
    let mut sealed = true;
    let mut fallback_key: Option<Box<TUnion>> = None;
    let mut fallback_value: Option<Box<TUnion>> = None;
    let mut index = 0i64;

    for child in children {
        match child {
            ParseTree::KeyedArrayProperty(p) => {
                let mut value_type = if let Some(c) = p.children.first() {
                    match get_type_from_tree(c, interner) {
                        TypeOrUnion::Union(u) => u,
                        TypeOrUnion::Atomic(a) => TUnion::new(a),
                    }
                } else {
                    TUnion::mixed()
                };

                // Determine key
                if let Ok(num) = p.value.parse::<i64>() {
                    if num != index {
                        is_list = false;
                    }
                    properties.insert(ArrayKey::Int(num), value_type);
                    index = num + 1;
                } else {
                    is_list = false;
                    // Handle quoted keys
                    let mut key = if (p.value.starts_with('\'') && p.value.ends_with('\''))
                        || (p.value.starts_with('"') && p.value.ends_with('"'))
                    {
                        p.value[1..p.value.len() - 1].to_string()
                    } else {
                        p.value.clone()
                    };

                    if key.ends_with('?') {
                        key.pop();
                        value_type.possibly_undefined = true;
                    }

                    properties.insert(ArrayKey::String(key), value_type);
                }
            }
            ParseTree::FieldEllipsis(_) => {
                sealed = false;
            }
            ParseTree::Generic(g) if g.value.is_empty() => {
                // Fallback types from ...
                if g.children.len() >= 2 {
                    fallback_key = Some(Box::new(match get_type_from_tree(&g.children[0], interner) {
                        TypeOrUnion::Union(u) => u,
                        TypeOrUnion::Atomic(a) => TUnion::new(a),
                    }));
                    fallback_value = Some(Box::new(match get_type_from_tree(&g.children[1], interner) {
                        TypeOrUnion::Union(u) => u,
                        TypeOrUnion::Atomic(a) => TUnion::new(a),
                    }));
                } else if g.children.len() == 1 {
                    fallback_value = Some(Box::new(match get_type_from_tree(&g.children[0], interner) {
                        TypeOrUnion::Union(u) => u,
                        TypeOrUnion::Atomic(a) => TUnion::new(a),
                    }));
                }
            }
            _ => {
                // Implicit index
                let value_type = match get_type_from_tree(child, interner) {
                    TypeOrUnion::Union(u) => u,
                    TypeOrUnion::Atomic(a) => TUnion::new(a),
                };
                properties.insert(ArrayKey::Int(index), value_type);
                index += 1;
            }
        }
    }

    (properties, is_list, sealed, fallback_key, fallback_value)
}

/// Parse callable children into parameters and return type.
fn parse_callable_children(
    children: &[ParseTree],
    interner: &Interner,
) -> (
    Vec<pzoom_code_info::t_atomic::FunctionLikeParameter>,
    Option<Box<TUnion>>,
) {
    let mut params = Vec::new();

    for child in children {
        match child {
            ParseTree::CallableParam(cp) => {
                let param_type = if let Some(c) = cp.children.first() {
                    match get_type_from_tree(c, interner) {
                        TypeOrUnion::Union(u) => u,
                        TypeOrUnion::Atomic(a) => TUnion::new(a),
                    }
                } else {
                    TUnion::mixed()
                };

                params.push(pzoom_code_info::t_atomic::FunctionLikeParameter {
                    name: cp.name.as_ref().map(|n| interner.intern(n)),
                    param_type,
                    is_optional: cp.has_default,
                    is_variadic: cp.variadic,
                    by_ref: false,
                });
            }
            _ => {
                // Direct type without CallableParam wrapper
                let param_type = match get_type_from_tree(child, interner) {
                    TypeOrUnion::Union(u) => u,
                    TypeOrUnion::Atomic(a) => TUnion::new(a),
                };

                params.push(pzoom_code_info::t_atomic::FunctionLikeParameter {
                    name: None,
                    param_type,
                    is_optional: false,
                    is_variadic: false,
                    by_ref: false,
                });
            }
        }
    }

    (params, None)
}

/// Convert a type name string to an atomic type.
fn atomic_from_string(
    name: &str,
    type_params: Option<Vec<TUnion>>,
    interner: &Interner,
) -> Option<TAtomic> {
    let lower_name = name.to_lowercase();

    // Check for string literal
    if (name.starts_with('\'') && name.ends_with('\''))
        || (name.starts_with('"') && name.ends_with('"'))
    {
        let value = &name[1..name.len() - 1];
        return Some(TAtomic::TLiteralString {
            value: value.to_string(),
        });
    }

    // Check for numeric literal
    if let Ok(value) = name.parse::<i64>() {
        return Some(TAtomic::TLiteralInt { value });
    }
    if let Ok(value) = name.parse::<f64>() {
        if name.contains('.') {
            return Some(TAtomic::TLiteralFloat { value });
        }
    }

    // Check for class constant
    if name.contains("::") {
        let parts: Vec<&str> = name.splitn(2, "::").collect();
        if parts.len() == 2 {
            if parts[1].to_lowercase() == "class" {
                // Foo::class is a literal class string
                return Some(TAtomic::TLiteralClassString {
                    name: parts[0].to_string(),
                });
            }
            // Other class constants - would need resolution
            // Keep the full `Class::CONST` token so wildcard expansion can resolve it later.
            return Some(TAtomic::TNamedObject {
                name: interner.intern(name),
                type_params: None,
            });
        }
    }

    Some(match lower_name.as_str() {
        // Scalar types
        "int" | "integer" => {
            // Handle int ranges like int<0, max>
            if let Some(params) = type_params {
                if params.len() == 2 {
                    let min = params[0].get_single().and_then(|a| {
                        if let TAtomic::TLiteralInt { value } = a {
                            Some(*value)
                        } else {
                            None
                        }
                    });
                    let max = params[1].get_single().and_then(|a| {
                        if let TAtomic::TLiteralInt { value } = a {
                            Some(*value)
                        } else {
                            None
                        }
                    });
                    return Some(TAtomic::TIntRange { min, max });
                }
            }
            TAtomic::TInt
        }
        "float" | "double" => TAtomic::TFloat,
        "string" => TAtomic::TString,
        "bool" | "boolean" => TAtomic::TBool,
        "true" => TAtomic::TTrue,
        "false" => TAtomic::TFalse,
        "null" => TAtomic::TNull,
        "void" => TAtomic::TVoid,
        "never" | "no-return" | "never-return" | "never-returns" => TAtomic::TNothing,
        "mixed" => TAtomic::TMixed,
        "object" => TAtomic::TObject,
        "resource" | "open-resource" => TAtomic::TResource,
        "closed-resource" => TAtomic::TClosedResource,

        // Special int types
        "positive-int" => TAtomic::TPositiveInt,
        "negative-int" => TAtomic::TNegativeInt,
        "non-negative-int" => TAtomic::TIntRange {
            min: Some(0),
            max: None,
        },
        "non-positive-int" => TAtomic::TIntRange {
            min: None,
            max: Some(0),
        },
        "literal-int" => TAtomic::TInt,

        // Special string types
        "non-empty-string" => TAtomic::TNonEmptyString,
        "numeric-string" => TAtomic::TNumericString,
        "lowercase-string" => TAtomic::TLowercaseString,
        "non-empty-lowercase-string" => TAtomic::TNonEmptyLowercaseString,
        "truthy-string" | "non-falsy-string" => TAtomic::TTruthyString,
        "literal-string" | "non-empty-literal-string" => TAtomic::TLiteralString {
            value: pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
        },
        "callable-string" => TAtomic::TString, // Simplified

        "class-string" => {
            if let Some(params) = type_params {
                if let Some(first) = params.into_iter().next() {
                    if let Some(atomic) = first.get_single() {
                        return Some(TAtomic::TClassString {
                            as_type: Some(Box::new(atomic.clone())),
                        });
                    }
                }
            }
            TAtomic::TClassString { as_type: None }
        }
        "interface-string" | "enum-string" | "trait-string" => TAtomic::TClassString { as_type: None },

        // Numeric types
        "scalar" => TAtomic::TScalar,
        "numeric" => TAtomic::TNumeric,
        "array-key" => TAtomic::TArrayKey,

        // Array types
        "array" | "associative-array" => {
            if let Some(params) = type_params {
                match params.len() {
                    1 => TAtomic::TArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(params.into_iter().next().unwrap()),
                    },
                    2 => {
                        let mut iter = params.into_iter();
                        TAtomic::TArray {
                            key_type: Box::new(iter.next().unwrap()),
                            value_type: Box::new(iter.next().unwrap()),
                        }
                    }
                    _ => TAtomic::TArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(TUnion::mixed()),
                    },
                }
            } else {
                TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                }
            }
        }
        "non-empty-array" => {
            if let Some(params) = type_params {
                match params.len() {
                    1 => TAtomic::TNonEmptyArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(params.into_iter().next().unwrap()),
                    },
                    2 => {
                        let mut iter = params.into_iter();
                        TAtomic::TNonEmptyArray {
                            key_type: Box::new(iter.next().unwrap()),
                            value_type: Box::new(iter.next().unwrap()),
                        }
                    }
                    _ => TAtomic::TNonEmptyArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(TUnion::mixed()),
                    },
                }
            } else {
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                }
            }
        }
        "list" => {
            let value_type = type_params
                .and_then(|p| p.into_iter().next())
                .unwrap_or_else(TUnion::mixed);
            TAtomic::TList {
                value_type: Box::new(value_type),
            }
        }
        "non-empty-list" => {
            let value_type = type_params
                .and_then(|p| p.into_iter().next())
                .unwrap_or_else(TUnion::mixed);
            TAtomic::TNonEmptyList {
                value_type: Box::new(value_type),
            }
        }
        "callable-array" | "callable-list" => TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        },

        // Iterable
        "iterable" => {
            if let Some(params) = type_params {
                match params.len() {
                    1 => TAtomic::TIterable {
                        key_type: Box::new(TUnion::mixed()),
                        value_type: Box::new(params.into_iter().next().unwrap()),
                    },
                    2 => {
                        let mut iter = params.into_iter();
                        TAtomic::TIterable {
                            key_type: Box::new(iter.next().unwrap()),
                            value_type: Box::new(iter.next().unwrap()),
                        }
                    }
                    _ => TAtomic::TIterable {
                        key_type: Box::new(TUnion::mixed()),
                        value_type: Box::new(TUnion::mixed()),
                    },
                }
            } else {
                TAtomic::TIterable {
                    key_type: Box::new(TUnion::mixed()),
                    value_type: Box::new(TUnion::mixed()),
                }
            }
        }

        // Callable
        "callable" | "pure-callable" => TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: if lower == "pure-callable" {
                Some(true)
            } else {
                None
            },
        },
        "closure" | "\\closure" | "pure-closure" => TAtomic::TClosure {
            params: None,
            return_type: None,
            is_pure: if lower == "pure-closure" {
                Some(true)
            } else {
                None
            },
        },
        "callable-object" | "stringable-object" => TAtomic::TObject,

        // Special utility types
        "key-of" | "value-of" | "properties-of" | "public-properties-of"
        | "protected-properties-of" | "private-properties-of" | "class-string-map"
        | "int-mask" | "int-mask-of" | "arraylike-object" => {
            // These need special handling with context
            TAtomic::TMixed
        }

        // Self/static/parent - need context
        "self" | "static" | "parent" | "$this" => TAtomic::TObject,

        // Empty string - invalid
        "" => return None,

        // Named object (class/interface)
        _ => {
            let str_id = interner.intern(name);
            TAtomic::TNamedObject {
                name: str_id,
                type_params: type_params,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_interner() -> Interner {
        Interner::new()
    }

    #[test]
    fn test_parse_scalar_types() {
        let interner = make_interner();

        let t = parse_type_string("int", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TInt)));

        let t = parse_type_string("string", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TString)));

        let t = parse_type_string("bool", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TBool)));

        let t = parse_type_string("null", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TNull)));
    }

    #[test]
    fn test_parse_union_type() {
        let interner = make_interner();

        let t = parse_type_string("int|string", &interner);
        assert_eq!(t.types.len(), 2);
    }

    #[test]
    fn test_parse_generic_array() {
        let interner = make_interner();

        let t = parse_type_string("array<int, string>", &interner);
        if let Some(TAtomic::TArray { key_type, value_type }) = t.get_single() {
            assert!(matches!(key_type.get_single(), Some(TAtomic::TInt)));
            assert!(matches!(value_type.get_single(), Some(TAtomic::TString)));
        } else {
            panic!("Expected TArray");
        }
    }

    #[test]
    fn test_parse_list() {
        let interner = make_interner();

        let t = parse_type_string("list<string>", &interner);
        if let Some(TAtomic::TList { value_type }) = t.get_single() {
            assert!(matches!(value_type.get_single(), Some(TAtomic::TString)));
        } else {
            panic!("Expected TList");
        }
    }

    #[test]
    fn test_parse_class_name() {
        let interner = make_interner();

        let t = parse_type_string("DateTime", &interner);
        if let Some(TAtomic::TNamedObject { name, .. }) = t.get_single() {
            assert_eq!(&*interner.lookup(*name), "DateTime");
        } else {
            panic!("Expected TNamedObject");
        }
    }

    #[test]
    fn test_parse_array_suffix() {
        let interner = make_interner();

        let t = parse_type_string("string[]", &interner);
        if let Some(TAtomic::TArray { value_type, .. }) = t.get_single() {
            assert!(matches!(value_type.get_single(), Some(TAtomic::TString)));
        } else {
            panic!("Expected TArray, got {:?}", t.get_single());
        }
    }

    #[test]
    fn test_parse_intersection() {
        let interner = make_interner();

        let t = parse_type_string("A&B", &interner);
        // Currently returns first type
        assert!(t.get_single().is_some());
    }

    #[test]
    fn test_parse_class_constant() {
        let interner = make_interner();

        let t = parse_type_string("Foo::class", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TLiteralClassString { .. })));
    }

    #[test]
    fn test_parse_string_literal() {
        let interner = make_interner();

        let t = parse_type_string("'hello'", &interner);
        if let Some(TAtomic::TLiteralString { value }) = t.get_single() {
            assert_eq!(value, "hello");
        } else {
            panic!("Expected TLiteralString");
        }
    }

    #[test]
    fn test_parse_numeric_literal() {
        let interner = make_interner();

        let t = parse_type_string("123", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TLiteralInt { value: 123 })));

        let t = parse_type_string("12.5", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TLiteralFloat { value }) if (*value - 12.5).abs() < 0.001));
    }

    #[test]
    fn test_parse_special_string_types() {
        let interner = make_interner();

        let t = parse_type_string("non-empty-string", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TNonEmptyString)));

        let t = parse_type_string("numeric-string", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TNumericString)));

        let t = parse_type_string("lowercase-string", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TLowercaseString)));
    }

    #[test]
    fn test_parse_int_types() {
        let interner = make_interner();

        let t = parse_type_string("positive-int", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TPositiveInt)));

        let t = parse_type_string("negative-int", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TNegativeInt)));

        let t = parse_type_string("non-negative-int", &interner);
        assert!(matches!(t.get_single(), Some(TAtomic::TIntRange { min: Some(0), max: None })));
    }
}
