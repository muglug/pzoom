//! Docblock parser - extracts tags from PHPDoc comments.
//!
//! Based on Psalm's DocblockParser.php. This parser extracts the structure
//! of docblocks (description and tags) without parsing types - type parsing
//! is done separately by the analyzer.

use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::Interner;
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

    for (k, (_, line)) in lines.iter().enumerate() {
        if line.contains('@') && is_tag_line(line) {
            last_tag_line = Some(k);
        } else if line.trim().is_empty() {
            last_tag_line = None;
        } else if let Some(last) = last_tag_line {
            merge_info.push((k, last));
        }
    }

    // Second pass: perform merges (in reverse to preserve indices)
    for (cont_idx, target_idx) in merge_info.iter().rev() {
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
        docblock.combined_tags.insert("template".to_string(), combined);
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
        docblock.combined_tags.insert("extends".to_string(), combined);
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

    // Method tags
    if docblock.tags.contains_key("method") || docblock.tags.contains_key("psalm-method") {
        let mut combined = FxHashMap::default();
        for key in ["method", "psalm-method"] {
            if let Some(tags) = docblock.tags.get(key) {
                combined.extend(tags.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        docblock.combined_tags.insert("method".to_string(), combined);
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
        docblock.combined_tags.insert("return".to_string(), combined);
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

/// Parse a type string into a TUnion.
/// This is public so it can be used by declaration_collector.
pub fn parse_type_string(type_str: &str, interner: &Interner) -> TUnion {
    parse_simple_type(type_str, interner)
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

    for (i, ch) in trimmed.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                end_idx = i;
                break;
            }
            ' ' | '\t' if depth == 0 => {
                let remaining = trimmed[i..].trim_start();
                if remaining.starts_with('$') || remaining.is_empty() {
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

/// Parse a simple type string into a TUnion.
/// This is a simplified parser for use during scanning.
fn parse_simple_type(type_str: &str, interner: &Interner) -> TUnion {
    let trimmed = type_str.trim();
    if trimmed.is_empty() {
        return TUnion::mixed();
    }

    // Handle nullable
    if let Some(inner) = trimmed.strip_prefix('?') {
        let mut result = parse_simple_type(inner, interner);
        result.add_type(TAtomic::TNull);
        return result;
    }

    // Handle union types (but not inside generics)
    if let Some(union_parts) = split_union_at_depth_zero(trimmed) {
        let types: Vec<TAtomic> = union_parts
            .iter()
            .map(|part| parse_atomic_type(part.trim(), interner))
            .collect();
        return TUnion::from_types(types);
    }

    // Single type
    TUnion::new(parse_atomic_type(trimmed, interner))
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

fn parse_atomic_type(type_str: &str, interner: &Interner) -> TAtomic {
    let lower = type_str.to_lowercase();

    // Check for generic syntax
    let (base_name, generic_params) = if let Some(start_idx) = type_str.find('<') {
        let base = &type_str[..start_idx];

        let after_open = &type_str[start_idx + 1..];
        if let Some(end_idx) = find_matching_close(after_open) {
            let params_inner = &after_open[..end_idx];
            let params = split_generic_params(params_inner, interner);
            (base.to_lowercase(), Some(params))
        } else {
            (lower.clone(), None)
        }
    } else {
        (lower.clone(), None)
    };

    match base_name.as_str() {
        "int" | "integer" => TAtomic::TInt,
        "float" | "double" => TAtomic::TFloat,
        "string" => TAtomic::TString,
        "bool" | "boolean" => TAtomic::TBool,
        "true" => TAtomic::TTrue,
        "false" => TAtomic::TFalse,
        "null" => TAtomic::TNull,
        "void" => TAtomic::TVoid,
        "mixed" => TAtomic::TMixed,
        "object" => TAtomic::TObject,
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
        "list" => {
            let value_type = generic_params
                .and_then(|p| p.into_iter().next())
                .unwrap_or_else(TUnion::mixed);
            TAtomic::TList {
                value_type: Box::new(value_type),
            }
        }
        "iterable" => TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        },
        "callable" => TAtomic::TCallable {
            params: None,
            return_type: None,
        },
        "resource" => TAtomic::TResource,
        "never" | "no-return" => TAtomic::TNothing,
        _ => {
            // Named object (class/interface)
            let name = if type_str.starts_with('\\') {
                &type_str[1..]
            } else if generic_params.is_some() {
                &type_str[..type_str.find('<').unwrap_or(type_str.len())]
            } else {
                type_str
            };
            TAtomic::TNamedObject {
                name: interner.intern(name),
                type_params: generic_params,
            }
        }
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
}
