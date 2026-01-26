//! pzoom CLI - Command-line interface for the pzoom PHP static analyzer.

use clap::{Parser, Subcommand};
use pzoom_analyzer::{find_and_load_psalm_config, load_psalm_config, Config};
use pzoom_orchestrator::{Analyzer, Populator, Scanner};
use std::path::PathBuf;
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

    let config = load_config(&cli);

    match cli.command {
        Some(Commands::Analyze { paths }) => analyze(&config, &paths),
        Some(Commands::Info) => show_info(&config),
        Some(Commands::ClearCache) => clear_cache(&config),
        None => analyze(&config, &[cli.path]),
    }
}

fn load_config(cli: &Cli) -> Config {
    let mut config = if let Some(ref config_path) = cli.config {
        // Load from specified config file
        load_psalm_config(config_path).unwrap_or_default()
    } else {
        // Try to find psalm.xml in current directory
        find_and_load_psalm_config(".").unwrap_or_default()
    };

    if let Some(threads) = cli.threads {
        config.threads = threads;
    }

    config
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

    // Set exclude patterns from config
    scanner.set_exclude_patterns(&config.exclude_patterns);

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

    scanner.scan_directories(&[project_root.as_path()]);
    let scan_result = scanner.finish();

    if !scan_result.errors.is_empty() {
        for error in &scan_result.errors {
            eprintln!("Scan error in {}: {}", error.file_path, error.message);
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

    // Check if we should analyze specific files or the whole codebase
    let analyze_all = paths.len() == 1 && paths[0] == PathBuf::from(".");

    let result = if analyze_all {
        analyzer.analyze()
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

    // Filter out issues from stub files
    let user_issues: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| !stub_files.contains(&issue.file_path))
        .collect();

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
            let path_a = interner.lookup(a.file_path);
            let path_b = interner.lookup(b.file_path);
            path_a
                .cmp(&path_b)
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.start_column.cmp(&b.start_column))
        });

        for issue in sorted_issues {
            println!(
                "{:?} - {}:{}:{} - {}",
                issue.kind,
                interner.lookup(issue.file_path),
                issue.start_line,
                issue.start_column,
                issue.message
            );
        }
        ExitCode::FAILURE
    }
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
