//! Codebase-wide unused-definition detection, ported from Hakana's
//! `orchestrator/unused_symbols.rs` (`find_unused_definitions`).
//!
//! Runs once, after every file has been analyzed and their [`SymbolReferences`]
//! graphs merged, so a definition referenced from *any* file is seen as
//! referenced. Per file it runs [`report_unused_declarations`]
//! (the Psalm-matching class/method/property/return-value rules), fed the merged
//! reference sets, then applies inline `@psalm-suppress` filtering. Config-level
//! (`<UnusedClass>` directories) and baseline suppression are applied later, by
//! the CLI, to every issue.

use crate::config::Config;
use crate::file_analyzer::{
    class_docblock_suppression_match_for_issue, line_suppression_match_for_issue,
};
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::symbol_references::SymbolReferences;
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

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
#[allow(clippy::too_many_arguments)]
pub fn find_unused_definitions(
    codebase: &CodebaseInfo,
    interner: &Interner,
    config: &Config,
    files: &[StrId],
    symbol_references: &SymbolReferences,
    referenced_properties: &FxHashSet<(StrId, StrId)>,
    method_returns_used: &FxHashSet<(StrId, StrId)>,
    used_method_params: &FxHashSet<(StrId, StrId, usize)>,
    param_unused_candidates: &[crate::function_analysis_data::ParamUnusedCandidate],
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
        .chain(codebase.plugin_referenced_classes.iter().copied())
        .collect();
    let mut referenced_class_members: FxHashSet<(StrId, StrId)> = referenced
        .iter()
        .filter(|(_, member)| *member != StrId::EMPTY)
        .copied()
        .collect();
    referenced_class_members.extend(referenced_overridden);

    // Group the deferred non-private param candidates (Psalm's
    // checkMethodParamReferences) by file. A candidate is reported only if no
    // implementation — its own body or any override that propagated up the
    // chain — referenced the param (`!isMethodParamUsed`).
    let mut param_candidates_by_file: FxHashMap<StrId, Vec<&crate::function_analysis_data::ParamUnusedCandidate>> =
        FxHashMap::default();
    for candidate in param_unused_candidates {
        if used_method_params.contains(&(candidate.class_id, candidate.method_lc, candidate.offset))
        {
            continue;
        }
        param_candidates_by_file
            .entry(candidate.file_path)
            .or_default()
            .push(candidate);
    }

    let mut issues: Vec<Issue> = Vec::new();

    for file_path in files {
        let Some(file_info) = codebase.files.get(file_path) else {
            continue;
        };
        let contents = &file_info.contents;
        let line_starts = line_start_offsets(contents);

        let mut raw = report_unused_declarations(
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

        // Psalm reports a final method's unused param as UnusedParam, every
        // other as PossiblyUnusedParam. Append them here so they share the
        // declaration-issue suppression filter below.
        if let Some(candidates) = param_candidates_by_file.get(file_path) {
            for candidate in candidates {
                raw.push(Issue::new(
                    if candidate.is_final {
                        IssueKind::UnusedParam
                    } else {
                        IssueKind::PossiblyUnusedParam
                    },
                    format!(
                        "Param #{} is never referenced in this method",
                        candidate.offset + 1
                    ),
                    candidate.file_path,
                    candidate.span.0,
                    candidate.span.1,
                    candidate.line,
                    candidate.col,
                ));
            }
        }

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

/// Port of Psalm's `ClassLikes::consolidateAnalyzedData` reporting
/// (checkClassReferences / checkMethodReferences / checkPropertyReferences),
/// scoped to the classes declared in the analyzed file.
/// Per-file unused class/method/property/return-value detection, emitting
/// Psalm's issue kinds. Runs once per analyzed file from the codebase-wide
/// [`find_unused_definitions`], which supplies the merged reference sets (so a
/// definition referenced from any file is seen as used) and the file's
/// line-start offsets. Returns raw, unsuppressed issues.
#[allow(clippy::too_many_arguments)]
fn report_unused_declarations(
    file_path: StrId,
    file_info: &pzoom_code_info::file_info::FileInfo,
    line_starts: &[usize],
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &Interner,
    referenced: &rustc_hash::FxHashSet<StrId>,
    referenced_class_members: &rustc_hash::FxHashSet<(StrId, StrId)>,
    referenced_properties: &rustc_hash::FxHashSet<(StrId, StrId)>,
    method_returns_used: &rustc_hash::FxHashSet<(StrId, StrId)>,
) -> Vec<Issue> {
    use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};

    let magic_method_skips = [
        "__destruct",
        "__clone",
        "__invoke",
        "__unset",
        "__isset",
        "__sleep",
        "__wakeup",
        "__serialize",
        "__unserialize",
        "__set_state",
        "__debuginfo",
        "__tostring",
        "__construct",
        "__call",
        "__callstatic",
        "__get",
        "__set",
    ];

    let mut new_issues: Vec<Issue> = Vec::new();
    for class_id in &file_info.classes {
        let Some(class_info) = codebase.get_class(*class_id) else {
            continue;
        };
        if class_info.kind == ClassLikeKind::Trait {
            continue;
        }
        let class_name = interner.lookup(*class_id).to_string();
        // Anonymous classes (`new class {}`) are used where they are defined.
        if class_name.starts_with("@anonymous") {
            continue;
        }
        let class_referenced = referenced.contains(class_id);

        // A class whose parent/interface/trait never resolved has its body
        // skipped during analysis (ClassAnalyzer bails on
        // `invalid_dependencies`, mirroring Psalm's `if ($storage->
        // invalid_dependencies) return;`). With the body un-analyzed, no
        // references were recorded from inside it, and its real inheritance
        // chain is unknown — so its members (overrides of the unresolved
        // parent, and privates only ever called from the skipped body) all
        // look unused. Reporting them would be pure noise, so skip the
        // member checks, just as Psalm cannot reason about such a class.
        let class_has_unresolved_deps = class_info
            .invalid_dependencies
            .iter()
            .any(|dependency| codebase.get_class(*dependency).is_none());

        // A method or property supplied by a trait is declared in the trait's
        // file, so its stored offset indexes that file, not the using class's.
        // Report it against its declaring file (Psalm uses the storage's own
        // location), caching each foreign file's line table.
        let mut foreign_line_starts: FxHashMap<StrId, Vec<usize>> = FxHashMap::default();
        let mut emit =
            |kind: IssueKind, message: String, start: u32, end: u32, decl_file: StrId| {
                if decl_file == file_path {
                    let (line, col) = line_column(line_starts, start);
                    new_issues.push(Issue::new(kind, message, file_path, start, end, line, col));
                } else {
                    let starts = foreign_line_starts.entry(decl_file).or_insert_with(|| {
                        codebase
                            .files
                            .get(&decl_file)
                            .map(|file| line_start_offsets(&file.contents))
                            .unwrap_or_default()
                    });
                    let (line, col) = line_column(starts, start);
                    new_issues.push(Issue::new(kind, message, decl_file, start, end, line, col));
                }
            };

        if !class_info.is_public_api && !class_referenced && !class_info.dynamically_callable {
            // Psalm anchors class-wide issues on the NAME token, not the start
            // of the whole declaration span.
            let (name_start, name_end) = class_info.name_location.unwrap_or((
                class_info.start_offset,
                class_info.start_offset.saturating_add(1),
            ));
            emit(
                IssueKind::UnusedClass,
                format!("Class {} is never used", class_name),
                name_start,
                name_end,
                file_path,
            );
        } else if class_has_unresolved_deps {
            // Members of a class with unresolved dependencies are not checked
            // (see the comment on `class_has_unresolved_deps`).
        } else {
            // Methods (Psalm checkMethodReferences) — appearing in this class:
            // declared here or supplied by a used trait.
            for (method_name_id, method_info) in &class_info.methods {
                let declared_here = method_info.declaring_class == Some(*class_id)
                    || method_info
                        .declaring_class
                        .is_some_and(|declaring| class_info.used_traits.contains(&declaring));
                if !declared_here {
                    continue;
                }
                // A method a framework invokes reflectively (a PHPUnit `test*`
                // method or `@dataProvider` provider, flagged at populate time)
                // is never called from analyzed code; don't report it unused.
                if method_info.dynamically_callable {
                    continue;
                }
                // Psalm's `canReportIssues`: a method declared outside the
                // project (a vendor trait's `#[Before]` hook supplied to a test
                // class, say) is not ours to act on and may be framework-called.
                if !codebase
                    .files
                    .get(&method_info.file_path)
                    .is_some_and(|file| file.is_in_project_dirs)
                {
                    continue;
                }
                let method_name = interner.lookup(*method_name_id).to_string();
                let method_lc_name = method_name.to_lowercase();
                let method_lc = interner.intern(&method_lc_name);
                // A private constructor that is never called (no `new`, no
                // internal factory) is Psalm's UnusedConstructor — keep it in
                // the pass; every other magic method is runtime-invoked.
                let is_private_constructor = method_lc_name == "__construct"
                    && matches!(method_info.visibility, Visibility::Private);
                if magic_method_skips.contains(&method_lc_name.as_str()) && !is_private_constructor
                {
                    continue;
                }
                // Psalm: Serializable's serialize/unserialize and
                // JsonSerializable's jsonSerialize are called by the runtime.
                let implements_named = |needle: &str| {
                    class_info
                        .all_parent_interfaces
                        .iter()
                        .chain(class_info.interfaces.iter())
                        .any(|interface_id| {
                            interner.lookup(*interface_id).eq_ignore_ascii_case(needle)
                        })
                };
                if ((method_lc_name == "serialize" || method_lc_name == "unserialize")
                    && implements_named("Serializable"))
                    || (method_lc_name == "jsonserialize" && implements_named("JsonSerializable"))
                {
                    continue;
                }
                if method_info.is_public_api
                    || (class_info.is_public_api
                        && (matches!(method_info.visibility, Visibility::Public)
                            || (matches!(method_info.visibility, Visibility::Protected)
                                && !class_info.is_final)))
                {
                    continue;
                }
                let method_referenced = referenced_class_members.contains(&(*class_id, method_lc))
                    || method_info.declaring_class.is_some_and(|declaring| {
                        referenced_class_members.contains(&(declaring, method_lc))
                    });
                if !method_referenced {
                    // A referenced (or concrete) overridden parent method
                    // keeps this one alive (Psalm's has_parent_references).
                    let has_parent_references = class_info
                        .overridden_method_ids
                        .get(method_name_id)
                        .is_some_and(|parents| {
                            parents.iter().any(|parent_id| {
                                let parent_class = codebase.get_class(*parent_id);
                                // A parent declared in a stub or a scanned-only
                                // dependency (vendor) is one whose callers pzoom
                                // never sees, so an override of it might well be
                                // called externally — Psalm keeps it alive via
                                // `!canReportIssues(parent)` and Hakana via the
                                // non-`user_defined` parent check.
                                if parent_class
                                    .and_then(|parent| parent.methods.get(method_name_id))
                                    .map(|parent_method| parent_method.file_path)
                                    .or_else(|| parent_class.map(|parent| parent.file_path))
                                    .and_then(|file_path| codebase.files.get(&file_path))
                                    .is_some_and(|file| !file.is_in_project_dirs || file.is_stub)
                                {
                                    return true;
                                }
                                let parent_referenced =
                                    referenced_class_members.contains(&(*parent_id, method_lc));
                                // Psalm checks `!$parent_method_storage->abstract`.
                                // Interface methods (and unmarked concrete
                                // parents) have `abstract == false`, so a
                                // concrete-or-interface parent keeps the override
                                // alive unconditionally; only a genuinely abstract
                                // parent method requires its own reference. This
                                // mirrors PhpParser's `isAbstract()` being false
                                // for interface method nodes.
                                let parent_abstract = parent_class
                                    .and_then(|parent| parent.methods.get(method_name_id))
                                    .is_some_and(|parent_method| parent_method.is_abstract);
                                !parent_abstract || parent_referenced
                            })
                        });
                    if has_parent_references {
                        continue;
                    }
                    let method_id = format!("{}::{}", class_name, method_name);
                    // Anchor on the method NAME token (Psalm's name location),
                    // not the first modifier of the declaration.
                    let (name_start, name_end) = method_info.name_location.unwrap_or((
                        method_info.start_offset,
                        method_info.start_offset.saturating_add(1),
                    ));
                    if is_private_constructor {
                        emit(
                            IssueKind::UnusedConstructor,
                            format!("Cannot find any calls to private constructor {}", method_id),
                            name_start,
                            name_end,
                            method_info.file_path,
                        );
                    } else if matches!(method_info.visibility, Visibility::Private) {
                        emit(
                            IssueKind::UnusedMethod,
                            format!("Cannot find any calls to private method {}", method_id),
                            name_start,
                            name_end,
                            method_info.file_path,
                        );
                    } else {
                        emit(
                            IssueKind::PossiblyUnusedMethod,
                            format!("Cannot find any calls to method {}", method_id),
                            name_start,
                            name_end,
                            method_info.file_path,
                        );
                    }
                } else if method_info.get_return_type().is_some_and(|return_type| {
                    !return_type.is_void()
                        && !return_type.is_nothing()
                        // Psalm skips probably-fluent methods (returning
                        // static/$this) unless static.
                        && !(!method_info.is_static
                            && return_type.types.iter().any(|atomic| matches!(
                                atomic,
                                pzoom_code_info::TAtomic::TNamedObject { is_static: true, .. }
                            ) || matches!(
                                atomic,
                                pzoom_code_info::TAtomic::TNamedObject { name, .. }
                                    if *name == StrId::STATIC || *name == *class_id
                            )))
                }) && !method_returns_used.contains(&(*class_id, method_lc))
                    && !method_info.declaring_class.is_some_and(|declaring| {
                        method_returns_used.contains(&(declaring, method_lc))
                    })
                {
                    let (start, end) = method_info
                        .return_type_location
                        .unwrap_or((method_info.start_offset, method_info.start_offset + 1));
                    // Psalm: a private method's unused return is the definite
                    // UnusedReturnValue; a public/protected one is the
                    // possibly-unused variant (it may be called externally).
                    if matches!(method_info.visibility, Visibility::Private) {
                        emit(
                            IssueKind::UnusedReturnValue,
                            "The return value for this private method is never used".to_string(),
                            start,
                            end,
                            method_info.file_path,
                        );
                    } else {
                        emit(
                            IssueKind::PossiblyUnusedReturnValue,
                            "The return value for this method is never used".to_string(),
                            start,
                            end,
                            method_info.file_path,
                        );
                    }
                }
            }

            // Properties (Psalm checkPropertyReferences).
            for (prop_name_id, prop_info) in &class_info.properties {
                let declared_here = prop_info.declaring_class == *class_id
                    || class_info.used_traits.contains(&prop_info.declaring_class);
                if !declared_here || prop_info.is_promoted {
                    continue;
                }
                if class_info.is_public_api
                    && (matches!(prop_info.visibility, Visibility::Public)
                        || (matches!(prop_info.visibility, Visibility::Protected)
                            && !class_info.is_final))
                {
                    continue;
                }
                let prop_referenced = referenced_properties.contains(&(*class_id, *prop_name_id))
                    || referenced_properties.contains(&(prop_info.declaring_class, *prop_name_id));
                if prop_referenced {
                    continue;
                }
                // An overriding property defers to its parent's verdict
                // (Psalm's `overridden_property_ids`): a property that
                // redeclares one from any ancestor — including ancestors
                // declared in a stub or a scanned-only dependency (vendor),
                // whose accesses pzoom never analyzes — stays alive, mirroring
                // the non-reportable-parent handling for methods above.
                let overrides_parent = class_info
                    .parent_class
                    .iter()
                    .chain(class_info.all_parent_classes.iter())
                    .any(|parent_id| {
                        codebase
                            .get_class(*parent_id)
                            .is_some_and(|parent| parent.properties.contains_key(prop_name_id))
                    });
                if overrides_parent {
                    continue;
                }
                let prop_name = interner.lookup(*prop_name_id);
                let property_id = format!("{}::${}", class_name, prop_name);
                // The stored offset already points at the leading `$`; span the
                // whole `$name` token so the highlight matches the property name.
                let name_start = prop_info.start_offset;
                let name_end = name_start.saturating_add(1 + prop_name.len() as u32);
                // A trait-supplied property is declared in the trait's file.
                let prop_decl_file = codebase
                    .get_class(prop_info.declaring_class)
                    .map_or(file_path, |declaring| declaring.file_path);
                if matches!(prop_info.visibility, Visibility::Private) {
                    emit(
                        IssueKind::UnusedProperty,
                        format!(
                            "Cannot find any references to private property {}",
                            property_id
                        ),
                        name_start,
                        name_end,
                        prop_decl_file,
                    );
                } else {
                    emit(
                        IssueKind::PossiblyUnusedProperty,
                        format!("Cannot find any references to property {}", property_id),
                        name_start,
                        name_end,
                        prop_decl_file,
                    );
                }
            }
        }

        // ClassMustBeFinal (Psalm consolidateAnalyzedData).
        let has_children = codebase
            .all_classlike_descendants
            .get(class_id)
            .is_some_and(|descendants| !descendants.is_empty());
        if !class_info.is_public_api
            && !has_children
            && !class_info.is_abstract
            && !class_info.is_final
            && class_info.kind == ClassLikeKind::Class
        {
            emit(
                IssueKind::ClassMustBeFinal,
                format!(
                    "Class {} is never extended and is not part of the public API, and thus must be made final.",
                    class_name
                ),
                class_info.start_offset,
                class_info.start_offset.saturating_add(1),
                file_path,
            );
        }
    }

    new_issues.sort_by_key(|issue| issue.location.start_offset);
    new_issues
}
