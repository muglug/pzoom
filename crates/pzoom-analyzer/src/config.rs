//! Configuration for the analyzer.

use pzoom_code_info::issue::IssueKind;
use rustc_hash::{FxHashMap, FxHashSet};

/// Error level for analysis (Psalm-compatible).
///
/// Mirrors Psalm's `Config::$level` (1-8). A lower number is stricter: every
/// issue whose [`IssueKind::error_level`] is a positive value below the
/// configured level is downgraded from *error* to *info*. Psalm defaults to
/// level 1 (`public int $level = 1;`), at which nothing is downgraded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorLevel {
    /// Most strict - report all issues (Psalm's default).
    #[default]
    Level1 = 1,
    /// Less strict.
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

    /// The level as the comparable integer Psalm uses (`$this->level`).
    pub fn as_int(self) -> i8 {
        self as i8
    }
}

/// How a single emitted issue should be surfaced, mirroring Psalm's
/// `Config::REPORT_*` reporting levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportingLevel {
    /// Reported as an error (fails the run / non-zero exit). Psalm `REPORT_ERROR`.
    Error,
    /// Reported as informational only (does not fail the run). Psalm `REPORT_INFO`.
    Info,
    /// Not reported at all. Psalm `REPORT_SUPPRESS`.
    Suppress,
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

    /// Unscoped per-issue reporting-level overrides declared by an
    /// `<IssueName errorLevel="info|error"/>` handler (Psalm's
    /// `IssueHandler::$error_level`). Presence of a key also means an issue
    /// handler exists for that type, which — as in Psalm's
    /// `Config::getReportingLevelForFile` — takes precedence over the
    /// `ERROR_LEVEL`-vs-`level` downgrade. `errorLevel="suppress"` is recorded
    /// in [`Self::suppressed_issues`]/[`Self::issue_handler_suppressions`]
    /// instead, so only `Error`/`Info` ever appear here.
    pub issue_handler_levels: FxHashMap<String, ReportingLevel>,

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

    /// Compiled-in analysis plugins active for this project (the pzoom analogue
    /// of Hakana's `Config.hooks`). Populated during config load from the
    /// project's declared dependencies — see [`crate::plugin`]. Shared by
    /// reference across analysis threads, hence `Arc`.
    pub plugins: Vec<std::sync::Arc<dyn crate::plugin::Plugin>>,

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
            issue_handler_levels: FxHashMap::default(),
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
            plugins: Vec::new(),
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

    /// Compute the reporting level for an emitted issue, mirroring Psalm's
    /// `Config::getReportingLevelForIssue` / `getReportingLevelForFile`.
    ///
    /// Precedence (matching Psalm):
    /// 1. A path-scoped or global `errorLevel="suppress"` handler →
    ///    [`ReportingLevel::Suppress`].
    /// 2. An unscoped `<IssueName errorLevel="info|error"/>` handler wins over
    ///    the level-based downgrade (Psalm delegates to the issue handler when
    ///    one exists for the type).
    /// 3. Otherwise the per-issue [`IssueKind::error_level`] is compared to the
    ///    configured [`Self::error_level`]: a positive issue level strictly
    ///    below the configured level is downgraded to [`ReportingLevel::Info`];
    ///    everything else is an [`ReportingLevel::Error`].
    ///
    /// `file_path` is the display-relative path used for scoped handlers.
    pub fn reporting_level_for_issue(&self, kind: IssueKind, file_path: &str) -> ReportingLevel {
        let issue_type = format!("{kind:?}");

        if self.is_issue_suppressed_for_path(&issue_type, file_path) {
            return ReportingLevel::Suppress;
        }

        if let Some(level) = self.issue_handler_levels.get(&issue_type) {
            return *level;
        }

        let issue_level = kind.error_level();
        if issue_level > 0 && issue_level < self.error_level.as_int() {
            return ReportingLevel::Info;
        }

        ReportingLevel::Error
    }

    /// Record an unscoped `<IssueName errorLevel="info|error"/>` handler level.
    /// `"suppress"` is handled via the suppression maps, so it is ignored here.
    pub fn set_issue_handler_level(&mut self, issue_type: &str, level: &str) {
        let reporting = match level {
            "info" => ReportingLevel::Info,
            "error" => ReportingLevel::Error,
            _ => return,
        };
        self.issue_handler_levels
            .insert(issue_type.to_string(), reporting);
    }

    /// Record that an issue handler exists for `issue_type` without an explicit
    /// unscoped level. Like Psalm's `IssueHandler::$error_level` default of
    /// `REPORT_ERROR`, the mere presence of a handler makes the type report at
    /// *error* regardless of the configured level (the level-based downgrade is
    /// bypassed). A later explicit `info`/`error` override replaces this.
    pub fn note_issue_handler(&mut self, issue_type: &str) {
        self.issue_handler_levels
            .entry(issue_type.to_string())
            .or_insert(ReportingLevel::Error);
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

#[cfg(test)]
mod reporting_level_tests {
    use super::{Config, ErrorLevel, ReportingLevel};
    use pzoom_code_info::issue::IssueKind;

    #[test]
    fn default_level_is_one_like_psalm() {
        // Psalm's `Config::$level` defaults to 1, at which nothing is downgraded.
        assert_eq!(Config::default().error_level, ErrorLevel::Level1);
    }

    #[test]
    fn level_one_reports_everything_as_error() {
        let config = Config::default();
        // MixedAssignment has ERROR_LEVEL 1; at level 1 it stays an error.
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::MixedAssignment, "src/A.php"),
            ReportingLevel::Error,
        );
    }

    #[test]
    fn lower_level_issues_downgrade_to_info() {
        let mut config = Config::default();
        config.error_level = ErrorLevel::Level5;
        // ERROR_LEVEL 1 < 5 -> info.
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::MixedAssignment, "src/A.php"),
            ReportingLevel::Info,
        );
        // ERROR_LEVEL 3 < 5 -> info.
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::PossiblyInvalidArgument, "src/A.php"),
            ReportingLevel::Info,
        );
        // ERROR_LEVEL 6 >= 5 -> still an error.
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::InvalidArgument, "src/A.php"),
            ReportingLevel::Error,
        );
    }

    #[test]
    fn negative_level_issues_never_downgrade() {
        let mut config = Config::default();
        config.error_level = ErrorLevel::Level8;
        // UndefinedClass inherits the -1 base; never downgraded even at level 8.
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::UndefinedClass, "src/A.php"),
            ReportingLevel::Error,
        );
    }

    #[test]
    fn global_suppression_wins() {
        let mut config = Config::default();
        config.error_level = ErrorLevel::Level8;
        config
            .suppressed_issues
            .insert("MixedAssignment".to_string());
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::MixedAssignment, "src/A.php"),
            ReportingLevel::Suppress,
        );
    }

    #[test]
    fn unscoped_handler_level_overrides_downgrade() {
        let mut config = Config::default();
        config.error_level = ErrorLevel::Level8;

        // A bare handler keeps the issue at error, bypassing the level downgrade.
        config.note_issue_handler("MixedAssignment");
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::MixedAssignment, "src/A.php"),
            ReportingLevel::Error,
        );

        // An explicit errorLevel="info" forces info regardless of level.
        config.set_issue_handler_level("InvalidArgument", "info");
        assert_eq!(
            config.reporting_level_for_issue(IssueKind::InvalidArgument, "src/A.php"),
            ReportingLevel::Info,
        );
    }
}
