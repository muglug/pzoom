//! Phase 1: Scanning - Parse files and collect symbols.
//!
//! The scanner discovers PHP files, parses them using the mago parser,
//! and extracts class/function/constant declarations.

use std::path::Path;

use bumpalo::Bump;
use glob::Pattern;
use pzoom_code_info::CodebaseInfo;
use pzoom_code_info::codebase_info::FileInfo;
use pzoom_str::{Interner, StrId, ThreadedInterner};
use std::sync::Arc;
use pzoom_code_info::class_type_alias::ClassTypeAlias;
use pzoom_syntax::declaration_collector::CollectedDeclarations;
use pzoom_syntax::{DeclarationCollector, FileId, parse_file_content};
use rustc_hash::FxHashMap;
use rust_embed::Embed;
use rustc_hash::FxHashSet;
use walkdir::WalkDir;

/// Embedded stub files for PHP built-in types and functions.
#[derive(Embed)]
#[folder = "../../stubs/"]
struct EmbeddedStubs;

/// `Some(extension_name)` when the stub path points into
/// `extensions/optional/` — the PECL/third-party stubs that are only loaded
/// for explicitly-enabled extensions.
fn optional_extension_name(path: &str) -> Option<&str> {
    let idx = path.find("extensions/optional/")?;
    let rest = &path[idx + "extensions/optional/".len()..];
    rest.strip_suffix(".phpstub")
        .filter(|stem| !stem.contains('/'))
}

/// `Some(version_id)` for version overlay stubs
/// (`_php_versions/Php82.phpstub` -> 80200): the stub only applies when the
/// analysis PHP version is at least that version.
fn version_overlay_min_id(path: &str) -> Option<u32> {
    let idx = path.find("_php_versions/")?;
    let rest = &path[idx + "_php_versions/".len()..];
    let stem = rest.strip_suffix(".phpstub")?.strip_prefix("Php")?;
    if stem.len() != 2 {
        return None;
    }
    let major: u32 = stem[0..1].parse().ok()?;
    let minor: u32 = stem[1..2].parse().ok()?;
    Some(major * 10_000 + minor * 100)
}

/// Result of the scanning phase.
pub struct ScanResult {
    pub codebase: CodebaseInfo,
    pub interner: Interner,
    pub file_count: usize,
    pub errors: Vec<ScanError>,
    /// Files that are stubs (issues from these should typically be filtered out).
    pub stub_files: FxHashSet<StrId>,
}

/// An error encountered during scanning.
#[derive(Debug)]
pub struct ScanError {
    pub file_path: String,
    pub message: String,
    pub line: Option<u32>,
}

