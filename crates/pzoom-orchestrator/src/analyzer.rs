//! Phase 3: Analyzing - Type check and detect issues.
//!
//! The analyzer drives type checking across the project's files, distributing
//! them across worker threads. Per-file analysis (parse, resolve, run the
//! statement analyzer, collect issues) lives in
//! [`pzoom_analyzer::file_analyzer::FileAnalyzer`], mirroring Psalm's split
//! between the project `Analyzer` and `FileAnalyzer`.

use pzoom_analyzer::Config;
use pzoom_analyzer::file_analyzer::{FileAnalyzer, FileReferenceData};
use pzoom_code_info::symbol_references::SymbolReferences;
use pzoom_code_info::{CodebaseInfo, Issue};
use pzoom_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Result of the analysis phase.
pub struct AnalysisResult {
    pub issues: Vec<Issue>,
    pub file_count: usize,
}

/// The analyzer phase of the pipeline.
pub struct Analyzer<'a> {
    codebase: &'a CodebaseInfo,
    interner: &'a Interner,
    config: &'a Config,
}

impl<'a> Analyzer<'a> {
    pub fn new(codebase: &'a CodebaseInfo, interner: &'a Interner, config: &'a Config) -> Self {
        Self {
            codebase,
            interner,
            config,
        }
    }

    /// Run the analysis phase on all files.
    pub fn analyze(&self) -> AnalysisResult {
        let files: Vec<_> = self.codebase.files.keys().copied().collect();
        let issues = self.analyze_in_groups(&files);

        AnalysisResult {
            file_count: files.len(),
            issues,
        }
    }

    /// Run the analysis phase on specific files only.
    ///
    /// This allows analyzing a subset of the codebase while still having
    /// access to the full type information from scanning the entire project.
    pub fn analyze_files(&self, files_to_analyze: &[StrId]) -> AnalysisResult {
        let issues = self.analyze_in_groups(files_to_analyze);

        AnalysisResult {
            file_count: files_to_analyze.len(),
            issues,
        }
    }

    fn analyze_in_groups(&self, files_to_analyze: &[StrId]) -> Vec<Issue> {
        if files_to_analyze.is_empty() {
            return Vec::new();
        }

        // Match Hakana's strategy: distribute files across N groups and spawn one worker per group.
        let mut group_size = self.config.threads.max(1);
        if (files_to_analyze.len() / group_size) < 4 || cfg!(target_arch = "wasm32") {
            group_size = 1;
        }

        // Single group: analyze inline. Avoids spawn overhead, and threads are
        // unavailable on wasm32.
        if group_size == 1 {
            let file_analyzer = FileAnalyzer::new(self.codebase, self.interner, self.config);
            let mut issues = Vec::new();
            let mut refs = ReferenceAccumulator::default();
            for file_path in files_to_analyze {
                let (file_issues, file_refs) = file_analyzer.analyze(*file_path);
                issues.extend(file_issues);
                refs.merge(file_refs);
            }
            return self.finalize_unused(files_to_analyze, issues, refs);
        }

        let mut file_groups = FxHashMap::default();
        for (i, file_path) in files_to_analyze.iter().enumerate() {
            let group = i % group_size;
            file_groups
                .entry(group)
                .or_insert_with(Vec::new)
                .push(*file_path);
        }

        let mut issues = Vec::new();
        let mut refs = ReferenceAccumulator::default();

        // NOTE: FileInfo records mago's parser diagnostics (parse_errors), but
        // they are not surfaced as ParseError issues yet: mago recovers from
        // several constructs it mis-flags (multiline double-quoted strings,
        // `as final` trait aliases), so blanket surfacing produces false
        // positives. Revisit when mago's parser matures.

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(file_groups.len());

            for (_, file_group) in file_groups {
                handles.push(scope.spawn(move || {
                    let file_analyzer =
                        FileAnalyzer::new(self.codebase, self.interner, self.config);
                    let mut group_issues = Vec::new();
                    let mut group_refs = ReferenceAccumulator::default();

                    for file_path in file_group {
                        let (file_issues, file_refs) = file_analyzer.analyze(file_path);
                        group_issues.extend(file_issues);
                        group_refs.merge(file_refs);
                    }

                    (group_issues, group_refs)
                }));
            }

            for handle in handles {
                let (group_issues, group_refs) = handle.join().unwrap();
                issues.extend(group_issues);
                refs.merge_accumulator(group_refs);
            }
        });

        self.finalize_unused(files_to_analyze, issues, refs)
    }

    /// Codebase-wide unused-definition pass (Hakana's `find_unused_definitions`),
    /// run once the per-file reference graphs have been merged.
    fn finalize_unused(
        &self,
        files_to_analyze: &[StrId],
        mut issues: Vec<Issue>,
        refs: ReferenceAccumulator,
    ) -> Vec<Issue> {
        if self.config.find_unused_code {
            issues.extend(pzoom_analyzer::unused_symbols::find_unused_definitions(
                self.codebase,
                self.interner,
                self.config,
                files_to_analyze,
                &refs.symbol_references,
                &refs.referenced_properties,
                &refs.method_returns_used,
            ));
        }
        issues
    }
}

/// Accumulates every analyzed file's [`FileReferenceData`] into one codebase-wide
/// reference graph for the unused-definition pass.
#[derive(Default)]
struct ReferenceAccumulator {
    symbol_references: SymbolReferences,
    referenced_properties: FxHashSet<(StrId, StrId)>,
    method_returns_used: FxHashSet<(StrId, StrId)>,
}

impl ReferenceAccumulator {
    fn merge(&mut self, data: FileReferenceData) {
        self.symbol_references.extend(data.symbol_references);
        self.referenced_properties
            .extend(data.referenced_properties);
        self.method_returns_used.extend(data.method_returns_used);
    }

    fn merge_accumulator(&mut self, other: ReferenceAccumulator) {
        self.symbol_references.extend(other.symbol_references);
        self.referenced_properties
            .extend(other.referenced_properties);
        self.method_returns_used.extend(other.method_returns_used);
    }
}
