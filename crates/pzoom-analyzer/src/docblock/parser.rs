//! Docblock parser - extracts tags from PHPDoc comments.

use rustc_hash::FxHashMap;

/// A parsed docblock with description and tags.
#[derive(Debug, Clone, Default)]
pub struct ParsedDocblock {
    /// The main description text (before any tags).
    pub description: String,
    /// All extracted tags, keyed by tag name (without @).
    pub tags: FxHashMap<String, Vec<DocblockTag>>,
}

/// A single docblock tag with its content.
#[derive(Debug, Clone)]
pub struct DocblockTag {
    /// The tag name (e.g., "param", "return", "psalm-param").
    pub name: String,
    /// The raw content after the tag name.
    pub content: String,
    /// For @param tags: the parameter name (e.g., "$foo").
    pub param_name: Option<String>,
    /// For @param/@return/@var tags: the type string.
    pub type_string: Option<String>,
}

/// Parse a docblock comment string into structured data.
///
/// Handles both standard PHPDoc tags and Psalm-specific tags.
/// Supports multi-line tag content.
pub fn parse_docblock(docblock: &str) -> ParsedDocblock {
    let mut result = ParsedDocblock::default();

    // Strip the docblock delimiters and normalize
    let content = strip_docblock_delimiters(docblock);

    let mut current_tag: Option<(String, String)> = None;
    let mut description_lines: Vec<&str> = Vec::new();
    let mut in_description = true;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check if this line starts a new tag
        if let Some(tag_match) = parse_tag_line(trimmed) {
            // Save the previous tag if any
            if let Some((tag_name, tag_content)) = current_tag.take() {
                add_tag(&mut result.tags, &tag_name, tag_content.trim());
            }

            in_description = false;
            current_tag = Some(tag_match);
        } else if in_description {
            // Part of the description
            if !trimmed.is_empty() {
                description_lines.push(trimmed);
            }
        } else if let Some((_, ref mut content)) = current_tag {
            // Continuation of a tag (multi-line)
            if !trimmed.is_empty() {
                content.push(' ');
                content.push_str(trimmed);
            }
        }
    }

    // Save the last tag
    if let Some((tag_name, tag_content)) = current_tag {
        add_tag(&mut result.tags, &tag_name, tag_content.trim());
    }

    result.description = description_lines.join(" ");

    // Resolve combined tags (psalm-* takes priority over phpstan-* over standard)
    resolve_combined_tags(&mut result);

    result
}

/// Strip the /** */ delimiters and leading asterisks from a docblock.
fn strip_docblock_delimiters(docblock: &str) -> String {
    let mut result = String::new();

    for line in docblock.lines() {
        let trimmed = line.trim();

        // Skip opening delimiter
        if trimmed.starts_with("/**") {
            let rest = trimmed.strip_prefix("/**").unwrap_or("").trim();
            if !rest.is_empty() && !rest.starts_with('*') {
                result.push_str(rest);
                result.push('\n');
            }
            continue;
        }

        // Skip closing delimiter
        if trimmed == "*/" || trimmed.ends_with("*/") {
            let rest = trimmed.strip_suffix("*/").unwrap_or("").trim();
            let rest = rest.strip_prefix('*').unwrap_or(rest).trim();
            if !rest.is_empty() {
                result.push_str(rest);
                result.push('\n');
            }
            continue;
        }

        // Strip leading asterisk
        let content = if trimmed.starts_with('*') {
            trimmed[1..].trim_start()
        } else {
            trimmed
        };

        result.push_str(content);
        result.push('\n');
    }

    result
}

/// Try to parse a line as a tag line, returning (tag_name, content) if successful.
fn parse_tag_line(line: &str) -> Option<(String, String)> {
    if !line.starts_with('@') {
        return None;
    }

    let rest = &line[1..];

    // Find the end of the tag name (space, tab, or end of line)
    let tag_end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());

    let tag_name = &rest[..tag_end];

    // Validate tag name (alphanumeric, hyphens, backslashes allowed)
    if tag_name.is_empty() || !tag_name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '\\' || c == ':') {
        return None;
    }

    let content = rest[tag_end..].trim().to_string();

    Some((tag_name.to_lowercase(), content))
}

/// Add a tag to the tags map, parsing type and param name for known tags.
fn add_tag(tags: &mut FxHashMap<String, Vec<DocblockTag>>, name: &str, content: &str) {
    let tag = match name {
        "param" | "psalm-param" | "phpstan-param" => parse_param_tag(name, content),
        "return" | "psalm-return" | "phpstan-return" => parse_return_tag(name, content),
        "var" | "psalm-var" | "phpstan-var" => parse_var_tag(name, content),
        "throws" | "psalm-throws" => parse_throws_tag(name, content),
        "template" | "psalm-template" | "phpstan-template" => parse_template_tag(name, content),
        "template-covariant" | "psalm-template-covariant" => parse_template_tag(name, content),
        "extends" | "template-extends" | "psalm-extends" | "phpstan-extends" => parse_extends_tag(name, content),
        "implements" | "template-implements" | "psalm-implements" | "phpstan-implements" => parse_extends_tag(name, content),
        "property" | "property-read" | "property-write"
        | "psalm-property" | "psalm-property-read" | "psalm-property-write" => parse_property_tag(name, content),
        "method" | "psalm-method" | "phpstan-method" => parse_method_tag(name, content),
        _ => DocblockTag {
            name: name.to_string(),
            content: content.to_string(),
            param_name: None,
            type_string: None,
        },
    };

    tags.entry(name.to_string()).or_default().push(tag);
}

