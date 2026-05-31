//! Helpers for Psalm-style inline suppression lookup.

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn is_issue_suppressed_at(
    analyzer: &StatementsAnalyzer<'_>,
    issue_offset: u32,
    issue_name: &str,
) -> bool {
    if analyzer.config.is_issue_suppressed(issue_name) {
        return true;
    }

    let source = analyzer.source;
    let offset = (issue_offset as usize).min(source.len());
    let scope_start = analyzer
        .function_info
        .map(|function_info| function_info.start_offset as usize)
        .unwrap_or(0)
        .min(offset);

    if has_suppressing_docblock(&source[scope_start..offset], issue_name) {
        return true;
    }

    // Psalm merges suppressions from enclosing scopes (file -> class -> function).
    // Check the docblock attached to the enclosing class declaration so a
    // class-level `@psalm-suppress` covers all of its members.
    if let Some(class_id) = analyzer
        .function_info
        .and_then(|function_info| function_info.declaring_class)
    {
        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            let class_start = (class_info.start_offset as usize).min(source.len());
            if let Some(docblock) = preceding_docblock(&source[..class_start]) {
                if docblock_suppresses_issue(docblock, issue_name) {
                    return true;
                }
            }
        }
    }

    false
}

/// Return the docblock (`/** ... */`) immediately preceding `prefix` (the source
/// up to a declaration), if one is present with only whitespace in between.
fn preceding_docblock(prefix: &str) -> Option<&str> {
    let trimmed = prefix.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    let end = trimmed.len();
    let start = trimmed.rfind("/**")?;
    Some(&trimmed[start..end])
}

fn has_suppressing_docblock(scope_source: &str, issue_name: &str) -> bool {
    let mut cursor = 0usize;

    while let Some(start_rel) = scope_source[cursor..].find("/**") {
        let start = cursor + start_rel;
        let after_start = start + 3;
        let Some(end_rel) = scope_source[after_start..].find("*/") else {
            break;
        };
        let end = after_start + end_rel + 2;
        let docblock = &scope_source[start..end];

        if docblock_suppresses_issue(docblock, issue_name) {
            return true;
        }

        cursor = end;
    }

    false
}

fn docblock_suppresses_issue(docblock: &str, issue_name: &str) -> bool {
    docblock.lines().any(|line| {
        let Some(idx) = line.find("@psalm-suppress") else {
            return false;
        };

        let after_tag = &line[idx + "@psalm-suppress".len()..];
        after_tag
            .split(|c: char| c.is_whitespace() || c == ',')
            .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\\'))
            .any(|token| token == issue_name)
    })
}
