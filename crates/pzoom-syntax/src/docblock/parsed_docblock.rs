//! Docblock comment parsing - extracts description and tags from PHPDoc comments.
//!
//! Mirrors Psalm's `DocblockParser.php` / `ParsedDocblock.php`: it extracts the
//! structure of a docblock (description and tags) without parsing types. Type
//! parsing lives in [`super::type_parser`].

use rustc_hash::FxHashMap;

/// Extracts the `$name` of a `@param`-style tag's content, skipping over any
/// generic/shape/callable type syntax (`<>`, `{}`, `()`) so a `$` inside the
/// type isn't mistaken for the parameter variable. Returns `None` when the tag
/// has no variable (e.g. an anonymous `@param SomeType`).
fn extract_param_tag_name(content: &str) -> Option<String> {
    let mut depth: u32 = 0;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                let start = idx + 1;
                let mut end = start;
                for (name_idx, name_ch) in content[start..].char_indices() {
                    if name_ch.is_ascii_alphanumeric() || name_ch == '_' {
                        end = start + name_idx + name_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if end > start {
                    return Some(content[start..end].to_string());
                }
                return None;
            }
            _ => {}
        }
    }

    None
}

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
    let docblock = if let Some(s) = docblock.strip_suffix("*/") {
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
        } else if let Some(last) = last_tag_line
            && last_tag_can_continue {
                merge_info.push((k, last));
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
        if first_line_padding.is_none()
            && let Some(asterisk_pos) = line.find('*') {
                first_line_padding = Some(if asterisk_pos > 1 {
                    line[..asterisk_pos - 1].to_string()
                } else {
                    String::new()
                });
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
            let cleaned = line.trim_start_matches([' ', '*']);
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
            .chars().nth(1)
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false)
}

/// Parse a line as a tag, returning (tag_type, data, data_offset) if successful.
fn parse_tag_line(line: &str) -> Option<(String, String, usize)> {
    // Pattern: ^ *\*?\s*@([\w\-\\\:]+) *(.*)$
    let mut idx = line.len() - line.trim_start().len();
    let mut rest = &line[idx..];
    if rest.starts_with('*') {
        idx += 1;
        rest = &line[idx..];
        idx += rest.len() - rest.trim_start().len();
        rest = &line[idx..];
    }

    if !rest.starts_with('@') {
        return None;
    }

    idx += 1;
    let rest = &line[idx..];

    // Find end of tag name
    let tag_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '\\' && c != ':')
        .unwrap_or(rest.len());

    if tag_end == 0 {
        return None;
    }

    let tag_type = rest[..tag_end].to_string();
    let after_tag = &rest[tag_end..];
    let leading_ws = after_tag.len() - after_tag.trim_start().len();
    let data = after_tag.trim().to_string();

    // Offset of the data within the line (not end-anchored: trailing
    // whitespace must not shift it).
    let data_offset = idx + tag_end + leading_ws;

    Some((tag_type, data, data_offset))
}

/// Clean up asterisks in multi-line tag content. Only continuation lines
/// carry `*` decoration — a literal `*` in the first line's data (e.g.
/// `@param * $x`) is content, not decoration (Psalm strips per comment line
/// before splitting, so it keeps it too).
fn clean_multiline_data(data: &str) -> String {
    data.lines()
        .enumerate()
        .map(|(index, line)| {
            let trimmed = line.trim();
            if index == 0 {
                trimmed
            } else if trimmed == "*" {
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

    // Param tags. A single parameter may be documented by more than one tag
    // flavour (e.g. a plain `@param array` alongside a `@psalm-param
    // array<TKey, T>`); Psalm lets the analysis-specific tag win. Dedupe by
    // parameter name in increasing priority order (`param` < `phpstan-param` <
    // `psalm-param`) so the most specific type survives.
    if docblock.tags.contains_key("param")
        || docblock.tags.contains_key("psalm-param")
        || docblock.tags.contains_key("phpstan-param")
    {
        let mut combined = FxHashMap::default();
        let mut name_to_offset: FxHashMap<String, usize> = FxHashMap::default();
        for key in ["param", "phpstan-param", "psalm-param"] {
            if let Some(tags) = docblock.tags.get(key) {
                let mut ordered: Vec<_> = tags.iter().collect();
                ordered.sort_by_key(|(offset, _)| **offset);
                for (offset, content) in ordered {
                    if let Some(param_name) = extract_param_tag_name(content)
                        && let Some(prev_offset) = name_to_offset.insert(param_name, *offset) {
                            combined.remove(&prev_offset);
                        }
                    combined.insert(*offset, content.clone());
                }
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

                for line in lines.values() {
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

    /// Like [`Self::get_return`], also yielding the tag content's absolute
    /// file offset (the start of the type string).
    pub fn get_return_with_offset(&self) -> Option<(usize, &str)> {
        self.combined_tags
            .get("return")
            .and_then(|m| m.iter().next())
            .map(|(offset, s)| (*offset, s.as_str()))
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

