//! Phase 1: Scanning - Parse files and collect symbols.
//!
//! The scanner discovers PHP files, parses them using the mago parser,
//! and extracts class/function/constant declarations.

use std::path::Path;

use bumpalo::Bump;
use glob::Pattern;
use pzoom_code_info::codebase_info::FileInfo;
use pzoom_code_info::CodebaseInfo;
use pzoom_str::{Interner, StrId};
use pzoom_syntax::{parse_file_content, DeclarationCollector, FileId};
use rust_embed::Embed;
use rustc_hash::FxHashSet;
use walkdir::WalkDir;

/// Embedded stub files for PHP built-in types and functions.
#[derive(Embed)]
#[folder = "../../stubs/"]
struct EmbeddedStubs;

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
    interner: Interner,
    codebase: CodebaseInfo,
    errors: Vec<ScanError>,
    file_count: usize,
    stub_files: FxHashSet<StrId>,
    /// Whether we're currently scanning stubs (affects how files are tracked).
    scanning_stubs: bool,
    /// Glob patterns for files to exclude from scanning.
    exclude_patterns: Vec<Pattern>,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            interner: Interner::new(),
            codebase: CodebaseInfo::new(),
            errors: Vec::new(),
            file_count: 0,
            stub_files: FxHashSet::default(),
            scanning_stubs: false,
            exclude_patterns: Vec::new(),
        }
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
    /// Scan the embedded stubs for PHP built-in types and functions.
    pub fn scan_stubs(&mut self) {
        self.scanning_stubs = true;

        // Iterate over all embedded stub files
        for file_path in EmbeddedStubs::iter() {
            let path_str = file_path.as_ref();

            // Only process .phpstub files
            if !path_str.ends_with(".phpstub") {
                continue;
            }

            if let Some(content) = EmbeddedStubs::get(path_str) {
                let content_str = match std::str::from_utf8(&content.data) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Create a virtual path for the stub
                let virtual_path = format!("stubs/{}", path_str);

                if let Err(e) = self.scan_content(&virtual_path, content_str) {
                    self.errors.push(ScanError {
                        file_path: virtual_path,
                        message: e.to_string(),
                        line: None,
                    });
                }
            }
        }

        self.scanning_stubs = false;
    }

    /// Scan a specific stubs directory from the filesystem (for additional user stubs).
    pub fn scan_stub_directory(&mut self, stubs_dir: &Path) {
        if !stubs_dir.exists() {
            return;
        }

        self.scanning_stubs = true;

        // Scan all .phpstub files in the directory recursively
        self.scan_directory_for_stubs(stubs_dir);

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
        ScanResult {
            codebase: self.codebase,
            interner: self.interner,
            file_count: self.file_count,
            errors: self.errors,
            stub_files: self.stub_files,
        }
    }

    /// Scan a directory for stub files (.phpstub).
    fn scan_directory_for_stubs(&mut self, dir: &Path) {
        let walker = WalkDir::new(dir).follow_links(true).into_iter();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process .phpstub files
            if path.extension().is_some_and(|ext| ext == "phpstub") {
                if let Err(e) = self.scan_file_path(path) {
                    self.errors.push(ScanError {
                        file_path: path.display().to_string(),
                        message: e.to_string(),
                        line: None,
                    });
                }
            }
        }
    }

    /// Scan a single directory recursively for PHP files.
    fn scan_directory(&mut self, dir: &Path) {
        let walker = WalkDir::new(dir).follow_links(true).into_iter();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip excluded files
            if self.should_exclude(path) {
                continue;
            }

            // Process .php and .phpstub files
            if let Some(ext) = path.extension() {
                if ext == "php" || ext == "phpstub" {
                    if let Err(e) = self.scan_file_path(path) {
                        self.errors.push(ScanError {
                            file_path: path.display().to_string(),
                            message: e.to_string(),
                            line: None,
                        });
                    }
                }
            }
        }
    }

    /// Scan a single file by path.
    fn scan_file_path(&mut self, path: &Path) -> Result<(), std::io::Error> {
        let contents = std::fs::read_to_string(path)?;
        // Canonicalize path to get absolute path for consistent lookups
        let path_str = path
            .canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.display().to_string());
        self.scan_file(&path_str, &contents);
        Ok(())
    }

    /// Scan content from a string (used for embedded stubs).
    fn scan_content(&mut self, path: &str, contents: &str) -> Result<(), std::io::Error> {
        self.scan_file(path, contents);
        Ok(())
    }

    /// Scan a single file with the given contents.
    pub fn scan_file(&mut self, path: &str, contents: &str) {
        let file_path_id = self.interner.intern(path);
        let file_id = FileId::new(path);

        // Track stub files
        if self.scanning_stubs || path.ends_with(".phpstub") {
            self.stub_files.insert(file_path_id);
        }

        // Create arena for parsing
        let arena = Bump::new();

        // Parse the file
        let (program, parse_error) = parse_file_content(&arena, file_id, contents);

        // Record any parse errors
        if let Some(error) = parse_error {
            self.errors.push(ScanError {
                file_path: path.to_string(),
                message: format!("Parse error: {:?}", error),
                line: None,
            });
        }

        // Collect declarations
        let collector = DeclarationCollector::new(&mut self.interner, file_path_id, &program.trivia);
        let declarations = collector.collect(program);

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

        // Record file info
        let file_info = FileInfo {
            path: file_path_id,
            classes: file_classes,
            functions: file_functions,
            constants: file_constants,
            content_hash: compute_hash(contents),
            contents: contents.to_string(),
        };
        self.codebase.files.insert(file_path_id, file_info);

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
}
