//! Type string tokenizer - converts type strings into tokens.
//!
//! Based on Psalm's TypeTokenizer.php

/// Reserved words in Psalm type syntax.
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

/// A token in a type string.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeToken {
    /// The token value
    pub value: String,
    /// The offset in the original string
    pub offset: usize,
    /// The original text (for namespace resolution)
    pub original_text: Option<String>,
}

impl TypeToken {
    pub fn new(value: impl Into<String>, offset: usize) -> Self {
        Self {
            value: value.into(),
            offset,
            original_text: None,
        }
    }

    pub fn with_original(value: impl Into<String>, offset: usize, original: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            offset,
            original_text: Some(original.into()),
        }
    }
}

/// Tokenize a type string into tokens.
pub fn tokenize(type_string: &str) -> Result<Vec<TypeToken>, String> {
    let mut tokens: Vec<TypeToken> = vec![TypeToken::new("", 0)];
    let mut was_char = false;
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    let chars: Vec<char> = type_string.chars().collect();
    let mut was_space = false;
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Handle quoted strings
        if quote_char.is_none() && ch == ' ' {
            was_space = true;
            i += 1;
            continue;
        }

        // Handle space followed by $ or ...$ for callable params
        if was_space
            && (ch == '$'
                || (ch == '.'
                    && chars.get(i + 1) == Some(&'.')
                    && chars.get(i + 2) == Some(&'.')
                    && chars.get(i + 3).map(|c| *c == '$').unwrap_or(false)))
        {
            let _rtc = tokens.len();
            tokens.push(TypeToken::new(" ", i.saturating_sub(1)));
            tokens.push(TypeToken::new("", i));
            was_char = false;
        } else if was_space {
            // Check for "as ", "is ", "of " keywords
            let remaining: String = chars[i..].iter().take(3).collect();
            if remaining == "as " || remaining == "is " || remaining == "of " {
                let _rtc = tokens.len();
                let keyword: String = chars[i..i + 2].iter().collect();
                tokens.push(TypeToken::new(keyword, i.saturating_sub(1)));
                tokens.push(TypeToken::new("", i + 1));
                i += 2;
                was_char = false;
                was_space = false;
                continue;
            } else if was_char {
                tokens.push(TypeToken::new("", i));
            }
        } else if was_char {
            tokens.push(TypeToken::new("", i));
        }

        // Inside quoted string
        if let Some(quote) = quote_char {
            if ch == quote && !escaped {
                quote_char = None;
                if let Some(last) = tokens.last_mut() {
                    last.value.push(ch);
                }
                was_char = true;
                i += 1;
                was_space = false;
                continue;
            }

            was_char = false;

            if ch == '\\' && !escaped {
                let next = chars.get(i + 1);
                if next == Some(&quote) || next == Some(&'\\') {
                    escaped = true;
                    i += 1;
                    continue;
                }
            }

            escaped = false;
            if let Some(last) = tokens.last_mut() {
                last.value.push(ch);
            }
            i += 1;
            was_space = false;
            continue;
        }

        // Start of quoted string
        if ch == '"' || ch == '\'' {
            let rtc = tokens.len() - 1;
            if tokens[rtc].value.is_empty() {
                tokens[rtc] = TypeToken::new(ch.to_string(), i);
            } else {
                tokens.push(TypeToken::new(ch.to_string(), i));
            }
            quote_char = Some(ch);
            was_char = false;
            was_space = false;
            i += 1;
            continue;
        }

        // Special single-character tokens
        if matches!(
            ch,
            '<' | '>' | '|' | '?' | ',' | '{' | '}' | '[' | ']' | '(' | ')' | ' ' | '&' | '='
        ) {
            // Handle func_num_args()
            if ch == '(' {
                let rtc = tokens.len() - 1;
                if tokens[rtc].value == "func_num_args" && chars.get(i + 1) == Some(&')') {
                    tokens[rtc].value = "func_num_args()".to_string();
                    i += 2;
                    continue;
                }
            }

            let rtc = tokens.len() - 1;
            if tokens[rtc].value.is_empty() {
                tokens[rtc] = TypeToken::new(ch.to_string(), i);
            } else {
                tokens.push(TypeToken::new(ch.to_string(), i));
            }
            was_char = true;
            was_space = false;
            i += 1;
            continue;
        }

        // Colon handling (: vs ::)
        if ch == ':' {
            if chars.get(i + 1) == Some(&':') {
                let rtc = tokens.len() - 1;
                if tokens[rtc].value.is_empty() {
                    tokens[rtc] = TypeToken::new("::", i);
                } else {
                    tokens.push(TypeToken::new("::", i));
                }
                was_char = true;
                was_space = false;
                i += 2;
                continue;
            }

            let rtc = tokens.len() - 1;
            if tokens[rtc].value.is_empty() {
                tokens[rtc] = TypeToken::new(":", i);
            } else {
                tokens.push(TypeToken::new(":", i));
            }
            was_char = true;
            was_space = false;
            i += 1;
            continue;
        }

        // Dot handling (. in floats, or ...)
        if ch == '.' {
            // Check if it's part of a float
            let prev_is_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let next_is_digit = chars.get(i + 1).map(|c| c.is_ascii_digit()).unwrap_or(false);
            if prev_is_digit && next_is_digit {
                if let Some(last) = tokens.last_mut() {
                    last.value.push(ch);
                }
                was_char = false;
                was_space = false;
                i += 1;
                continue;
            }

            // Must be ...
            if chars.get(i + 1) != Some(&'.') || chars.get(i + 2) != Some(&'.') {
                return Err(format!("Unexpected token {} at position {}", ch, i));
            }

            let rtc = tokens.len() - 1;
            if tokens[rtc].value.is_empty() {
                tokens[rtc] = TypeToken::new("...", i);
            } else {
                tokens.push(TypeToken::new("...", i));
            }
            was_char = true;
            was_space = false;
            i += 3;
            continue;
        }

        // Regular character - append to current token
        if let Some(last) = tokens.last_mut() {
            last.value.push(ch);
        }
        was_char = false;
        was_space = false;
        i += 1;
    }

    Ok(tokens)
}