/// Parse a @param tag: @param Type $name [description]
fn parse_param_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, rest) = split_type_and_rest(content);

    // Extract parameter name
    let param_name = rest
        .split_whitespace()
        .next()
        .filter(|s| s.starts_with('$'))
        .map(|s| s.to_string());

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name,
        type_string: type_str,
    }
}

/// Parse a @return tag: @return Type [description]
fn parse_return_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, _) = split_type_and_rest(content);

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name: None,
        type_string: type_str,
    }
}

/// Parse a @var tag: @var Type [$name] [description]
fn parse_var_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, rest) = split_type_and_rest(content);

    // Extract variable name if present
    let param_name = rest
        .split_whitespace()
        .next()
        .filter(|s| s.starts_with('$'))
        .map(|s| s.to_string());

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name,
        type_string: type_str,
    }
}

/// Parse a @throws tag: @throws ExceptionType [description]
fn parse_throws_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, _) = split_type_and_rest(content);

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name: None,
        type_string: type_str,
    }
}

/// Parse a @template tag: @template T [of BaseType]
fn parse_template_tag(name: &str, content: &str) -> DocblockTag {
    let parts: Vec<&str> = content.split_whitespace().collect();
    let template_name = parts.first().map(|s| s.to_string());

    // Check for "of" constraint
    let type_str = if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("of") {
        Some(parts[2..].join(" "))
    } else {
        None
    };

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name: template_name,
        type_string: type_str,
    }
}

/// Parse a @extends/@implements tag: @extends ClassName<T>
fn parse_extends_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, _) = split_type_and_rest(content);

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name: None,
        type_string: type_str,
    }
}

/// Parse a @property tag: @property Type $name [description]
fn parse_property_tag(name: &str, content: &str) -> DocblockTag {
    let (type_str, rest) = split_type_and_rest(content);

    let param_name = rest
        .split_whitespace()
        .next()
        .filter(|s| s.starts_with('$'))
        .map(|s| s.to_string());

    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name,
        type_string: type_str,
    }
}

/// Parse a @method tag: @method ReturnType methodName(params)
fn parse_method_tag(name: &str, content: &str) -> DocblockTag {
    // This is complex - for now just store the content
    // Full parsing would need to extract return type, method name, and params
    DocblockTag {
        name: name.to_string(),
        content: content.to_string(),
        param_name: None,
        type_string: None,
    }
}

/// Split a type string from the rest of the content.
/// Handles complex types with generics, unions, etc.
fn split_type_and_rest(content: &str) -> (Option<String>, &str) {
    if content.is_empty() {
        return (None, content);
    }

    let chars: Vec<char> = content.chars().collect();
    let mut depth: u32 = 0; // Track <> and {} nesting
    let mut paren_depth: u32 = 0; // Track () nesting
    let mut end_pos = 0;

    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '<' | '{' => depth += 1,
            '>' | '}' => depth = depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ' ' | '\t' if depth == 0 && paren_depth == 0 => {
                end_pos = i;
                break;
            }
            _ => {}
        }
        end_pos = i + 1;
    }

    if end_pos == 0 {
        return (None, content);
    }

    let type_str = content[..end_pos].trim();
    let rest = if end_pos < content.len() {
        content[end_pos..].trim()
    } else {
        ""
    };

    if type_str.is_empty() {
        (None, content)
    } else {
        (Some(type_str.to_string()), rest)
    }
}

/// Resolve combined tags - psalm-* takes priority over phpstan-* over standard.
fn resolve_combined_tags(docblock: &mut ParsedDocblock) {
    // For each standard tag, if psalm-* or phpstan-* versions exist, prefer them
    let tag_groups = [
        ("param", &["psalm-param", "phpstan-param"][..]),
        ("return", &["psalm-return", "phpstan-return"][..]),
        ("var", &["psalm-var", "phpstan-var"][..]),
        ("template", &["psalm-template", "phpstan-template"][..]),
        ("extends", &["psalm-extends", "phpstan-extends", "template-extends"][..]),
        ("implements", &["psalm-implements", "phpstan-implements", "template-implements"][..]),
        ("property", &["psalm-property"][..]),
        ("property-read", &["psalm-property-read"][..]),
        ("property-write", &["psalm-property-write"][..]),
        ("method", &["psalm-method", "phpstan-method"][..]),
    ];

    for (canonical, variants) in tag_groups {
        // Collect all tags under the canonical name
        let mut all_tags: Vec<DocblockTag> = Vec::new();

        // Add psalm-* first (highest priority)
        for variant in variants.iter().filter(|v| v.starts_with("psalm-")) {
            if let Some(tags) = docblock.tags.get(*variant) {
                all_tags.extend(tags.iter().cloned());
            }
        }

        // Then phpstan-*
        for variant in variants.iter().filter(|v| v.starts_with("phpstan-")) {
            if let Some(tags) = docblock.tags.get(*variant) {
                all_tags.extend(tags.iter().cloned());
            }
        }

        // Then template-* variants
        for variant in variants.iter().filter(|v| v.starts_with("template-")) {
            if let Some(tags) = docblock.tags.get(*variant) {
                all_tags.extend(tags.iter().cloned());
            }
        }

        // Finally standard tags
        if let Some(tags) = docblock.tags.get(canonical) {
            all_tags.extend(tags.iter().cloned());
        }

        if !all_tags.is_empty() {
            docblock.tags.insert(format!("_{}", canonical), all_tags);
        }
    }
}

