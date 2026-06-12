//! Type-string tokenizer.
//!
//! A faithful port of Psalm's `Internal/Type/TypeTokenizer.php`. Turns a PHPDoc
//! type string into a flat list of `(value, offset)` tokens that
//! [`super::parse_tree_creator::ParseTreeCreator`] consumes.
//!
//! The port follows Psalm block-by-block. The one intentional deviation is that
//! we iterate over Unicode scalar values (`char`) rather than PHP's per-byte
//! `str_split`, and offsets are char indices. Type strings are effectively
//! ASCII in practice, so this matches Psalm's behaviour while staying UTF-8
//! safe.

/// Reserved type words, mirroring `TypeTokenizer::PSALM_RESERVED_WORDS`.
pub const PSALM_RESERVED_WORDS: &[&str] = &[
    "int",
    "string",
    "float",
    "bool",
    "false",
    "true",
    "object",
    "empty",
    "callable",
    "array",
    "non-empty-array",
    "non-empty-string",
    "non-falsy-string",
    "truthy-string",
    "iterable",
    "null",
    "mixed",
    "numeric-string",
    "class-string",
    "interface-string",
    "enum-string",
    "trait-string",
    "callable-string",
    "callable-array",
    "callable-list",
    "callable-object",
    "stringable-object",
    "pure-callable",
    "pure-Closure",
    "literal-string",
    "non-empty-literal-string",
    "lowercase-string",
    "non-empty-lowercase-string",
    "positive-int",
    "non-negative-int",
    "negative-int",
    "non-positive-int",
    "literal-int",
    "boolean",
    "integer",
    "double",
    "real",
    "resource",
    "void",
    "self",
    "static",
    "scalar",
    "numeric",
    "no-return",
    "never-return",
    "never-returns",
    "never",
    "array-key",
    "key-of",
    "value-of",
    "properties-of",
    "public-properties-of",
    "protected-properties-of",
    "private-properties-of",
    "non-empty-countable",
    "list",
    "non-empty-list",
    "class-string-map",
    "open-resource",
    "closed-resource",
    "associative-array",
    "arraylike-object",
    "int-mask",
    "int-mask-of",
];

/// Whether `word` is one of Psalm's reserved type words.
pub fn is_reserved_word(word: &str) -> bool {
    PSALM_RESERVED_WORDS.contains(&word)
}

/// Like [`is_reserved_word`], but ASCII-case-insensitive (e.g. `pure-closure`
/// matches the canonical `pure-Closure`). Docblock validation lowercases the
/// base token before comparing, so it needs this variant.
pub fn is_reserved_word_ignore_ascii_case(word: &str) -> bool {
    PSALM_RESERVED_WORDS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(word))
}

/// A single token: a value string and the offset at which it starts.
///
/// Psalm models tokens as `array{0: string, 1: int, 2?: string}`, where the
/// optional third element is the "original" (pre-resolution) text added by
/// `getFullyQualifiedTokens`. We keep that as [`TypeToken::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeToken {
    pub value: String,
    pub offset: usize,
    pub text: Option<String>,
}

impl TypeToken {
    fn new(value: impl Into<String>, offset: usize) -> Self {
        Self {
            value: value.into(),
            offset,
            text: None,
        }
    }
}

