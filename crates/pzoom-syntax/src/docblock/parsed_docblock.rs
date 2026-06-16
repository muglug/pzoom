//! Docblock comment parsing - extracts description and tags from PHPDoc comments.
//!
//! Mirrors Psalm's `DocblockParser.php` / `ParsedDocblock.php`: it extracts the
//! structure of a docblock (description and tags) without parsing types. Type
//! parsing lives in [`super::type_parser`].

use std::borrow::Cow;

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

/// The occurrences of a single tag name within a docblock, as `(absolute file
/// offset, content)` pairs in document order.
///
/// A docblock carries only a handful of tags, and consumers iterate these far
/// more often than they look one up by offset, so a flat `Vec` beats the
/// `FxHashMap<usize, String>` this replaces: no per-tag offset hashing, one
/// growable allocation instead of a hash table, and a trivial drop. Offsets are
/// unique file positions, so appends never collide.
#[derive(Debug, Clone, Default)]
pub struct TagList {
    entries: Vec<(usize, String)>,
}

impl TagList {
    fn with_one(offset: usize, content: String) -> Self {
        Self {
            entries: vec![(offset, content)],
        }
    }

    /// Append a tag occurrence. Callers only ever pass distinct file offsets,
    /// so this never needs to dedupe.
    fn push(&mut self, offset: usize, content: String) {
        self.entries.push((offset, content));
    }

    /// Drop the occurrence at `offset`, if present (used when a later tag
    /// flavour supersedes an earlier one for the same parameter).
    fn remove_offset(&mut self, offset: usize) {
        self.entries.retain(|(o, _)| *o != offset);
    }

    /// Number of occurrences.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no occurrences.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The contents, in document order.
    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(_, content)| content)
    }

    /// The `(offset, content)` pairs, in document order. Mirrors the old
    /// `FxHashMap::iter` element type so callers are unaffected.
    pub fn iter(&self) -> impl Iterator<Item = (&usize, &String)> {
        self.entries
            .iter()
            .map(|(offset, content)| (offset, content))
    }
}

/// A parsed docblock with description and tags.
#[derive(Debug, Clone, Default)]
pub struct ParsedDocblock {
    /// The main description text (before any tags).
    pub description: String,
    /// All extracted tags, keyed by tag name (without @).
    pub tags: FxHashMap<String, TagList>,
    /// Combined tags with precedence resolution (psalm-* > phpstan-* > standard).
    pub combined_tags: FxHashMap<String, TagList>,
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

    // Normalize multi-line @specials. Only allocate when a tab is actually
    // present — the overwhelming majority of docblocks have none.
    let docblock: Cow<str> = if docblock.contains('\t') {
        Cow::Owned(docblock.replace('\t', " "))
    } else {
        Cow::Borrowed(docblock)
    };

    // Borrow each line from `docblock` rather than copying it. Lines only get
    // promoted to owned `String`s when they're actually mutated (continuation
    // merges or `\r` stripping), which most aren't.
    let mut lines: Vec<Cow<str>> = docblock.lines().map(Cow::Borrowed).collect();

    let has_r = docblock.contains('\r');

    let mut special: FxHashMap<String, TagList> = FxHashMap::default();
    let mut first_line_padding = None;

    // Join continuation lines to their tag
    // First pass: identify which lines to merge
    let mut merge_info: Vec<(usize, usize)> = Vec::new(); // (continuation_idx, target_idx)
    let mut last_tag_line: Option<usize> = None;
    let mut last_tag_can_continue = false;

    for (k, line) in lines.iter().enumerate() {
        if line.contains('@') && is_tag_line(line) {
            last_tag_line = Some(k);
            last_tag_can_continue = parse_tag_line(line)
                .map(|(_, data, _)| !data.is_empty())
                .unwrap_or(false);
        } else if line.trim().is_empty() {
            last_tag_line = None;
            last_tag_can_continue = false;
        } else if let Some(last) = last_tag_line
            && last_tag_can_continue
        {
            merge_info.push((k, last));
        }
    }

    // Second pass: perform merges in source order so multiline tag content
    // preserves line order.
    for (cont_idx, target_idx) in &merge_info {
        let cont_line = lines[*cont_idx].clone();
        let target = lines[*target_idx].to_mut();
        target.push('\n');
        target.push_str(&cont_line);
    }

    // Remove continuation lines (in reverse order to preserve indices)
    let to_remove: Vec<_> = merge_info.iter().map(|(k, _)| *k).collect();
    for k in to_remove.into_iter().rev() {
        lines.remove(k);
    }

    let mut line_offset = 0usize;
    let mut description_lines = Vec::new();

