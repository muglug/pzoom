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

    /// Per-issue `<referencedProperty name="Class::$prop"/>` suppressions from
    /// psalm.xml issue handlers (Psalm's `PropertyIssueHandlerType`).
    pub issue_property_suppressions: FxHashMap<String, Vec<String>>,

    /// PHP version to target (e.g., "8.2").
    pub php_version: String,

    /// Whether `php_version` was set explicitly (psalm.xml `phpVersion`),
    /// as opposed to defaulted; when false, composer.json `require.php`
    /// inference may apply (Psalm's cli > config > composer precedence).
    pub php_version_explicit: bool,

    /// Require `#[Override]` on methods that override an inherited method
    /// (Psalm's `ensureOverrideAttribute` config flag; default false).
    pub ensure_override_attribute: bool,

    /// Report a literal int offset into an array whose presence is not proven
    /// (Psalm's `ensureArrayIntOffsetsExist`; default false).
    pub ensure_array_int_offsets_exist: bool,

    /// Report a literal string offset into an array whose presence is not proven
    /// (Psalm's `ensureArrayStringOffsetsExist`; default false).
    pub ensure_array_string_offsets_exist: bool,

    /// Whether to use strict types by default.
    pub strict_types: bool,

    /// Whether to enable taint analysis.
    pub taint_analysis: bool,

    /// Maximum depth for taint tracking.
    pub taint_max_depth: u32,

    /// Literal strings at or over this length degrade to
    /// non-empty-/non-falsy-string (Psalm's `maxStringLength`).
    pub max_string_length: usize,

    /// Whether to report unused code.
    pub report_unused: bool,

    /// Whether to report unused declarations (classes, methods, properties,
    /// params) after analysis — Psalm's `find_unused_code` /
    /// `Codebase::reportUnusedCode()` (which also turns on unused-variable
    /// reporting).
    pub find_unused_code: bool,

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

    /// Stub files registered by well-known Psalm plugins named in
    /// `<plugins><pluginClass .../></plugins>` (pzoom has no plugin runtime;
    /// the stubs those plugins would `addStubFile` are loaded directly).
    pub plugin_stubs: Vec<String>,

    /// Functions that are forbidden.
    pub forbidden_functions: FxHashSet<String>,

    /// Whether to find unused Psalm suppress annotations.
    pub find_unused_suppress: bool,

    /// Psalm's `limitMethodComplexity`: emit ComplexMethod/ComplexFunction
    /// for function-likes whose data-flow graph exceeds the size/path-length
    /// limits.
    pub limit_method_complexity: bool,

    /// Psalm's `allConstantsGlobal`: treat every scanned `define()` as a
    /// global constant regardless of call flow.
    pub all_constants_global: bool,

    /// Path to a Psalm-style error baseline XML file.
    pub error_baseline: Option<String>,

    /// Whether to report unused baseline entries.
    pub find_unused_baseline_entry: bool,

    /// Optional extensions enabled via psalm.xml `<enableExtensions>`.
    pub enabled_extensions: Vec<String>,

    /// Optional extensions disabled via psalm.xml `<disableExtensions>`
    /// (wins over every other enablement source).
    pub disabled_extensions: Vec<String>,
}

impl Config {
    /// The configured PHP version as a comparable id (e.g. "7.1" -> 70100),
    /// mirroring Psalm's `analysis_php_version_id`.
    pub fn php_version_id(&self) -> u32 {
        let mut parts = self.php_version.split('.');
        let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(8);
        let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        major * 10_000 + minor * 100
    }
}

/// Parse a "X.Y.Z" PHP version string into a (major, minor, patch) tuple,
/// defaulting the major to 8 and missing components to 0.
pub fn parse_php_version_tuple(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    (major, minor, patch)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project_dirs: vec![".".to_string()],
            exclude_patterns: vec!["vendor/**".to_string(), "tests/**".to_string()],
            suppressed_issues: FxHashSet::default(),
            issue_handler_suppressions: FxHashMap::default(),
            issue_property_suppressions: FxHashMap::default(),
            php_version: "8.2".to_string(),
            php_version_explicit: false,
            ensure_override_attribute: false,
            ensure_array_int_offsets_exist: false,
            ensure_array_string_offsets_exist: false,
            strict_types: false,
            taint_analysis: false,
            taint_max_depth: 20,
            max_string_length: pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
            report_unused: false,
            find_unused_code: false,
            threads: num_cpus(),
            cache_dir: None,
            error_level: ErrorLevel::default(),
            use_docblock_types: true,
            report_mixed_issues: true,
            stubs: Vec::new(),
            plugin_stubs: Vec::new(),
            forbidden_functions: FxHashSet::default(),
            find_unused_suppress: false,
            limit_method_complexity: false,
            all_constants_global: false,
            error_baseline: None,
            find_unused_baseline_entry: false,
            enabled_extensions: Vec::new(),
            disabled_extensions: Vec::new(),
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

    /// Check if an issue type is suppressed for a specific property id
    /// (`Class::$prop`, declaring-class based) via `<referencedProperty>`.
    pub fn is_issue_suppressed_for_property(&self, issue_type: &str, property_id: &str) -> bool {
        self.issue_property_suppressions
            .get(issue_type)
            .is_some_and(|ids| ids.iter().any(|id| id == property_id))
    }

    pub fn add_issue_property_suppression(&mut self, issue_type: &str, property_id: String) {
        self.issue_property_suppressions
            .entry(issue_type.to_string())
            .or_default()
            .push(property_id.trim_start_matches('\\').to_string());
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
    let stripped = normalized.strip_prefix("./").unwrap_or(&normalized);
    // Collapse redundant slashes so a config directory written with a trailing
    // slash (`<directory name="src/Foo/"/>` -> pattern `src/Foo//**`) still
    // matches `src/Foo/Bar.php`.
    let mut result = String::with_capacity(stripped.len());
    let mut prev_was_slash = false;
    for ch in stripped.chars() {
        if ch == '/' {
            if !prev_was_slash {
                result.push('/');
            }
            prev_was_slash = true;
        } else {
            result.push(ch);
            prev_was_slash = false;
        }
    }
    result
}
