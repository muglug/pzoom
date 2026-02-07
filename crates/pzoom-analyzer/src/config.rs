//! Configuration for the analyzer.

use rustc_hash::{FxHashMap, FxHashSet};

/// Error level for analysis (Psalm-compatible).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorLevel {
    /// Most strict - report all issues.
    Level1 = 1,
    /// Default level.
    #[default]
    Level2 = 2,
    /// Less strict.
    Level3 = 3,
    /// Relaxed.
    Level4 = 4,
    /// Most relaxed.
    Level5 = 5,
    /// Most lenient - only critical errors.
    Level6 = 6,
    /// Very lenient.
    Level7 = 7,
    /// Extremely lenient.
    Level8 = 8,
}

impl ErrorLevel {
    pub fn from_int(n: u8) -> Self {
        match n {
            1 => ErrorLevel::Level1,
            2 => ErrorLevel::Level2,
            3 => ErrorLevel::Level3,
            4 => ErrorLevel::Level4,
            5 => ErrorLevel::Level5,
            6 => ErrorLevel::Level6,
            7 => ErrorLevel::Level7,
            8 => ErrorLevel::Level8,
            _ => ErrorLevel::Level2,
        }
    }
}

/// Configuration options for analysis.
#[derive(Clone, Debug)]
pub struct Config {
    /// Directories to analyze.
    pub project_dirs: Vec<String>,

    /// File patterns to exclude.
    pub exclude_patterns: Vec<String>,

    /// Issue types to suppress.
    pub suppressed_issues: FxHashSet<String>,

    /// Issue suppressions scoped to file/directory patterns from Psalm issueHandlers.
    pub issue_handler_suppressions: FxHashMap<String, Vec<String>>,

    /// PHP version to target (e.g., "8.2").
    pub php_version: String,

    /// Whether to use strict types by default.
    pub strict_types: bool,

    /// Whether to enable taint analysis.
    pub taint_analysis: bool,

    /// Maximum depth for taint tracking.
    pub taint_max_depth: u32,

    /// Whether to report unused code.
    pub report_unused: bool,

    /// Number of threads for parallel analysis.
    pub threads: usize,

    /// Path to cache directory.
    pub cache_dir: Option<String>,

    /// Error level (1-8, Psalm-compatible).
    pub error_level: ErrorLevel,

    /// Whether to use docblock types for type inference.
    pub use_docblock_types: bool,

    /// Whether to report mixed type issues.
    pub report_mixed_issues: bool,

    /// Stub files for external type definitions.
    pub stubs: Vec<String>,

    /// Functions that are forbidden.
    pub forbidden_functions: FxHashSet<String>,

    /// Whether to find unused Psalm suppress annotations.
    pub find_unused_suppress: bool,

    /// Path to a Psalm-style error baseline XML file.
    pub error_baseline: Option<String>,

    /// Whether to report unused baseline entries.
    pub find_unused_baseline_entry: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project_dirs: vec![".".to_string()],
            exclude_patterns: vec!["vendor/**".to_string(), "tests/**".to_string()],
            suppressed_issues: FxHashSet::default(),
            issue_handler_suppressions: FxHashMap::default(),
            php_version: "8.2".to_string(),
            strict_types: false,
            taint_analysis: false,
            taint_max_depth: 20,
            report_unused: false,
            threads: num_cpus(),
            cache_dir: None,
            error_level: ErrorLevel::default(),
            use_docblock_types: true,
            report_mixed_issues: true,
            stubs: Vec::new(),
            forbidden_functions: FxHashSet::default(),
            find_unused_suppress: false,
            error_baseline: None,
            find_unused_baseline_entry: false,
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if an issue type should be suppressed.
    pub fn is_issue_suppressed(&self, issue_type: &str) -> bool {
        self.suppressed_issues.contains(issue_type)
    }

    /// Check if an issue type should be suppressed for a specific display-relative file path.
    pub fn is_issue_suppressed_for_path(&self, issue_type: &str, file_path: &str) -> bool {
        if self.is_issue_suppressed(issue_type) {
            return true;
        }

        let Some(patterns) = self.issue_handler_suppressions.get(issue_type) else {
            return false;
        };

        let normalized_path = normalize_path(file_path);

        patterns.iter().any(|pattern| {
            if let Some(dir) = pattern.strip_suffix("/**") {
                normalized_path == dir || normalized_path.starts_with(&format!("{}/", dir))
            } else {
                normalized_path == *pattern || normalized_path.ends_with(&format!("/{}", pattern))
            }
        })
    }

    pub fn add_issue_handler_suppression_pattern(&mut self, issue_type: &str, pattern: String) {
        let normalized_pattern = normalize_path(&pattern);
        self.issue_handler_suppressions
            .entry(issue_type.to_string())
            .or_default()
            .push(normalized_pattern);
    }
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}