/// The scanner phase of the pipeline.
pub struct Scanner {
    interner: Arc<Interner>,
    codebase: CodebaseInfo,
    errors: Vec<ScanError>,
    file_count: usize,
    stub_files: FxHashSet<StrId>,
    /// Whether we're currently scanning stubs (affects how files are tracked).
    scanning_stubs: bool,
    /// Whether we're scanning dependency sources (vendor/) — scanned for
    /// declarations but not part of the analyzed project (Psalm's
    /// `isInProjectDirs` is false for them).
    scanning_dependencies: bool,
    /// Glob patterns for files to exclude from scanning.
    exclude_patterns: Vec<Pattern>,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            interner: Arc::new(Interner::new()),
            codebase: CodebaseInfo::new(),
            errors: Vec::new(),
            file_count: 0,
            stub_files: FxHashSet::default(),
            scanning_stubs: false,
            scanning_dependencies: false,
            exclude_patterns: Vec::new(),
        }
    }

    /// Scan directories holding dependency sources (vendor/): declarations are
    /// collected, but the files are not project files (`is_in_project_dirs`
    /// false), so stub classes may member-override theirs.
    pub fn scan_dependency_directories(&mut self, dirs: &[&Path]) {
        self.scanning_dependencies = true;
        for dir in dirs {
            self.scan_directory(dir);
        }
        self.scanning_dependencies = false;
    }

    /// Set exclude patterns for files to skip during scanning.
    pub fn set_exclude_patterns(&mut self, patterns: &[String]) {
        self.exclude_patterns = patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();
    }

    /// Check if a path should be excluded based on exclude patterns.
    fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.display().to_string();
        // Normalize path by removing leading "./"
        let normalized_path = path_str.strip_prefix("./").unwrap_or(&path_str);

        for pattern in &self.exclude_patterns {
            // Try matching the full path
            if pattern.matches(&path_str) || pattern.matches(normalized_path) {
                return true;
            }

            // Try matching relative to the pattern's base directory
            // e.g., pattern "vendor/**" should match "vendor/foo/bar.php"
            if pattern.matches_path(path) {
                return true;
            }

            // Check if path contains the directory from the pattern
            // e.g., pattern "vendor/**" -> check if path contains "/vendor/" or starts with "vendor/"
            let pattern_str = pattern.as_str();
            if let Some(dir_part) = pattern_str.strip_suffix("/**") {
                if normalized_path.starts_with(&format!("{}/", dir_part))
                    || normalized_path == dir_part
                    || path_str.contains(&format!("/{}/", dir_part))
                {
                    return true;
                }
            }

            // Also check just the file name for simple patterns like "*.txt"
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if pattern.matches(file_name) {
                    return true;
                }
            }
        }
        false
    }

    /// Scan the built-in PHP stubs.
    ///
    /// This should be called before scanning user files. The stubs provide
    /// type information for PHP's built-in functions and classes.
    ///
    /// Stubs for extensions bundled with PHP (`stubs/extensions/*.phpstub`)
    /// are always loaded. Stubs under `stubs/extensions/optional/` cover
    /// PECL/third-party extensions and are loaded only when named in
    /// `enabled_optional_extensions` (mirrors Psalm loading extension stubs
    /// only for enabled extensions).
    pub fn scan_stubs(&mut self, enabled_optional_extensions: &FxHashSet<String>) {
        self.scan_stubs_for_php_version(enabled_optional_extensions, u32::MAX)
    }

    /// Like [`Self::scan_stubs`], but version overlays under
    /// `stubs/_php_versions/PhpXY.phpstub` load only when `php_version_id`
    /// reaches that version (Psalm's internal Php74/Php80/Php82/... stubs).
    /// The `_php_versions` directory sorts before `extensions/`, so overlay
    /// members win the stub-vs-stub merge.
    pub fn scan_stubs_for_php_version(
        &mut self,
        enabled_optional_extensions: &FxHashSet<String>,
        php_version_id: u32,
    ) {
        self.scanning_stubs = true;

        // Iterate over all embedded stub files
        let mut files = Vec::new();
        for file_path in EmbeddedStubs::iter() {
            let path_str = file_path.as_ref();

            // Only process .phpstub files
            if !path_str.ends_with(".phpstub") {
                continue;
            }

            if let Some(extension) = optional_extension_name(path_str)
                && !enabled_optional_extensions.contains(extension)
            {
                continue;
            }

            if let Some(min_version_id) = version_overlay_min_id(path_str)
                && php_version_id < min_version_id
            {
                continue;
            }

            if let Some(content) = EmbeddedStubs::get(path_str) {
                let content_str = match std::str::from_utf8(&content.data) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Create a virtual path for the stub
                files.push((format!("stubs/{}", path_str), content_str.to_string()));
            }
        }
        files.sort();

        self.scan_contents_parallel(files);

        self.scanning_stubs = false;
    }

    /// The names of every optional extension with embedded stubs.
    pub fn embedded_optional_extensions() -> FxHashSet<String> {
        EmbeddedStubs::iter()
            .filter_map(|path| optional_extension_name(path.as_ref()).map(str::to_string))
            .collect()
    }

    /// Scan a specific stubs directory from the filesystem (for additional
    /// user stubs). `stubs/extensions/optional/` entries are gated the same
    /// way as in [`Self::scan_stubs`].
    /// Scan a single file as a STUB regardless of its extension (Psalm's
    /// `addStubFile`, used for plugin-registered stubs like
    /// plugin-mockery's `stubs/Mockery.php`).
    pub fn scan_stub_file(&mut self, path: &Path) {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return;
        };
        let canonical = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned();
        self.scanning_stubs = true;
        self.scan_file(&canonical, &contents);
        self.scanning_stubs = false;
    }

    pub fn scan_stub_directory(
        &mut self,
        stubs_dir: &Path,
        enabled_optional_extensions: &FxHashSet<String>,
    ) {
        if !stubs_dir.exists() {
            return;
        }

        self.scanning_stubs = true;

        // Scan all .phpstub files in the directory recursively
        self.scan_directory_for_stubs(stubs_dir, enabled_optional_extensions);

        self.scanning_stubs = false;
    }

    /// Scan all PHP files in the given directories.
    pub fn scan_directories(&mut self, dirs: &[&Path]) {
        for dir in dirs {
            self.scan_directory(dir);
        }
    }

    /// Finish scanning and return the result.
    pub fn finish(self) -> ScanResult {
        let interner = Arc::try_unwrap(self.interner)
            .expect("all ThreadedInterner handles are dropped when scanning ends");
        ScanResult {
            codebase: self.codebase,
            interner,
            file_count: self.file_count,
            errors: self.errors,
            stub_files: self.stub_files,
        }
    }

    /// Scan a directory for stub files (.phpstub).
    fn scan_directory_for_stubs(
        &mut self,
        dir: &Path,
        enabled_optional_extensions: &FxHashSet<String>,
    ) {
        let walker = WalkDir::new(dir).follow_links(true).into_iter();
        let mut stub_paths = Vec::new();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process .phpstub files
            if path.extension().is_some_and(|ext| ext == "phpstub") {
                let path_str = path.to_string_lossy().replace('\\', "/");
                if let Some(extension) = optional_extension_name(&path_str)
                    && !enabled_optional_extensions.contains(extension)
                {
                    continue;
                }
                stub_paths.push(path.to_path_buf());
            }
        }

        stub_paths.sort();

        self.scan_paths_parallel(stub_paths);
    }

    /// Scan a single directory recursively for PHP files.
    fn scan_directory(&mut self, dir: &Path) {
        let walker = WalkDir::new(dir).follow_links(true).into_iter();
        let mut source_paths = Vec::new();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip excluded files
            if self.should_exclude(path) {
                continue;
            }

            // Process .php and .phpstub files
            if let Some(ext) = path.extension() {
                if ext == "php" || ext == "phpstub" {
                    source_paths.push(path.to_path_buf());
                }
            }
        }

        source_paths.sort();

        self.scan_paths_parallel(source_paths);
    }

    /// Scan a batch of file paths: parse + collect in parallel (each worker
    /// thread interning through a `ThreadedInterner` over the shared
    /// interner), then register the results serially in the given order so
    /// duplicate-symbol precedence stays deterministic.
    fn scan_paths_parallel(&mut self, paths: Vec<std::path::PathBuf>) {
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    let path_str = path
                        .canonicalize()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| path.display().to_string());
                    files.push((path_str, contents));
                }
                Err(e) => self.errors.push(ScanError {
                    file_path: path.display().to_string(),
                    message: e.to_string(),
                    line: None,
                }),
            }
        }
        self.scan_contents_parallel(files);
    }

    /// Parallel collect + serial register over `(path, contents)` pairs.
    ///
    /// Cross-file `@psalm-import-type` resolution needs the definitions
    /// before the importing file is collected; a cheap pre-wave harvests
    /// `@psalm-type`/`@phpstan-type` definitions from the (few) files that
    /// textually contain one, so same-batch imports resolve regardless of
    /// file order.
    fn scan_contents_parallel(&mut self, files: Vec<(String, String)>) {
        if files.is_empty() {
            return;
        }

        let definers: Vec<&(String, String)> = files
            .iter()
            .filter(|(_, contents)| {
                contents.contains("@psalm-type") || contents.contains("@phpstan-type")
            })
            .collect();
        if !definers.is_empty() {
            let known_type_aliases = self.codebase.type_aliases.clone();
            let interner = ThreadedInterner::new(self.interner.clone());
            for (path, contents) in definers {
                let collected = collect_file(
                    &interner,
                    &known_type_aliases,
                    path,
                    contents,
                    self.scanning_stubs,
                );
                for type_alias in collected.declarations.type_aliases {
                    self.codebase
                        .type_aliases
                        .entry(type_alias.name)
                        .or_insert(type_alias);
                }
            }
        }

        let known_type_aliases = self.codebase.type_aliases.clone();
        let scanning_stubs = self.scanning_stubs;
        let parent = self.interner.clone();

        let worker_count = if cfg!(target_arch = "wasm32") {
            // Threads are unavailable on wasm32; collect inline.
            1
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(8)
                .min(files.len())
                .max(1)
        };
        let chunk_size = files.len().div_ceil(worker_count);

        if worker_count == 1 {
            let interner = ThreadedInterner::new(parent);
            let collected: Vec<CollectedFile> = files
                .iter()
                .map(|(path, contents)| {
                    collect_file(
                        &interner,
                        &known_type_aliases,
                        path,
                        contents,
                        scanning_stubs,
                    )
                })
                .collect();
            drop(interner);
            for (file, (path, contents)) in collected.into_iter().zip(files) {
                self.register_collected_file(file, &path, contents);
            }
            return;
        }

        let collected: Vec<CollectedFile> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(worker_count);
            for chunk in files.chunks(chunk_size) {
                let parent = parent.clone();
                let known_type_aliases = &known_type_aliases;
                handles.push(scope.spawn(move || {
                    let interner = ThreadedInterner::new(parent);
                    chunk
                        .iter()
                        .map(|(path, contents)| {
                            collect_file(
                                &interner,
                                known_type_aliases,
                                path,
                                contents,
                                scanning_stubs,
                            )
                        })
                        .collect::<Vec<_>>()
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().expect("scanner worker panicked"))
                .collect()
        });

        for (file, (path, contents)) in collected.into_iter().zip(files) {
            self.register_collected_file(file, &path, contents);
        }
    }

    /// Scan a single file with the given contents.
    pub fn scan_file(&mut self, path: &str, contents: &str) {
        let interner = ThreadedInterner::new(self.interner.clone());

        // Same pre-wave as scan_contents_parallel: harvest type-alias
        // definitions first so `@psalm-import-type` resolves even when the
        // defining class appears later in the file.
        if contents.contains("@psalm-type") || contents.contains("@phpstan-type") {
            let harvested = collect_file(
                &interner,
                &self.codebase.type_aliases,
                path,
                contents,
                self.scanning_stubs,
            );
            for type_alias in harvested.declarations.type_aliases {
                self.codebase
                    .type_aliases
                    .entry(type_alias.name)
                    .or_insert(type_alias);
            }
        }

        let collected = collect_file(
            &interner,
            &self.codebase.type_aliases,
            path,
            contents,
            self.scanning_stubs,
        );
        drop(interner);
        self.register_collected_file(collected, path, contents.to_string());
    }

    /// Serial half of scanning: insert file metadata and register the
    /// collected declarations into the codebase.
    fn register_collected_file(&mut self, collected: CollectedFile, path: &str, contents: String) {
        let CollectedFile {
            file_path_id,
            is_stub,
            mut declarations,
            file_parse_errors,
            scan_errors,
        } = collected;

        self.errors.extend(scan_errors);
        if is_stub {
            self.stub_files.insert(file_path_id);
        }

        let inline_annotations = std::mem::take(&mut declarations.inline_annotations);
        let docblock_parse_issues = std::mem::take(&mut declarations.docblock_parse_issues);
        let type_alias_imports = std::mem::take(&mut declarations.type_alias_imports);

        // Register file metadata first so symbol registration can resolve stub/project precedence.
        self.codebase.files.insert(
            file_path_id,
            FileInfo {
                path: file_path_id,
                classes: Vec::new(),
                functions: Vec::new(),
                constants: Vec::new(),
                content_hash: compute_hash(&contents),
                contents,
                parse_errors: file_parse_errors,
                docblock_parse_issues,
                is_stub,
                // The phpstorm-derived `stubs/extensions/*` stubs are lower precedence
                // than pzoom's own curated stubs (mirrors Psalm).
                is_low_precedence_stub: is_stub && path.contains("extensions/"),
                is_in_project_dirs: !is_stub && !self.scanning_dependencies,
                inline_annotations,
                type_alias_imports,
            },
        );

        // Track what's defined in this file
        let mut file_classes = Vec::new();
        let mut file_functions = Vec::new();
        let mut file_constants = Vec::new();

        // Register classes
        for class in declarations.classes {
            file_classes.push(class.name);
            self.codebase.register_class(class);
        }

        // Register functions
        for func in declarations.functions {
            file_functions.push(func.name);
            self.codebase.register_function(func);
        }

        // Register constants
        for constant in declarations.constants {
            file_constants.push(constant.name);
            self.codebase.constants.insert(constant.name, constant);
        }

        // Register type aliases
        for type_alias in declarations.type_aliases {
            self.codebase
                .type_aliases
                .insert(type_alias.name, type_alias);
        }

        // define() calls seen anywhere in the file; promoted to global
        // constants after populate under allConstantsGlobal.
        self.codebase
            .global_defines
            .extend(declarations.global_defines);

        if let Some(file_info) = self.codebase.files.get_mut(&file_path_id) {
            file_info.classes = file_classes;
            file_info.functions = file_functions;
            file_info.constants = file_constants;
        }

        self.file_count += 1;
    }

    /// Check if a file path is a stub file.
    pub fn is_stub_file(&self, path_id: StrId) -> bool {
        self.stub_files.contains(&path_id)
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// The thread-safe half of a file scan: everything collected from parsing a
/// single file, before any codebase mutation.
struct CollectedFile {
    file_path_id: StrId,
    is_stub: bool,
    declarations: CollectedDeclarations,
    file_parse_errors: Vec<(u32, String)>,
    scan_errors: Vec<ScanError>,
}

/// Parse a file and collect its declarations. Runs on worker threads; all
/// interning goes through the per-thread `ThreadedInterner`, whose ids come
/// from the shared parent interner (Hakana's threaded-scanner design).
fn collect_file(
    interner: &ThreadedInterner,
    known_type_aliases: &FxHashMap<StrId, ClassTypeAlias>,
    path: &str,
    contents: &str,
    scanning_stubs: bool,
) -> CollectedFile {
    let file_path_id = interner.intern(path);
    let file_id = FileId::new(path);

    let is_stub = scanning_stubs || path.ends_with(".phpstub");

    // Create arena for parsing
    let arena = Bump::new();

    // Parse the file
    let (program, parse_error) = parse_file_content(&arena, file_id, contents);

    // Record any parse errors
    let mut file_parse_errors: Vec<(u32, String)> = Vec::new();
    let mut scan_errors: Vec<ScanError> = Vec::new();
    if let Some(error) = parse_error {
        use pzoom_syntax::HasSpan;
        file_parse_errors.push((error.span().start.offset, format!("{}", error)));
        scan_errors.push(ScanError {
            file_path: path.to_string(),
            message: format!("Parse error: {:?}", error),
            line: None,
        });
    }

    // Collect declarations
    let collector = DeclarationCollector::new(
        interner,
        file_path_id,
        contents,
        known_type_aliases,
        &program.trivia,
    );
    let declarations = collector.collect(program);

    CollectedFile {
        file_path_id,
        is_stub,
        declarations,
        file_parse_errors,
        scan_errors,
    }
}

/// Compute a simple hash of file contents for cache invalidation.
fn compute_hash(contents: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_simple_class() {
        let mut scanner = Scanner::new();

        scanner.scan_file(
            "test.php",
            r#"<?php
namespace App;

class User {
    public string $name;
    private int $age;

    public function getName(): string {
        return $this->name;
    }
}
"#,
        );

        let result = scanner.finish();

        assert_eq!(result.file_count, 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.codebase.classlike_infos.len(), 1);

        // Find the class
        let class_name = result.interner.lookup(
            *result
                .codebase
                .classlike_infos
                .keys()
                .next()
                .expect("should have a class"),
        );
        assert_eq!(&*class_name, "App\\User");
    }

    #[test]
    fn test_scan_function() {
        let mut scanner = Scanner::new();

        scanner.scan_file(
            "test.php",
            r#"<?php
function greet(string $name): string {
    return "Hello, $name!";
}
"#,
        );

        let result = scanner.finish();

        assert_eq!(result.file_count, 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.codebase.functionlike_infos.len(), 1);
    }

    #[test]
    fn test_scan_interface_and_trait() {
        let mut scanner = Scanner::new();

        scanner.scan_file(
            "test.php",
            r#"<?php
namespace App\Contracts;

interface Nameable {
    public function getName(): string;
}

trait HasName {
    public string $name;

    public function getName(): string {
        return $this->name;
    }
}
"#,
        );

        let result = scanner.finish();

        assert_eq!(result.file_count, 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.codebase.classlike_infos.len(), 2);
    }

    /// JetBrains' #[Pure] attribute (phpstorm-stubs) marks functions/methods
    /// pure, equivalent to @psalm-pure (bare) / @pure (Pure(true)).
    #[test]
    fn test_pure_attribute_sets_is_pure() {
        let mut scanner = Scanner::new();
        scanner.scan_file(
            "test.php",
            r#"<?php
#[Pure]
function pureFn(int $x): int {}

/**
 * @param string $y
 */
#[\JetBrains\PhpStorm\Pure(true)]
function pureGlobalFn(string $y): int {}

function plainFn(): int {}

class C {
    #[Pure]
    public function pureMethod(): int {}
    public function plainMethod(): int {}
}
"#,
        );

        let result = scanner.finish();

        let fn_pure = |name: &str| -> bool {
            result
                .codebase
                .functionlike_infos
                .iter()
                .find(|(id, _)| result.interner.lookup(**id).eq_ignore_ascii_case(name))
                .map(|(_, info)| info.is_pure)
                .unwrap_or_else(|| panic!("function {name} not found"))
        };
        assert!(fn_pure("pureFn"), "#[Pure] function should be pure");
        assert!(
            fn_pure("pureGlobalFn"),
            "#[Pure(true)] function with a docblock should be pure"
        );
        assert!(!fn_pure("plainFn"), "unmarked function should not be pure");

        let class = result
            .codebase
            .classlike_infos
            .values()
            .next()
            .expect("class C");
        let method_pure = |name: &str| -> bool {
            class
                .methods
                .iter()
                .find(|(id, _)| result.interner.lookup(**id).eq_ignore_ascii_case(name))
                .map(|(_, info)| info.is_pure)
                .unwrap_or_else(|| panic!("method {name} not found"))
        };
        assert!(method_pure("pureMethod"), "#[Pure] method should be pure");
        assert!(!method_pure("plainMethod"), "unmarked method should not be pure");
    }

    /// Every class and function must be declared in exactly one stub file.
    ///
    /// The embedded stubs are a single flat universe: Psalm-derived docblock
    /// annotations have been folded into the JetBrains-derived signature stubs
    /// under stubs/extensions/, so a symbol appearing in two files would mean
    /// one silently shadows or merges with the other. Re-declarations *within*
    /// one file are allowed (version-conditional variants).
    #[test]
    fn test_stubs_declare_each_symbol_in_one_file() {
        let mut scanner = Scanner::new();
        // The invariant must hold across default AND optional extension stubs.
        scanner.scan_stubs(&Scanner::embedded_optional_extensions());
        let result = scanner.finish();

        let mut symbol_files: FxHashMap<(&str, StrId), FxHashSet<StrId>> = FxHashMap::default();
        for (path, file_info) in &result.codebase.files {
            // PHP-version overlays re-declare symbols by design (newest-wins
            // folding) — they are not part of the one-symbol-one-file invariant.
            if result
                .interner
                .lookup(*path)
                .contains("_php_versions/")
            {
                continue;
            }
            for class in &file_info.classes {
                symbol_files
                    .entry(("class", *class))
                    .or_default()
                    .insert(*path);
            }
            for function in &file_info.functions {
                symbol_files
                    .entry(("function", *function))
                    .or_default()
                    .insert(*path);
            }
        }

        let mut duplicates: Vec<String> = symbol_files
            .iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|((kind, name), files)| {
                let mut paths: Vec<String> = files
                    .iter()
                    .map(|p| result.interner.lookup(*p).to_string())
                    .collect();
                paths.sort();
                format!(
                    "{} {} declared in: {}",
                    kind,
                    result.interner.lookup(*name),
                    paths.join(", ")
                )
            })
            .collect();
        duplicates.sort();

        assert!(
            duplicates.is_empty(),
            "symbols declared in more than one stub file:\n{}",
            duplicates.join("\n")
        );
    }
}
