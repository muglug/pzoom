//! Helpers for Psalm-style inline suppression lookup.

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn is_issue_suppressed_at(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_offset: u32,
    issue_name: &str,
) -> bool {
    // Psalm checks config-level suppression before inline suppressions and
    // never marks an inline token used for it (IssueBuffer::isSuppressed).
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

    if let Some(token_offset) = suppressing_docblock_match(&source[scope_start..offset], issue_name)
    {
        analysis_data
            .used_suppression_offsets
            .push((scope_start + token_offset) as u32);
        return true;
    }

    // The function's own docblock precedes its start offset (and issues that
    // point into the docblock, like a @return location, precede it too) —
    // check it directly so `@psalm-suppress` next to `@return` works.
    if let Some(function_start) = analyzer
        .function_info
        .map(|function_info| function_info.start_offset as usize)
    {
        let function_start = function_start.min(source.len());
        if let Some((docblock_start, docblock)) = preceding_docblock(&source[..function_start]) {
            if let Some(token_offset) = docblock_suppression_match(docblock, issue_name) {
                analysis_data
                    .used_suppression_offsets
                    .push((docblock_start + token_offset) as u32);
                return true;
            }
        }
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
            if let Some((docblock_start, docblock)) = preceding_docblock(&source[..class_start]) {
                if let Some(token_offset) = docblock_suppression_match(docblock, issue_name) {
                    analysis_data
                        .used_suppression_offsets
                        .push((docblock_start + token_offset) as u32);
                    return true;
                }
            }
        }
    }

    false
}

/// The docblock (`/** ... */`) immediately preceding `prefix` (the source up
/// to a declaration), if one is present with only whitespace in between,
/// together with its byte offset.
pub(crate) fn preceding_docblock(prefix: &str) -> Option<(usize, &str)> {
    let trimmed = prefix.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    let end = trimmed.len();
    let start = trimmed.rfind("/**")?;
    Some((start, &trimmed[start..end]))
}

/// Byte offset (within `scope_source`) of the first suppression token that
/// matches `issue_name` inside any docblock, if one exists.
fn suppressing_docblock_match(scope_source: &str, issue_name: &str) -> Option<usize> {
    let mut cursor = 0usize;
    // Keep the NEAREST (last) matching docblock before the issue, not the first:
    // with two `@psalm-suppress X` in one scope each issue must mark its own
    // (closest) suppression used, otherwise the later one looks unused.
    let mut nearest = None;

    while let Some(start_rel) = scope_source[cursor..].find("/**") {
        let start = cursor + start_rel;
        let after_start = start + 3;
        let Some(end_rel) = scope_source[after_start..].find("*/") else {
            break;
        };
        let end = after_start + end_rel + 2;
        let docblock = &scope_source[start..end];

        if let Some(token_offset) = docblock_suppression_match(docblock, issue_name) {
            nearest = Some(start + token_offset);
        }

        cursor = end;
    }

    nearest
}

/// Byte offset (within `docblock`) of the suppression token matching
/// `issue_name`, if any.
pub(crate) fn docblock_suppression_match(docblock: &str, issue_name: &str) -> Option<usize> {
    let mut line_start = 0usize;

    for line in docblock.split('\n') {
        if let Some(content_start) = suppression_tag_content_start(line) {
            let content = &line[content_start..];
            let mut token_start: Option<usize> = None;

            for (index, ch) in content
                .char_indices()
                .chain(std::iter::once((content.len(), ' ')))
            {
                let is_separator = ch.is_whitespace() || ch == ',' || ch == '*';
                match (token_start, is_separator) {
                    (None, false) => token_start = Some(index),
                    (Some(start), true) => {
                        let token = &content[start..index];
                        let trimmed = token
                            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\\');
                        // Psalm's issue hierarchy: every Tainted* issue
                        // extends TaintedInput, so suppressing TaintedInput
                        // covers them all.
                        if trimmed == issue_name
                            || (trimmed == "TaintedInput" && issue_name.starts_with("Tainted"))
                        {
                            let trim_lead = token.find(trimmed).unwrap_or(0);
                            return Some(line_start + content_start + start + trim_lead);
                        }
                        token_start = None;
                    }
                    _ => {}
                }
            }
        }

        line_start += line.len() + 1;
    }

    None
}

/// Content offset just past a suppression tag in `line`. `@psalm-fixme`
/// suppresses exactly like `@psalm-suppress`, marking the issue as a known
/// problem to fix (e.g. migrated from a baseline).
pub(crate) fn suppression_tag_content_start(line: &str) -> Option<usize> {
    ["@psalm-suppress", "@psalm-fixme"]
        .iter()
        .filter_map(|tag| line.find(tag).map(|tag_pos| (tag_pos, tag_pos + tag.len())))
        .min()
        .map(|(_, content_start)| content_start)
}
