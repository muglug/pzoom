//! pzoom CLI - Command-line interface for the pzoom PHP static analyzer.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod shortcodes;

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

    /// Enable the unused-declaration pass (UnusedClass/Method/Property, …).
    /// References are aggregated codebase-wide (see unused_symbols), so this is
    /// also enabled automatically by `findUnusedCode` in psalm.xml; the flag
    /// forces it on for an ad-hoc run.
    #[arg(long)]
    find_unused_code: bool,

    /// Extra stub file(s) or director(ies) to scan for type information but
    /// never analyze — the ingestion point for stubs produced by a PHP
    /// stub-provider (see bin/pzoom). A directory is scanned for its `.php` /
    /// `.phpstub` files. Repeatable; merged with psalm.xml `<stubs>`. `global`
    /// so it's accepted before or after the subcommand (bin/pzoom appends it).
    #[arg(long = "stubs", value_name = "PATH", global = true)]
    stubs: Vec<PathBuf>,
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

    if cli.find_unused_code {
        config.find_unused_code = true;
    }

    // Extra stubs from the command line (e.g. those a PHP stub-provider
    // generated, passed by bin/pzoom) join the psalm.xml `<stubs>` set. Resolve
    // them to absolute paths now, against the invoking cwd, so they don't get
    // re-rooted at the project directory when scanned.
    for stub in &cli.stubs {
        let absolute = stub
            .canonicalize()
            .unwrap_or_else(|_| stub.clone())
            .to_string_lossy()
            .into_owned();
        config.stubs.push(absolute);
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

/// Recursively collect `.php` / `.phpstub` files under a stub directory passed
/// via `--stubs` or psalm.xml `<stubs>`.
fn collect_stub_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "php" || ext == "phpstub")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn analyze(config: &Config, paths: &[PathBuf]) -> ExitCode {
    let start_time = std::time::Instant::now();

    println!("Scanning files...");

    // Determine the project root to scan
    // If paths is just ".", scan current directory
    // Otherwise, find the project root from the first specified path
    let project_root = if paths.len() == 1 && paths[0] == *"." {
        PathBuf::from(".")
    } else {
        // Find project root from the first path
        let first_path = paths[0].canonicalize().unwrap_or_else(|_| paths[0].clone());
        find_project_root(&first_path)
    };
    let display_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.clone());

    // Phase 1: Scan the full codebase
    // We always scan the entire project directory to get complete type information,
    // even when analyzing a single file (Hakana-style behavior).
    let mut scanner = Scanner::new();
    scanner.set_exclude_patterns(&config.exclude_patterns);

    // Optional (PECL/third-party) extension stubs load only when enabled:
    // `php -m`, a project php.ini, composer.json ext-* requires, and
    // psalm.xml enableExtensions/disableExtensions.
    let enabled_extensions = pzoom_orchestrator::resolve_enabled_extensions(
        &display_root,
        &config.enabled_extensions,
        &config.disabled_extensions,
    );

    // Scan built-in stubs first to get PHP standard library types
    scanner.scan_stubs_for_php_version(&enabled_extensions, config.php_version_id());

    // Plugin-registered stubs (psalm/plugin-mockery etc.) load as stubs
    // BEFORE project/vendor sources so their members override the real
    // definitions (register_class's stub-overrides-dependency merge).
    for plugin_stub in &config.plugin_stubs {
        let stub_path = display_root.join(plugin_stub);
        if stub_path.is_file() {
            scanner.scan_stub_file(&stub_path);
        }
    }

    // User and provider stubs: psalm.xml `<stubs>` entries plus any `--stubs`
    // paths (the latter resolved absolute already; an absolute join is a no-op).
    // Each entry is a stub file or a directory of `.php` / `.phpstub` stubs —
    // type information only, never analyzed or reported.
    for stub in &config.stubs {
        let stub_path = display_root.join(stub);
        if stub_path.is_file() {
            scanner.scan_stub_file(&stub_path);
        } else if stub_path.is_dir() {
            for file in collect_stub_files(&stub_path) {
                scanner.scan_stub_file(&file);
            }
        }
    }

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

    let dir_refs: Vec<&Path> = analysis_dirs.iter().map(|d| d.as_path()).collect();
    scanner.scan_directories(&dir_refs);

    // Scan dependency sources for type information, but don't analyze them
    // unless explicitly targeted. They are not project files (Psalm's
    // isInProjectDirs), so stub classes may member-override theirs.
    //
    // Resolve dependencies on demand through Composer's autoload maps (like
    // Psalm), scanning only the referenced subset of `vendor/`. Walking the
    // whole tree pulls in every package's tests, fixtures, and example data —
    // files PHP never autoloads, which only add noise and can shadow builtins
    // (an illegal global `function strlen(){}` test fixture, say). Fall back to
    // a full walk when there is no Composer autoloader.
    let vendor_dir = display_root.join("vendor");
    if vendor_dir.is_dir() {
        match pzoom_orchestrator::ComposerAutoload::load(&vendor_dir) {
            Some(autoload) => scanner.scan_dependencies_on_demand(&autoload),
            None => scanner.scan_dependency_directories(&[vendor_dir.as_path()]),
        }
    }

    for file in &analysis_files {
        if let Ok(contents) = std::fs::read_to_string(file) {
            let canonical = file
                .canonicalize()
                .unwrap_or_else(|_| file.clone())
                .to_string_lossy()
                .into_owned();
            scanner.scan_file(&canonical, &contents);
        }
    }

    let mut scan_result = scanner.finish();

    // Psalm types builtins through its per-version CallMap, with its own
    // stubs overriding a curated subset; apply the same over the scanned
    // stub functions for the analysis PHP version.
    pzoom_orchestrator::apply_call_map(
        &mut scan_result.codebase,
        &scan_result.interner,
        config.php_version_id(),
    );

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

    // Let active plugins record framework knowledge on the populated codebase
    // (e.g. the PHPUnit plugin flags TestCase subclasses + their test methods)
    // and collect any diagnostics they emit (e.g. @dataProvider validation).
    // Runs before the immutable borrow taken by `Analyzer::new`.
    let plugin_issues =
        pzoom_analyzer::plugin::run_after_populate(&config.plugins, &mut codebase, &interner);

    if config.all_constants_global {
        pzoom_orchestrator::register_global_defined_constants(&mut codebase);
    }

    if std::env::var("PZOOM_MEM_STATS").is_ok() {
        print_mem_stats(&codebase, &interner);
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
    let analyze_all = paths.len() == 1 && paths[0] == *".";
    let targeting_project_root = paths.len() == 1
        && paths[0]
            .canonicalize()
            .ok()
            .is_some_and(|p| p == display_root);

    let mut result = if analyze_all || targeting_project_root {
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
                for &file_id in codebase.files.keys() {
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

    // Plugin diagnostics from the post-populate hook join the analysis issues so
    // they share the same stub/global/baseline suppression below.
    result.issues.extend(plugin_issues);

    let mut baseline = load_error_baseline(config, &display_root);

    // Filter out issues from stubs, globally suppressed issues, and baseline-covered issues.
    let mut user_issues = Vec::new();
    for issue in &result.issues {
        if stub_files.contains(&issue.location.file_path) {
            continue;
        }

        let issue_name = format!("{:?}", issue.kind);
        let file_path = interner.lookup(issue.location.file_path);
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

    // Output results in Psalm's console report format.
    let use_color = std::io::IsTerminal::is_terminal(&std::io::stdout());
    println!();

    // Sort issues by file path, then line; issues on the same line emit
    // in alphabetical order (kind, then message), with column as the
    // final tiebreak — emission order is analysis-order-dependent and
    // would otherwise vary run to run.
    let mut sorted_issues = user_issues;
    sorted_issues.sort_by_cached_key(|issue| {
        (
            format_display_path(&interner.lookup(issue.location.file_path), &display_root),
            issue.location.start_line,
            format!("{:?}", issue.kind),
            issue.message.clone(),
            issue.location.start_column,
        )
    });

    let error_count = sorted_issues.len();

    for issue in &sorted_issues {
        print!(
            "{}\n\n",
            format_console_issue(issue, &codebase, &interner, &display_root, use_color)
        );
    }

    // Psalm's IssueBuffer summary: dashes, error count (or success block),
    // dashes, then the timing/memory line.
    println!("{}", "-".repeat(30));
    if error_count > 0 {
        if use_color {
            println!("\u{1b}[0;31m{} errors\u{1b}[0m found", error_count);
        } else {
            println!("{} errors found", error_count);
        }
    } else {
        print_success_message(use_color);
    }
    println!("{}\n", "-".repeat(30));

    println!(
        "Checks took {:.2} seconds and used {}MB of memory",
        start_time.elapsed().as_secs_f64(),
        format_number(peak_memory_bytes() as f64 / (1024.0 * 1024.0), 3),
    );

    if matches!(std::env::var("PZOOM_TYPE_COVERAGE").as_deref(), Ok("1") | Ok("true")) {
        let (mixed, non_mixed) = pzoom_analyzer::type_coverage::snapshot();
        let total = mixed + non_mixed;
        let pct = if total > 0 { non_mixed as f64 / total as f64 * 100.0 } else { 0.0 };
        eprintln!("PZOOM-TYPE-COVERAGE mixed={mixed} non_mixed={non_mixed} total={total} coverage={pct:.4}%");
    }

    if error_count > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// Psalm's `IssueBuffer::printSuccessMessage`: a green block around
/// "No errors found!".
fn print_success_message(use_color: bool) {
    let message_with_padding = format!("{0}No errors found!{0}", " ".repeat(7));
    if use_color {
        let padding = " ".repeat(30);
        println!("\u{1b}[42;2m{padding}\u{1b}[0m");
        println!("\u{1b}[42;30;2m{message_with_padding}\u{1b}[0m");
        println!("\u{1b}[42;2m{padding}\u{1b}[0m");
    } else {
        println!();
        println!("{message_with_padding}");
        println!();
    }
}

/// PHP `number_format($n, $decimals)`: thousands separated with commas.
fn format_number(value: f64, decimals: usize) -> String {
    let formatted = format!("{value:.decimals$}");
    let (int_part, frac_part) = formatted
        .split_once('.')
        .unwrap_or((formatted.as_str(), ""));
    let mut grouped = String::new();
    let digits: Vec<char> = int_part.chars().collect();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) && *c != '-' {
            grouped.push(',');
        }
        grouped.push(*c);
    }
    if frac_part.is_empty() {
        grouped
    } else {
        format!("{grouped}.{frac_part}")
    }
}

/// Peak resident set size in bytes (`memory_get_peak_usage` stand-in).
fn peak_memory_bytes() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    // ru_maxrss is bytes on macOS, kilobytes on Linux.
    #[cfg(target_os = "macos")]
    return usage.ru_maxrss as u64;
    #[cfg(not(target_os = "macos"))]
    return (usage.ru_maxrss as u64) * 1024;
}

/// Format one issue the way Psalm's `Report\ConsoleReport::format` does:
/// an `ERROR: Kind - file:line:col - message (see https://psalm.dev/NNN)`
/// header followed by the source snippet with the selection highlighted.
/// Taint traces replace the main snippet; other secondary references follow it.
fn format_console_issue(
    issue: &pzoom_code_info::Issue,
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    display_root: &Path,
    use_color: bool,
) -> String {
    let kind = format!("{:?}", issue.kind);
    let mut out = String::new();

    if use_color {
        out.push_str("\u{1b}[0;31mERROR\u{1b}[0m");
    } else {
        out.push_str("ERROR");
    }

    let issue_reference = match shortcodes::shortcode(&kind) {
        Some(code) => format!(" (see https://psalm.dev/{code:03})"),
        None => String::new(),
    };

    // Psalm's taint messages stop before the path; the trace renders as
    // labelled snippets below instead.
    let message = if issue.taint_trace.is_empty() {
        issue.message.as_str()
    } else {
        issue
            .message
            .split(" in path: ")
            .next()
            .unwrap_or(&issue.message)
    };

    out.push_str(&format!(
        ": {} - {} - {}{}\n",
        kind,
        format_file_reference(&issue.location, interner, display_root, use_color),
        message,
        issue_reference,
    ));

    if !issue.taint_trace.is_empty() {
        for node in &issue.taint_trace {
            out.push_str(&format_trace_snippet(
                &node.label,
                node.location.as_ref(),
                codebase,
                interner,
                display_root,
                use_color,
            ));
        }
    } else {
        if let Some(snippet) = format_snippet(&issue.location, codebase, use_color, true) {
            out.push_str(&snippet);
        }

        if !issue.secondary_locations.is_empty() {
            out.push('\n');
            for secondary in &issue.secondary_locations {
                out.push_str(&format_trace_snippet(
                    &secondary.message,
                    Some(&secondary.location),
                    codebase,
                    interner,
                    display_root,
                    use_color,
                ));
            }
        }
    }

    // The caller appends the blank-line issue separator.
    while out.ends_with('\n') {
        out.pop();
    }

    out
}

/// Psalm's `ConsoleReport::getFileReference`: `file:line:col`, with the
/// basename highlighted when color is enabled.
fn format_file_reference(
    location: &pzoom_code_info::CodeLocation,
    interner: &pzoom_str::Interner,
    display_root: &Path,
    use_color: bool,
) -> String {
    let display_path = format_display_path(&interner.lookup(location.file_path), display_root);
    if !use_color {
        return format!(
            "{}:{}:{}",
            display_path, location.start_line, location.start_column
        );
    }

    let (dir_part, base_part) = match display_path.rfind('/') {
        Some(idx) => display_path.split_at(idx + 1),
        None => ("", display_path.as_str()),
    };

    format!(
        "{}\u{1b}[1;31m{}:{}:{}\u{1b}[0m",
        dir_part, base_part, location.start_line, location.start_column
    )
}

/// One taint-trace / secondary-reference entry, as in
/// `ConsoleReport::getTaintSnippets`.
fn format_trace_snippet(
    label: &str,
    location: Option<&pzoom_code_info::CodeLocation>,
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    display_root: &Path,
    use_color: bool,
) -> String {
    let Some(location) = location else {
        return format!("  {label}\n    <no known location>\n\n");
    };

    let mut out = format!(
        "  {} - {}\n",
        label,
        format_file_reference(location, interner, display_root, use_color),
    );
    if let Some(snippet) = format_snippet(location, codebase, use_color, false) {
        out.push_str(&snippet);
    }
    out.push('\n');
    out
}

/// The source snippet for a location with the selection highlighted, following
/// Psalm `CodeLocation`'s preview bounds: from the start of the selection's
/// first line through the end of its last line, truncated 200 chars past the
/// selection. Errors highlight white-on-red; trace entries black-on-white.
fn format_snippet(
    location: &pzoom_code_info::CodeLocation,
    codebase: &pzoom_code_info::CodebaseInfo,
    use_color: bool,
    is_error: bool,
) -> Option<String> {
    let contents = codebase
        .files
        .get(&location.file_path)
        .map(|file| file.contents.as_str())?;

    let mut selection_start = location.start_offset as usize;
    let mut selection_end = location.end_offset as usize;
    if selection_end > contents.len() || selection_start > selection_end {
        return None;
    }

    let preview_start = contents[..selection_start]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let mut preview_end = contents[selection_end..]
        .find('\n')
        .map(|idx| selection_end + idx)
        .unwrap_or(selection_end);

    selection_start = selection_start.max(preview_start);
    selection_end = selection_end.min(preview_end);

    // Psalm truncates previews that run more than 200 characters past the
    // selection (long single-line files).
    if preview_end - selection_end > 200 {
        preview_end = contents[..selection_end + 200]
            .rfind('\n')
            .filter(|&idx| idx >= selection_end)
            .unwrap_or(selection_end + 50);
    }

    let snippet = contents.get(preview_start..preview_end)?;

    if !use_color {
        // Psalm appends the raw snippet with no trailing newline here; the
        // issue separator provides the line break.
        return Some(format!("{snippet}\n"));
    }

    let highlight_start = selection_start - preview_start;
    let highlight_end = selection_end - preview_start;
    let before = snippet.get(..highlight_start)?;
    let selected = snippet.get(highlight_start..highlight_end)?;
    let after = snippet.get(highlight_end..)?;
    let highlight = if is_error {
        "\u{1b}[97;41m"
    } else {
        "\u{1b}[30;47m"
    };

    Some(format!("{before}{highlight}{selected}\u{1b}[0m{after}\n"))
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
        .get(&issue.location.file_path)
        .map(|file| file.contents.as_str());

    let Some(file_contents) = file_contents else {
        return String::new();
    };

    let start = issue.location.start_offset as usize;
    let end = issue.location.end_offset as usize;

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

    if let (Ok(canon_root), Ok(canon_path)) = (root.canonicalize(), path_buf.canonicalize())
        && let Ok(rel) = canon_path.strip_prefix(&canon_root)
    {
        return rel.to_string_lossy().replace('\\', "/");
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path_buf.strip_prefix(&cwd) {
            return rel.to_string_lossy().replace('\\', "/");
        }

        if let (Ok(canon_cwd), Ok(canon_path)) = (cwd.canonicalize(), path_buf.canonicalize())
            && let Ok(rel) = canon_path.strip_prefix(&canon_cwd)
        {
            return rel.to_string_lossy().replace('\\', "/");
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
            // A `dir/**` pattern excludes `dir` wherever it appears in the path,
            // not only as a leading prefix — mirroring the scanner's
            // `should_exclude` so a config-ignored directory (e.g. `data`) is
            // dropped from analysis even at a nested depth, and even when the
            // file entered the codebase via on-demand dependency resolution
            // (which does not consult the exclude list while scanning).
            if let Some(rel) = &rel_string
                && (rel == dir || rel.starts_with(&format!("{}/", dir)))
            {
                return true;
            }

            return path_string.contains(&format!("/{}/", dir))
                || path_string.ends_with(&format!("/{}", dir));
        }

        if let Some(rel) = &rel_string
            && (rel == &pattern || rel.starts_with(&format!("{}/", pattern)))
        {
            return true;
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

/// One-off memory accounting dump (PZOOM_MEM_STATS=1). Serialized JSON sizes
/// are proxies for in-memory footprint; proportions matter more than absolutes.
fn print_mem_stats(codebase: &pzoom_code_info::CodebaseInfo, interner: &pzoom_str::Interner) {
    // Interner
    let mut interner_bytes = 0usize;
    for i in 0..interner.len() {
        interner_bytes += interner.lookup(pzoom_str::StrId(i as u32)).len();
    }
    eprintln!(
        "[mem] interner: {} strings, {:.1} MB",
        interner.len(),
        interner_bytes as f64 / 1e6
    );

    // Files
    let mut contents_bytes = 0usize;
    let mut annotation_bytes = 0usize;
    for file in codebase.files.values() {
        contents_bytes += file.contents.len();
        annotation_bytes += serde_json::to_vec(&file.inline_annotations)
            .map(|v| v.len())
            .unwrap_or(0);
    }
    eprintln!(
        "[mem] files: {} entries, contents {:.1} MB, inline annotations ~{:.1} MB (json)",
        codebase.files.len(),
        contents_bytes as f64 / 1e6,
        annotation_bytes as f64 / 1e6
    );

    // Classlikes: methods declared vs inherited copies
    let mut total_methods = 0usize;
    let mut inherited_methods = 0usize;
    let mut methods_bytes = 0usize;
    let inherited_methods_bytes = 0usize;
    let mut props_total = 0usize;
    let mut props_inherited = 0usize;
    let mut props_bytes = 0usize;
    let props_inherited_bytes = 0usize;
    let mut consts_bytes = 0usize;
    let mut rest_bytes = 0usize;
    let mut seen_method_allocs = std::collections::HashSet::new();
    let mut seen_property_allocs = std::collections::HashSet::new();
    for class_info in codebase.classlike_infos.values() {
        for (method_name, method_info) in &class_info.methods {
            total_methods += 1;
            // Arc-shared entries only cost once; count bytes per unique allocation.
            if seen_method_allocs.insert(std::sync::Arc::as_ptr(method_info) as usize) {
                methods_bytes += serde_json::to_vec(&**method_info)
                    .map(|v| v.len())
                    .unwrap_or(0);
            }
            let declared_here = class_info
                .declaring_method_ids
                .get(method_name)
                .is_none_or(|declaring| *declaring == class_info.name);
            if !declared_here {
                inherited_methods += 1;
            }
        }
        for (prop_name, prop_info) in &class_info.properties {
            props_total += 1;
            if seen_property_allocs.insert(std::sync::Arc::as_ptr(prop_info) as usize) {
                props_bytes += serde_json::to_vec(&**prop_info)
                    .map(|v| v.len())
                    .unwrap_or(0);
            }
            let declared_here = class_info
                .declaring_property_ids
                .get(prop_name)
                .is_none_or(|declaring| *declaring == class_info.name);
            if !declared_here {
                props_inherited += 1;
            }
        }
        consts_bytes += serde_json::to_vec(&class_info.constants)
            .map(|v| v.len())
            .unwrap_or(0);
        rest_bytes += serde_json::to_vec(&class_info.all_parent_classes)
            .map(|v| v.len())
            .unwrap_or(0)
            + serde_json::to_vec(&class_info.all_parent_interfaces)
                .map(|v| v.len())
                .unwrap_or(0)
            + serde_json::to_vec(&class_info.declaring_method_ids)
                .map(|v| v.len())
                .unwrap_or(0)
            + serde_json::to_vec(&class_info.appearing_method_ids)
                .map(|v| v.len())
                .unwrap_or(0);
    }
    let _ = inherited_methods_bytes;
    eprintln!(
        "[mem] classlikes: {} entries; methods {} total ({} inherited Arc-shares = {:.0}%), unique storage ~{:.1} MB json",
        codebase.classlike_infos.len(),
        total_methods,
        inherited_methods,
        inherited_methods as f64 / total_methods.max(1) as f64 * 100.0,
        methods_bytes as f64 / 1e6
    );
    let _ = props_inherited_bytes;
    eprintln!(
        "[mem] classlike properties: {} total ({} inherited Arc-shares), unique storage ~{:.1} MB json; constants ~{:.1} MB; id/parent maps ~{:.1} MB",
        props_total,
        props_inherited,
        props_bytes as f64 / 1e6,
        consts_bytes as f64 / 1e6,
        rest_bytes as f64 / 1e6
    );

    let functions_bytes: usize = codebase
        .functionlike_infos
        .values()
        .map(|info| serde_json::to_vec(info).map(|v| v.len()).unwrap_or(0))
        .sum();
    eprintln!(
        "[mem] functions: {} entries, ~{:.1} MB json",
        codebase.functionlike_infos.len(),
        functions_bytes as f64 / 1e6
    );
}