/// Fix scalar terms to their canonical form.
pub fn fix_scalar_terms(type_string: &str) -> String {
    let lower = type_string.to_lowercase();
    match lower.as_str() {
        "int" | "void" | "float" | "string" | "bool" | "callable" | "iterable" | "array"
        | "object" | "true" | "false" | "null" | "mixed" => lower,
        _ => match type_string {
            "boolean" => "bool".to_string(),
            "integer" => "int".to_string(),
            "double" | "real" => "float".to_string(),
            _ => type_string.to_string(),
        },
    }
}

/// Check if a token is a reserved word.
pub fn is_reserved_word(word: &str) -> bool {
    PSALM_RESERVED_WORDS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_type() {
        let tokens = tokenize("int").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value, "int");
    }

    #[test]
    fn test_union_type() {
        let tokens = tokenize("int|string").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].value, "int");
        assert_eq!(tokens[1].value, "|");
        assert_eq!(tokens[2].value, "string");
    }

    #[test]
    fn test_generic_type() {
        let tokens = tokenize("array<int, string>").unwrap();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].value, "array");
        assert_eq!(tokens[1].value, "<");
        assert_eq!(tokens[2].value, "int");
        assert_eq!(tokens[3].value, ",");
        assert_eq!(tokens[4].value, "string");
        assert_eq!(tokens[5].value, ">");
    }

    #[test]
    fn test_string_literal() {
        let tokens = tokenize("'hello'").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].value, "'hello'");
    }

    #[test]
    fn test_array_shape() {
        let tokens = tokenize("array{foo: string}").unwrap();
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].value, "array");
        assert_eq!(tokens[1].value, "{");
        assert_eq!(tokens[2].value, "foo");
        assert_eq!(tokens[3].value, ":");
        assert_eq!(tokens[4].value, "string");
        assert_eq!(tokens[5].value, "}");
    }

    #[test]
    fn test_callable() {
        let tokens = tokenize("callable(int): string").unwrap();
        assert_eq!(tokens[0].value, "callable");
        assert_eq!(tokens[1].value, "(");
        assert_eq!(tokens[2].value, "int");
        assert_eq!(tokens[3].value, ")");
        assert_eq!(tokens[4].value, ":");
        assert_eq!(tokens[5].value, "string");
    }

    #[test]
    fn test_class_constant() {
        let tokens = tokenize("MyClass::CONSTANT").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].value, "MyClass");
        assert_eq!(tokens[1].value, "::");
        assert_eq!(tokens[2].value, "CONSTANT");
    }

    #[test]
    fn test_intersection() {
        let tokens = tokenize("A&B").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].value, "A");
        assert_eq!(tokens[1].value, "&");
        assert_eq!(tokens[2].value, "B");
    }

    #[test]
    fn test_ellipsis() {
        let tokens = tokenize("callable(int...): void").unwrap();
        assert!(tokens.iter().any(|t| t.value == "..."));
    }

    #[test]
    fn test_fix_scalar_terms() {
        assert_eq!(fix_scalar_terms("INT"), "int");
        assert_eq!(fix_scalar_terms("boolean"), "bool");
        assert_eq!(fix_scalar_terms("integer"), "int");
        assert_eq!(fix_scalar_terms("double"), "float");
        assert_eq!(fix_scalar_terms("MyClass"), "MyClass");
    }
}
