//! Phase 3: Analyzing - Type check and detect issues.
//!
//! The analyzer drives type checking across the project's files, distributing
//! them across worker threads. Per-file analysis (parse, resolve, run the
//! statement analyzer, collect issues) lives in
//! [`pzoom_analyzer::file_analyzer::FileAnalyzer`], mirroring Psalm's split
//! between the project `Analyzer` and `FileAnalyzer`.

use pzoom_analyzer::Config;
use pzoom_analyzer::file_analyzer::FileAnalyzer;
use pzoom_code_info::{CodebaseInfo, Issue};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

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
        if (files_to_analyze.len() / group_size) < 4 {
            group_size = 1;
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

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(file_groups.len());

            for (_, file_group) in file_groups {
                handles.push(scope.spawn(move || {
                    let file_analyzer = FileAnalyzer::new(self.codebase, self.interner, self.config);
                    let mut group_issues = Vec::new();

                    for file_path in file_group {
                        group_issues.extend(file_analyzer.analyze(file_path));
                    }

                    group_issues
                }));
            }

            for handle in handles {
                issues.extend(handle.join().unwrap());
            }
        });

        issues
    }
}