/// Tokenise a type string. Mirrors `TypeTokenizer::tokenize` with
/// `$ignore_space = true`.
///
/// Returns `Err` on the same malformed inputs Psalm throws
/// `TypeParseTreeException` for (a lone `.` that is not part of `...` or a
/// float).
pub fn tokenize(string_type: &str) -> Result<Vec<TypeToken>, String> {
    // $type_tokens = [['', 0]];
    let mut type_tokens: Vec<TypeToken> = vec![TypeToken::new("", 0)];
    let mut was_char = false;
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    let chars: Vec<char> = string_type.chars().collect();
    let c = chars.len();
    let mut was_space = false;

    // Psalm tracks $rtc (index of the last token); here it is always
    // `type_tokens.len() - 1`, so `[$rtc]` maps to `last_mut()` and
    // `[++$rtc] = X` maps to `push(X)`.
    let mut i = 0usize;
    while i < c {
        let ch = chars[i];

        // if (!$quote_char && $char === ' ' && $ignore_space)
        if quote_char.is_none() && ch == ' ' {
            was_space = true;
            i += 1;
            continue;
        }

        let next_is_variadic_dollar = ch == '.'
            && chars.get(i + 1) == Some(&'.')
            && chars.get(i + 2) == Some(&'.')
            && chars.get(i + 3) == Some(&'$');

        if was_space && (ch == '$' || next_is_variadic_dollar) {
            // "$this" in a type context is a type token (equivalent to
            // "static"), not a parameter name (Psalm TypeTokenizer).
            let is_this_token = ch == '$'
                && chars[i..].starts_with(&['$', 't', 'h', 'i', 's'])
                && chars
                    .get(i + 5)
                    .is_none_or(|next| !next.is_alphanumeric() && *next != '_');
            if is_this_token {
                if !type_tokens.last().unwrap().value.is_empty() {
                    type_tokens.push(TypeToken::new("", i));
                }
            } else {
                // $type_tokens[++$rtc] = [' ', $i - 1];
                type_tokens.push(TypeToken::new(" ", i.wrapping_sub(1)));
                // $type_tokens[++$rtc] = ['', $i];
                type_tokens.push(TypeToken::new("", i));
            }
        } else if was_space && {
            let slice: String = chars[i..(i + 3).min(c)].iter().collect();
            slice == "as " || slice == "is " || slice == "of "
        } {
            // $type_tokens[++$rtc] = [$char . $chars[$i+1], $i - 1];
            let keyword: String = [ch, chars[i + 1]].iter().collect();
            type_tokens.push(TypeToken::new(keyword, i.wrapping_sub(1)));
            // $type_tokens[++$rtc] = ['', ++$i];
            i += 1;
            type_tokens.push(TypeToken::new("", i));
            was_char = false;
            // continue; (outer loop ++$i)
            i += 1;
            continue;
        } else if was_char {
            // $type_tokens[++$rtc] = ['', $i];
            type_tokens.push(TypeToken::new("", i));
        }

        // if ($quote_char) { ... }
        if let Some(qc) = quote_char {
            if ch == qc && i > 0 && !escaped {
                quote_char = None;
                type_tokens.last_mut().unwrap().value.push(ch);
                was_char = true;
                i += 1;
                continue;
            }

            was_char = false;

            if ch == '\\'
                && !escaped
                && i < c - 1
                && (chars[i + 1] == qc || chars[i + 1] == '\\')
            {
                escaped = true;
                i += 1;
                continue;
            }

            escaped = false;
            type_tokens.last_mut().unwrap().value.push(ch);
            i += 1;
            continue;
        }

        // if ($char === '"' || $char === '\'')
        if ch == '"' || ch == '\'' {
            if type_tokens.last().unwrap().value.is_empty() {
                *type_tokens.last_mut().unwrap() = TypeToken::new(ch.to_string(), i);
            } else {
                type_tokens.push(TypeToken::new(ch.to_string(), i));
            }
            quote_char = Some(ch);
            was_char = false;
            was_space = false;
            i += 1;
            continue;
        }

        // Single-character structural tokens.
        if matches!(
            ch,
            '<' | '>' | '|' | '?' | ',' | '{' | '}' | '[' | ']' | '(' | ')' | ' ' | '&' | '='
        ) {
            // func_num_args() special-case
            if ch == '('
                && type_tokens.last().unwrap().value == "func_num_args"
                && chars.get(i + 1) == Some(&')')
            {
                type_tokens.last_mut().unwrap().value = "func_num_args()".to_string();
                i += 1; // ++$i; outer loop adds another
                i += 1;
                continue;
            }

            if type_tokens.last().unwrap().value.is_empty() {
                *type_tokens.last_mut().unwrap() = TypeToken::new(ch.to_string(), i);
            } else {
                type_tokens.push(TypeToken::new(ch.to_string(), i));
            }

            was_char = true;
            was_space = false;
            i += 1;
            continue;
        }

        // Colon: '::' or ':'
        if ch == ':' {
            if i + 1 < c && chars[i + 1] == ':' {
                if type_tokens.last().unwrap().value.is_empty() {
                    *type_tokens.last_mut().unwrap() = TypeToken::new("::", i);
                } else {
                    type_tokens.push(TypeToken::new("::", i));
                }
                was_char = true;
                was_space = false;
                i += 1; // ++$i for the second colon
                i += 1; // outer loop advance
                continue;
            }

            if type_tokens.last().unwrap().value.is_empty() {
                *type_tokens.last_mut().unwrap() = TypeToken::new(":", i);
            } else {
                type_tokens.push(TypeToken::new(":", i));
            }
            was_char = true;
            was_space = false;
            i += 1;
            continue;
        }

        // Dot: float fragment or '...'
        if ch == '.' {
            if i + 1 < c
                && chars[i + 1].is_ascii_digit()
                && i > 0
                && chars[i - 1].is_ascii_digit()
            {
                type_tokens.last_mut().unwrap().value.push(ch);
                was_char = false;
                was_space = false;
                i += 1;
                continue;
            }

            if i + 2 >= c || chars[i + 1] != '.' || chars[i + 2] != '.' {
                return Err(format!("Unexpected token {}", ch));
            }

            if type_tokens.last().unwrap().value.is_empty() {
                *type_tokens.last_mut().unwrap() = TypeToken::new("...", i);
            } else {
                type_tokens.push(TypeToken::new("...", i));
            }
            was_char = true;
            was_space = false;
            i += 2; // $i += 2;
            i += 1; // outer loop advance
            continue;
        }

        // Default: append char to the current token.
        type_tokens.last_mut().unwrap().value.push(ch);
        was_char = false;
        was_space = false;
        i += 1;
    }

    // `$this` as a type reads as `static` (Psalm's ParseTreeCreator).
    for token in type_tokens.iter_mut() {
        if token.value == "$this" {
            token.value = "static".to_string();
        }
    }

    Ok(type_tokens)
}

