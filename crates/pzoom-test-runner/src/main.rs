//! Test runner for pzoom.
//!
//! Runs inference tests in a format similar to Hakana's test runner.
//! Each test is a directory containing:
//! - input.php: The PHP code to analyze
//! - output.txt: Expected issues (empty if test should pass)

use clap::Parser;
use pzoom_analyzer::Config;
use pzoom_code_info::CodebaseInfo;
use pzoom_orchestrator::{Analyzer, Populator, Scanner};
use pzoom_str::Interner;
use rustc_hash::FxHashSet;
use similar::{ChangeTag, TextDiff};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "pzoom-test")]
#[command(about = "Test runner for pzoom", long_about = None)]
struct Cli {
    /// Test directory or specific test to run
    #[arg(default_value = "tests")]
    test_path: String,

    /// Update expected output files with actual results
    #[arg(long)]
    update: bool,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Reuse codebase between tests (faster, scans stubs only once)
    #[arg(long)]
    reuse_codebase: bool,
}

/// Holds pre-scanned stub data that can be reused across tests.
struct BaseCodebase {
    codebase: CodebaseInfo,
    interner: Interner,
    stub_files: FxHashSet<pzoom_str::StrId>,
}

fn main() {
    let cli = Cli::parse();

    let cwd = env::current_dir().unwrap();
    let test_path = if Path::new(&cli.test_path).is_absolute() {
        cli.test_path.clone()
    } else {
        cwd.join(&cli.test_path).to_string_lossy().to_string()
    };

    let stubs_path = cwd.join("stubs").to_string_lossy().to_string();

    let test_folders = match get_all_test_folders(&test_path) {
        Ok(folders) => folders,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if test_folders.is_empty() {
        eprintln!("No tests found in {}", test_path);
        std::process::exit(1);
    }

    // Pre-scan stubs if reusing codebase
    let base_codebase = if cli.reuse_codebase && test_folders.len() > 1 {
        let start = Instant::now();
        if cli.verbose {
            eprintln!("Pre-scanning stubs for reuse...");
        }

        let mut scanner = Scanner::new();
        if Path::new(&stubs_path).exists() {
            scanner.scan_stub_directory(Path::new(&stubs_path));
        }
        let scan_result = scanner.finish();

        if cli.verbose {
            eprintln!(
                "Scanned {} stub files in {:?}",
                scan_result.file_count,
                start.elapsed()
            );
        }

        Some(BaseCodebase {
            codebase: scan_result.codebase,
            interner: scan_result.interner,
            stub_files: scan_result.stub_files,
        })
    } else {
        None
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    let start = Instant::now();

    for test_folder in &test_folders {
        if test_folder.contains("skipped-") || test_folder.contains("SKIPPED-") {
            skipped += 1;
            print!("S");
            io::stdout().flush().unwrap();
            continue;
        }

        let result = if let Some(ref base) = base_codebase {
            run_test_with_base(test_folder, base, cli.update, cli.verbose)
        } else {
            run_test(test_folder, &stubs_path, cli.update, cli.verbose)
        };

        match result {
            TestResult::Pass => {
                passed += 1;
                print!(".");
            }
            TestResult::Fail(diff) => {
                failed += 1;
                print!("F");
                failures.push((test_folder.clone(), diff));
            }
            TestResult::Updated => {
                passed += 1;
                print!("U");
            }
            TestResult::Skip => {
                skipped += 1;
                print!("S");
            }
        }
        io::stdout().flush().unwrap();
    }

    let elapsed = start.elapsed();
    println!("\n");

    if !failures.is_empty() {
        println!("Failures:\n");
        for (folder, diff) in &failures {
            println!("=== {} ===\n{}\n", folder, diff);
        }
    }

    println!(
        "Tests: {} passed, {} failed, {} skipped (in {:?})",
        passed, failed, skipped, elapsed
    );

    if failed > 0 {
        std::process::exit(1);
    }
}

enum TestResult {
    Pass,
    Fail(String),
    Updated,
    Skip,
}

/// Run a test using a pre-scanned base codebase (faster).
fn run_test_with_base(
    test_folder: &str,
    base: &BaseCodebase,
    update: bool,
    verbose: bool,
) -> TestResult {
    let input_path = format!("{}/input.php", test_folder);
    let output_path = format!("{}/output.txt", test_folder);

    // Check if input.php exists
    if !Path::new(&input_path).exists() {
        return TestResult::Skip;
    }

    if verbose {
        eprintln!("Running test: {}", test_folder);
    }

    // Read input file
    let input_contents = match fs::read_to_string(&input_path) {
        Ok(c) => c,
        Err(e) => {
            return TestResult::Fail(format!("Failed to read input.php: {}", e));
        }
    };

    // Clone the base codebase and interner
    let mut codebase = base.codebase.clone();
    let mut interner = base.interner.clone();
    let stub_files = base.stub_files.clone();

    // Scan just the test file
    let file_path_id = interner.intern(&input_path);
    let file_id = pzoom_syntax::FileId::new(&input_path);

    let arena = bumpalo::Bump::new();
    let (program, _parse_error) = pzoom_syntax::parse_file_content(&arena, file_id, &input_contents);

    let collector = pzoom_syntax::DeclarationCollector::new(&mut interner, file_path_id, &program.trivia);
    let declarations = collector.collect(program);

    // Track what's defined in this file
    let mut file_classes = Vec::new();
    let mut file_functions = Vec::new();
    let mut file_constants = Vec::new();

    for class in declarations.classes {
        file_classes.push(class.name);
        codebase.register_class(class);
    }

    for func in declarations.functions {
        file_functions.push(func.name);
        codebase.register_function(func);
    }

    for constant in declarations.constants {
        file_constants.push(constant.name);
        codebase.constants.insert(constant.name, constant);
    }

    // Record file info
    let file_info = pzoom_code_info::codebase_info::FileInfo {
        path: file_path_id,
        classes: file_classes,
        functions: file_functions,
        constants: file_constants,
        content_hash: compute_hash(&input_contents),
        contents: input_contents,
    };
    codebase.files.insert(file_path_id, file_info);

    // Run analysis
    run_analysis_and_compare(
        &mut codebase,
        &interner,
        &stub_files,
        &input_path,
        &output_path,
        update,
    )
}

/// Run a test without a pre-scanned base codebase (slower, scans stubs each time).
fn run_test(test_folder: &str, stubs_path: &str, update: bool, verbose: bool) -> TestResult {
    let input_path = format!("{}/input.php", test_folder);
    let output_path = format!("{}/output.txt", test_folder);

    // Check if input.php exists
    if !Path::new(&input_path).exists() {
        return TestResult::Skip;
    }

    if verbose {
        eprintln!("Running test: {}", test_folder);
    }

    // Read input file
    let input_contents = match fs::read_to_string(&input_path) {
        Ok(c) => c,
        Err(e) => {
            return TestResult::Fail(format!("Failed to read input.php: {}", e));
        }
    };

    // Run analysis
    let mut scanner = Scanner::new();

    // Scan stubs first (using the stub-specific method to properly handle .phpstub files)
    if Path::new(stubs_path).exists() {
        scanner.scan_stub_directory(Path::new(stubs_path));
    }

    // Scan the test file
    scanner.scan_file(&input_path, &input_contents);

    // Finish scanning
    let scan_result = scanner.finish();
    let mut codebase = scan_result.codebase;
    let interner = scan_result.interner;
    let stub_files = scan_result.stub_files;

    run_analysis_and_compare(
        &mut codebase,
        &interner,
        &stub_files,
        &input_path,
        &output_path,
        update,
    )
}

/// Run analysis and compare results to expected output.
fn run_analysis_and_compare(
    codebase: &mut CodebaseInfo,
    interner: &Interner,
    _stub_files: &FxHashSet<pzoom_str::StrId>,
    input_path: &str,
    output_path: &str,
    update: bool,
) -> TestResult {
    let config = Config::default();

    // Populate
    {
        let mut populator = Populator::new(codebase, interner);
        populator.populate();
    }

    // Analyze
    let analyzer = Analyzer::new(codebase, interner, &config);
    let result = analyzer.analyze();

    // Format output in Psalm style: IssueKind - file:line:column - message
    let mut output_lines: Vec<String> = Vec::new();
    for issue in &result.issues {
        let file_path = interner.lookup(issue.file_path);
        // Only include issues from the test file, not stubs
        if file_path.contains(input_path) || file_path.ends_with("input.php") {
            output_lines.push(format!(
                "{:?} - {}:{}:{} - {}",
                issue.kind, "input.php", issue.start_line, issue.start_column, issue.message
            ));
        }
    }
    output_lines.sort();
    let actual_output = output_lines.join("\n");

    // Read expected output (if file doesn't exist, expect no errors)
    let expected_output = if Path::new(output_path).exists() {
        fs::read_to_string(output_path)
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        String::new()
    };

    // Compare
    // If expected is a single line with no newlines, check if actual contains expected
    // This allows simple expected outputs like "InvalidReturnStatement"
    let matches = if actual_output.trim() == expected_output.trim() {
        true
    } else if !expected_output.contains('\n') && !expected_output.is_empty() {
        // Single-line expected output - check if any actual line contains it
        output_lines.iter().any(|line| line.contains(&expected_output))
    } else {
        false
    };

    if matches {
        TestResult::Pass
    } else if update {
        // Only write output.txt if there are actual issues
        if actual_output.trim().is_empty() {
            // No issues - remove output.txt if it exists
            if Path::new(output_path).exists() {
                let _ = fs::remove_file(output_path);
            }
        } else {
            // Has issues - write output.txt
            if let Err(e) = fs::write(output_path, actual_output.trim()) {
                return TestResult::Fail(format!("Failed to update output.txt: {}", e));
            }
        }
        TestResult::Updated
    } else {
        TestResult::Fail(format_diff(&expected_output, &actual_output))
    }
}

fn compute_hash(contents: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn format_diff(expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut output = String::new();

    output.push_str("Expected:\n");
    for line in expected.lines() {
        output.push_str(&format!("  {}\n", line));
    }
    output.push_str("\nActual:\n");
    for line in actual.lines() {
        output.push_str(&format!("  {}\n", line));
    }
    output.push_str("\nDiff:\n");

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    output
}

fn get_all_test_folders(test_path: &str) -> Result<Vec<String>, String> {
    let mut test_folders = Vec::new();

    if !Path::new(test_path).exists() {
        return Err(format!("Test path does not exist: {}", test_path));
    }

    // Check if this is a single test directory
    let input_php = format!("{}/input.php", test_path);
    if Path::new(&input_php).exists() {
        return Ok(vec![test_path.to_string()]);
    }

    // Walk directory to find test folders
    for entry in WalkDir::new(test_path)
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_dir() {
            let input_php = path.join("input.php");
            if input_php.exists() {
                test_folders.push(path.to_string_lossy().to_string());
            }
        }
    }

    Ok(test_folders)
}