impl ParsedDocblock {
    /// Get the resolved param tags (psalm-param > phpstan-param > param).
    pub fn get_params(&self) -> &[DocblockTag] {
        self.tags.get("_param").map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get the resolved return tag.
    pub fn get_return(&self) -> Option<&DocblockTag> {
        self.tags.get("_return").and_then(|v| v.first())
    }

    /// Get the resolved var tags.
    pub fn get_var(&self) -> Option<&DocblockTag> {
        self.tags.get("_var").and_then(|v| v.first())
    }

    /// Get template tags.
    pub fn get_templates(&self) -> &[DocblockTag] {
        self.tags.get("_template").map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get extends tags.
    pub fn get_extends(&self) -> &[DocblockTag] {
        self.tags.get("_extends").map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get implements tags.
    pub fn get_implements(&self) -> &[DocblockTag] {
        self.tags.get("_implements").map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Check if a tag exists.
    pub fn has_tag(&self, name: &str) -> bool {
        self.tags.contains_key(name)
    }

    /// Get raw tags by name.
    pub fn get_tags(&self, name: &str) -> &[DocblockTag] {
        self.tags.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Check if marked as deprecated.
    pub fn is_deprecated(&self) -> bool {
        self.has_tag("deprecated")
    }

    /// Check if marked as internal.
    pub fn is_internal(&self) -> bool {
        self.has_tag("internal") || self.has_tag("psalm-internal")
    }

    /// Check if marked as pure.
    pub fn is_pure(&self) -> bool {
        self.has_tag("pure") || self.has_tag("psalm-pure") || self.has_tag("phpstan-pure")
    }

    /// Get psalm-suppress tags.
    pub fn get_suppressed_issues(&self) -> Vec<&str> {
        self.tags
            .get("psalm-suppress")
            .map(|tags| {
                tags.iter()
                    .map(|t| t.content.split_whitespace().next().unwrap_or(""))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
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

        let parsed = parse_docblock(docblock);

        assert_eq!(parsed.description, "This is the description.");

        let params = parsed.get_params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].type_string.as_deref(), Some("string"));
        assert_eq!(params[0].param_name.as_deref(), Some("$name"));

        let ret = parsed.get_return().unwrap();
        assert_eq!(ret.type_string.as_deref(), Some("int"));
    }

    #[test]
    fn test_parse_psalm_tags_priority() {
        let docblock = r#"/**
         * @param string $x
         * @psalm-param non-empty-string $x
         */"#;

        let parsed = parse_docblock(docblock);
        let params = parsed.get_params();

        // psalm-param should come first (higher priority)
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_string.as_deref(), Some("non-empty-string"));
    }

    #[test]
    fn test_parse_generic_types() {
        let docblock = r#"/**
         * @param array<int, string> $items
         * @return list<array{id: int, name: string}>
         */"#;

        let parsed = parse_docblock(docblock);

        let params = parsed.get_params();
        assert_eq!(params[0].type_string.as_deref(), Some("array<int, string>"));

        let ret = parsed.get_return().unwrap();
        assert_eq!(ret.type_string.as_deref(), Some("list<array{id: int, name: string}>"));
    }

    #[test]
    fn test_parse_template() {
        let docblock = r#"/**
         * @template T of \Iterator
         * @param T $iterator
         * @return T
         */"#;

        let parsed = parse_docblock(docblock);
        let templates = parsed.get_templates();

        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].param_name.as_deref(), Some("T"));
        assert_eq!(templates[0].type_string.as_deref(), Some("\\Iterator"));
    }

    #[test]
    fn test_parse_suppress() {
        let docblock = r#"/**
         * @psalm-suppress MixedAssignment
         * @psalm-suppress MixedArgument Some reason
         */"#;

        let parsed = parse_docblock(docblock);
        let suppressed = parsed.get_suppressed_issues();

        assert_eq!(suppressed.len(), 2);
        assert!(suppressed.contains(&"MixedAssignment"));
        assert!(suppressed.contains(&"MixedArgument"));
    }
}