    for line in lines.iter_mut() {
        let original_line_length = line.len();

        if has_r {
            *line = Cow::Owned(line.replace('\r', ""));
        }

        // Detect first line padding
        if first_line_padding.is_none()
            && let Some(asterisk_pos) = line.find('*')
        {
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
                clean_multiline_data(data)
            } else {
                data.to_string()
            };

            let absolute_offset = data_offset + line_offset + 3 + offset_start;

            // Avoid re-allocating the tag-name key on repeat occurrences (a
            // docblock's several `@param`s would each otherwise allocate it).
            if let Some(list) = special.get_mut(tag_type) {
                list.push(absolute_offset, data);
            } else {
                special.insert(
                    tag_type.to_string(),
                    TagList::with_one(absolute_offset, data),
                );
            }
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
            .chars()
            .nth(1)
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false)
}

/// Parse a line as a tag, returning (tag_type, data, data_offset) if successful.
///
/// `tag_type` and `data` borrow from `line`; callers own them only at the point
/// they're stored, which keeps the cheap "does this line continue?" probe in the
/// continuation pass allocation-free.
fn parse_tag_line(line: &str) -> Option<(&str, &str, usize)> {
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

    let tag_type = &rest[..tag_end];
    let after_tag = &rest[tag_end..];
    let leading_ws = after_tag.len() - after_tag.trim_start().len();
    let data = after_tag.trim();

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
    // Skip leading empty lines without copying the slice.
    let Some(start) = lines.iter().position(|l| !l.trim().is_empty()) else {
        return String::new();
    };
    let lines = &lines[start..];

    // Find minimum indent
    let min_indent = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    // Remove common indent and join, building the result in a single buffer.
    let mut result = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        if l.len() >= min_indent {
            result.push_str(&l[min_indent..]);
        } else {
            result.push_str(l);
        }
    }

    result.truncate(result.trim_end().len());
    result
}

/// Bit per combinable tag group, OR'd together from the tags actually present
/// on a docblock so [`resolve_tags`] can skip groups it doesn't have.
mod tag_group {
    pub const TEMPLATE: u32 = 1 << 0;
    pub const TEMPLATE_COVARIANT: u32 = 1 << 1;
    pub const EXTENDS: u32 = 1 << 2;
    pub const IMPLEMENTS: u32 = 1 << 3;
    pub const USE: u32 = 1 << 4;
    pub const MIXIN: u32 = 1 << 5;
    pub const METHOD: u32 = 1 << 6;
    pub const PROPERTY: u32 = 1 << 7;
    pub const PROPERTY_READ: u32 = 1 << 8;
    pub const PROPERTY_WRITE: u32 = 1 << 9;
    pub const RETURN: u32 = 1 << 10;
    pub const PARAM: u32 = 1 << 11;
    pub const VAR: u32 = 1 << 12;
    pub const PARAM_OUT: u32 = 1 << 13;
    /// Not a combined group of its own — suppresses [`VAR`] resolution.
    pub const IGNORE_VAR: u32 = 1 << 14;
}

/// Map a raw tag name to the combinable group(s) it feeds, or `0` if it isn't
/// one. Called once per *present* tag rather than probing every possible
/// variant, which avoids ~40 hash lookups on the overwhelmingly common
/// docblocks that carry none of these tags.
fn classify_tag(key: &str) -> u32 {
    use tag_group::*;
    match key {
        "template" | "phpstan-template" | "psalm-template" => TEMPLATE,
        "template-covariant" | "phpstan-template-covariant" | "psalm-template-covariant" => {
            TEMPLATE_COVARIANT
        }
        "template-extends" | "inherits" | "extends" | "phpstan-extends" | "psalm-extends" => {
            EXTENDS
        }
        "template-implements" | "implements" | "phpstan-implements" | "psalm-implements" => {
            IMPLEMENTS
        }
        "template-use" | "use" | "phpstan-use" | "psalm-use" => USE,
        "mixin" | "phpstan-mixin" | "psalm-mixin" => MIXIN,
        "method" | "psalm-method" => METHOD,
        "property" | "phpstan-property" | "psalm-property" => PROPERTY,
        "property-read" | "phpstan-property-read" | "psalm-property-read" => PROPERTY_READ,
        "property-write" | "phpstan-property-write" | "psalm-property-write" => PROPERTY_WRITE,
        "return" | "psalm-return" | "phpstan-return" => RETURN,
        "param" | "phpstan-param" | "psalm-param" => PARAM,
        "var" | "phpstan-var" | "psalm-var" => VAR,
        "param-out" | "phpstan-param-out" | "psalm-param-out" => PARAM_OUT,
        "ignore-var" | "psalm-ignore-var" => IGNORE_VAR,
        _ => 0,
    }
}

/// Union the given source tag maps into one. The map key is the tag's absolute
/// file offset, which is unique across every tag in a docblock, so sources
/// never collide and order doesn't matter. The single-source case (by far the
/// most common) clones directly — one correctly-sized allocation with no
/// incremental rehashing.
fn combine_tag_groups(tags: &FxHashMap<String, TagList>, keys: &[&str]) -> TagList {
    let mut sources = keys.iter().filter_map(|k| tags.get(*k));
    let Some(first) = sources.next() else {
        return TagList::default();
    };

    let mut combined = first.clone();
    for src in sources {
        combined.entries.reserve(src.len());
        for (offset, content) in src.iter() {
            combined.push(*offset, content.clone());
        }
    }
    combined
}

