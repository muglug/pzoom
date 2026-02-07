//! Docblock parser - extracts tags from PHPDoc comments.
//!
//! Based on Psalm's DocblockParser.php. This parser extracts the structure
//! of docblocks (description and tags) without parsing types - type parsing
//! is done separately by the analyzer.

use pzoom_code_info::t_atomic::FunctionLikeParameter;
use pzoom_code_info::{ArrayKey, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

/// A parsed docblock with description and tags.
#[derive(Debug, Clone, Default)]
pub struct ParsedDocblock {
    /// The main description text (before any tags).
    pub description: String,
    /// All extracted tags, keyed by tag name (without @).
    /// The value is a map from offset to content.
    pub tags: FxHashMap<String, FxHashMap<usize, String>>,
    /// Combined tags with precedence resolution (psalm-* > phpstan-* > standard).
    pub combined_tags: FxHashMap<String, FxHashMap<usize, String>>,
    /// The first line's leading whitespace padding (for rendering).
    pub first_line_padding: String,
}

/// Parse a docblock comment string into structured data.
///
/// Based on Psalm's DocblockParser::parse().
/// `offset_start` is the absolute position of the docblock in the file.
pub fn parse(docblock: &str, offset_start: usize) -> ParsedDocblock {
    let docblock = docblock.trim();

    // Strip off the /** prefix
    let docblock = if docblock.starts_with("/**") {
        &docblock[3..]
    } else {
        docblock
    };

    // Strip off the */ suffix
    let docblock = if docblock.ends_with("*/") {
        let s = &docblock[..docblock.len() - 2];
        // Also strip trailing * if present
        if s.ends_with('*') {
            &s[..s.len() - 1]
        } else {
            s
        }
    } else {
        docblock
    };

    // Normalize multi-line @specials
    let docblock = docblock.replace('\t', " ");
    let mut lines: Vec<(usize, String)> = docblock
        .lines()
        .enumerate()
        .map(|(i, s)| (i, s.to_string()))
        .collect();

    let has_r = docblock.contains('\r');

    let mut special: FxHashMap<String, FxHashMap<usize, String>> = FxHashMap::default();
    let mut first_line_padding = None;

    // Join continuation lines to their tag
    // First pass: identify which lines to merge
    let mut merge_info: Vec<(usize, usize)> = Vec::new(); // (continuation_idx, target_idx)
    let mut last_tag_line: Option<usize> = None;
    let mut last_tag_can_continue = false;

    for (k, (_, line)) in lines.iter().enumerate() {
        if line.contains('@') && is_tag_line(line) {
            last_tag_line = Some(k);
            last_tag_can_continue = parse_tag_line(line)
                .map(|(_, data, _)| !data.is_empty())
                .unwrap_or(false);
        } else if line.trim().is_empty() {
            last_tag_line = None;
            last_tag_can_continue = false;
        } else if let Some(last) = last_tag_line {
            if last_tag_can_continue {
                merge_info.push((k, last));
            }
        }
    }

    // Second pass: perform merges in source order so multiline tag content
    // preserves line order.
    for (cont_idx, target_idx) in &merge_info {
        let cont_line = lines[*cont_idx].1.clone();
        lines[*target_idx].1.push('\n');
        lines[*target_idx].1.push_str(&cont_line);
    }

    // Remove continuation lines (in reverse order to preserve indices)
    let to_remove: Vec<_> = merge_info.iter().map(|(k, _)| *k).collect();
    for k in to_remove.into_iter().rev() {
        lines.remove(k);
    }

    let mut line_offset = 0usize;
    let mut description_lines = Vec::new();

    for (_, line) in lines.iter_mut() {
        let original_line_length = line.len();

        if has_r {
            *line = line.replace('\r', "");
        }

        // Detect first line padding
        if first_line_padding.is_none() {
            if let Some(asterisk_pos) = line.find('*') {
                first_line_padding = Some(if asterisk_pos > 1 {
                    line[..asterisk_pos - 1].to_string()
                } else {
                    String::new()
                });
            }
        }

        // Try to parse as a tag line
        if let Some((tag_type, data, data_offset)) = parse_tag_line(line) {
            // Clean up asterisks in multi-line content
            let data = if data.contains('*') {
                clean_multiline_data(&data)
            } else {
                data
            };

            let absolute_offset = data_offset + line_offset + 3 + offset_start;

            special
                .entry(tag_type)
                .or_default()
                .insert(absolute_offset, data);
        } else {
            // Not a tag line - part of description
            let cleaned = line.trim_start_matches(|c| c == ' ' || c == '*');
            description_lines.push(cleaned.to_string());
        }

        line_offset += original_line_length + 1;
    }

    // Smush the description to the left edge
    let description = normalize_description(&description_lines);

    let mut parsed = ParsedDocblock {
        description,
        tags: special,
        combined_tags: FxHashMap::default(),
        first_line_padding: first_line_padding.unwrap_or_default(),
    };

    resolve_tags(&mut parsed);

    parsed
}

/// Check if a line looks like it starts a tag.
fn is_tag_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let trimmed = trimmed.strip_prefix('*').unwrap_or(trimmed).trim_start();
    trimmed.starts_with('@')
        && trimmed
            .chars()
            .skip(1)
            .next()
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false)
}

