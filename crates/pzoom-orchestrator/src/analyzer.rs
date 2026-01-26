//! Phase 3: Analyzing - Type check and detect issues.
//!
//! The analyzer walks the AST and performs type checking,
//! detecting issues like type mismatches, undefined references,
//! and potential bugs.

use bumpalo::Bump;
use pzoom_analyzer::context::BlockContext;
use pzoom_analyzer::function_analysis_data::FunctionAnalysisData;
use pzoom_analyzer::stmt_analyzer;
use pzoom_analyzer::statements_analyzer::StatementsAnalyzer;
use pzoom_analyzer::Config;
use pzoom_code_info::{CodebaseInfo, Issue};
use pzoom_str::{Interner, StrId};
use pzoom_syntax::{parse_file_content, resolve_names, FileId};
use rayon::prelude::*;

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
        let files: Vec<_> = self.codebase.files.keys().collect();

        // Analyze files in parallel
        let issues: Vec<Issue> = files
            .par_iter()
            .flat_map(|file_path| self.analyze_file(**file_path))
            .collect();

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
        // Analyze only the specified files in parallel
        let issues: Vec<Issue> = files_to_analyze
            .par_iter()
            .flat_map(|file_path| self.analyze_file(*file_path))
            .collect();

        AnalysisResult {
            file_count: files_to_analyze.len(),
            issues,
        }
    }

    /// Analyze a single file.
    fn analyze_file(&self, file_path: StrId) -> Vec<Issue> {
        let Some(file_info) = self.codebase.files.get(&file_path) else {
            return Vec::new();
        };

        let path_str = self.interner.lookup(file_path);

        // Create arena for parsing
        let arena = Bump::new();
        let file_id = FileId::new(&*path_str);

        // Re-parse the file
        let (program, _parse_error) = parse_file_content(&arena, file_id, &file_info.contents);

        // Resolve names (handle use statements, namespace aliases, etc.)
        let resolved_names = resolve_names(&program, self.interner);

        // Create the analyzer context
        let stmt_analyzer = StatementsAnalyzer::new(
            self.codebase,
            self.interner,
            file_path,
            &file_info.contents,
            &resolved_names,
        );

        // Create analysis data and context
        let mut analysis_data = FunctionAnalysisData::new();
        let mut context = BlockContext::new();

        // Analyze the program's statements
        let _ = stmt_analyzer::analyze_stmts(
            &stmt_analyzer,
            program.statements.as_slice(),
            &mut analysis_data,
            &mut context,
        );

        // Return collected issues
        let _ = self.config;
        analysis_data.issues
    }
}
