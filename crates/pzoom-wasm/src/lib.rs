//! WebAssembly bindings for the pzoom playground (pzoom.dev).
//!
//! Mirrors the test-runner's reuse-codebase flow: the constructor scans the
//! embedded stubs and applies the CallMap once; each `get_results` call clones
//! that base codebase, scans the playground snippet into it, populates, and
//! analyzes just that one file.

use pzoom_analyzer::Config;
use pzoom_code_info::CodebaseInfo;
use pzoom_orchestrator::{Analyzer, Populator, Scanner};
use pzoom_str::Interner;
use serde_json::json;
use wasm_bindgen::prelude::*;

const PLAYGROUND_FILE: &str = "playground.php";
const PHP_VERSION: &str = "8.5";

#[wasm_bindgen]
pub struct ScannerAndAnalyzer {
    codebase: CodebaseInfo,
    interner: Interner,
}

#[wasm_bindgen]
impl ScannerAndAnalyzer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_results(&mut self, file_contents: String) -> String {
        let mut codebase = self.codebase.clone();

        // Scan just the playground file (through a single-thread
        // ThreadedInterner handle, as the declaration collector requires).
        let interner_arc = std::sync::Arc::new(self.interner.clone());
        let threaded_interner = pzoom_str::ThreadedInterner::new(interner_arc.clone());
        let file_path_id = threaded_interner.intern(PLAYGROUND_FILE);
        let file_id = pzoom_syntax::FileId::new(PLAYGROUND_FILE);

        let arena = bumpalo::Bump::new();
        let (program, _parse_error) =
            pzoom_syntax::parse_file_content(&arena, file_id, &file_contents);

        // Pre-wave (mirrors Scanner::scan_file): harvest type-alias
        // definitions first so `@psalm-import-type` resolves even when the
        // defining class appears later in the file.
        if file_contents.contains("@psalm-type") || file_contents.contains("@phpstan-type") {
            let harvest_collector = pzoom_syntax::DeclarationCollector::new(
                &threaded_interner,
                file_path_id,
                &file_contents,
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
            &file_contents,
            &codebase.type_aliases,
            &program.trivia,
        );
        let mut declarations = collector.collect(program);
        let inline_annotations = std::mem::take(&mut declarations.inline_annotations);
        let docblock_parse_issues = std::mem::take(&mut declarations.docblock_parse_issues);

        // Record file info first so symbol registration can resolve
        // stub/project precedence (mirrors Scanner::register_collected_file).
        let file_info = pzoom_code_info::codebase_info::FileInfo {
            path: file_path_id,
            classes: Vec::new(),
            functions: Vec::new(),
            constants: Vec::new(),
            content_hash: compute_hash(&file_contents),
            contents: file_contents,
            parse_errors: Vec::new(),
            docblock_parse_issues,
            is_stub: false,
            is_low_precedence_stub: false,
            is_in_project_dirs: true,
            inline_annotations,
            type_alias_imports: std::mem::take(&mut declarations.type_alias_imports),
        };
        codebase.files.insert(file_path_id, file_info);

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

        drop(threaded_interner);
        let interner = std::sync::Arc::try_unwrap(interner_arc)
            .expect("single-thread interner handle dropped above");

        let mut config = Config::default();
        config.php_version = PHP_VERSION.to_string();
        config.threads = 1;

        // The base codebase's CallMap was applied for PHP_VERSION in the
        // constructor, so only populate remains. Populating the base once at
        // startup means this pass only touches the playground file's symbols.
        {
            let mut populator = Populator::new(&mut codebase, &interner);
            populator.populate();
        }

        let analyzer = Analyzer::new(&codebase, &interner, &config);
        let result = analyzer.analyze_files(&[file_path_id]);

        let mut issue_json_objects = vec![];
        for issue in &result.issues {
            let kind_name = format!("{:?}", issue.kind);
            if config.is_issue_suppressed(&kind_name) {
                continue;
            }
            if issue.location.file_path != file_path_id {
                continue;
            }

            issue_json_objects.push(json!({
                "severity": "ERROR",
                "type": kind_name,
                "message": issue.message,
                "line_from": issue.location.start_line,
                "column_from": issue.location.start_column,
                "from": issue.location.start_offset,
                "to": issue.location.end_offset,
            }));
        }

        json!({ "results": issue_json_objects }).to_string()
    }
}

impl Default for ScannerAndAnalyzer {
    fn default() -> Self {
        console_error_panic_hook::set_once();

        let mut scanner = Scanner::new();
        scanner.scan_stubs(&rustc_hash::FxHashSet::default());
        let mut scan_result = scanner.finish();

        // Builtin signatures come from Psalm's CallMap for the playground's
        // PHP version.
        let php_version_id = {
            let mut parts = PHP_VERSION.split('.');
            let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(8);
            let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            major * 10_000 + minor * 100
        };
        pzoom_orchestrator::apply_call_map(
            &mut scan_result.codebase,
            &scan_result.interner,
            php_version_id,
        );

        // Populate the stub codebase once here: per-run populate then only
        // touches the playground file's own symbols.
        {
            let mut populator = Populator::new(&mut scan_result.codebase, &scan_result.interner);
            populator.populate();
        }

        Self {
            codebase: scan_result.codebase,
            interner: scan_result.interner,
        }
    }
}

fn compute_hash(contents: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
