//! Phase 3: Analyzing - Type check and detect issues.
//!
//! The analyzer walks the AST and performs type checking,
//! detecting issues like type mismatches, undefined references,
//! and potential bugs.

use bumpalo::Bump;
use pzoom_analyzer::Config;
use pzoom_analyzer::context::BlockContext;
use pzoom_analyzer::function_analysis_data::FunctionAnalysisData;
use pzoom_analyzer::statements_analyzer::StatementsAnalyzer;
use pzoom_analyzer::stmt_analyzer;
use pzoom_code_info::{CodebaseInfo, Issue};
use pzoom_str::{Interner, StrId};
use pzoom_syntax::{FileId, parse_file_content, resolve_names};
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
                    let mut group_issues = Vec::new();

                    for file_path in file_group {
                        group_issues.extend(self.analyze_file(file_path));
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
            self.config,
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
        analysis_data
            .issues
            .into_iter()
            .filter(|issue| !is_line_suppressed(&file_info.contents, issue))
            .collect()
    }
}

fn is_line_suppressed(contents: &str, issue: &Issue) -> bool {
    let issue_name = format!("{:?}", issue.kind);
    let lines: Vec<&str> = contents.lines().collect();
    let issue_line = issue.start_line as usize;

    if issue_line == 0 || issue_line > lines.len() + 1 {
        return false;
    }

    let mut line_no = issue_line;

    while line_no > 0 && line_no <= lines.len() {
        let line = lines[line_no - 1];
        if line_suppresses_issue(line, &issue_name) {
            return true;
        }

        if line_no == issue_line {
            line_no -= 1;
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            line_no -= 1;
            continue;
        }

        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/");

        if !is_comment {
            break;
        }

        line_no -= 1;
    }

    if function_docblock_suppresses_issue(&lines, issue_line, &issue_name) {
        return true;
    }

    false
}

fn line_suppresses_issue(line: &str, issue_name: &str) -> bool {
    let Some(tag_pos) = line.find("@psalm-suppress") else {
        return false;
    };

    let suppress_content = &line[tag_pos + "@psalm-suppress".len()..];
    suppress_content
        .split(|c: char| c.is_whitespace() || c == ',' || c == '*')
        .filter(|token| !token.is_empty())
        .any(|token| token.eq_ignore_ascii_case("all") || suppresses_issue(token, issue_name))
}

fn function_docblock_suppresses_issue(lines: &[&str], issue_line: usize, issue_name: &str) -> bool {
    if issue_line == 0 || issue_line > lines.len() {
        return false;
    }

    let mut function_line = None;
    for line_no in (1..=issue_line).rev() {
        let line = lines[line_no - 1].trim_start();
        if line.contains("function ") || line.contains(" fn ") {
            function_line = Some(line_no);
            break;
        }
    }

    let Some(function_line) = function_line else {
        return false;
    };

    if !line_is_within_function_scope(lines, function_line, issue_line) {
        return false;
    }

    let mut line_no = function_line.saturating_sub(1);
    while line_no > 0 {
        let line = lines[line_no - 1];
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            line_no -= 1;
            continue;
        }

        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/");

        if !is_comment {
            break;
        }

        if line_suppresses_issue(line, issue_name) {
            return true;
        }

        line_no -= 1;
    }

    false
}

fn line_is_within_function_scope(lines: &[&str], function_line: usize, issue_line: usize) -> bool {
    if issue_line < function_line {
        return false;
    }

    let mut depth: isize = 0;
    for line in &lines[(function_line - 1)..issue_line] {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }

    depth > 0
}

fn suppresses_issue(token: &str, issue_name: &str) -> bool {
    if token == issue_name {
        return true;
    }

    match token {
        "MixedArgument" => {
            return matches!(issue_name, "MixedArgument" | "MixedArgumentTypeCoercion");
        }
        "MixedReturnStatement" | "MixedInferredReturnType" => {
            return issue_name == "MixedReturnStatement";
        }
        "RedundantCastGivenDocblockType" => {
            return issue_name == "RedundantCast";
        }
        "RedundantConditionGivenDocblockType" => {
            return issue_name == "RedundantConditionGivenDocblockType";
        }
        _ => {}
    }

    if let Some(base) = token.strip_suffix("GivenDocblockType") {
        return issue_name == base;
    }

    false
}