/// Resolve combined tags with precedence (psalm-* > phpstan-* > standard).
fn resolve_tags(docblock: &mut ParsedDocblock) {
    use tag_group::*;

    if docblock.tags.is_empty() {
        return;
    }

    // One pass over the (typically few) present tags tells us which combinable
    // groups exist; the rest of this function only touches those.
    let mut present = 0u32;
    for key in docblock.tags.keys() {
        present |= classify_tag(key);
    }

    if present == 0 {
        return;
    }

    if present & TEMPLATE != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &["template", "phpstan-template", "psalm-template"],
        );
        docblock
            .combined_tags
            .insert("template".to_string(), combined);
    }

    if present & TEMPLATE_COVARIANT != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &[
                "template-covariant",
                "phpstan-template-covariant",
                "psalm-template-covariant",
            ],
        );
        docblock
            .combined_tags
            .insert("template-covariant".to_string(), combined);
    }

    if present & EXTENDS != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            // Increasing precedence: when one docblock binds the same parent via
            // several flavours, a downstream last-wins-by-target consumer keeps
            // the final entry, so the most specific tag must come last. The old
            // FxHashMap stored these by offset and won on hash order — a flat
            // Vec makes the precedence explicit instead.
            &[
                "extends",
                "inherits",
                "template-extends",
                "phpstan-extends",
                "psalm-extends",
            ],
        );
        docblock
            .combined_tags
            .insert("extends".to_string(), combined);
    }

    if present & IMPLEMENTS != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &[
                "implements",
                "template-implements",
                "phpstan-implements",
                "psalm-implements",
            ],
        );
        docblock
            .combined_tags
            .insert("implements".to_string(), combined);
    }

    if present & USE != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &["use", "template-use", "phpstan-use", "psalm-use"],
        );
        docblock.combined_tags.insert("use".to_string(), combined);
    }

    if present & MIXIN != 0 {
        let combined =
            combine_tag_groups(&docblock.tags, &["mixin", "phpstan-mixin", "psalm-mixin"]);
        docblock.combined_tags.insert("mixin".to_string(), combined);
    }

    if present & METHOD != 0 {
        let combined = combine_tag_groups(&docblock.tags, &["method", "psalm-method"]);
        docblock
            .combined_tags
            .insert("method".to_string(), combined);
    }

    if present & PROPERTY != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &["property", "phpstan-property", "psalm-property"],
        );
        docblock
            .combined_tags
            .insert("property".to_string(), combined);
    }

    if present & PROPERTY_READ != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &[
                "property-read",
                "phpstan-property-read",
                "psalm-property-read",
            ],
        );
        docblock
            .combined_tags
            .insert("property-read".to_string(), combined);
    }

    if present & PROPERTY_WRITE != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &[
                "property-write",
                "phpstan-property-write",
                "psalm-property-write",
            ],
        );
        docblock
            .combined_tags
            .insert("property-write".to_string(), combined);
    }

    // Return tags (psalm-return takes precedence, single winner — not a union).
    if present & RETURN != 0 {
        let combined = docblock
            .tags
            .get("psalm-return")
            .or_else(|| docblock.tags.get("phpstan-return"))
            .or_else(|| docblock.tags.get("return"))
            .cloned()
            .unwrap_or_default();
        docblock
            .combined_tags
            .insert("return".to_string(), combined);
    }

    // Param tags. A single parameter may be documented by more than one tag
    // flavour (e.g. a plain `@param array` alongside a `@psalm-param
    // array<TKey, T>`); Psalm lets the analysis-specific tag win. Dedupe by
    // parameter name in increasing priority order (`param` < `phpstan-param` <
    // `psalm-param`) so the most specific type survives.
    if present & PARAM != 0 {
        let mut combined = TagList::default();
        let mut name_to_offset: FxHashMap<String, usize> = FxHashMap::default();
        for key in ["param", "phpstan-param", "psalm-param"] {
            if let Some(tags) = docblock.tags.get(key) {
                let mut ordered: Vec<_> = tags.iter().collect();
                ordered.sort_by_key(|(offset, _)| **offset);
                for (offset, content) in ordered {
                    if let Some(param_name) = extract_param_tag_name(content)
                        && let Some(prev_offset) = name_to_offset.insert(param_name, *offset)
                    {
                        combined.remove_offset(prev_offset);
                    }
                    combined.push(*offset, content.clone());
                }
            }
        }
        docblock.combined_tags.insert("param".to_string(), combined);
    }

    // Var tags (suppressed by an ignore-var tag).
    if present & VAR != 0 && present & IGNORE_VAR == 0 {
        let combined = combine_tag_groups(&docblock.tags, &["var", "phpstan-var", "psalm-var"]);
        docblock.combined_tags.insert("var".to_string(), combined);
    }

    if present & PARAM_OUT != 0 {
        let combined = combine_tag_groups(
            &docblock.tags,
            &["param-out", "phpstan-param-out", "psalm-param-out"],
        );
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