/// Mirrors `TypeTokenizer::fixScalarTerms` (with `analysis_php_version_id`
/// left null, as pzoom always parses docblock types).
pub fn fix_scalar_terms(type_string: &str) -> String {
    let lc = type_string.to_lowercase();
    match lc.as_str() {
        "int" | "void" | "float" | "string" | "bool" | "callable" | "iterable" | "array"
        | "object" | "true" | "false" | "null" | "mixed" => lc,
        _ => match type_string {
            "boolean" => "bool".to_string(),
            "integer" => "int".to_string(),
            "double" | "real" => "float".to_string(),
            _ => type_string.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(s: &str) -> Vec<String> {
        tokenize(s).unwrap().into_iter().map(|t| t.value).collect()
    }

    #[test]
    fn simple_type() {
        assert_eq!(values("int"), vec!["int"]);
    }

    #[test]
    fn union_type() {
        assert_eq!(values("int|string"), vec!["int", "|", "string"]);
    }

    #[test]
    fn generic_type() {
        assert_eq!(
            values("array<int, string>"),
            vec!["array", "<", "int", ",", "string", ">"]
        );
    }

    #[test]
    fn string_literal() {
        assert_eq!(values("'hello'"), vec!["'hello'"]);
    }

    #[test]
    fn array_shape() {
        assert_eq!(
            values("array{foo: string}"),
            vec!["array", "{", "foo", ":", "string", "}"]
        );
    }

    #[test]
    fn callable_with_return() {
        assert_eq!(
            values("callable(int): string"),
            vec!["callable", "(", "int", ")", ":", "string"]
        );
    }

    #[test]
    fn class_constant() {
        assert_eq!(values("MyClass::CONSTANT"), vec!["MyClass", "::", "CONSTANT"]);
    }

    #[test]
    fn intersection() {
        assert_eq!(values("A&B"), vec!["A", "&", "B"]);
    }

    #[test]
    fn variadic_callable_param() {
        let tokens = values("callable(int...): void");
        assert!(tokens.contains(&"...".to_string()));
    }

    #[test]
    fn named_variadic_param_keeps_space() {
        // "Closure(int ...$args): void" — the space before ...$ becomes its own
        // token, mirroring Psalm.
        let tokens = values("Closure(int ...$args)");
        assert!(tokens.iter().any(|t| t == " "));
        assert!(tokens.iter().any(|t| t == "..."));
        assert!(tokens.iter().any(|t| t == "$args"));
    }

    #[test]
    fn template_as_keyword() {
        assert_eq!(
            values("T as Foo"),
            vec!["T", "as", "Foo"]
        );
    }

    #[test]
    fn float_literal_keeps_dot() {
        assert_eq!(values("1.5"), vec!["1.5"]);
    }

    #[test]
    fn func_num_args() {
        assert_eq!(
            values("func_num_args() is 1"),
            vec!["func_num_args()", "is", "1"]
        );
    }

    #[test]
    fn lone_dot_errors() {
        assert!(tokenize("a.b").is_err());
    }

    #[test]
    fn fix_scalar_terms_cases() {
        assert_eq!(fix_scalar_terms("INT"), "int");
        assert_eq!(fix_scalar_terms("boolean"), "bool");
        assert_eq!(fix_scalar_terms("integer"), "int");
        assert_eq!(fix_scalar_terms("double"), "float");
        assert_eq!(fix_scalar_terms("real"), "float");
        assert_eq!(fix_scalar_terms("MyClass"), "MyClass");
    }
}
