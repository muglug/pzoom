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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
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

    /// Number of worker threads (defaults to available CPUs)
    #[arg(short = 'j', long)]
    jobs: Option<usize>,
}

/// Holds pre-scanned stub data that can be reused across tests.
#[derive(Clone)]
struct BaseCodebase {
    codebase: CodebaseInfo,
    interner: Interner,
    stub_files: FxHashSet<pzoom_str::StrId>,
}

/// The PHP version the reused base codebase's CallMap was applied for
/// (the harness default, 8.0). Tests pinning another version via
/// php_version.txt are scanned from scratch for it (see
/// `test_requires_fresh_scan`).
const BASE_CALLMAP_PHP_VERSION_ID: u32 = 80_000;

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
    let t_prescan = Instant::now();
    let base_codebase = if cli.reuse_codebase && test_folders.len() > 1 {
        let start = Instant::now();
        if cli.verbose {
            eprintln!("Pre-scanning stubs for reuse...");
        }

        let mut scanner = Scanner::new();
        if Path::new(&stubs_path).exists() {
            scanner.scan_stub_directory(Path::new(&stubs_path), &test_enabled_extensions());
        }
        let mut scan_result = scanner.finish();

        // Builtin signatures come from Psalm's CallMap for the harness default
        // PHP version; per-test version pins re-apply on their clone.
        pzoom_orchestrator::apply_call_map(
            &mut scan_result.codebase,
            &scan_result.interner,
            BASE_CALLMAP_PHP_VERSION_ID,
        );

        // Populate the stub codebase once here: per-test populate then only
        // touches the test file's own symbols (is_populated flags skip the
        // rest), mirroring how Hakana reuses its pre-built core codebase.
        {
            let mut populator = Populator::new(&mut scan_result.codebase, &scan_result.interner);
            populator.populate();
        }

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
    if std::env::var("PZOOM_TEST_TIMING").is_ok() {
        eprintln!("TIMING prescan={:.0}ms", t_prescan.elapsed().as_secs_f64() * 1000.0);
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut failures: Vec<(usize, String, String)> = Vec::new();

    let start = Instant::now();

    let mut runnable_tests: Vec<(usize, String)> = Vec::new();
    for (index, test_folder) in test_folders.iter().enumerate() {
        if test_folder.contains("skipped-") || test_folder.contains("SKIPPED-") {
            skipped += 1;
            print!("S");
            io::stdout().flush().unwrap();
        } else {
            runnable_tests.push((index, test_folder.clone()));
        }
    }

    let default_jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let requested_jobs = cli.jobs.unwrap_or(default_jobs).max(1);
    let worker_count = requested_jobs.max(1).min(runnable_tests.len().max(1));

    if cli.verbose {
        eprintln!(
            "Running {} tests with {} worker(s)",
            runnable_tests.len(),
            worker_count
        );
    }

    if worker_count == 1 {
        for (index, test_folder) in &runnable_tests {
            let result = match base_codebase {
                Some(ref base) if !test_requires_fresh_scan(test_folder) => {
                    run_test_with_base(test_folder, base, cli.update, cli.verbose)
                }
                _ => run_test(test_folder, &stubs_path, cli.update, cli.verbose),
            };

            match result {
                TestResult::Pass => {
                    passed += 1;
                    print!(".");
                }
                TestResult::Fail(diff) => {
                    failed += 1;
                    print!("F");
                    failures.push((*index, test_folder.clone(), diff));
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
    } else {
        let jobs = Arc::new(runnable_tests);
        let next_job = Arc::new(AtomicUsize::new(0));
        let (result_tx, result_rx) = mpsc::channel::<(usize, String, TestResult)>();
        let mut handles = Vec::new();

        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let next_job = Arc::clone(&next_job);
            let result_tx = result_tx.clone();
            let stubs_path = stubs_path.clone();
            let base_codebase = base_codebase.clone();
            let update = cli.update;
            let verbose = cli.verbose;

            handles.push(std::thread::spawn(move || {
                loop {
                    let job_index = next_job.fetch_add(1, Ordering::Relaxed);
                    let Some((index, test_folder)) = jobs.get(job_index) else {
                        break;
                    };

                    let result = match base_codebase {
                        Some(ref base) if !test_requires_fresh_scan(test_folder) => {
                            run_test_with_base(test_folder, base, update, verbose)
                        }
                        _ => run_test(test_folder, &stubs_path, update, verbose),
                    };

                    if result_tx
                        .send((*index, test_folder.clone(), result))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        drop(result_tx);

        let total = jobs.len();
        for _ in 0..total {
            let (index, test_folder, result) = result_rx.recv().unwrap();
            match result {
                TestResult::Pass => {
                    passed += 1;
                    print!(".");
                }
                TestResult::Fail(diff) => {
                    failed += 1;
                    print!("F");
                    failures.push((index, test_folder, diff));
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

        for handle in handles {
            let _ = handle.join();
        }
    }

    let elapsed = start.elapsed();
    println!("\n");

    if !failures.is_empty() {
        failures.sort_by_key(|(index, _, _)| *index);
        println!("Failures:\n");
        for (_, folder, diff) in &failures {
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
    let t_clone = Instant::now();
    let mut codebase = base.codebase.clone();
    let interner = base.interner.clone();
    let stub_files = base.stub_files.clone();
    let clone_ms = t_clone.elapsed().as_secs_f64() * 1000.0;
    let t_scan = Instant::now();

    // Scan just the test file (through a single-thread ThreadedInterner
    // handle, as the declaration collector requires).
    let interner_arc = std::sync::Arc::new(interner);
    let threaded_interner = pzoom_str::ThreadedInterner::new(interner_arc.clone());
    let file_path_id = threaded_interner.intern(&input_path);
    let file_id = pzoom_syntax::FileId::new(&input_path);

    let arena = bumpalo::Bump::new();
    let (program, parse_error) =
        pzoom_syntax::parse_file_content(&arena, file_id, &input_contents);
    let parse_errors: Vec<(u32, String)> = parse_error
        .map(|error| {
            use pzoom_syntax::HasSpan;
            vec![(error.span().start.offset, format!("{}", error))]
        })
        .unwrap_or_default();

    // Pre-wave (mirrors Scanner::scan_file): harvest type-alias definitions
    // first so `@psalm-import-type` resolves even when the defining class
    // appears later in the file.
    if input_contents.contains("@psalm-type") || input_contents.contains("@phpstan-type") {
        let harvest_collector = pzoom_syntax::DeclarationCollector::new(
            &threaded_interner,
            file_path_id,
            &input_contents,
            &codebase.type_aliases,
            &program.trivia,
        );
        let harvested = harvest_collector.collect(program);
        for type_alias in harvested.type_aliases {
            codebase
                .type_aliases
                .entry(type_alias.name)
                .or_insert(type_alias);
        }
    }

    let collector = pzoom_syntax::DeclarationCollector::new(
        &threaded_interner,
        file_path_id,
        &input_contents,
        &codebase.type_aliases,
        &program.trivia,
    );
    let mut declarations = collector.collect(program);
    let inline_annotations = std::mem::take(&mut declarations.inline_annotations);
    let docblock_parse_issues = std::mem::take(&mut declarations.docblock_parse_issues);

    // Record file info first so symbol registration can resolve stub/project
    // precedence (mirrors Scanner::register_collected_file).
    let file_info = pzoom_code_info::codebase_info::FileInfo {
        path: file_path_id,
        classes: Vec::new(),
        functions: Vec::new(),
        constants: Vec::new(),
        content_hash: compute_hash(&input_contents),
        contents: input_contents,
        parse_errors,
        docblock_parse_issues,
        is_stub: false,
        is_low_precedence_stub: false,
        is_in_project_dirs: true,
        inline_annotations,
        type_alias_imports: std::mem::take(&mut declarations.type_alias_imports),
    };
    codebase.files.insert(file_path_id, file_info);

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

    for type_alias in declarations.type_aliases {
        codebase.type_aliases.insert(type_alias.name, type_alias);
    }

    codebase.global_defines.extend(declarations.global_defines);

    if let Some(file_info) = codebase.files.get_mut(&file_path_id) {
        file_info.classes = file_classes;
        file_info.functions = file_functions;
        file_info.constants = file_constants;
    }
    let scan_ms = t_scan.elapsed().as_secs_f64() * 1000.0;

    if std::env::var("PZOOM_TEST_TIMING").is_ok() {
        eprintln!("TIMING clone={:.1}ms scan={:.1}ms {}", clone_ms, scan_ms, test_folder);
    }

    drop(threaded_interner);
    let interner = std::sync::Arc::try_unwrap(interner_arc)
        .expect("single-thread interner handle dropped above");

    // Run analysis
    run_analysis_and_compare(
        &mut codebase,
        &interner,
        &stub_files,
        &input_path,
        &output_path,
        update,
        BASE_CALLMAP_PHP_VERSION_ID,
    )
}

/// Optional extensions enabled when scanning stubs for tests: none, mirroring
/// Psalm's test suite, which runs without PECL/third-party extension stubs.
fn test_enabled_extensions() -> rustc_hash::FxHashSet<String> {
    rustc_hash::FxHashSet::default()
}

/// Whether a test must be scanned from scratch instead of cloning the shared
/// base codebase (i.e. `--reuse-codebase` must be ineffective for it).
///
/// The base is scanned and populated once for the harness-default PHP version
/// with unused-code and taint tracking off. A test diverges from that when it:
/// - pins a different PHP version via `php_version.txt` — re-applying the
///   CallMap onto an already-populated base does not re-run the
///   version-dependent stub scan/populate; or
/// - is an unused-code or taint test — those enable extra scan/populate
///   tracking the shared base was not built with.
fn test_requires_fresh_scan(test_folder: &str) -> bool {
    let input_path = format!("{}/input.php", test_folder);

    if input_path.contains("/UnusedCode/") || input_path.contains("/Taint/") {
        return true;
    }

    if let Some(test_dir) = Path::new(&input_path).parent()
        && let Ok(version) = fs::read_to_string(test_dir.join("php_version.txt"))
    {
        let version = version.trim();
        if !version.is_empty() && php_version_id(version) != BASE_CALLMAP_PHP_VERSION_ID {
            return true;
        }
    }

    false
}

/// Convert a "X.Y[.Z]" PHP version string to a comparable id (matching
/// `Config::php_version_id`).
fn php_version_id(version: &str) -> u32 {
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(8);
    let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    major * 10_000 + minor * 100
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
        scanner.scan_stub_directory(Path::new(stubs_path), &test_enabled_extensions());
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
        0, // fresh scan: the CallMap has not been applied yet
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
    applied_callmap_php_version_id: u32,
) -> TestResult {
    let mut config = Config::default();
    // The harness default is PHP 8.0 (Psalm's own TestCase::setUp pins 7.4, but
    // pzoom's corpus targets 8.0); individual tests opt into another version via
    // 'php_version' (ported as php_version.txt).
    config.php_version = "8.0".to_string();
    config
        .forbidden_functions
        .extend(["var_dump".to_string(), "shell_exec".to_string()]);
    config
        .suppressed_issues
        .extend(load_test_suppressed_issues(input_path));
    // Psalm's OverrideTest runs with ensureOverrideAttribute enabled; every
    // other test class keeps the default (false).
    config.ensure_override_attribute = input_path.contains("/Override/");
    // Psalm's test harness always tracks unused suppressions
    // (TestCase::analyzeFile defaults $track_unused_suppressions = true).
    config.find_unused_suppress = true;
    // Psalm's UnusedVariableTest enables reportUnusedVariables() in setUp.
    config.report_unused = input_path.contains("/UnusedVariable/")
        || input_path.contains("/UnusedCode/");
    // Psalm's UnusedCodeTest calls reportUnusedCode(), enabling declaration
    // usage tracking on top of unused-variable reporting.
    config.find_unused_code = input_path.contains("/UnusedCode/");
    // Psalm's TaintTest calls trackTaintedInputs(). Its expectations only ever
    // assert taint issues (the harness suppresses a long legacy list of
    // analysis kinds), so taint mode reports Tainted* issues exclusively.
    config.taint_analysis = input_path.contains("/Taint/");
    // Psalm-on-Psalm runs with allConstantsGlobal; the GlobalConstants suite
    // covers the define()-anywhere registration it enables.
    config.all_constants_global = input_path.contains("/GlobalConstants/");
    // Psalm tests can pin a PHP version per test case; the pzoom port keeps it
    // in a php_version.txt sidecar next to input.php.
    if let Some(test_dir) = Path::new(input_path).parent()
        && let Ok(version) = fs::read_to_string(test_dir.join("php_version.txt")) {
            let version = version.trim();
            if !version.is_empty() {
                config.php_version = version.to_string();
            }
        }
    // Psalm enables some config flags per test case (e.g. ArrayAccessTest sets
    // Config::getInstance()->ensure_array_int_offsets_exist = true); the pzoom
    // port keeps each as a marker file next to input.php.
    if let Some(test_dir) = Path::new(input_path).parent() {
        if test_dir.join("ensure_array_int_offsets_exist").exists() {
            config.ensure_array_int_offsets_exist = true;
        }
        if test_dir.join("ensure_array_string_offsets_exist").exists() {
            config.ensure_array_string_offsets_exist = true;
        }
    }

    // Builtin signatures come from Psalm's CallMap for the analysis version;
    // skip when the (reused) base codebase was already applied for it.
    if applied_callmap_php_version_id != config.php_version_id() {
        pzoom_orchestrator::apply_call_map(codebase, interner, config.php_version_id());
    }

    // Populate
    let t_pop = Instant::now();
    {
        let mut populator = Populator::new(codebase, interner);
        populator.populate();
    }
    if config.all_constants_global {
        pzoom_orchestrator::register_global_defined_constants(codebase);
    }
    let pop_ms = t_pop.elapsed().as_secs_f64() * 1000.0;

    // Analyze only non-stub files (the test file): stubs provide type
    // information but are never analyzed — Psalm's harness does the same, and
    // analyzing the whole stub set per test dominated the runtime (~165ms of
    // a ~170ms test).
    let t_an = Instant::now();
    let analyzer = Analyzer::new(codebase, interner, &config);
    let files_to_analyze: Vec<pzoom_str::StrId> = codebase
        .files
        .iter()
        .filter(|(file_id, file_info)| !file_info.is_stub && !_stub_files.contains(*file_id))
        .map(|(file_id, _)| *file_id)
        .collect();
    let result = analyzer.analyze_files(&files_to_analyze);
    let an_ms = t_an.elapsed().as_secs_f64() * 1000.0;
    if std::env::var("PZOOM_TEST_TIMING").is_ok() {
        eprintln!("TIMING populate={:.1}ms analyze={:.1}ms {}", pop_ms, an_ms, input_path);
    }

    // Format output in Psalm style: IssueKind - file:line:column - message
    let mut output_lines: Vec<String> = Vec::new();
    for issue in &result.issues {
        let kind_name = format!("{:?}", issue.kind);
        if config.is_issue_suppressed(&kind_name) {
            continue;
        }

        // Taint mode is an allowlist: only taint findings are asserted.
        if config.taint_analysis && !kind_name.starts_with("Tainted") {
            continue;
        }

        let file_path = interner.lookup(issue.location.file_path);
        // Only include issues from the test file, not stubs
        if file_path.contains(input_path) || file_path.ends_with("input.php") {
            // Taint traces embed file positions; Psalm prints them relative
            // to the project root (`input.php:3:12`), so strip the test
            // directory prefix from the message.
            let mut message = issue.message.clone();
            if let Some(dir) = Path::new(file_path.as_ref()).parent() {
                message = message.replace(&format!("{}/", dir.display()), "");
            }
            // Secondary locations stay attached to their issue (one multi-line
            // entry), so sorting keeps each block together.
            let mut entry = format!(
                "{:?} - {}:{}:{} - {}",
                issue.kind, "input.php", issue.location.start_line, issue.location.start_column, message
            );
            for secondary in &issue.secondary_locations {
                let secondary_file = interner.lookup(secondary.location.file_path);
                let secondary_label = if secondary_file.contains(input_path)
                    || secondary_file.ends_with("input.php")
                {
                    "input.php".to_string()
                } else {
                    secondary_file
                        .rsplit('/')
                        .next()
                        .unwrap_or(&secondary_file)
                        .to_string()
                };
                entry.push_str(&format!(
                    "\n    {}:{}:{} - {}",
                    secondary_label,
                    secondary.location.start_line,
                    secondary.location.start_column,
                    secondary.message
                ));
            }
            output_lines.push(entry);
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
        output_lines
            .iter()
            .any(|line| line.contains(&expected_output))
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

fn load_test_suppressed_issues(input_path: &str) -> Vec<String> {
    let Some(test_dir) = Path::new(input_path).parent() else {
        return Vec::new();
    };

    let mut suppressed_issues = Vec::new();
    suppressed_issues.extend(load_suppressed_issues_file(
        &test_dir.join("error_levels.json"),
    ));
    suppressed_issues.extend(load_suppressed_issues_file(&test_dir.join("errors.json")));
    suppressed_issues.sort();
    suppressed_issues.dedup();
    suppressed_issues
}

fn load_suppressed_issues_file(path: &Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    serde_json::from_str::<Vec<String>>(&contents).unwrap_or_default()
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
