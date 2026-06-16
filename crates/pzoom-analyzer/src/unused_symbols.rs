//! Codebase-wide unused-definition detection, ported from Hakana's
//! `orchestrator/unused_symbols.rs` (`find_unused_definitions`).
//!
//! Runs once, after every file has been analyzed and their [`SymbolReferences`]
//! graphs merged, so a definition referenced from *any* file is seen as
//! referenced. Per file it reuses [`crate::file_analyzer::report_unused_declarations`]
//! (the Psalm-matching class/method/property/return-value rules), fed the merged
//! reference sets, then applies inline `@psalm-suppress` filtering. Config-level
//! (`<UnusedClass>` directories) and baseline suppression are applied later, by
//! the CLI, to every issue.

use crate::config::Config;
use crate::file_analyzer::{
    class_docblock_suppression_match_for_issue, line_suppression_match_for_issue,
    report_unused_declarations,
};
use pzoom_code_info::issue::Issue;
use pzoom_code_info::symbol_references::SymbolReferences;
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashSet;

/// Issue kinds emitted by the codebase-wide unused-definition pass. Their
/// `@psalm-suppress` annotations are owned by that pass (not the per-file
/// UnusedPsalmSuppress check), since the issues are only known once every file's
/// references have been merged.
pub(crate) fn is_unused_definition_kind(name: &str) -> bool {
    matches!(
        name,
        "UnusedClass"
            | "UnusedMethod"
            | "PossiblyUnusedMethod"
            | "UnusedProperty"
            | "PossiblyUnusedProperty"
            | "UnusedFunction"
            | "UnusedReturnValue"
            | "PossiblyUnusedReturnValue"
            | "UnusedConstructor"
            | "ClassMustBeFinal"
    )
}

/// 1-based (line, column) for `offset` given the file's line-start offsets.
pub(crate) fn line_column(line_starts: &[usize], offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    let line_idx = match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let col = offset - line_starts.get(line_idx).copied().unwrap_or(0) + 1;
    (line_idx as u32 + 1, col as u32)
}

pub(crate) fn line_start_offsets(contents: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in contents.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

pub(crate) fn record_union_classes(union: &TUnion, out: &mut FxHashSet<StrId>) {
    for atomic in &union.types {
        record_atomic_classes(atomic, out);
    }
}

fn record_atomic_classes(atomic: &TAtomic, out: &mut FxHashSet<StrId>) {
    match atomic {
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
            out.insert(*name);
            if let Some(type_params) = type_params {
                for param in type_params.iter() {
                    record_union_classes(param, out);
                }
            }
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => record_atomic_classes(as_type, out),
        _ => {}
    }
}

/// Codebase-wide unused-definition pass over `files` (the analyzed user files).
/// `referenced_properties` and `method_returns_used` are the merged per-file
/// sets (reads-only property accesses / used return values, which the symbol
/// graph does not distinguish).
pub fn find_unused_definitions(
    codebase: &CodebaseInfo,
    interner: &Interner,
    config: &Config,
    files: &[StrId],
    symbol_references: &SymbolReferences,
    referenced_properties: &FxHashSet<(StrId, StrId)>,
    method_returns_used: &FxHashSet<(StrId, StrId)>,
) -> Vec<Issue> {
    let referenced = symbol_references.get_referenced_symbols_and_members();
    let referenced_overridden = symbol_references.get_referenced_overridden_class_members();

    // Referenced classes (member == EMPTY) and class members (method / class
    // constant / enum case), straight from the merged graph — body references,
    // signature references (extends/implements, parameter/return types) and
    // file-scope references are all recorded into it during analysis.
    let referenced_classes: FxHashSet<StrId> = referenced
        .iter()
        .filter(|(_, member)| *member == StrId::EMPTY)
        .map(|(class, _)| *class)
        .collect();
    let mut referenced_class_members: FxHashSet<(StrId, StrId)> = referenced
        .iter()
        .filter(|(_, member)| *member != StrId::EMPTY)
        .copied()
        .collect();
    referenced_class_members.extend(referenced_overridden);

    let mut issues: Vec<Issue> = Vec::new();

    for file_path in files {
        let Some(file_info) = codebase.files.get(file_path) else {
            continue;
        };
        let contents = &file_info.contents;
        let line_starts = line_start_offsets(contents);

        let raw = report_unused_declarations(
            *file_path,
            file_info,
            &line_starts,
            codebase,
            interner,
            &referenced_classes,
            &referenced_class_members,
            referenced_properties,
            method_returns_used,
        );
        if raw.is_empty() {
            continue;
        }

        // Inline `@psalm-suppress` filtering (config-level + baseline run later,
        // in the CLI, over every issue). Declaration issues are covered by a
        // same/preceding-line comment or the class/method docblock.
        let lines: Vec<&str> = contents.lines().collect();
        let class_spans: Vec<(u32, u32)> = file_info
            .classes
            .iter()
            .filter_map(|class_id| codebase.get_class(*class_id))
            .map(|class_info| (class_info.start_offset, class_info.end_offset))
            .collect();
        let mut function_spans: Vec<(u32, u32)> = file_info
            .functions
            .iter()
            .filter_map(|function_id| codebase.get_function(*function_id))
            .map(|info| (info.start_offset, info.end_offset))
            .collect();
        for class_id in &file_info.classes {
            if let Some(class_info) = codebase.get_class(*class_id) {
                function_spans.extend(class_info.methods.values().filter_map(|method_info| {
                    (method_info.file_path == *file_path)
                        .then_some((method_info.start_offset, method_info.end_offset))
                }));
            }
        }

        for issue in raw {
            let issue_name = format!("{:?}", issue.kind);
            if config.is_issue_suppressed(&issue_name) {
                continue;
            }
            if line_suppression_match_for_issue(&lines, &line_starts, &issue).is_some()
                || class_docblock_suppression_match_for_issue(contents, &function_spans, &issue)
                    .is_some()
                || class_docblock_suppression_match_for_issue(contents, &class_spans, &issue)
                    .is_some()
            {
                continue;
            }
            issues.push(issue);
        }
    }

    issues
}
