//! pzoom CLI - Command-line interface for the pzoom PHP static analyzer.

use clap::{Parser, Subcommand};
use pzoom_analyzer::{
    Config, PsalmBaseline, find_and_load_psalm_config, load_psalm_baseline, load_psalm_config,
};
use pzoom_orchestrator::{Analyzer, Populator, Scanner};
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "pzoom")]
#[command(about = "A fast PHP static analyzer", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to project directory or file
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output format (text, json, checkstyle)
    #[arg(short, long, default_value = "text")]
    format: String,

    /// Number of threads to use
    #[arg(short, long)]
    threads: Option<usize>,

    /// Show only errors (no info or warnings)
    #[arg(long)]
    errors_only: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze PHP files for issues
    Analyze {
        /// Paths to analyze
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,
    },

    /// Show information about the codebase
    Info,

    /// Clear the cache
    ClearCache,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Analyze { paths }) => {
            let config = load_config(&cli, paths);
            analyze(&config, paths)
        }
        Some(Commands::Info) => {
            let config = load_config(&cli, &[PathBuf::from(".")]);
            show_info(&config)
        }
        Some(Commands::ClearCache) => {
            let config = load_config(&cli, &[PathBuf::from(".")]);
            clear_cache(&config)
        }
        None => {
            let default_paths = vec![cli.path.clone()];
            let config = load_config(&cli, &default_paths);
            analyze(&config, &default_paths)
        }
    }
}

fn load_config(cli: &Cli, paths: &[PathBuf]) -> Config {
    let mut config = if let Some(ref config_path) = cli.config {
        // Load from specified config file
        load_psalm_config(config_path).unwrap_or_default()
    } else if let Some(config_path) = find_config_for_paths(paths) {
        load_psalm_config(config_path).unwrap_or_default()
    } else {
        // Fall back to current directory if no target-specific config was found.
        find_and_load_psalm_config(".").unwrap_or_default()
    };

    if let Some(threads) = cli.threads {
        config.threads = threads;
    }

    config
}

fn find_config_for_paths(paths: &[PathBuf]) -> Option<PathBuf> {
    let first_path = paths.first()?;
    let start = if first_path.is_file() {
        first_path.parent().unwrap_or(first_path.as_path())
    } else {
        first_path.as_path()
    };

    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

    loop {
        for filename in ["psalm.xml", "psalm.xml.dist", "psalm.dist.xml"] {
            let candidate = current.join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        if !current.pop() {
            return None;
        }
    }
}

/// Find the project root directory from a path by looking for marker files.
fn find_project_root(path: &std::path::Path) -> PathBuf {
    let start_dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };

    // Walk up the directory tree looking for project markers
    let mut current = start_dir;
    loop {
        // Check for common project root markers
        for marker in &["psalm.xml", "psalm.xml.dist", "composer.json", ".git"] {
            if current.join(marker).exists() {
                return current.to_path_buf();
            }
        }

        // Move to parent directory
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }

    // Fall back to the start directory if no marker found
    start_dir.to_path_buf()
}

