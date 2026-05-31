//! Docblock type-string parsing - parses PHPDoc type expressions into types.
//!
//! Mirrors Psalm's `Internal/Type/TypeParser.php`: it turns a PHPDoc type string
//! (unions, intersections, generics, array shapes, callables, key-of/value-of,
//! conditional types, ...) into pzoom's `TUnion`/`TAtomic`. The docblock comment
//! structure is parsed separately in [`super::parsed_docblock`].
//!
//! Parsing follows Psalm's three-stage pipeline:
//! [`super::type_tokenizer`] → [`super::parse_tree_creator`] → [`get_type_from_tree`].

use super::parse_tree::{NodeId, NodeKind, ParseTreeArena};
use super::parse_tree_creator::ParseTreeCreator;
use super::type_tokenizer;
use pzoom_code_info::t_atomic::{FunctionLikeParameter, PropertiesOfVisibility};
use pzoom_code_info::type_resolution::TypeResolutionContext;
use pzoom_code_info::{ArrayKey, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

// ============================================================================
// Type extraction and parsing API
// ============================================================================

/// Extract the type string from tag content like "Type $name description".
/// This is public so it can be used by declaration_collector.
pub fn extract_type_string_from_content(content: &str) -> Option<&str> {
    extract_type_string(content)
}

/// A parsed conditional type split into condition and branch strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalTypeParts {
    pub condition: String,
    pub if_true: String,
    pub if_false: String,
}

/// Extract conditional type parts from a type string like `T is X ? A : B`.
pub fn extract_conditional_type_parts(type_str: &str) -> Option<ConditionalTypeParts> {
    let mut trimmed = type_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    while let Some(stripped) = strip_wrapping_parentheses(trimmed) {
        trimmed = stripped.trim();
    }

    let (condition, if_true, if_false) = split_conditional_parts_at_depth_zero(trimmed)?;
    Some(ConditionalTypeParts {
        condition: condition.to_string(),
        if_true: if_true.to_string(),
        if_false: if_false.to_string(),
    })
}

/// Extract a variable name (including the `$` prefix) from tag content.
/// Works for tags like `@var T $x` and `@param T $x`.
pub fn extract_var_name_from_content(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut depth: u32 = 0;
    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                let start = idx;
                let mut end = idx + 1;
                for (name_idx, name_ch) in trimmed[idx + 1..].char_indices() {
                    if name_ch.is_ascii_alphanumeric() || name_ch == '_' {
                        end = idx + 1 + name_idx + name_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                if end > start + 1 {
                    return Some(&trimmed[start..end]);
                }
                return None;
            }
            _ => {}
        }
    }

    None
}

/// Extract all variable names (including `$`) from tag content.
pub fn extract_var_names_from_content(content: &str) -> Vec<&str> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }

        let start = i;
        i += 1;

        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_alphanumeric() || b == b'_' {
                i += 1;
            } else {
                break;
            }
        }

        if i > start + 1 {
            result.push(&trimmed[start..i]);
        }
    }

    result
}

/// A type-string parse/validation failure. pzoom's analogue of Psalm's
/// `TypeParseTreeException` — the parser returns this instead of silently
/// degrading to `mixed`, so callers can decide whether to report an
/// `InvalidDocblock` issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParseError {
    pub message: String,
}

impl TypeParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Parse a type string into a `TUnion`, or a [`TypeParseError`] on malformed
/// input (Psalm's `TypeParseTreeException`). Public for `declaration_collector`.
pub fn parse_type_string(
    type_str: &str,
    interner: &Interner,
) -> Result<TUnion, TypeParseError> {
    parse_type_string_with_context(type_str, interner, &TypeResolutionContext::new())
}

/// Like [`parse_type_string`] but with an explicit [`TypeResolutionContext`],
/// mirroring the `$template_type_map` Psalm's `TypeParser` receives (and
/// Hakana's `typehint_resolver`). In-scope template params are recognised
/// during parsing so utility types resolve to their deferred forms inline.
pub fn parse_type_string_with_context(
    type_str: &str,
    interner: &Interner,
    context: &TypeResolutionContext,
) -> Result<TUnion, TypeParseError> {
    let mut parsed = parse_tokens(type_str, interner, context)?;
    parsed.from_docblock = true;
    Ok(parsed)
}