/// Parse a line as a tag, returning (tag_type, data, data_offset) if successful.
fn parse_tag_line(line: &str) -> Option<(String, String, usize)> {
    // Pattern: ^ *\*?\s*@([\w\-\\\:]+) *(.*)$
    let trimmed = line.trim_start();
    let trimmed = trimmed.strip_prefix('*').unwrap_or(trimmed).trim_start();

    if !trimmed.starts_with('@') {
        return None;
    }

    let rest = &trimmed[1..];

    // Find end of tag name
    let tag_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '\\' && c != ':')
        .unwrap_or(rest.len());

    if tag_end == 0 {
        return None;
    }

    let tag_type = rest[..tag_end].to_string();
    let data = rest[tag_end..].trim().to_string();

    // Calculate data offset within the line
    let data_offset = line.len() - data.len();

    Some((tag_type, data, data_offset))
}

/// Clean up asterisks in multi-line tag content.
fn clean_multiline_data(data: &str) -> String {
    data.lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "*" {
                ""
            } else {
                trimmed.strip_prefix('*').unwrap_or(trimmed).trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Normalize description lines (remove leading empty lines, normalize indent).
fn normalize_description(lines: &[String]) -> String {
    // Remove leading empty lines
    let lines: Vec<_> = lines
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .cloned()
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    // Find minimum indent
    let min_indent = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    // Remove common indent and join
    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Resolve combined tags with precedence (psalm-* > phpstan-* > standard).
fn resolve_tags(docblock: &mut ParsedDocblock) {
    // Template tags
    if docblock.tags.contains_key("template")
        || docblock.tags.contains_key("psalm-template")
        || docblock.tags.contains_key("phpstan-template")
    {
        let mut combined = FxHashMap::default();
        for key in ["template", "phpstan-template", "psalm-template"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("template".to_string(), combined);
    }

    // Template-covariant tags
    if docblock.tags.contains_key("template-covariant")
        || docblock.tags.contains_key("psalm-template-covariant")
        || docblock.tags.contains_key("phpstan-template-covariant")
    {
        let mut combined = FxHashMap::default();
        for key in [
            "template-covariant",
            "phpstan-template-covariant",
            "psalm-template-covariant",
        ] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("template-covariant".to_string(), combined);
    }

    // Extends tags
    if docblock.tags.contains_key("template-extends")
        || docblock.tags.contains_key("inherits")
        || docblock.tags.contains_key("extends")
        || docblock.tags.contains_key("psalm-extends")
        || docblock.tags.contains_key("phpstan-extends")
    {
        let mut combined = FxHashMap::default();
        for key in [
            "template-extends",
            "inherits",
            "extends",
            "phpstan-extends",
            "psalm-extends",
        ] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("extends".to_string(), combined);
    }

    // Implements tags
    if docblock.tags.contains_key("template-implements")
        || docblock.tags.contains_key("implements")
        || docblock.tags.contains_key("phpstan-implements")
        || docblock.tags.contains_key("psalm-implements")
    {
        let mut combined = FxHashMap::default();
        for key in [
            "template-implements",
            "implements",
            "phpstan-implements",
            "psalm-implements",
        ] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("implements".to_string(), combined);
    }

    // Use tags (for traits)
    if docblock.tags.contains_key("template-use")
        || docblock.tags.contains_key("use")
        || docblock.tags.contains_key("phpstan-use")
        || docblock.tags.contains_key("psalm-use")
    {
        let mut combined = FxHashMap::default();
        for key in ["template-use", "use", "phpstan-use", "psalm-use"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock.combined_tags.insert("use".to_string(), combined);
    }

    // Mixin tags
    if docblock.tags.contains_key("mixin")
        || docblock.tags.contains_key("phpstan-mixin")
        || docblock.tags.contains_key("psalm-mixin")
    {
        let mut combined = FxHashMap::default();
        for key in ["mixin", "phpstan-mixin", "psalm-mixin"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock.combined_tags.insert("mixin".to_string(), combined);
    }

    // Method tags
    if docblock.tags.contains_key("method") || docblock.tags.contains_key("psalm-method") {
        let mut combined = FxHashMap::default();
        for key in ["method", "psalm-method"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("method".to_string(), combined);
    }

    // Property tags
    if docblock.tags.contains_key("property")
        || docblock.tags.contains_key("psalm-property")
        || docblock.tags.contains_key("phpstan-property")
    {
        let mut combined = FxHashMap::default();
        for key in ["property", "phpstan-property", "psalm-property"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("property".to_string(), combined);
    }

    if docblock.tags.contains_key("property-read")
        || docblock.tags.contains_key("psalm-property-read")
        || docblock.tags.contains_key("phpstan-property-read")
    {
        let mut combined = FxHashMap::default();
        for key in [
            "property-read",
            "phpstan-property-read",
            "psalm-property-read",
        ] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("property-read".to_string(), combined);
    }

    if docblock.tags.contains_key("property-write")
        || docblock.tags.contains_key("psalm-property-write")
        || docblock.tags.contains_key("phpstan-property-write")
    {
        let mut combined = FxHashMap::default();
        for key in [
            "property-write",
            "phpstan-property-write",
            "psalm-property-write",
        ] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("property-write".to_string(), combined);
    }

    // Return tags (psalm-return takes precedence)
    if docblock.tags.contains_key("return")
        || docblock.tags.contains_key("psalm-return")
        || docblock.tags.contains_key("phpstan-return")
    {
        let combined = if let Some(tags) = docblock.tags.get("psalm-return") {
            tags.clone()
        } else if let Some(tags) = docblock.tags.get("phpstan-return") {
            tags.clone()
        } else if let Some(tags) = docblock.tags.get("return") {
            tags.clone()
        } else {
            FxHashMap::default()
        };
        docblock
            .combined_tags
            .insert("return".to_string(), combined);
    }

    // Param tags
    if docblock.tags.contains_key("param")
        || docblock.tags.contains_key("psalm-param")
        || docblock.tags.contains_key("phpstan-param")
    {
        let mut combined = FxHashMap::default();
        for key in ["param", "phpstan-param", "psalm-param"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock.combined_tags.insert("param".to_string(), combined);
    }

    // Var tags (check for ignore-var first)
    if (docblock.tags.contains_key("var")
        || docblock.tags.contains_key("psalm-var")
        || docblock.tags.contains_key("phpstan-var"))
        && !docblock.tags.contains_key("ignore-var")
        && !docblock.tags.contains_key("psalm-ignore-var")
    {
        let mut combined = FxHashMap::default();
        for key in ["var", "phpstan-var", "psalm-var"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock.combined_tags.insert("var".to_string(), combined);
    }

    // Param-out tags
    if docblock.tags.contains_key("param-out")
        || docblock.tags.contains_key("psalm-param-out")
        || docblock.tags.contains_key("phpstan-param-out")
    {
        let mut combined = FxHashMap::default();
        for key in ["param-out", "phpstan-param-out", "psalm-param-out"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock
            .combined_tags
            .insert("param-out".to_string(), combined);
    }
}

impl ParsedDocblock {
    /// Render the docblock back to a string.
    pub fn render(&self, left_padding: &str) -> String {
        let mut doc_comment_text = String::from("/**\n");

        let trimmed_description = self.description.trim();

        if !trimmed_description.is_empty() {
            for line in self.description.lines() {
                let trimmed_line = line.trim();
                if trimmed_line.is_empty() {
                    doc_comment_text.push_str(left_padding);
                    doc_comment_text.push_str(" *\n");
                } else {
                    doc_comment_text.push_str(left_padding);
                    doc_comment_text.push_str(" * ");
                    doc_comment_text.push_str(line);
                    doc_comment_text.push('\n');
                }
            }
        }

        if !self.tags.is_empty() {
            if !trimmed_description.is_empty() {
                doc_comment_text.push_str(left_padding);
                doc_comment_text.push_str(" *\n");
            }

            let mut last_type: Option<&str> = None;

            for (tag_type, lines) in &self.tags {
                if last_type.is_some() && last_type != Some("psalm-return") {
                    doc_comment_text.push_str(left_padding);
                    doc_comment_text.push_str(" *\n");
                }

                for (_, line) in lines {
                    doc_comment_text.push_str(left_padding);
                    doc_comment_text.push_str(" * @");
                    doc_comment_text.push_str(tag_type);
                    if !line.is_empty() {
                        doc_comment_text.push(' ');
                        doc_comment_text.push_str(line);
                    }
                    doc_comment_text.push('\n');
                }

                last_type = Some(tag_type);
            }
        }

        doc_comment_text.push_str(left_padding);
        doc_comment_text.push_str(" */\n");
        doc_comment_text.push_str(left_padding);

        doc_comment_text
    }

    /// Get the first var tag content.
    pub fn get_var(&self) -> Option<&str> {
        self.combined_tags
            .get("var")
            .and_then(|m| m.values().next())
            .map(|s| s.as_str())
    }

    /// Get the first return tag content.
    pub fn get_return(&self) -> Option<&str> {
        self.combined_tags
            .get("return")
            .and_then(|m| m.values().next())
            .map(|s| s.as_str())
    }

    /// Get all param tag contents.
    pub fn get_params(&self) -> impl Iterator<Item = &str> {
        self.combined_tags
            .get("param")
            .into_iter()
            .flat_map(|m| m.values())
            .map(|s| s.as_str())
    }
}

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

/// Parse a type string into a TUnion.
/// This is public so it can be used by declaration_collector.
pub fn parse_type_string(type_str: &str, interner: &Interner) -> TUnion {
    let mut parsed = parse_simple_type(type_str, interner);
    parsed.from_docblock = true;
    parsed
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
    lowered.starts_with("is ")
        || lowered.starts_with("as ")
        || lowered.starts_with("of ")
        || lowered.starts_with("extends ")
        || lowered.starts_with("super ")
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
fn parse_simple_type(type_str: &str, interner: &Interner) -> TUnion {
    let mut trimmed = type_str.trim();
    if trimmed.is_empty() {
        return TUnion::mixed();
    }

    // Strip full wrapping parentheses to simplify nested conditional parsing.
    while let Some(stripped) = strip_wrapping_parentheses(trimmed) {
        trimmed = stripped.trim();
    }

    // Handle nullable
    if let Some(inner) = trimmed.strip_prefix('?') {
        let mut result = parse_simple_type(inner, interner);
        result.add_type(TAtomic::TNull);
        return result;
    }

    if let Some(utility_union) = parse_special_utility_union(trimmed, interner) {
        return utility_union;
    }

    // Handle conditional types by combining both branches.
    // E.g. "(T is non-empty-array ? non-empty-list<key-of<T>> : list<key-of<T>>)".
    if let Some((if_true, if_false)) = split_conditional_at_depth_zero(trimmed) {
        let true_type = parse_simple_type(if_true, interner);
        let false_type = parse_simple_type(if_false, interner);

        let mut combined = true_type.types;
        for atomic in false_type.types {
            if !combined.contains(&atomic) {
                combined.push(atomic);
            }
        }

        return TUnion::from_types(combined);
    }

    // Handle union types (but not inside generics)
    if let Some(union_parts) = split_union_at_depth_zero(trimmed) {
        let mut types = Vec::new();
        for part in union_parts {
            let part_union = parse_simple_type(part.trim(), interner);
            for atomic in part_union.types {
                if !types.contains(&atomic) {
                    types.push(atomic);
                }
            }
        }

        return TUnion::from_types(types);
    }

    // Handle intersection types (but not inside generics)
    if let Some(intersection_parts) = split_intersection_at_depth_zero(trimmed) {
        let mut intersection_types = Vec::new();

        for part in intersection_parts {
            let part_union = parse_simple_type(part.trim(), interner);
            let mut part_iter = part_union.types.into_iter();
            let Some(part_atomic) = part_iter.next() else {
                continue;
            };

            if part_iter.next().is_some() {
                continue;
            }

            match part_atomic {
                TAtomic::TObjectIntersection { types } => {
                    for nested_type in types {
                        if !intersection_types.contains(&nested_type) {
                            intersection_types.push(nested_type);
                        }
                    }
                }
                _ => {
                    if !intersection_types.contains(&part_atomic) {
                        intersection_types.push(part_atomic);
                    }
                }
            }
        }

        return match intersection_types.len() {
            0 => TUnion::mixed(),
            1 => TUnion::new(intersection_types.pop().unwrap()),
            _ => TUnion::new(TAtomic::TObjectIntersection {
                types: intersection_types,
            }),
        };
    }

    // Single type
    TUnion::new(parse_atomic_type(trimmed, interner))
}

fn parse_special_utility_union(type_str: &str, interner: &Interner) -> Option<TUnion> {
    let start_idx = type_str.find('<')?;
    let base_name = type_str[..start_idx].trim().to_ascii_lowercase();
    if base_name != "key-of" && base_name != "value-of" {
        return None;
    }

    let after_open = &type_str[start_idx + 1..];
    let end_idx = find_matching_close(after_open)?;
    if !after_open[end_idx + 1..].trim().is_empty() {
        return None;
    }

    let params = split_generic_params(&after_open[..end_idx], interner);
    let first_param = params.first();

    Some(match (base_name.as_str(), first_param) {
        ("key-of", Some(param)) => resolve_key_of_union_to_union(param),
        ("value-of", Some(param)) => resolve_value_of_union_to_union(param),
        ("key-of", None) => TUnion::array_key(),
        ("value-of", None) => TUnion::mixed(),
        _ => return None,
    })
}

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

fn split_conditional_at_depth_zero(s: &str) -> Option<(&str, &str)> {
    split_conditional_parts_at_depth_zero(s).map(|(_, if_true, if_false)| (if_true, if_false))
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
fn split_union_at_depth_zero(s: &str) -> Option<Vec<&str>> {
    let mut depth: u32 = 0;
    let mut parts = Vec::new();
    let mut start = 0;
    let mut found_union = false;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
                found_union = true;
            }
            _ => {}
        }
    }

    if found_union {
        parts.push(&s[start..]);
        Some(parts)
    } else {
        None
    }
}

/// Split a type string by & at depth 0.
fn split_intersection_at_depth_zero(s: &str) -> Option<Vec<&str>> {
    let mut depth: u32 = 0;
    let mut parts = Vec::new();
    let mut start = 0;
    let mut found_intersection = false;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '&' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
                found_intersection = true;
            }
            _ => {}
        }
    }

    if found_intersection {
        parts.push(&s[start..]);
        Some(parts)
    } else {
        None
    }
}

fn parse_callable_type(type_str: &str, interner: &Interner) -> Option<TAtomic> {
    let trimmed = type_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (is_closure, is_pure, rest) =
        if let Some(rest) = strip_case_insensitive_prefix(trimmed, "callable") {
            if rest.starts_with('-') {
                return None;
            }
            (false, false, rest)
        } else if let Some(rest) = strip_case_insensitive_prefix(trimmed, "pure-callable") {
            (false, true, rest)
        } else if let Some(rest) = strip_case_insensitive_prefix(trimmed, "closure") {
            (true, false, rest)
        } else if let Some(rest) = strip_case_insensitive_prefix(trimmed, "\\closure") {
            (true, false, rest)
        } else if let Some(rest) = strip_case_insensitive_prefix(trimmed, "pure-closure") {
            (true, true, rest)
        } else {
            return None;
        };

    let rest = rest.trim_start();
    if rest.is_empty() {
        return Some(if is_closure {
            TAtomic::TClosure {
                params: None,
                return_type: None,
                is_pure: if is_pure { Some(true) } else { None },
            }
        } else {
            TAtomic::TCallable {
                params: None,
                return_type: None,
                is_pure: if is_pure { Some(true) } else { None },
            }
        });
    }

    if !rest.starts_with('(') {
        return None;
    }

    let close_idx = find_matching_parenthesis(rest)?;
    let params = parse_callable_params(&rest[1..close_idx], interner);
    let trailing = rest[close_idx + 1..].trim();

    let return_type = if trailing.is_empty() {
        None
    } else if let Some(after_colon) = trailing.strip_prefix(':') {
        let parsed = after_colon.trim();
        if parsed.is_empty() {
            None
        } else {
            Some(Box::new(parse_simple_type(parsed, interner)))
        }
    } else {
        return None;
    };

    Some(if is_closure {
        TAtomic::TClosure {
            params: Some(params),
            return_type,
            is_pure: if is_pure { Some(true) } else { None },
        }
    } else {
        TAtomic::TCallable {
            params: Some(params),
            return_type,
            is_pure: if is_pure { Some(true) } else { None },
        }
    })
}

fn parse_callable_params(params: &str, interner: &Interner) -> Vec<FunctionLikeParameter> {
    let params = params.trim();
    if params.is_empty() {
        return vec![];
    }

    split_top_level(params, ',')
        .into_iter()
        .filter_map(|raw_param| {
            let mut raw = raw_param.trim();
            if raw.is_empty() {
                return None;
            }

            let mut by_ref = false;
            let mut is_variadic = false;
            let mut is_optional = false;

            if let Some(stripped) = raw.strip_suffix('=') {
                raw = stripped.trim_end();
                is_optional = true;
            }

            if let Some(stripped) = raw.strip_prefix('&') {
                raw = stripped.trim_start();
                by_ref = true;
            }

            if let Some(stripped) = raw.strip_prefix("...") {
                raw = stripped.trim_start();
                is_variadic = true;
            }

            let (mut type_part, name) = split_type_and_param_name(raw);

            if let Some(stripped) = type_part.strip_suffix("...") {
                type_part = stripped.trim_end();
                is_variadic = true;
            }

            if let Some(stripped) = type_part.strip_suffix('&') {
                type_part = stripped.trim_end();
                by_ref = true;
            }

            let param_type = if type_part.is_empty() {
                TUnion::mixed()
            } else {
                parse_simple_type(type_part, interner)
            };

            Some(FunctionLikeParameter {
                name: name.map(|n| interner.intern(n)),
                param_type,
                is_optional,
                is_variadic,
                by_ref,
            })
        })
        .collect()
}

fn split_type_and_param_name(param: &str) -> (&str, Option<&str>) {
    let mut depth: u32 = 0;

    for (idx, ch) in param.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                let name_start = idx + 1;
                let mut name_end = name_start;

                for (name_idx, name_ch) in param[name_start..].char_indices() {
                    if is_param_name_char(name_ch) {
                        name_end = name_start + name_idx + name_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                if name_end == name_start {
                    return (param[..idx].trim_end(), None);
                }

                return (
                    param[..idx].trim_end(),
                    Some(param[name_start..name_end].trim_end()),
                );
            }
            _ => {}
        }
    }

    (param.trim(), None)
}

fn split_top_level(s: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            c if c == delimiter && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&s[start..]);
    parts
}

fn find_matching_parenthesis(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_case_insensitive_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() < prefix.len() {
        return None;
    }

    if s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

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

fn is_param_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn parse_atomic_type(type_str: &str, interner: &Interner) -> TAtomic {
    if let Some(callable_type) = parse_callable_type(type_str, interner) {
        return callable_type;
    }

    if let Some(shape_type) = parse_array_shape_type(type_str, interner) {
        return shape_type;
    }

    if let Some(inner) = type_str.strip_suffix("[]") {
        let value_type = parse_simple_type(inner, interner);
        return TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(value_type),
        };
    }

    // String literal types.
    if let Some(value) = strip_wrapping_quotes(type_str) {
        return TAtomic::TLiteralString { value };
    }

    // Numeric literal types.
    if let Ok(value) = type_str.parse::<i64>() {
        return TAtomic::TLiteralInt { value };
    }
    if let Ok(value) = type_str.parse::<f64>() {
        if type_str.contains('.') || type_str.contains('e') || type_str.contains('E') {
            return TAtomic::TLiteralFloat { value };
        }
    }

    if let Some(class_name) = strip_case_insensitive_suffix(type_str, "::class") {
        let class_name = class_name.trim();
        if class_name.is_empty() {
            return TAtomic::TClassString { as_type: None };
        }

        let parsed = parse_simple_type(class_name, interner);
        if let Some(single_atomic) = parsed.get_single().cloned() {
            return match single_atomic {
                TAtomic::TLiteralString { value } => TAtomic::TLiteralClassString { name: value },
                TAtomic::TLiteralClassString { name } => TAtomic::TLiteralClassString { name },
                TAtomic::TNamedObject {
                    name,
                    type_params: None,
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

        return TAtomic::TClassString { as_type: None };
    }

    let lower = type_str.to_lowercase();

    // Check for generic syntax
    let (base_name, generic_params, generic_param_parts) =
        if let Some(start_idx) = type_str.find('<') {
            let base = &type_str[..start_idx];

            let after_open = &type_str[start_idx + 1..];
            if let Some(end_idx) = find_matching_close(after_open) {
                let params_inner = &after_open[..end_idx];
                let params = split_generic_params(params_inner, interner);
                let parts = split_generic_param_parts(params_inner);
                (base.to_lowercase(), Some(params), Some(parts))
            } else {
                (lower.clone(), None, None)
            }
        } else {
            (lower.clone(), None, None)
        };

    match base_name.as_str() {
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
        "resource" => TAtomic::TResource,
        "closed-resource" => TAtomic::TClosedResource,
        "never" | "no-return" | "never-return" | "never-returns" => TAtomic::TNothing,

        // Scalar refinements.
        "array-key" => TAtomic::TArrayKey,
        "scalar" => TAtomic::TScalar,
        "numeric" => TAtomic::TNumeric,
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
        "non-empty-string" => TAtomic::TNonEmptyString,
        "numeric-string" => TAtomic::TNumericString,
        "lowercase-string" => TAtomic::TLowercaseString,
        "non-empty-lowercase-string" => TAtomic::TNonEmptyLowercaseString,
        "literal-string" | "non-empty-literal-string" => TAtomic::TLiteralString {
            value: pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
        },
        "truthy-string" | "non-falsy-string" => TAtomic::TTruthyString,

        // Utility types.
        "key-of" => generic_params
            .as_ref()
            .and_then(|params| params.first())
            .map(resolve_key_of_union)
            .unwrap_or(TAtomic::TArrayKey),
        "value-of" => generic_params
            .as_ref()
            .and_then(|params| params.first())
            .map(resolve_value_of_union)
            .unwrap_or(TAtomic::TMixed),
        "class-string-map" => {
            if let (Some(params), Some(raw_parts)) =
                (generic_params.as_ref(), generic_param_parts.as_ref())
                && let Some((template_name, template_bound)) = raw_parts
                    .first()
                    .and_then(|raw| split_template_constraint(raw))
            {
                let template_name = template_name.trim();
                let template_name_id = interner.intern(template_name);
                let template_bound = parse_simple_type(template_bound.trim(), interner);
                let template_atomic = TAtomic::TTemplateParam {
                    name: template_name_id,
                    defining_entity: StrId::EMPTY,
                    as_type: Box::new(template_bound.clone()),
                };

                let key_type = TUnion::new(TAtomic::TClassString {
                    as_type: Some(Box::new(template_atomic.clone())),
                });
                let value_type = params
                    .get(1)
                    .map(|value_param| {
                        replace_template_named_object_in_union(
                            value_param,
                            template_name,
                            &template_atomic,
                            interner,
                        )
                    })
                    .unwrap_or_else(TUnion::mixed);

                return TAtomic::TArray {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                };
            }

            let (key_type, value_type) = if let Some(params) = generic_params {
                match params.len() {
                    0 => (
                        TUnion::new(TAtomic::TClassString { as_type: None }),
                        TUnion::mixed(),
                    ),
                    1 => (
                        normalize_class_string_key_union(&params[0]),
                        TUnion::mixed(),
                    ),
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
        "properties-of"
        | "public-properties-of"
        | "protected-properties-of"
        | "private-properties-of"
        | "int-mask"
        | "int-mask-of"
        | "arraylike-object" => TAtomic::TMixed,

        // Array-ish.
        "array" => {
            if let Some(params) = generic_params {
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
            if let Some(params) = generic_params {
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
        "iterable" => {
            if let Some(params) = generic_params {
                match params.len() {
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
                }
            } else {
                TAtomic::TIterable {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                }
            }
        }

        // String/class helpers.
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

        _ => {
            // Named object (class/interface)
            let mut name = type_str;
            if generic_params.is_some() {
                name = &name[..name.find('<').unwrap_or(name.len())];
            }
            if let Some(indexed_access_type) = parse_indexed_access_type(type_str, interner) {
                return indexed_access_type;
            }
            TAtomic::TNamedObject {
                name: interner.intern(name.trim()),
                type_params: generic_params,
            }
        }
    }
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

fn parse_array_shape_type(type_str: &str, interner: &Interner) -> Option<TAtomic> {
    let trimmed = type_str.trim();
    let (is_list, inner) = if let Some(rest) = strip_case_insensitive_prefix(trimmed, "array{") {
        (false, rest)
    } else if let Some(rest) = strip_case_insensitive_prefix(trimmed, "list{") {
        (true, rest)
    } else {
        return None;
    };

    if !inner.ends_with('}') {
        return None;
    }

    let inner = &inner[..inner.len() - 1];
    let mut properties = FxHashMap::default();
    let mut fallback_key_type: Option<Box<TUnion>> = None;
    let mut fallback_value_type: Option<Box<TUnion>> = None;
    let mut sealed = true;
    let mut next_list_index = 0_i64;
    let mut has_implicit_list_fields = false;

    for field in split_shape_fields(inner) {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }

        if field == "..." {
            sealed = false;
            fallback_key_type = Some(Box::new(TUnion::array_key()));
            fallback_value_type = Some(Box::new(TUnion::mixed()));
            continue;
        }

        if is_list {
            if let Some((key_part, value_part)) = split_shape_key_value(field) {
                let key = parse_shape_key(key_part.trim())?;
                let mut value_type = parse_simple_type(value_part.trim(), interner);
                if key_part.trim_end().ends_with('?') {
                    value_type.possibly_undefined = true;
                }
                properties.insert(key, value_type);
            } else {
                let value_type = parse_simple_type(field, interner);
                properties.insert(ArrayKey::Int(next_list_index), value_type);
                next_list_index += 1;
            }
            continue;
        }

        let Some((key_part, value_part)) = split_shape_key_value(field) else {
            let value_type = parse_simple_type(field, interner);
            properties.insert(ArrayKey::Int(next_list_index), value_type);
            next_list_index += 1;
            has_implicit_list_fields = true;
            continue;
        };

        let mut key_part = key_part.trim();
        let mut optional = false;
        if let Some(stripped) = key_part.strip_suffix('?') {
            key_part = stripped.trim();
            optional = true;
        }

        let key = parse_shape_key(key_part)?;
        let mut value_type = parse_simple_type(value_part.trim(), interner);
        if optional {
            value_type.possibly_undefined = true;
        }
        properties.insert(key, value_type);
    }

    let resolved_is_list =
        (is_list || has_implicit_list_fields) && keyed_array_properties_form_list(&properties);

    Some(TAtomic::TKeyedArray {
        properties,
        is_list: resolved_is_list,
        sealed,
        fallback_key_type,
        fallback_value_type,
    })
}

fn split_shape_fields(s: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut string_char: Option<char> = None;
    let mut escape = false;

    for (idx, ch) in s.char_indices() {
        if let Some(active_quote) = string_char {
            if ch == active_quote && !escape {
                string_char = None;
            }
            if ch == '\\' {
                escape = !escape;
            } else {
                escape = false;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                string_char = Some(ch);
                escape = false;
            }
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(s[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }

    if start <= s.len() {
        fields.push(s[start..].trim());
    }

    fields
}

fn looks_like_indexed_access_type(type_name: &str) -> bool {
    let Some(open_idx) = type_name.find('[') else {
        return false;
    };

    type_name.ends_with(']') && open_idx > 0 && open_idx + 1 < type_name.len() - 1
}

fn parse_indexed_access_type(type_str: &str, interner: &Interner) -> Option<TAtomic> {
    let trimmed = type_str.trim();
    if !looks_like_indexed_access_type(trimmed) {
        return None;
    }

    let open_idx = trimmed.find('[')?;
    let array_fragment = trimmed[..open_idx].trim();
    let offset_fragment = trimmed[open_idx + 1..trimmed.len() - 1].trim();

    if array_fragment.is_empty() || offset_fragment.is_empty() {
        return None;
    }

    let array_type = parse_simple_type(array_fragment, interner);
    let offset_type = parse_simple_type(offset_fragment, interner);

    Some(TAtomic::TNamedObject {
        name: StrId::PZOOM_INDEXED_ACCESS,
        type_params: Some(vec![array_type, offset_type]),
    })
}

fn split_shape_key_value(field: &str) -> Option<(&str, &str)> {
    let mut depth: i32 = 0;
    let mut string_char: Option<char> = None;
    let mut escape = false;

    for (idx, ch) in field.char_indices() {
        if let Some(active_quote) = string_char {
            if ch == active_quote && !escape {
                string_char = None;
            }
            if ch == '\\' {
                escape = !escape;
            } else {
                escape = false;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                string_char = Some(ch);
                escape = false;
            }
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth -= 1,
            ':' if depth == 0 => {
                let key = field[..idx].trim();
                let value = field[idx + 1..].trim();
                if key.is_empty() || value.is_empty() {
                    return None;
                }
                return Some((key, value));
            }
            _ => {}
        }
    }

    None
}

fn parse_shape_key(key: &str) -> Option<ArrayKey> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(inner) = strip_wrapping_quotes(trimmed) {
        return Some(ArrayKey::String(inner));
    }

    if let Ok(int_key) = trimmed.parse::<i64>() {
        return Some(ArrayKey::Int(int_key));
    }

    Some(ArrayKey::String(trimmed.to_string()))
}

fn keyed_array_properties_form_list(properties: &FxHashMap<ArrayKey, TUnion>) -> bool {
    if properties.is_empty() {
        return true;
    }

    let mut int_keys = Vec::with_capacity(properties.len());
    for key in properties.keys() {
        let ArrayKey::Int(value) = key else {
            return false;
        };
        if *value < 0 {
            return false;
        }
        int_keys.push(*value);
    }

    int_keys.sort_unstable();
    for (idx, value) in int_keys.iter().enumerate() {
        if *value != idx as i64 {
            return false;
        }
    }

    true
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
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
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
fn find_matching_close(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split generic parameters by comma at depth 0.
fn split_generic_params(s: &str, interner: &Interner) -> Vec<TUnion> {
    let mut params = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let param = s[start..i].trim();
                if !param.is_empty() {
                    params.push(parse_simple_type(param, interner));
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last parameter
    let param = s[start..].trim();
    if !param.is_empty() {
        params.push(parse_simple_type(param, interner));
    }

    params
}

fn split_generic_param_parts(s: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut depth: u32 = 0;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let param = s[start..i].trim();
                if !param.is_empty() {
                    params.push(param.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let param = s[start..].trim();
    if !param.is_empty() {
        params.push(param.to_string());
    }

    params
}

fn split_template_constraint(s: &str) -> Option<(&str, &str)> {
    let lower = s.to_ascii_lowercase();

    if let Some(idx) = lower.find(" as ") {
        let left = s[..idx].trim();
        let right = s[idx + 4..].trim();
        if !left.is_empty() && !right.is_empty() {
            return Some((left, right));
        }
    }

    if let Some(idx) = lower.find(" of ") {
        let left = s[..idx].trim();
        let right = s[idx + 4..].trim();
        if !left.is_empty() && !right.is_empty() {
            return Some((left, right));
        }
    }

    None
}

fn replace_template_named_object_in_union(
    union: &TUnion,
    template_name: &str,
    template_atomic: &TAtomic,
    interner: &Interner,
) -> TUnion {
    let mut replaced = union.clone();
    for atomic in &mut replaced.types {
        replace_template_named_object_in_atomic(atomic, template_name, template_atomic, interner);
    }
    replaced
}

fn replace_template_named_object_in_atomic(
    atomic: &mut TAtomic,
    template_name: &str,
    template_atomic: &TAtomic,
    interner: &Interner,
) {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            if type_params.is_none() && interner.lookup(*name).as_ref() == template_name {
                *atomic = template_atomic.clone();
                return;
            }

            if let Some(type_params) = type_params {
                for param in type_params {
                    *param = replace_template_named_object_in_union(
                        param,
                        template_name,
                        template_atomic,
                        interner,
                    );
                }
            }
        }
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            **key_type = replace_template_named_object_in_union(
                key_type,
                template_name,
                template_atomic,
                interner,
            );
            **value_type = replace_template_named_object_in_union(
                value_type,
                template_name,
                template_atomic,
                interner,
            );
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            **value_type = replace_template_named_object_in_union(
                value_type,
                template_name,
                template_atomic,
                interner,
            );
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => replace_template_named_object_in_atomic(
            as_type,
            template_name,
            template_atomic,
            interner,
        ),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let ty = parse_type_string("callable(int, string=): bool", &interner);
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
        let ty = parse_type_string("callable(string, string) : bool", &interner);
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
        let ty = parse_type_string("positive-int|0|false", &interner);

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
        let ty = parse_type_string("string[]", &interner);
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
        let ty = parse_type_string("array{\"a1\", \"a2\"}", &interner);
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
        let ty = parse_type_string("int<0, max>", &interner);
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
        let _ = parse_type_string("'", &interner);
    }

    #[test]
    fn test_malformed_single_paren_type_does_not_panic() {
        let interner = Interner::default();
        let _ = parse_type_string("(", &interner);
    }

    #[test]
    fn test_parse_conditional_return_type_with_func_num_args() {
        let interner = Interner::default();
        let ty = parse_type_string(
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
        let ty = parse_type_string(
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

        let never_return = parse_type_string("never-return", &interner);
        assert!(matches!(never_return.get_single(), Some(TAtomic::TNothing)));

        let never_returns = parse_type_string("never-returns", &interner);
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