fn analyze(config: &Config, paths: &[PathBuf]) -> ExitCode {
    println!("Scanning files...");

    // Phase 1: Scan the full codebase
    // We always scan the entire project directory to get complete type information,
    // even when analyzing a single file (Hakana-style behavior).
    let mut scanner = Scanner::new();

    // Scan built-in stubs first to get PHP standard library types
    scanner.scan_stubs();

    // Determine the project root to scan
    // If paths is just ".", scan current directory
    // Otherwise, find the project root from the first specified path
    let project_root = if paths.len() == 1 && paths[0] == PathBuf::from(".") {
        PathBuf::from(".")
    } else {
        // Find project root from the first path
        let first_path = paths[0].canonicalize().unwrap_or_else(|_| paths[0].clone());
        find_project_root(&first_path)
    };
    let display_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.clone());

    let mut analysis_dirs: Vec<PathBuf> = Vec::new();
    let mut analysis_files: Vec<PathBuf> = Vec::new();

    if !config.project_dirs.is_empty() {
        for configured_path in &config.project_dirs {
            let candidate = display_root.join(configured_path);
            if candidate.is_dir() {
                analysis_dirs.push(candidate);
            } else if candidate.is_file() {
                analysis_files.push(candidate);
            }
        }
    }

    if analysis_dirs.is_empty() && analysis_files.is_empty() {
        analysis_dirs.push(project_root.clone());
    }

    // Scan dependency sources for type information, but don't analyze them unless explicitly targeted.
    let mut scan_dirs = analysis_dirs.clone();
    let vendor_dir = display_root.join("vendor");
    if vendor_dir.is_dir() {
        scan_dirs.push(vendor_dir);
    }

    let dir_refs: Vec<&Path> = scan_dirs.iter().map(|d| d.as_path()).collect();
    scanner.scan_directories(&dir_refs);

    for file in &analysis_files {
        if let Ok(contents) = std::fs::read_to_string(&file) {
            let canonical = file
                .canonicalize()
                .unwrap_or_else(|_| file.clone())
                .to_string_lossy()
                .into_owned();
            scanner.scan_file(&canonical, &contents);
        }
    }

    let scan_result = scanner.finish();

    if !scan_result.errors.is_empty() {
        for error in &scan_result.errors {
            eprintln!(
                "Scan error in {}: {}",
                format_display_path(&error.file_path, &display_root),
                error.message
            );
        }
    }

    println!("Scanned {} files", scan_result.file_count);

    // Phase 2: Populate
    println!("Resolving types...");
    let mut codebase = scan_result.codebase;
    let interner = scan_result.interner;
    let stub_files = scan_result.stub_files;
    {
        let mut populator = Populator::new(&mut codebase, &interner);
        populator.populate();
    }

    // Phase 3: Analyze
    // Determine which files to analyze based on the provided paths
    println!("Analyzing...");
    let analyzer = Analyzer::new(&codebase, &interner, config);

    let configured_analysis_files = collect_configured_analysis_files(
        &codebase,
        &interner,
        &analysis_dirs,
        &analysis_files,
        &config.exclude_patterns,
        &display_root,
    );

    // Check if we should analyze specific files or the whole codebase
    let analyze_all = paths.len() == 1 && paths[0] == PathBuf::from(".");
    let targeting_project_root = paths.len() == 1
        && paths[0]
            .canonicalize()
            .ok()
            .is_some_and(|p| p == display_root);

    let result = if analyze_all || targeting_project_root {
        if configured_analysis_files.is_empty() {
            analyzer.analyze()
        } else {
            analyzer.analyze_files(&configured_analysis_files)
        }
    } else {
        // Convert paths to StrIds and analyze only those files
        let mut files_to_analyze = Vec::new();

        for path in paths {
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    eprintln!("Warning: {} does not exist", path.display());
                    continue;
                }
            };

            if canonical.is_file() {
                // Single file - find it in the codebase
                let path_str = canonical.to_string_lossy();
                if let Some(str_id) = interner.find(&path_str) {
                    if codebase.files.contains_key(&str_id) {
                        files_to_analyze.push(str_id);
                    } else {
                        eprintln!("Warning: {} not found in codebase", path.display());
                    }
                } else {
                    eprintln!("Warning: {} not found in codebase", path.display());
                }
            } else if canonical.is_dir() {
                // Directory - find all files under it in the codebase
                let dir_str = canonical.to_string_lossy();
                for (&file_id, _) in &codebase.files {
                    let file_path = interner.lookup(file_id);
                    if file_path.starts_with(&*dir_str) {
                        files_to_analyze.push(file_id);
                    }
                }
            }
        }

        if files_to_analyze.is_empty() {
            eprintln!("No valid files to analyze");
            return ExitCode::FAILURE;
        }

        analyzer.analyze_files(&files_to_analyze)
    };

    let mut baseline = load_error_baseline(config, &display_root);

    // Filter out issues from stubs, globally suppressed issues, and baseline-covered issues.
    let mut user_issues = Vec::new();
    for issue in &result.issues {
        if stub_files.contains(&issue.file_path) {
            continue;
        }

        let issue_name = format!("{:?}", issue.kind);
        let file_path = interner.lookup(issue.file_path);
        let display_file_path = format_display_path(&file_path, &display_root);

        if config.is_issue_suppressed_for_path(&issue_name, &display_file_path) {
            continue;
        }

        if let Some(baseline) = baseline.as_mut() {
            let selected_text = extract_issue_selected_text(issue, &codebase);

            if baseline.suppresses(&display_file_path, &issue_name, &selected_text) {
                continue;
            }
        }

        user_issues.push(issue);
    }

    // Output results
    println!("\nAnalyzed {} files", result.file_count);

    if user_issues.is_empty() {
        println!("No issues found!");
        ExitCode::SUCCESS
    } else {
        println!("Found {} issues:\n", user_issues.len());

        // Sort issues alphabetically by file path, then by line, then by column
        let mut sorted_issues = user_issues;
        sorted_issues.sort_by(|a, b| {
            let path_a = format_display_path(&interner.lookup(a.file_path), &display_root);
            let path_b = format_display_path(&interner.lookup(b.file_path), &display_root);
            path_a
                .cmp(&path_b)
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.start_column.cmp(&b.start_column))
        });

        for issue in sorted_issues {
            let display_path =
                format_display_path(&interner.lookup(issue.file_path), &display_root);
            println!(
                "{:?} - {}:{}:{} - {}",
                issue.kind, display_path, issue.start_line, issue.start_column, issue.message
            );
        }
        ExitCode::FAILURE
    }
}