/// Extract the type string from tag content like "Type $name description".
fn extract_type_string(content: &str) -> Option<&str> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Find the end of the type (handles generics with < >)
    let mut depth: u32 = 0;
    let mut end_idx = trimmed.len();
    let mut in_quote: Option<char> = None;
    let mut quote_escape = false;

    for (i, ch) in trimmed.char_indices() {
        if let Some(active_quote) = in_quote {
            if quote_escape {
                quote_escape = false;
                continue;
            }

            if ch == '\\' {
                quote_escape = true;
                continue;
            }

            if ch == active_quote {
                in_quote = None;
            }

            continue;
        }

        match ch {
            '\'' | '"' => {
                in_quote = Some(ch);
            }
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '\n' | '\r' if depth == 0 => {
                end_idx = i;
                break;
            }
            '$' if depth == 0 => {
                end_idx = i;
                break;
            }
            ' ' | '\t' if depth == 0 => {
                let remaining = trimmed[i..].trim_start();
                let prev_non_ws = trimmed[..i].chars().rev().find(|ch| !ch.is_whitespace());

                // Keep callable return type segments like "callable(...): int"
                // intact even when there is whitespace after ':'.
                if matches!(prev_non_ws, Some(':')) {
                    continue;
                }

                if starts_with_param_marker(remaining)
                    || starts_with_inline_docblock_tag(remaining)
                    || remaining.is_empty()
                    || !looks_like_type_continuation(remaining)
                {
                    end_idx = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let type_str = trimmed[..end_idx].trim();
    if type_str.is_empty() {
        None
    } else {
        Some(type_str)
    }
}

fn starts_with_inline_docblock_tag(s: &str) -> bool {
    s.trim_start().starts_with("{@")
}

fn looks_like_type_continuation(s: &str) -> bool {
    let remaining = s.trim_start();

    if remaining.is_empty()
        || starts_with_param_marker(remaining)
        || starts_with_inline_docblock_tag(remaining)
    {
        return false;
    }

    if remaining.starts_with('|')
        || remaining.starts_with('&')
        || remaining.starts_with('?')
        || remaining.starts_with(':')
        || remaining.starts_with(',')
        || remaining.starts_with(')')
        || remaining.starts_with('>')
    {
        return true;
    }

    let lowered = remaining.to_ascii_lowercase();
    let starts_with_relational_keyword = lowered.starts_with("is ")
        || lowered.starts_with("as ")
        || lowered.starts_with("of ")
        || lowered.starts_with("extends ")
        || lowered.starts_with("super ");

    // These words only continue a type inside a conditional/bound expression
    // such as `func_num_args() is 1 ? A : B`, which always contains a ternary
    // `?`. Without it we are looking at a free-text description after the type
    // (e.g. `@var mixed Is null for add operations`), which must not be folded
    // into the type string.
    starts_with_relational_keyword && remaining.contains('?')
}

fn starts_with_param_marker(s: &str) -> bool {
    let mut remaining = s.trim_start();

    if let Some(after_ref) = remaining.strip_prefix('&') {
        remaining = after_ref.trim_start();
    }

    if remaining.starts_with('$') {
        return true;
    }

    if let Some(after_unpack) = remaining.strip_prefix("...") {
        remaining = after_unpack.trim_start();

        if let Some(after_ref) = remaining.strip_prefix('&') {
            remaining = after_ref.trim_start();
        }

        if remaining.starts_with('$') {
            return true;
        }
    }

    false
}

/// Parse a simple type string into a TUnion.
/// This is a simplified parser for use during scanning.
fn strip_wrapping_parentheses(s: &str) -> Option<&str> {
    if !s.starts_with('(') || !s.ends_with(')') {
        return None;
    }
    if s.len() < 2 {
        return None;
    }

    let mut depth: i32 = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && idx != s.len() - 1 {
                    return None;
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }

    if depth == 0 {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

fn split_conditional_parts_at_depth_zero(s: &str) -> Option<(&str, &str, &str)> {
    let mut depth: i32 = 0;
    let mut question_idx: Option<usize> = None;

    for (idx, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth -= 1,
            '?' if depth == 0 => {
                question_idx = Some(idx);
                break;
            }
            _ => {}
        }
    }

    let question_idx = question_idx?;
    let mut depth: i32 = 0;
    let mut nested_ternary_depth = 0i32;
    let mut colon_idx: Option<usize> = None;

    for (idx, ch) in s[question_idx + 1..].char_indices() {
        let absolute_idx = question_idx + 1 + idx;
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth -= 1,
            '?' if depth == 0 => nested_ternary_depth += 1,
            ':' if depth == 0 => {
                if nested_ternary_depth == 0 {
                    colon_idx = Some(absolute_idx);
                    break;
                }
                nested_ternary_depth -= 1;
            }
            _ => {}
        }
    }

    let colon_idx = colon_idx?;
    let condition = s[..question_idx].trim();
    let if_true = s[question_idx + 1..colon_idx].trim();
    let if_false = s[colon_idx + 1..].trim();

    if condition.is_empty() || if_true.is_empty() || if_false.is_empty() {
        return None;
    }

    Some((condition, if_true, if_false))
}

/// Split a type string by | at depth 0.
fn strip_case_insensitive_suffix<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    if s.len() < suffix.len() {
        return None;
    }

    let split = s.len() - suffix.len();
    if s[split..].eq_ignore_ascii_case(suffix) {
        Some(&s[..split])
    } else {
        None
    }
}

fn strip_wrapping_quotes(s: &str) -> Option<String> {
    if s.len() < 2 {
        return None;
    }

    let bytes = s.as_bytes();
    let first = bytes[0];
    let last = *bytes.last().unwrap();

    if !((first == b'\'' && last == b'\'') || (first == b'"' && last == b'"')) {
        return None;
    }

    let quote = first as char;
    let inner = &s[1..s.len() - 1];
    let mut escaped = false;
    let mut unescaped = String::with_capacity(inner.len());

    for ch in inner.chars() {
        if escaped {
            if ch == quote || ch == '\\' {
                unescaped.push(ch);
            } else {
                unescaped.push('\\');
                unescaped.push(ch);
            }
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        unescaped.push(ch);
    }

    if escaped {
        unescaped.push('\\');
    }

    Some(unescaped)
}

fn normalize_class_string_key_union(key_union: &TUnion) -> TUnion {
    if key_union.types.iter().all(|atomic| {
        matches!(
            atomic,
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
        )
    }) {
        return key_union.clone();
    }

    if key_union.is_single() {
        if let Some(single_atomic) = key_union.get_single() {
            match single_atomic {
                TAtomic::TNamedObject { .. }
                | TAtomic::TTemplateParam { .. }
                | TAtomic::TTemplateParamClass { .. }
                | TAtomic::TObjectIntersection { .. } => {
                    return TUnion::new(TAtomic::TClassString {
                        as_type: Some(Box::new(single_atomic.clone())),
                    });
                }
                _ => {}
            }
        }
    }

    TUnion::new(TAtomic::TClassString { as_type: None })
}

fn template_param_deferred_key_of(union: &TUnion, is_key_of: bool) -> Option<TAtomic> {
    let TAtomic::TTemplateParam {
        name,
        defining_entity,
        as_type,
    } = union.get_single()?
    else {
        return None;
    };

    if is_key_of {
        Some(TAtomic::TTemplateKeyOf {
            param_name: *name,
            defining_entity: *defining_entity,
            as_type: as_type.clone(),
        })
    } else {
        Some(TAtomic::TTemplateValueOf {
            param_name: *name,
            defining_entity: *defining_entity,
            as_type: as_type.clone(),
        })
    }
}

fn resolve_key_of_union(union: &TUnion) -> TAtomic {
    union_to_atomic_or(&resolve_key_of_union_to_union(union), TAtomic::TArrayKey)
}

fn resolve_key_of_union_to_union(union: &TUnion) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &union.types {
        let key_union = resolve_key_of_atomic_to_union(atomic);
        extend_atomic_vec_unique(&mut key_types, &key_union.types);
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        TUnion::from_types(key_types)
    }
}

fn resolve_key_of_atomic_to_union(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { key_type, .. }
        | TAtomic::TNonEmptyArray { key_type, .. }
        | TAtomic::TIterable { key_type, .. } => normalize_array_key_union_for_docblock(key_type),
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => TUnion::int(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            ..
        } => {
            let mut key_types: Vec<TAtomic> = Vec::new();
            for key in properties.keys() {
                let key_atomic = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TAtomic::TLiteralInt { value: *value }
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value) => TAtomic::TLiteralString {
                        value: value.clone(),
                    },
                };
                if !key_types.contains(&key_atomic) {
                    key_types.push(key_atomic);
                }
            }

            if let Some(fallback_key_type) = fallback_key_type {
                let normalized_fallback = normalize_array_key_union_for_docblock(fallback_key_type);
                for fallback_atomic in normalized_fallback.types {
                    if !key_types.contains(&fallback_atomic) {
                        key_types.push(fallback_atomic);
                    }
                }
            }

            if key_types.is_empty() {
                TUnion::array_key()
            } else {
                TUnion::from_types(key_types)
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_key_of_union_to_union(as_type),
        _ => TUnion::array_key(),
    }
}

fn resolve_value_of_union(union: &TUnion) -> TAtomic {
    union_to_atomic_or(&resolve_value_of_union_to_union(union), TAtomic::TMixed)
}

fn resolve_value_of_union_to_union(union: &TUnion) -> TUnion {
    let mut value_types = Vec::new();

    for atomic in &union.types {
        let value_union = resolve_value_of_atomic_to_union(atomic);
        extend_atomic_vec_unique(&mut value_types, &value_union.types);
    }

    if value_types.is_empty() {
        TUnion::mixed()
    } else {
        TUnion::from_types(value_types)
    }
}

fn resolve_value_of_atomic_to_union(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TIterable { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => (**value_type).clone(),
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut value_types: Vec<TAtomic> = Vec::new();

            for value in properties.values() {
                extend_atomic_vec_unique(&mut value_types, &value.types);
            }

            if let Some(fallback_value_type) = fallback_value_type {
                extend_atomic_vec_unique(&mut value_types, &fallback_value_type.types);
            }

            if value_types.is_empty() {
                TUnion::mixed()
            } else {
                TUnion::from_types(value_types)
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_value_of_union_to_union(as_type),
        _ => TUnion::mixed(),
    }
}

fn union_to_atomic_or(union: &TUnion, default: TAtomic) -> TAtomic {
    union.get_single().cloned().unwrap_or(default)
}

fn extend_atomic_vec_unique(target: &mut Vec<TAtomic>, source: &[TAtomic]) {
    for atomic in source {
        if !target.contains(atomic) {
            target.push(atomic.clone());
        }
    }
}

fn normalize_array_key_union_for_docblock(key_union: &TUnion) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &key_union.types {
        if let Some(normalized_atomic) = normalize_array_key_atomic_for_docblock(atomic) {
            if matches!(normalized_atomic, TAtomic::TArrayKey) {
                return TUnion::array_key();
            }

            if !key_types.contains(&normalized_atomic) {
                key_types.push(normalized_atomic);
            }
        }
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        TUnion::from_types(key_types)
    }
}

fn normalize_array_key_atomic_for_docblock(atomic: &TAtomic) -> Option<TAtomic> {
    match atomic {
        TAtomic::TInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TIntRange { .. }
        | TAtomic::TString
        | TAtomic::TLiteralString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TClassString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TTruthyString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TArrayKey => Some(atomic.clone()),
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => Some(TAtomic::TArrayKey),
        _ => None,
    }
}

/// Find the index of the matching closing `>` bracket.
// ============================================================================
// Faithful parse pipeline: TypeTokenizer -> ParseTreeCreator -> getTypeFromTree
// ============================================================================

/// Result of converting a parse-tree node: either a single atomic or a union,
/// mirroring Psalm's `Atomic|Union` return from `getTypeFromTree`.
enum TypeResult {
    Atomic(TAtomic),
    Union(TUnion),
}

impl TypeResult {
    fn into_union(self) -> TUnion {
        match self {
            TypeResult::Atomic(a) => TUnion::new(a),
            TypeResult::Union(u) => u,
        }
    }
}

/// Mirrors Psalm's `TypeParser::parseTokens` entry point: tokenize, build the
/// parse tree, then convert it. Returns `Err` (Psalm's `TypeParseTreeException`)
/// on malformed input.
fn parse_tokens(
    type_str: &str,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> Result<TUnion, TypeParseError> {
    let trimmed = type_str.trim();
    if trimmed.is_empty() {
        return Ok(TUnion::mixed());
    }

    // Psalm's docblock layer collapses multi-line types onto a single line
    // before tokenizing (the tokenizer only treats ASCII space as whitespace).
    // pzoom's tag extraction preserves newlines/tabs, so normalise them here.
    let normalized: String = trimmed
        .chars()
        .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
        .collect();
    let trimmed = normalized.as_str();

    let tokens = type_tokenizer::tokenize(trimmed).map_err(TypeParseError::new)?;

    // pzoom tolerates trailing commas in shapes/generics/callables, e.g.
    // `array{a: int,}` — Psalm's tokenizer does not, so drop any `,` that sits
    // immediately before a closing bracket before building the tree.
    let tokens = strip_trailing_commas(tokens);

    // Mirror Psalm's getConditionalSanitizedTypeTokens: a parameter conditional
    // `($param is T ? A : B)` tokenises a stray space before `$param`. When
    // `$param` is a known function parameter, drop that space so the condition
    // parses (otherwise it looks like a misplaced callable-param marker).
    let tokens = strip_param_conditional_spaces(tokens, interner, ctx);

    let (tree, root) = ParseTreeCreator::new(tokens)
        .create()
        .map_err(TypeParseError::new)?;

    Ok(get_type_from_tree(&tree, root, interner, ctx).into_union())
}

/// Drop the space token the tokenizer inserts before a `$param` that is the
/// subject of a conditional (`… $param is …`), when `$param` is a known
/// function parameter. Mirrors Psalm's `getConditionalSanitizedTypeTokens`
/// unsetting the preceding space.
fn strip_param_conditional_spaces(
    tokens: Vec<type_tokenizer::TypeToken>,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> Vec<type_tokenizer::TypeToken> {
    if ctx.param_names.is_empty() {
        return tokens;
    }

    let mut out: Vec<type_tokenizer::TypeToken> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].value == " " {
            if let (Some(name_tok), Some(is_tok)) = (tokens.get(i + 1), tokens.get(i + 2)) {
                // Param names are stored as written (including the `$`), matching
                // the token value here and `ParamInfo.name`.
                if is_tok.value == "is"
                    && name_tok.value.starts_with('$')
                    && ctx.is_param(interner.intern(name_tok.value.as_str()))
                {
                    // Skip the space token.
                    i += 1;
                    continue;
                }
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

/// Drop any `,` token that is immediately followed by a closing bracket, so
/// trailing commas in shapes/generics/callables parse (a pzoom leniency).
fn strip_trailing_commas(
    tokens: Vec<type_tokenizer::TypeToken>,
) -> Vec<type_tokenizer::TypeToken> {
    let mut out: Vec<type_tokenizer::TypeToken> = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        if token.value == "," {
            if let Some(next) = tokens.get(i + 1) {
                if matches!(next.value.as_str(), "}" | ")" | ">" | "]") {
                    continue;
                }
            }
        }
        out.push(token.clone());
    }
    out
}

/// Port of `TypeParser::getTypeFromTree`: dispatch on the node kind.
fn get_type_from_tree(
    tree: &ParseTreeArena,
    id: NodeId,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let kind = tree.kind(id).clone();

    match kind {
        NodeKind::Generic { value } => generic_tree_to_type(tree, id, &value, interner, ctx),

        NodeKind::Union => union_tree_to_type(tree, id, interner, ctx),

        NodeKind::Intersection => intersection_tree_to_type(tree, id, interner, ctx),

        NodeKind::KeyedArray { value } => keyed_array_tree_to_type(tree, id, &value, interner, ctx),

        NodeKind::CallableWithReturnType => {
            let children = tree.children(id).to_vec();
            let callable = children
                .first()
                .map(|c| get_type_from_tree(tree, *c, interner, ctx));
            let return_type = children
                .get(1)
                .map(|c| Box::new(get_type_from_tree(tree, *c, interner, ctx).into_union()));

            match callable {
                Some(TypeResult::Atomic(TAtomic::TCallable {
                    params, is_pure, ..
                })) => TypeResult::Atomic(TAtomic::TCallable {
                    params,
                    return_type,
                    is_pure,
                }),
                Some(TypeResult::Atomic(TAtomic::TClosure {
                    params, is_pure, ..
                })) => TypeResult::Atomic(TAtomic::TClosure {
                    params,
                    return_type,
                    is_pure,
                }),
                Some(other) => other,
                None => TypeResult::Union(TUnion::mixed()),
            }
        }

        NodeKind::Callable { value } => callable_tree_to_type(tree, id, &value, interner, ctx),

        NodeKind::Encapsulation => match tree.children(id).first() {
            Some(c) => get_type_from_tree(tree, *c, interner, ctx),
            None => TypeResult::Union(TUnion::mixed()),
        },

        NodeKind::Nullable => match tree.children(id).first() {
            Some(c) => {
                let inner = get_type_from_tree(tree, *c, interner, ctx).into_union();
                let mut types = inner.types;
                if !types.contains(&TAtomic::TNull) {
                    types.push(TAtomic::TNull);
                }
                TypeResult::Union(TUnion::from_types(types))
            }
            None => TypeResult::Union(TUnion::mixed()),
        },

        NodeKind::IndexedAccess { value } => {
            let array_param = tree
                .children(id)
                .first()
                .and_then(|c| tree.value(*c))
                .unwrap_or("")
                .to_string();
            let array_type = build_named_atomic(
                &type_tokenizer::fix_scalar_terms(&array_param).to_lowercase(),
                &array_param,
                None,
                interner,
                ctx,
            );
            let offset_type =
                build_named_atomic(&value.to_lowercase(), &value, None, interner, ctx);
            TypeResult::Atomic(TAtomic::TNamedObject {
                name: StrId::PZOOM_INDEXED_ACCESS,
                type_params: Some(vec![TUnion::new(array_type), TUnion::new(offset_type)]),
                is_static: false,
                remapped_params: false,
            })
        }

        NodeKind::TemplateAs {
            param_name,
            as_type,
        } => {
            // Psalm returns a TTemplateParam whose `as` is the named object.
            TypeResult::Atomic(TAtomic::TTemplateParam {
                name: interner.intern(&param_name),
                defining_entity: StrId::EMPTY,
                as_type: Box::new(TUnion::new(TAtomic::named_object(interner.intern(&as_type)))),
            })
        }

        NodeKind::Conditional { .. } => {
            // pzoom flattens conditional *type expressions* into the union of
            // both branches (conditional *return types* are modelled separately
            // via `extract_conditional_type_parts` in the declaration collector).
            let mut types: Vec<TAtomic> = Vec::new();
            for child in tree.children(id) {
                let union = get_type_from_tree(tree, *child, interner, ctx).into_union();
                for atomic in union.types {
                    if !types.contains(&atomic) {
                        types.push(atomic);
                    }
                }
            }
            if types.is_empty() {
                TypeResult::Union(TUnion::mixed())
            } else {
                TypeResult::Union(TUnion::from_types(types))
            }
        }

        NodeKind::Value { value, text, .. } => {
            value_to_type(&value, text.as_deref(), interner, ctx)
        }

        // @method signatures are not represented in pzoom's type model here.
        NodeKind::Method { .. } | NodeKind::MethodWithReturnType | NodeKind::MethodParam { .. } => {
            TypeResult::Union(TUnion::mixed())
        }

        // These never appear as a standalone type; fall back to the first child.
        NodeKind::Root
        | NodeKind::KeyedArrayProperty { .. }
        | NodeKind::CallableParam { .. }
        | NodeKind::TemplateIs { .. }
        | NodeKind::FieldEllipsis => match tree.children(id).first() {
            Some(c) => get_type_from_tree(tree, *c, interner, ctx),
            None => TypeResult::Union(TUnion::mixed()),
        },
    }
}

/// Convert a `Value` node (`int`, `'literal'`, `123`, `Foo::class`, `Foo::BAR`,
/// a class name, ...) to a type.
fn value_to_type(
    value: &str,
    _text: Option<&str>,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    // Literal string.
    if let Some(inner) = strip_wrapping_quotes(value) {
        return TypeResult::Atomic(TAtomic::TLiteralString { value: inner });
    }

    // `Foo::class` and friends.
    if let Some(class_name) = strip_case_insensitive_suffix(value, "::class") {
        return TypeResult::Atomic(class_string_from_class_const(class_name.trim(), interner, ctx));
    }

    // Other class constants `Foo::BAR` — kept as a token-named object so that
    // wildcard/const resolution can happen later (current pzoom behaviour).
    if value.contains("::") {
        return TypeResult::Atomic(TAtomic::named_object(interner.intern(value)));
    }

    // Numeric literals.
    if let Ok(int_value) = value.parse::<i64>() {
        return TypeResult::Atomic(TAtomic::TLiteralInt { value: int_value });
    }
    if let Ok(float_value) = value.parse::<f64>() {
        if value.contains('.') || value.contains('e') || value.contains('E') {
            return TypeResult::Atomic(TAtomic::TLiteralFloat { value: float_value });
        }
    }

    let lower = type_tokenizer::fix_scalar_terms(value).to_lowercase();
    TypeResult::Atomic(build_named_atomic(&lower, value, None, interner, ctx))
}

/// Shared `Foo::class` handling (used by both value nodes and array-shape keys).
fn class_string_from_class_const(
    class_name: &str,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TAtomic {
    if class_name.is_empty() {
        return TAtomic::TClassString { as_type: None };
    }

    let parsed = parse_tokens(class_name, interner, ctx).unwrap_or_else(|_| TUnion::mixed());
    if let Some(single_atomic) = parsed.get_single().cloned() {
        return match single_atomic {
            TAtomic::TLiteralString { value } => TAtomic::TLiteralClassString { name: value },
            TAtomic::TLiteralClassString { name } => TAtomic::TLiteralClassString { name },
            TAtomic::TNamedObject {
                name,
                type_params: None,
                ..
            } => {
                let resolved = interner.lookup(name);
                if resolved.as_ref().eq_ignore_ascii_case("self")
                    || resolved.as_ref().eq_ignore_ascii_case("static")
                    || resolved.as_ref().eq_ignore_ascii_case("parent")
                {
                    TAtomic::TClassString {
                        as_type: Some(Box::new(TAtomic::TNamedObject {
                            name,
                            type_params: None,
                            is_static: false,
                            remapped_params: false,
                        })),
                    }
                } else {
                    TAtomic::TLiteralClassString {
                        name: resolved.to_string(),
                    }
                }
            }
            other => TAtomic::TClassString {
                as_type: Some(Box::new(other)),
            },
        };
    }

    TAtomic::TClassString { as_type: None }
}

/// Port of `getTypeFromGenericTree`: a base type name applied to type params.
fn generic_tree_to_type(
    tree: &ParseTreeArena,
    id: NodeId,
    value: &str,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let params: Vec<TUnion> = tree
        .children(id)
        .iter()
        .map(|c| get_type_from_tree(tree, *c, interner, ctx).into_union())
        .collect();

    let generic_type_value = type_tokenizer::fix_scalar_terms(value).to_lowercase();

    // key-of/value-of resolve to a (possibly multi-atomic) union, matching the
    // previous top-level behaviour.
    match generic_type_value.as_str() {
        "key-of" => {
            if let Some(param) = params.first() {
                if let Some(template) = template_param_deferred_key_of(param, true) {
                    return TypeResult::Atomic(template);
                }
                return TypeResult::Union(resolve_key_of_union_to_union(param));
            }
            return TypeResult::Union(TUnion::array_key());
        }
        "value-of" => {
            if let Some(param) = params.first() {
                if let Some(template) = template_param_deferred_key_of(param, false) {
                    return TypeResult::Atomic(template);
                }
                return TypeResult::Union(resolve_value_of_union_to_union(param));
            }
            return TypeResult::Union(TUnion::mixed());
        }
        // `int-mask<A, B, C, ...>` — Psalm's getTypeFromGenericTree: each member
        // must be a single int (or scalar class constant). For the all-literal
        // case it returns the union of every OR-combination of the values via
        // getComputedIntsFromMask. pzoom has no TClassConstant/TIntMask atomic,
        // so a member that is not a literal int collapses to the faithful
        // supertype `int` (TIntMask is always an int subtype).
        "int-mask" => {
            let mut potential_ints: Vec<i64> = Vec::new();
            for param in &params {
                match param.get_single() {
                    Some(TAtomic::TLiteralInt { value }) => potential_ints.push(*value),
                    _ => return TypeResult::Atomic(TAtomic::TInt),
                }
            }
            if potential_ints.is_empty() {
                return TypeResult::Atomic(TAtomic::TInt);
            }
            return TypeResult::Union(TUnion::from_types(get_computed_ints_from_mask(
                &potential_ints,
            )));
        }
        // `int-mask-of<T>` — Psalm wraps a class-constant/value-of/key-of
        // reference in TIntMaskOf. pzoom lacks those atomics; the result is
        // always a subset of `int`.
        "int-mask-of" => {
            return TypeResult::Atomic(TAtomic::TInt);
        }
        // `arraylike-object<V>` / `arraylike-object<K, V>` — Psalm builds a
        // Traversable<K, V> intersected with ArrayAccess<K, V> & Countable
        // (defaulting the key to `mixed` when only one param is given).
        "arraylike-object" => {
            let mut params = params;
            if params.len() == 1 {
                params.insert(0, TUnion::mixed());
            }
            let traversable = TAtomic::named_object_with_params(
                interner.intern("Traversable"),
                Some(params.clone()),
            );
            let array_access = TAtomic::named_object_with_params(
                interner.intern("ArrayAccess"),
                Some(params),
            );
            let countable = TAtomic::named_object(interner.intern("Countable"));
            return TypeResult::Atomic(TAtomic::TObjectIntersection {
                types: vec![traversable, array_access, countable],
            });
        }
        _ => {}
    }

    TypeResult::Atomic(build_named_atomic(
        &generic_type_value,
        value,
        Some(params),
        interner,
        ctx,
    ))
}

/// Port of `TypeParser::getComputedIntsFromMask`: every OR-combination of the
/// given int values, plus 0, as literal ints (order/uniqueness preserved).
fn get_computed_ints_from_mask(potential_ints: &[i64]) -> Vec<TAtomic> {
    let mut potential_values: Vec<i64> = Vec::new();

    for &ith in potential_ints {
        let mut new_values: Vec<i64> = vec![ith];
        if ith != 0 {
            for &potential_value in &potential_values {
                new_values.push(ith | potential_value);
            }
        }
        new_values.extend(potential_values.iter().copied());
        potential_values = new_values;
    }

    potential_values.insert(0, 0);

    let mut seen = std::collections::HashSet::new();
    potential_values
        .into_iter()
        .filter(|v| seen.insert(*v))
        .map(|value| TAtomic::TLiteralInt { value })
        .collect()
}

/// Port of `getTypeFromUnionTree`.
fn union_tree_to_type(
    tree: &ParseTreeArena,
    id: NodeId,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let mut types: Vec<TAtomic> = Vec::new();
    let mut has_null = false;

    for child in tree.children(id) {
        let atomic_union = if tree.is_nullable(*child) {
            has_null = true;
            match tree.children(*child).first() {
                Some(inner) => get_type_from_tree(tree, *inner, interner, ctx).into_union(),
                None => TUnion::mixed(),
            }
        } else {
            get_type_from_tree(tree, *child, interner, ctx).into_union()
        };

        for atomic in atomic_union.types {
            if !types.contains(&atomic) {
                types.push(atomic);
            }
        }
    }

    if has_null && !types.contains(&TAtomic::TNull) {
        types.push(TAtomic::TNull);
    }

    if types.is_empty() {
        TypeResult::Union(TUnion::mixed())
    } else {
        TypeResult::Union(TUnion::from_types(types))
    }
}

/// Port of `getTypeFromIntersectionTree`, using pzoom's `TObjectIntersection`.
fn intersection_tree_to_type(
    tree: &ParseTreeArena,
    id: NodeId,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let mut intersection_types: Vec<TAtomic> = Vec::new();

    for child in tree.children(id) {
        let union = get_type_from_tree(tree, *child, interner, ctx).into_union();
        let mut iter = union.types.into_iter();
        let Some(atomic) = iter.next() else {
            continue;
        };
        if iter.next().is_some() {
            // Intersection members cannot be unions.
            continue;
        }

        match atomic {
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if !intersection_types.contains(&nested) {
                        intersection_types.push(nested);
                    }
                }
            }
            other => {
                if !intersection_types.contains(&other) {
                    intersection_types.push(other);
                }
            }
        }
    }

    match intersection_types.len() {
        0 => TypeResult::Union(TUnion::mixed()),
        1 => TypeResult::Atomic(intersection_types.pop().unwrap()),
        _ => TypeResult::Atomic(TAtomic::TObjectIntersection {
            types: intersection_types,
        }),
    }
}

/// Port of `getTypeFromCallableTree`.
fn callable_tree_to_type(
    tree: &ParseTreeArena,
    id: NodeId,
    value: &str,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let mut params: Vec<FunctionLikeParameter> = Vec::new();

    for child in tree.children(id) {
        if let NodeKind::CallableParam {
            variadic,
            has_default,
            name,
        } = tree.kind(*child).clone()
        {
            let param_type = match tree.children(*child).first() {
                Some(c) => get_type_from_tree(tree, *c, interner, ctx).into_union(),
                None => TUnion::mixed(),
            };
            params.push(FunctionLikeParameter {
                name: name.as_deref().map(|n| interner.intern(n)),
                param_type,
                is_optional: has_default,
                is_variadic: variadic,
                by_ref: false,
            });
        } else {
            let param_type = get_type_from_tree(tree, *child, interner, ctx).into_union();
            params.push(FunctionLikeParameter {
                name: None,
                param_type,
                is_optional: false,
                is_variadic: false,
                by_ref: false,
            });
        }
    }

    let lower = value.to_lowercase();
    let is_pure = lower.starts_with("pure-");

    if matches!(lower.as_str(), "closure" | "\\closure" | "pure-closure") {
        TypeResult::Atomic(TAtomic::TClosure {
            params: Some(params),
            return_type: None,
            is_pure: if is_pure { Some(true) } else { None },
        })
    } else {
        TypeResult::Atomic(TAtomic::TCallable {
            params: Some(params),
            return_type: None,
            is_pure: if is_pure { Some(true) } else { None },
        })
    }
}

/// Port of `getTypeFromKeyedArrayTree`, producing pzoom's `TKeyedArray`.
fn keyed_array_tree_to_type(
    tree: &ParseTreeArena,
    id: NodeId,
    value: &str,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TypeResult {
    let mut children = tree.children(id).to_vec();

    // A trailing empty GenericTree carries the `...<K, V>` extra/fallback params.
    let mut extra_params: Option<Vec<NodeId>> = None;
    if let Some(last) = children.last() {
        if let NodeKind::Generic { value: gv } = tree.kind(*last) {
            if gv.is_empty() {
                extra_params = Some(tree.children(*last).to_vec());
                children.pop();
            }
        }
    }

    // Strip the trailing `callable-` marker for the is_list/`type` checks,
    // mirroring Psalm's `str_starts_with($type, 'callable-')` handling.
    let type_name: &str = value.strip_prefix("callable-").unwrap_or(value);

    let mut properties: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
    let mut sealed = true;
    // is_list tracking follows Psalm's getTypeFromKeyedArrayTree exactly.
    let mut is_list = true;
    let mut had_optional = false;
    let mut had_explicit = false;
    let mut had_implicit = false;
    let mut previous_property_key: i64 = -1;

    let child_count = children.len();
    for (i, child) in children.iter().enumerate() {
        if tree.is_field_ellipsis(*child) {
            if i != child_count - 1 {
                // Unexpected `...` — degrade to mixed (Psalm throws).
                return TypeResult::Union(TUnion::mixed());
            }
            sealed = false;
            break;
        }

        if let NodeKind::KeyedArrayProperty { value: prop_value } = tree.kind(*child).clone() {
            let mut value_type = match tree.children(*child).first() {
                Some(c) => get_type_from_tree(tree, *c, interner, ctx).into_union(),
                None => TUnion::mixed(),
            };
            let possibly_undefined = tree.possibly_undefined(*child);

            let key = keyed_array_key(&prop_value);

            // Psalm marks the shape non-list on the first explicit key that is
            // not the next sequential int, on `array`/`callable-array` shapes,
            // on a required key after an optional one, or on any string key.
            let key_int = match &key {
                ArrayKey::Int(n) => Some(*n),
                ArrayKey::String(_) => None,
            };
            if is_list
                && (key_int.is_none()
                    || (had_optional && !possibly_undefined)
                    || type_name == "array"
                    || previous_property_key != key_int.unwrap_or(previous_property_key) - 1)
            {
                is_list = false;
            }
            had_explicit = true;
            if let Some(n) = key_int {
                previous_property_key = n;
            }

            if possibly_undefined {
                value_type.possibly_undefined = true;
                had_optional = true;
            }
            properties.insert(key, value_type);
        } else {
            // Implicit entry — keyed by its position, list stays intact.
            had_implicit = true;
            let value_type = get_type_from_tree(tree, *child, interner, ctx).into_union();
            properties.insert(ArrayKey::Int(i as i64), value_type);
        }
    }
    let _ = (had_explicit, had_implicit);

    // `object{...}` is an object with known properties, not an array.
    if value == "object" {
        return TypeResult::Atomic(TAtomic::TObjectWithProperties { properties });
    }

    let mut fallback_key_type: Option<Box<TUnion>> = None;
    let mut fallback_value_type: Option<Box<TUnion>> = None;

    if let Some(extra) = extra_params {
        let extra_unions: Vec<TUnion> = extra
            .iter()
            .map(|c| get_type_from_tree(tree, *c, interner, ctx).into_union())
            .collect();
        match extra_unions.len() {
            1 => {
                fallback_key_type = Some(Box::new(TUnion::array_key()));
                fallback_value_type = Some(Box::new(extra_unions.into_iter().next().unwrap()));
            }
            2 => {
                let mut iter = extra_unions.into_iter();
                fallback_key_type = Some(Box::new(iter.next().unwrap()));
                fallback_value_type = Some(Box::new(iter.next().unwrap()));
            }
            _ => {}
        }
        sealed = false;
    } else if !sealed {
        fallback_key_type = Some(Box::new(TUnion::array_key()));
        fallback_value_type = Some(Box::new(TUnion::mixed()));
    }

    TypeResult::Atomic(TAtomic::TKeyedArray {
        properties,
        is_list,
        sealed,
        fallback_key_type,
        fallback_value_type,
    })
}

/// Convert a keyed-array property key token to an [`ArrayKey`], handling quoted
/// keys, integer keys, and `Foo::class` keys.
fn keyed_array_key(key: &str) -> ArrayKey {
    let trimmed = key.trim();

    if let Some(inner) = strip_wrapping_quotes(trimmed) {
        return ArrayKey::String(inner);
    }

    if let Ok(int_key) = trimmed.parse::<i64>() {
        return ArrayKey::Int(int_key);
    }

    ArrayKey::String(trimmed.to_string())
}

/// The base-name dispatch from Psalm's generic/scalar atomic construction,
/// factored out of the legacy `parse_atomic_type` so it can be driven by parse
/// tree nodes. `base_name` is the lower-cased, scalar-fixed name; `raw_name` is
/// the original text (used for named objects).
fn build_named_atomic(
    base_name: &str,
    raw_name: &str,
    generic_params: Option<Vec<TUnion>>,
    interner: &Interner,
    ctx: &TypeResolutionContext,
) -> TAtomic {
    match base_name {
        "int" | "integer" => {
            if let Some(params) = generic_params.as_ref() {
                if params.len() == 2 {
                    let min = params[0].get_single().and_then(|a| match a {
                        TAtomic::TLiteralInt { value } => Some(*value),
                        _ => None,
                    });
                    let max = params[1].get_single().and_then(|a| match a {
                        TAtomic::TLiteralInt { value } => Some(*value),
                        _ => None,
                    });
                    return TAtomic::TIntRange { min, max };
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
        "mixed" => TAtomic::TMixed,
        "non-empty-mixed" => TAtomic::TNonEmptyMixed,
        "object" | "stringable-object" | "callable-object" => TAtomic::TObject,
        "resource" | "open-resource" => TAtomic::TResource,
        "closed-resource" => TAtomic::TClosedResource,
        "never" | "no-return" | "never-return" | "never-returns" => TAtomic::TNothing,

        "array-key" => TAtomic::TArrayKey,
        "scalar" => TAtomic::TScalar,
        "numeric" => TAtomic::TNumeric,
        "positive-int" => TAtomic::TIntRange {
            min: Some(1),
            max: None,
        },
        "negative-int" => TAtomic::TIntRange {
            min: None,
            max: Some(-1),
        },
        "non-negative-int" => TAtomic::TIntRange {
            min: Some(0),
            max: None,
        },
        "non-positive-int" => TAtomic::TIntRange {
            min: None,
            max: Some(0),
        },
        "literal-int" => TAtomic::TInt,
        "non-empty-string" => TAtomic::TNonEmptyString,
        "numeric-string" => TAtomic::TNumericString,
        "lowercase-string" => TAtomic::TLowercaseString,
        "non-empty-lowercase-string" => TAtomic::TNonEmptyLowercaseString,
        "literal-string" | "non-empty-literal-string" => TAtomic::TLiteralString {
            value: pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
        },
        "truthy-string" | "non-falsy-string" => TAtomic::TTruthyString,

        "key-of" => generic_params
            .as_ref()
            .and_then(|params| params.first())
            .map(|param| {
                template_param_deferred_key_of(param, true)
                    .unwrap_or_else(|| resolve_key_of_union(param))
            })
            .unwrap_or(TAtomic::TArrayKey),
        "value-of" => generic_params
            .as_ref()
            .and_then(|params| params.first())
            .map(|param| {
                template_param_deferred_key_of(param, false)
                    .unwrap_or_else(|| resolve_value_of_union(param))
            })
            .unwrap_or(TAtomic::TMixed),
        "class-string-map" => {
            let (key_type, value_type) = if let Some(params) = generic_params {
                match params.len() {
                    0 => (
                        TUnion::new(TAtomic::TClassString { as_type: None }),
                        TUnion::mixed(),
                    ),
                    1 => (normalize_class_string_key_union(&params[0]), TUnion::mixed()),
                    _ => (
                        normalize_class_string_key_union(&params[0]),
                        params[1].clone(),
                    ),
                }
            } else {
                (
                    TUnion::new(TAtomic::TClassString { as_type: None }),
                    TUnion::mixed(),
                )
            };
            TAtomic::TArray {
                key_type: Box::new(key_type),
                value_type: Box::new(value_type),
            }
        }
        // `properties-of<C>` (and visibility-scoped variants). When the param is
        // an in-scope template (resolved via the context), it stays deferred as
        // `TTemplatePropertiesOf`; a concrete class becomes `TPropertiesOf`
        // (which the declaration collector's resolution pass also rewrites to
        // the template form when no context was threaded). Mirrors Psalm's
        // getTypeFromGenericTree handling.
        "properties-of"
        | "public-properties-of"
        | "protected-properties-of"
        | "private-properties-of" => {
            let visibility = match base_name {
                "public-properties-of" => PropertiesOfVisibility::Public,
                "protected-properties-of" => PropertiesOfVisibility::Protected,
                "private-properties-of" => PropertiesOfVisibility::Private,
                _ => PropertiesOfVisibility::All,
            };
            generic_params
                .as_ref()
                .and_then(|params| params.first())
                .and_then(|param| match param.get_single() {
                    Some(TAtomic::TTemplateParam {
                        name,
                        defining_entity,
                        ..
                    }) => Some(TAtomic::TTemplatePropertiesOf {
                        param_name: *name,
                        defining_entity: *defining_entity,
                        visibility_filter: visibility,
                    }),
                    Some(TAtomic::TNamedObject {
                        name,
                        type_params: None,
                        ..
                    }) => Some(TAtomic::TPropertiesOf {
                        classlike_name: *name,
                        visibility_filter: visibility,
                    }),
                    _ => None,
                })
                .unwrap_or(TAtomic::TMixed)
        }
        // Bare (non-generic) `int-mask`/`int-mask-of` are int subtypes; the
        // generic forms are handled faithfully in `generic_tree_to_type`.
        "int-mask" | "int-mask-of" => TAtomic::TInt,
        // Bare `arraylike-object` is just an object; the generic form is the
        // Traversable&ArrayAccess&Countable intersection (see generic handling).
        "arraylike-object" => TAtomic::TObject,

        "array" => match generic_params {
            Some(params) => match params.len() {
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
            },
            None => TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
        },
        "non-empty-array" => match generic_params {
            Some(params) => match params.len() {
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
            },
            None => TAtomic::TNonEmptyArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
        },
        "list" => {
            let value_type = generic_params
                .and_then(|p| p.into_iter().next())
                .unwrap_or_else(TUnion::mixed);
            TAtomic::TList {
                value_type: Box::new(value_type),
            }
        }
        "non-empty-list" => {
            let value_type = generic_params
                .and_then(|p| p.into_iter().next())
                .unwrap_or_else(TUnion::mixed);
            TAtomic::TNonEmptyList {
                value_type: Box::new(value_type),
            }
        }
        "iterable" => match generic_params {
            Some(params) => match params.len() {
                1 => TAtomic::TIterable {
                    key_type: Box::new(TUnion::array_key()),
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
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                },
            },
            None => TAtomic::TIterable {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
        },

        "class-string" | "interface-string" | "enum-string" | "trait-string" => {
            let as_type = generic_params
                .and_then(|mut params| params.drain(..).next())
                .and_then(|param| param.get_single().cloned())
                .map(Box::new);
            TAtomic::TClassString { as_type }
        }
        "callable-string" => TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        },
        "callable" => TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        },
        "pure-callable" => TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: Some(true),
        },
        "closure" | "\\closure" => TAtomic::TClosure {
            params: None,
            return_type: None,
            is_pure: None,
        },
        "pure-closure" => TAtomic::TClosure {
            params: None,
            return_type: None,
            is_pure: Some(true),
        },

        _ => {
            let name = interner.intern(raw_name.trim());

            // An in-scope template parameter (no type params) becomes a
            // TTemplateParam inline, mirroring Hakana's typehint resolver and
            // Psalm's Atomic::create template_type_map check.
            if generic_params.is_none() {
                if let Some(binding) = ctx.get_template(name) {
                    return TAtomic::TTemplateParam {
                        name: binding.name,
                        defining_entity: binding.defining_entity,
                        as_type: Box::new(binding.as_type.clone()),
                    };
                }
            }

            TAtomic::TNamedObject {
                name,
                type_params: generic_params,
                is_static: false,
                remapped_params: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docblock::parse;

    /// Test helper: parse a (valid) type string, falling back to `mixed` on the
    /// few intentionally-malformed inputs.
    fn parse_ty(type_str: &str, interner: &Interner) -> TUnion {
        parse_type_string(type_str, interner).unwrap_or_else(|_| TUnion::mixed())
    }

    // A space precedes `$string` here (as it does after multi-line docblocks are
    // collapsed to a single line), which is what trips the parser.
    const SPACED_PARAM_CONDITIONAL: &str =
        "( $string is non-empty-string ? positive-int : int )";

    #[test]
    fn parameter_conditional_errors_without_param_context() {
        // Without param context the leading space before `$string` is a stray
        // callable-param marker -> parse error (Psalm's TypeParseTreeException).
        let interner = Interner::default();
        let result = parse_type_string(SPACED_PARAM_CONDITIONAL, &interner);
        assert!(result.is_err());
    }

    #[test]
    fn parameter_conditional_parses_with_param_context() {
        // With `$string` registered as a parameter, the conditional parses
        // (flattened to the union of its branches).
        let interner = Interner::default();
        let mut ctx = TypeResolutionContext::new();
        ctx.param_names.push(interner.intern("$string"));
        let ty = parse_type_string_with_context(SPACED_PARAM_CONDITIONAL, &interner, &ctx)
            .expect("param conditional should parse with param context");
        let id = ty.get_id(Some(&interner));
        // `positive-int` lowers to a `TIntRange` (Psalm-style), displayed as
        // `int<1, max>`.
        assert!(id.contains("int<1, max>"), "unexpected: {id}");
        assert!(id.contains("int"), "unexpected: {id}");
    }

    #[test]
    fn test_parse_simple_docblock() {
        let docblock = r#"/**
         * This is the description.
         *
         * @param string $name The name
         * @return int
         */"#;

        let parsed = parse(docblock, 0);

        assert!(parsed.description.contains("This is the description"));
        assert!(parsed.tags.contains_key("param"));
        assert!(parsed.tags.contains_key("return"));
    }

    #[test]
    fn test_parse_var_tag() {
        let docblock = r#"/** @var array<int, string> $items */"#;

        let parsed = parse(docblock, 0);

        let var_content = parsed.get_var().unwrap();
        assert!(var_content.contains("array<int, string>"));
    }

    #[test]
    fn test_psalm_tag_precedence() {
        let docblock = r#"/**
         * @param string $x
         * @psalm-param non-empty-string $x
         */"#;

        let parsed = parse(docblock, 0);

        // Both should be in combined_tags
        let params: Vec<_> = parsed.get_params().collect();
        assert!(params.iter().any(|p| p.contains("non-empty-string")));
    }

    #[test]
    fn test_multiline_tag() {
        let docblock = r#"/**
         * @param array<int, array{
         *     id: int,
         *     name: string
         * }> $items
         */"#;

        let parsed = parse(docblock, 0);

        let params: Vec<_> = parsed.get_params().collect();
        assert_eq!(params.len(), 1);
        // The multi-line content should be joined
        assert!(params[0].contains("array<int, array{"));
    }

    #[test]
    fn test_parse_callable_signature_type() {
        let interner = Interner::default();
        let ty = parse_ty("callable(int, string=): bool", &interner);
        let atomic = ty.get_single().expect("single callable type");

        match atomic {
            TAtomic::TCallable {
                params: Some(params),
                return_type: Some(return_type),
                ..
            } => {
                assert_eq!(params.len(), 2);
                assert!(!params[0].is_optional);
                assert!(params[1].is_optional);
                assert!(return_type.is_single());
            }
            other => panic!("unexpected type: {:?}", other),
        }
    }

    #[test]
    fn test_parse_callable_signature_with_spaced_colon() {
        let interner = Interner::default();
        let ty = parse_ty("callable(string, string) : bool", &interner);
        let atomic = ty.get_single().expect("single callable type");

        match atomic {
            TAtomic::TCallable {
                params: Some(params),
                return_type: Some(_),
                ..
            } => {
                assert_eq!(params.len(), 2);
            }
            other => panic!("unexpected type: {:?}", other),
        }
    }

    #[test]
    fn test_parse_literal_int_union() {
        let interner = Interner::default();
        let ty = parse_ty("positive-int|0|false", &interner);

        assert!(
            ty.types
                .iter()
                .any(|t| matches!(t, TAtomic::TLiteralInt { value: 0 }))
        );
        assert!(
            !ty.types
                .iter()
                .any(|t| matches!(t, TAtomic::TNamedObject { .. }))
        );
    }

    #[test]
    fn test_parse_array_suffix_type() {
        let interner = Interner::default();
        let ty = parse_ty("string[]", &interner);
        let atomic = ty.get_single().expect("single type");

        match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            } => {
                assert!(matches!(key_type.get_single(), Some(TAtomic::TArrayKey)));
                assert!(matches!(value_type.get_single(), Some(TAtomic::TString)));
            }
            other => panic!("unexpected type: {:?}", other),
        }
    }

    #[test]
    fn test_parse_array_shape_with_implicit_list_items() {
        let interner = Interner::default();
        let ty = parse_ty("array{\"a1\", \"a2\"}", &interner);
        let atomic = ty.get_single().expect("single type");

        match atomic {
            TAtomic::TKeyedArray {
                properties,
                is_list,
                ..
            } => {
                assert!(*is_list);
                assert!(properties.contains_key(&ArrayKey::Int(0)));
                assert!(properties.contains_key(&ArrayKey::Int(1)));
            }
            other => panic!("unexpected type: {:?}", other),
        }
    }

    #[test]
    fn test_parse_int_range_with_max_bound() {
        let interner = Interner::default();
        let ty = parse_ty("int<0, max>", &interner);
        let atomic = ty.get_single().expect("single type");

        match atomic {
            TAtomic::TIntRange { min, max } => {
                assert_eq!(*min, Some(0));
                assert_eq!(*max, None);
            }
            other => panic!("unexpected type: {:?}", other),
        }
    }

    #[test]
    fn test_malformed_single_quote_type_does_not_panic() {
        let interner = Interner::default();
        let _ = parse_ty("'", &interner);
    }

    #[test]
    fn test_malformed_single_paren_type_does_not_panic() {
        let interner = Interner::default();
        let _ = parse_ty("(", &interner);
    }

    #[test]
    fn test_parse_conditional_return_type_with_func_num_args() {
        let interner = Interner::default();
        let ty = parse_ty(
            "(
                func_num_args() is 1
                ? TValue
                : TValue|TDefault
            )",
            &interner,
        );

        let ty_id = ty.get_id(Some(&interner));
        assert!(
            !ty_id.contains("func_num_args()"),
            "unexpected type id: {ty_id}"
        );
        assert!(!ty_id.contains("?"), "unexpected type id: {ty_id}");
    }

    #[test]
    fn test_parse_nested_conditional_return_type() {
        let interner = Interner::default();
        let ty = parse_ty(
            "(
                T is self::TYPE_STRING
                ? string
                : (T is self::TYPE_INT ? int : bool)
            )",
            &interner,
        );

        let ty_id = ty.get_id(Some(&interner));
        assert!(ty_id.contains("string"), "unexpected type id: {ty_id}");
        assert!(ty_id.contains("int"), "unexpected type id: {ty_id}");
        assert!(ty_id.contains("bool"), "unexpected type id: {ty_id}");
        assert!(!ty_id.contains(" is "), "unexpected type id: {ty_id}");
    }

    #[test]
    fn test_parse_never_return_aliases() {
        let interner = Interner::default();

        let never_return = parse_ty("never-return", &interner);
        assert!(matches!(never_return.get_single(), Some(TAtomic::TNothing)));

        let never_returns = parse_ty("never-returns", &interner);
        assert!(matches!(
            never_returns.get_single(),
            Some(TAtomic::TNothing)
        ));
    }

    #[test]
    fn test_extract_multiline_return_conditional_type() {
        let parsed = parse(
            r#"/**
 * @template TKey
 * @template TValue
 * @return (
 *     func_num_args() is 1
 *     ? TValue
 *     : TValue|TDefault
 * )
 */"#,
            0,
        );

        let return_content = parsed.get_return().expect("missing return tag");
        let extracted = extract_type_string_from_content(return_content).expect("missing type");

        assert!(
            extracted.contains("func_num_args() is 1"),
            "extracted: {extracted}"
        );
        assert!(extracted.contains("? TValue"), "extracted: {extracted}");
        assert!(
            extracted.contains(": TValue|TDefault"),
            "extracted: {extracted}"
        );
    }

    #[test]
    fn test_extract_return_literal_string_with_spaces() {
        let parsed = parse(
            r#"/**
 * @return "+1 day"|"+2 day"
 */"#,
            0,
        );

        let return_content = parsed.get_return().expect("missing return tag");
        let extracted = extract_type_string_from_content(return_content).expect("missing type");

        assert_eq!(extracted, "\"+1 day\"|\"+2 day\"");
    }
}