fn load_error_baseline(config: &Config, project_root: &Path) -> Option<PsalmBaseline> {
    let baseline_path = config.error_baseline.as_ref()?;
    let baseline_path = {
        let path = PathBuf::from(baseline_path);
        if path.is_relative() {
            project_root.join(path)
        } else {
            path
        }
    };

    match load_psalm_baseline(&baseline_path) {
        Ok(baseline) => Some(baseline),
        Err(error) => {
            eprintln!(
                "Warning: failed to load baseline {}: {}",
                baseline_path.display(),
                error
            );
            None
        }
    }
}

fn extract_issue_selected_text(
    issue: &pzoom_code_info::Issue,
    codebase: &pzoom_code_info::CodebaseInfo,
) -> String {
    let file_contents = codebase
        .files
        .get(&issue.file_path)
        .map(|file| file.contents.as_str());

    let Some(file_contents) = file_contents else {
        return String::new();
    };

    let start = issue.start_offset as usize;
    let end = issue.end_offset as usize;

    file_contents
        .get(start..end)
        .map(str::trim)
        .unwrap_or("")
        .replace("\r\n", "\n")
}

fn format_display_path(path: &str, root: &Path) -> String {
    let path_buf = PathBuf::from(path);

    if let Ok(rel) = path_buf.strip_prefix(root) {
        return rel.to_string_lossy().replace('\\', "/");
    }

    if let (Ok(canon_root), Ok(canon_path)) = (root.canonicalize(), path_buf.canonicalize()) {
        if let Ok(rel) = canon_path.strip_prefix(&canon_root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path_buf.strip_prefix(&cwd) {
            return rel.to_string_lossy().replace('\\', "/");
        }

        if let (Ok(canon_cwd), Ok(canon_path)) = (cwd.canonicalize(), path_buf.canonicalize()) {
            if let Ok(rel) = canon_path.strip_prefix(&canon_cwd) {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
    }

    path_buf.to_string_lossy().replace('\\', "/")
}

fn collect_configured_analysis_files(
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    analysis_dirs: &[PathBuf],
    analysis_files: &[PathBuf],
    exclude_patterns: &[String],
    project_root: &Path,
) -> Vec<pzoom_str::StrId> {
    let canonical_dirs: Vec<PathBuf> = analysis_dirs
        .iter()
        .map(|d| d.canonicalize().unwrap_or_else(|_| d.clone()))
        .collect();

    let canonical_files: FxHashSet<PathBuf> = analysis_files
        .iter()
        .map(|f| f.canonicalize().unwrap_or_else(|_| f.clone()))
        .collect();

    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let mut file_ids = Vec::new();
    let mut seen = FxHashSet::default();

    for &file_id in codebase.files.keys() {
        let file_path = PathBuf::from(interner.lookup(file_id).as_ref());
        let canonical_file = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.clone());

        let in_analysis_scope = canonical_files.contains(&canonical_file)
            || canonical_dirs.iter().any(|d| canonical_file.starts_with(d));

        if !in_analysis_scope
            || is_excluded_from_analysis(&canonical_file, &canonical_root, exclude_patterns)
        {
            continue;
        }

        if seen.insert(file_id) {
            file_ids.push(file_id);
        }
    }

    file_ids
}

fn is_excluded_from_analysis(path: &Path, root: &Path, exclude_patterns: &[String]) -> bool {
    if exclude_patterns.is_empty() {
        return false;
    }

    let path_string = path.to_string_lossy().replace('\\', "/");
    let rel_string = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    exclude_patterns.iter().any(|pattern| {
        let pattern = pattern.replace('\\', "/");

        if let Some(dir) = pattern.strip_suffix("/**") {
            if let Some(rel) = &rel_string {
                return rel == dir || rel.starts_with(&format!("{}/", dir));
            }

            return path_string.contains(&format!("/{}/", dir))
                || path_string.ends_with(&format!("/{}", dir));
        }

        if let Some(rel) = &rel_string {
            if rel == &pattern || rel.starts_with(&format!("{}/", pattern)) {
                return true;
            }
        }

        path_string.ends_with(&format!("/{}", pattern)) || file_name == pattern
    })
}

fn show_info(_config: &Config) -> ExitCode {
    println!("pzoom - PHP static analyzer");
    println!("Version: 0.1.0");
    ExitCode::SUCCESS
}

fn clear_cache(config: &Config) -> ExitCode {
    if let Some(ref cache_dir) = config.cache_dir {
        match std::fs::remove_dir_all(cache_dir) {
            Ok(_) => {
                println!("Cache cleared");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Failed to clear cache: {}", e);
                ExitCode::FAILURE
            }
        }
    } else {
        println!("No cache directory configured");
        ExitCode::SUCCESS
    }
}
