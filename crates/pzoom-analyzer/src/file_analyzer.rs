//! File analyzer.
//!
//! Mirrors Psalm's `FileAnalyzer` and Hakana's `file_analyzer.rs`: drives type
//! analysis of a single file — re-parse it, resolve names, run the statement
//! analyzer over the program, and return the (non line-suppressed) issues. The
//! orchestrator delegates per-file analysis here.

use bumpalo::Bump;

use pzoom_code_info::{CodebaseInfo, Issue, IssueKind};
use pzoom_str::{Interner, StrId};
use pzoom_syntax::{FileId, parse_file_content, resolve_names};

use crate::config::Config;
use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer;

pub struct FileAnalyzer<'a> {
    codebase: &'a CodebaseInfo,
    interner: &'a Interner,
    config: &'a Config,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(codebase: &'a CodebaseInfo, interner: &'a Interner, config: &'a Config) -> Self {
        Self {
            codebase,
            interner,
            config,
        }
    }

    /// Analyze a single file and return its (non line-suppressed) issues.
    pub fn analyze(&self, file_path: StrId) -> (Vec<Issue>, FileReferenceData) {
        let Some(file_info) = self.codebase.files.get(&file_path) else {
            return (Vec::new(), FileReferenceData::default());
        };

        let path_str = self.interner.lookup(file_path);

        // Create arena for parsing.
        let arena = Bump::new();
        let file_id = FileId::new(&*path_str);

        // Re-parse the file.
        let (program, _parse_error) = parse_file_content(&arena, file_id, &file_info.contents);

        // Resolve names (handle use statements, namespace aliases, etc.).
        let resolved_names = resolve_names(&program, self.interner);

        // Create the analyzer context.
        let stmt_analyzer = StatementsAnalyzer::new(
            self.codebase,
            self.interner,
            file_path,
            &file_info.contents,
            &resolved_names,
            self.config,
        )
        .with_arena(&arena);

        // Create analysis data and context.
        let mut analysis_data = FunctionAnalysisData::new();
        if self.config.taint_analysis {
            // Taint mode builds a whole-program graph (Psalm's
            // trackTaintedInputs / Hakana's WholeProgram(Taint)).
            analysis_data.data_flow_graph = pzoom_code_info::data_flow::graph::DataFlowGraph::new(
                pzoom_code_info::GraphKind::WholeProgram(
                    pzoom_code_info::data_flow::graph::WholeProgramKind::Taint,
                ),
            );
        }
        let mut context = BlockContext::new();
        // File-scope code has no enclosing function or class, so attribute its
        // symbol references to the file itself (its path StrId), mirroring how
        // Hakana always has a referencing function-like.
        context.function_context.calling_functionlike_id =
            Some(crate::context::FunctionLikeId::Function(file_path));

        // Analyze the program's statements.
        let _ = stmt_analyzer::analyze_stmts(
            &stmt_analyzer,
            program.statements.as_slice(),
            &mut analysis_data,
            &mut context,
        );

        // Hakana's end-of-functionlike type-variable pass for the pseudo-main:
        // Hack has no global code, but PHP's file scope mints and constrains
        // type variables like any function body, so reconcile the leftovers
        // (each top-level function/method already reconciled and cleared its
        // own) at the end of the file.
        crate::expr::call_analyzer::check_type_variable_bounds_at_function_end(
            &stmt_analyzer,
            &mut analysis_data,
            pzoom_code_info::CodeLocation::new(file_path, 0, 1, 1, 1),
        );

        // Parser diagnostics are mostly suppressed (mago recovers from
        // constructs it mis-flags), but first-class-callable syntax in a
        // `new` expression is a real PHP compile error ("Cannot create
        // Closure for new expression") — surface those.
        for (offset, _message) in &file_info.parse_errors {
            let start = (*offset as usize).saturating_sub(60);
            let end = ((*offset as usize) + 10).min(file_info.contents.len());
            let window = file_info.contents.get(start..end).unwrap_or("");
            if window.contains("(...)") && window.contains("new ") {
                let (line, col) = stmt_analyzer.get_line_column(*offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ParseError,
                    "Cannot create Closure for new expression",
                    file_path,
                    *offset,
                    offset.saturating_add(1),
                    line,
                    col,
                ));
            }
        }

        // Scan-time docblock problems (e.g. a malformed `@psalm-type`
        // definition) surface as InvalidDocblock.
        // `@psalm-import-type` source validation (Psalm's ClassLikeNodeScanner):
        // the source class must exist and define the imported alias.
        for (source_class, imported_alias) in &file_info.type_alias_imports {
            let scoped_alias = self.interner.intern(&format!(
                "{}::{}",
                self.interner.lookup(*source_class),
                imported_alias
            ));
            if self.codebase.type_aliases.contains_key(&scoped_alias) {
                continue;
            }
            let source_exists = self.codebase.get_class(*source_class).is_some()
                || self
                    .codebase
                    .classlike_name_lookup
                    .contains_key(&self.interner.lookup(*source_class).to_ascii_lowercase());
            let (kind, message) = if source_exists {
                (
                    IssueKind::InvalidTypeImport,
                    format!(
                        "Invalid type import: {} does not define the type {}",
                        self.interner.lookup(*source_class),
                        imported_alias
                    ),
                )
            } else {
                (
                    IssueKind::UndefinedDocblockClass,
                    format!(
                        "Docblock class {} does not exist",
                        self.interner.lookup(*source_class)
                    ),
                )
            };
            analysis_data.add_issue(Issue::new(kind, message, file_path, 0, 1, 1, 1));
        }

        for (offset, message) in &file_info.docblock_parse_issues {
            let (line, col) = stmt_analyzer.get_line_column(*offset);
            let kind = if message.starts_with("Invalid type import") {
                IssueKind::InvalidTypeImport
            } else if message.starts_with("Docblock-defined class") {
                IssueKind::UndefinedDocblockClass
            } else {
                IssueKind::InvalidDocblock
            };
            analysis_data.add_issue(Issue::new(
                kind,
                message.clone(),
                file_path,
                *offset,
                offset.saturating_add(1),
                line,
                col,
            ));
        }

        // Report malformed `@psalm-check-type[-exact]` assertions (missing
        // variable or type). Well-formed assertions are evaluated against the
        // in-scope context during statement analysis; malformed ones need no
        // context and may not be attached to any statement, so sweep them here.
        for (offset, annotations) in &file_info.inline_annotations.check_type_annotations {
            for annotation in annotations {
                if annotation.var_id.is_some() && annotation.check_type.is_some() {
                    continue;
                }

                let (line, col) = stmt_analyzer.get_line_column(*offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidDocblock,
                    format!(
                        "Invalid format for @psalm-check-type{}",
                        if annotation.is_exact { "-exact" } else { "" }
                    ),
                    file_path,
                    *offset,
                    *offset,
                    line,
                    col,
                ));
            }
        }

        // Psalm's limitMethodComplexity (StatementsAnalyzer::checkUnreferencedVars):
        // ComplexMethod/ComplexFunction for function-likes whose variable-use
        // data-flow graph exceeds the size/path-length limits.
        if self.config.limit_method_complexity {
            report_complex_functions(program, &mut analysis_data, file_path, &stmt_analyzer);
        }

        // Unused-variable analysis (Psalm's checkUnreferencedVars /
        // checkParamReferences over its VariableUseGraph; pzoom uses Hakana's
        // walk over the function-body data flow graph). Runs before the
        // suppression filter so `@psalm-suppress UnusedVariable` applies.
        if self.config.report_unused {
            report_unused_variables(&mut analysis_data, file_path, &stmt_analyzer, self.interner);
        }
        if self.config.taint_analysis {
            // Psalm `getTaintFlowGraphWithSuppressed`: statements whose
            // docblock carries `@psalm-suppress TaintedInput` contribute no
            // taint paths at all (the taint may surface elsewhere, so the
            // positional issue filter below cannot catch these).
            let suppressed_taint_ranges: Vec<(u32, u32)> = analysis_data
                .stmt_suppression_ranges
                .iter()
                .filter_map(|&(docblock_start, docblock_end, stmt_start, stmt_end)| {
                    let docblock = file_info
                        .contents
                        .get(docblock_start as usize..docblock_end as usize)?;
                    crate::issue_suppression::docblock_suppression_match(docblock, "TaintedInput")
                        .map(|_| (stmt_start, stmt_end))
                })
                .collect();
            let taint_issues = crate::taint_analyzer::find_tainted_data(
                &analysis_data.data_flow_graph,
                self.interner,
                &suppressed_taint_ranges,
            );
            analysis_data.issues.extend(taint_issues);
        }
        // Unused class/method/function detection is codebase-wide (it needs
        // references from every file), so it runs in a second pass after this
        // one — see `unused_symbols::find_unused_definitions`. Hand off this
        // file's contribution to the merged symbol-reference graph.
        let reference_data = FileReferenceData {
            symbol_references: std::mem::take(&mut analysis_data.symbol_references),
            referenced_properties: std::mem::take(&mut analysis_data.referenced_properties),
            method_returns_used: std::mem::take(&mut analysis_data.method_returns_used),
            used_method_params: std::mem::take(&mut analysis_data.used_method_params),
            param_unused_candidates: std::mem::take(&mut analysis_data.param_unused_candidates),
        };

        // Psalm's findUnusedPsalmSuppress feature (always on in its test
        // harness): inline-suppression matches record the suppressing token's
        // source position (IssueBuffer::$used_suppressions); registered
        // suppressions that never matched report UnusedPsalmSuppress at the
        // end (IssueBuffer::processUnusedSuppressions).
        let mut used_suppressions: rustc_hash::FxHashSet<usize> = analysis_data
            .used_suppression_offsets
            .iter()
            .map(|offset| *offset as usize)
            .collect();

        // Line slices and line-start offsets are shared by every issue's
        // suppression check below (previously rebuilt per issue, per name).
        let suppression_lines: Vec<&str> = file_info.contents.lines().collect();
        let suppression_line_offsets = line_start_offsets(&file_info.contents);

        // Class-level docblock suppressions cover every issue inside the
        // class body (Psalm merges class suppressions into each member's
        // suppressed-issues list).
        let class_spans: Vec<(u32, u32)> = file_info
            .classes
            .iter()
            .filter_map(|class_id| self.codebase.get_class(*class_id))
            .map(|class_info| (class_info.start_offset, class_info.end_offset))
            .collect();

        // Function/method docblock suppressions likewise cover the whole body
        // (Psalm's getSuppressedIssues carries the functionlike's list).
        let mut function_spans: Vec<(u32, u32)> = file_info
            .functions
            .iter()
            .filter_map(|function_id| self.codebase.get_function(*function_id))
            .map(|function_info| (function_info.start_offset, function_info.end_offset))
            .collect();
        for class_id in &file_info.classes {
            if let Some(class_info) = self.codebase.get_class(*class_id) {
                function_spans.extend(class_info.methods.values().filter_map(|method_info| {
                    (method_info.file_path == file_path)
                        .then_some((method_info.start_offset, method_info.end_offset))
                }));
            }
        }

        let mut filtered: Vec<Issue> = Vec::new();
        for issue in analysis_data.issues {
            // Psalm checks config-level suppression BEFORE inline
            // suppressions, without marking any inline token used
            // (IssueBuffer::isSuppressed order).
            if self
                .config
                .is_issue_suppressed(&format!("{:?}", issue.kind))
            {
                filtered.push(issue);
                continue;
            }
            match line_suppression_match_for_issue(
                &suppression_lines,
                &suppression_line_offsets,
                &issue,
            )
            .or_else(|| {
                stmt_docblock_suppression_match_for_issue(
                    &file_info.contents,
                    &analysis_data.stmt_suppression_ranges,
                    &issue,
                )
            })
            .or_else(|| {
                class_docblock_suppression_match_for_issue(
                    &file_info.contents,
                    &function_spans,
                    &issue,
                )
            })
            .or_else(|| {
                class_docblock_suppression_match_for_issue(
                    &file_info.contents,
                    &class_spans,
                    &issue,
                )
            }) {
                Some(token_offset) => {
                    used_suppressions.insert(token_offset);
                }
                None => filtered.push(issue),
            }
        }

        if self.config.find_unused_suppress {
            // Psalm only registers function/method-level (FunctionLikeAnalyzer)
            // and statement-level (StatementsAnalyzer) suppressions as
            // unused-suppression candidates; it never registers a class-level
            // docblock suppression (ClassLikeStorage::$suppressed_issues). So an
            // unused `@psalm-suppress` in a classlike docblock is not reported.
            // Collect each classlike's preceding-docblock span to exclude those
            // candidates and match Psalm.
            let class_docblock_spans: Vec<(usize, usize)> = file_info
                .classes
                .iter()
                .filter_map(|class_id| self.codebase.get_class(*class_id))
                .filter(|class_info| class_info.file_path == file_path)
                .filter_map(|class_info| {
                    let class_start = class_info.start_offset as usize;
                    let prefix = file_info.contents.get(..class_start)?;
                    let (docblock_start, docblock) =
                        crate::issue_suppression::preceding_docblock(prefix)?;
                    Some((docblock_start, docblock_start + docblock.len()))
                })
                .collect();

            // Suppressions are collected from parsed comments, mirroring Psalm
            // and Hakana. A comment inside a string literal is not a comment, so
            // `@psalm-suppress` tokens embedded in PHP-code fixtures (heredocs,
            // quoted code in test providers) are ignored for free — no
            // string-span filtering required.
            let comment_spans: Vec<(usize, &str)> = program
                .trivia
                .comments()
                .map(|comment| (comment.span.start.offset as usize, comment.value))
                .collect();
            for candidate in collect_suppression_candidates(&comment_spans) {
                if used_suppressions.contains(&candidate.offset) {
                    continue;
                }
                // Class-level docblock suppression — exempt from the unused
                // pass, as in Psalm (see class_docblock_spans above).
                if class_docblock_spans
                    .iter()
                    .any(|&(start, end)| candidate.offset >= start && candidate.offset < end)
                {
                    continue;
                }
                // Issues that Psalm only checks under find_unused_variables
                // (pzoom's report_unused) are never emitted when that mode is
                // off, so a `@psalm-suppress` of one is not "unused" — Psalm's
                // findUnusedPsalmSuppress pass does not flag it. Skip the
                // candidate to match.
                if !self.config.report_unused && issue_gated_on_report_unused(&candidate.name) {
                    continue;
                }
                // Unused-definition issues are emitted by the codebase-wide pass
                // (after this one), which owns whether their `@psalm-suppress`
                // was used, so this per-file pass must not pre-judge them.
                if self.config.find_unused_code
                    && crate::unused_symbols::is_unused_definition_kind(&candidate.name)
                {
                    continue;
                }
                let (line, col) = stmt_analyzer.get_line_column(candidate.offset as u32);
                filtered.push(Issue::new(
                    IssueKind::UnusedPsalmSuppress,
                    // Psalm's message is the bare "This suppression is never
                    // used" and identifies the suppression only by the reported
                    // range. We name the suppressed issue too so a spurious
                    // UnusedPsalmSuppress (pzoom not emitting an issue Psalm
                    // does) can be grouped by which suppression went unused.
                    format!("Suppression of {} is never used", candidate.name),
                    file_path,
                    candidate.offset as u32,
                    (candidate.offset + candidate.name.len()) as u32,
                    line,
                    col,
                ));
            }
        }

        (filtered, reference_data)
    }
}

/// A file's contribution to the codebase-wide unused-definition analysis: its
/// symbol-reference graph plus the per-file sets the graph does not capture
/// (reads-only property accesses and used method return values).
#[derive(Default)]
pub struct FileReferenceData {
    pub symbol_references: pzoom_code_info::symbol_references::SymbolReferences,
    /// Reads-only property accesses and used method return values, which the
    /// symbol graph does not distinguish (it records writes too).
    pub referenced_properties: rustc_hash::FxHashSet<(StrId, StrId)>,
    pub method_returns_used: rustc_hash::FxHashSet<(StrId, StrId)>,
    /// `(class, lowercase method, offset)` param-use triples (Psalm's
    /// `method_param_uses`), already propagated up each override chain.
    pub used_method_params: rustc_hash::FxHashSet<(StrId, StrId, usize)>,
    /// Non-private method params unused in their own body, awaiting the
    /// codebase-wide verdict.
    pub param_unused_candidates: Vec<crate::function_analysis_data::ParamUnusedCandidate>,
}

/// Whether the graph holds an assignment source for `name` within the given
/// function span (used for the by-ref out-param rule).
fn body_writes_variable(
    graph: &pzoom_code_info::data_flow::graph::DataFlowGraph,
    interner: &Interner,
    name: &str,
    function_start: u32,
    function_end: u32,
) -> bool {
    use pzoom_code_info::data_flow::node::DataFlowNodeId;
    graph.sources.keys().any(|id| match id {
        DataFlowNodeId::Var(node_var, _, start, _) => {
            *start > function_start
                && *start < function_end
                && &*interner.lookup(node_var.0) == name
        }
        _ => false,
    })
}

/// Psalm's `$_`-and-`$unused*` rule for parameters
/// (FunctionLikeAnalyzer::isIgnoredForUnusedParam).
fn is_ignored_for_unused_param(var_name: &str) -> bool {
    var_name.starts_with("$_") || (var_name.starts_with("$unused") && var_name != "$unused")
}

/// Psalm's limitMethodComplexity (StatementsAnalyzer::checkUnreferencedVars over
/// its VariableUseGraph): emit ComplexMethod / ComplexFunction for a method /
/// top-level function whose data-flow graph is too large *and* too spread out.
///
/// Psalm flags when `edge_count > max_graph_size` AND
/// `mean_path_length > max_avg_path_length` AND
/// `edge_count / unique_destinations > 1.1`, where each edge's length is the
/// line distance between its endpoints. pzoom's variable-use graph (ported from
/// Hakana) is materially denser than Psalm's, so matching Psalm's 200 size
/// limit flags ~4x too many methods; the size limit is calibrated to pzoom's
/// graph (800) while the path-length (70) and branch ratio (1.1) match Psalm.
///
/// Each closure/arrow gets its own graph in Psalm, so an edge is attributed to
/// the innermost function-like containing both endpoints; closures/arrows never
/// report themselves.
fn report_complex_functions(
    program: &mago_syntax::ast::Program<'_>,
    analysis_data: &mut FunctionAnalysisData,
    file_path: StrId,
    stmt_analyzer: &StatementsAnalyzer<'_>,
) {
    use mago_span::HasSpan as _;
    use pzoom_code_info::data_flow::node::DataFlowNodeId;
    type N<'a, 'b> = mago_syntax::ast::node::Node<'a, 'b>;

    // Calibrated to pzoom's (denser) graph; see the doc comment above.
    const MAX_GRAPH_SIZE: usize = 800;
    const MAX_AVG_PATH_LENGTH: f64 = 70.0;
    const MIN_BRANCH_RATIO: f64 = 1.1;

    // (span_start, span_end, report): report = Some((is_method, name_offset))
    // for a method/top-level function; None for closures/arrows (they bound
    // attribution but never report).
    let func_spans = N::Program(program).filter_map(|node| {
        let report = match node {
            N::Method(m) => Some((true, (m.name.span().start.offset, m.name.span().end.offset))),
            N::Function(f) => Some((false, (f.name.span().start.offset, f.name.span().end.offset))),
            N::Closure(_) | N::ArrowFunction(_) => None,
            _ => return None,
        };
        let s = node.span();
        Some((s.start.offset, s.end.offset, report))
    });
    if func_spans.is_empty() {
        return;
    }
    let innermost = |off: u32| -> Option<usize> {
        func_spans
            .iter()
            .enumerate()
            .filter(|(_, (s, e, _))| off >= *s && off < *e)
            .min_by_key(|(_, (s, e, _))| e - s)
            .map(|(i, _)| i)
    };

    // (is_method, name_offset, count, round(mean)) for each function-like over
    // the limits. Computed under an immutable borrow of the graph, then emitted.
    let emits = {
        let g = &analysis_data.data_flow_graph;
        let node_pos = |id: &DataFlowNodeId| {
            g.vertices
                .get(id)
                .or_else(|| g.sources.get(id))
                .or_else(|| g.sinks.get(id))
                .and_then(|n| n.get_pos())
        };
        let mut owner_len: rustc_hash::FxHashMap<usize, u64> = rustc_hash::FxHashMap::default();
        let mut owner_dest: rustc_hash::FxHashMap<
            usize,
            rustc_hash::FxHashMap<&DataFlowNodeId, usize>,
        > = rustc_hash::FxHashMap::default();
        for (from, dests) in &g.forward_edges {
            let Some(fp) = node_pos(from) else { continue };
            let Some(of) = innermost(fp.start_offset) else {
                continue;
            };
            for (to, _) in dests {
                let Some(tp) = node_pos(to) else { continue };
                if tp.file_path != fp.file_path || innermost(tp.start_offset) != Some(of) {
                    continue;
                }
                let length = (fp.start_line as i64 - tp.start_line as i64).unsigned_abs();
                if length == 0 {
                    continue;
                }
                *owner_len.entry(of).or_insert(0) += length;
                *owner_dest.entry(of).or_default().entry(to).or_insert(0) += 1;
            }
        }
        let mut emits = Vec::new();
        for (owner, dests) in &owner_dest {
            let Some((is_method, (name_start, name_end))) = func_spans[*owner].2 else {
                continue;
            };
            let count: usize = dests.values().copied().sum();
            if count == 0 {
                continue;
            }
            let mean = owner_len[owner] as f64 / count as f64;
            let branch_ratio = count as f64 / dests.len().max(1) as f64;
            if count > MAX_GRAPH_SIZE
                && mean > MAX_AVG_PATH_LENGTH
                && branch_ratio > MIN_BRANCH_RATIO
            {
                emits.push((is_method, (name_start, name_end), count, mean.round() as i64));
            }
        }
        emits
    };

    for (is_method, name_offset, count, mean_round) in emits {
        let (start_line, start_col) = stmt_analyzer.get_line_column(name_offset.0);
        let (kind, noun) = if is_method {
            (IssueKind::ComplexMethod, "method")
        } else {
            (IssueKind::ComplexFunction, "function")
        };
        analysis_data.add_issue(Issue::new(
            kind,
            format!(
                "This {noun}\u{2019}s complexity is greater than the project limit \
                 (method graph size = {count}, average path length = {mean_round})"
            ),
            file_path,
            name_offset.0,
            name_offset.1,
            start_line,
            start_col,
        ));
    }
}

/// Report UnusedVariable / UnusedForeachValue / UnusedParam /
/// UnusedClosureParam from the function-body data flow graph.
///
/// Variables: Psalm's `StatementsAnalyzer::checkUnreferencedVars` — every
/// assignment registers a source; one whose forward closure reaches no use
/// sink reports UnusedVariable ("$x is never referenced or the value is not
/// used"), or UnusedForeachValue when the assignment is a foreach value
/// target. Parameters: Psalm's `FunctionLikeAnalyzer::checkParamReferences` —
/// grouped per function-like, only the trailing run of unused parameters
/// reports (an unused param before a used one is required positionally), and
/// only for plain functions, closures and private methods.
pub(crate) fn report_unused_variables(
    analysis_data: &mut crate::function_analysis_data::FunctionAnalysisData,
    file_path: StrId,
    stmt_analyzer: &StatementsAnalyzer<'_>,
    interner: &Interner,
) {
    use pzoom_code_info::VariableSourceKind;
    use pzoom_code_info::data_flow::node::{DataFlowNodeId, DataFlowNodeKind};

    let (unused_nodes, unused_but_referenced_nodes) =
        crate::unused_variable_analyzer::check_variables_used(&analysis_data.data_flow_graph);

    let mut new_issues: Vec<Issue> = Vec::new();
    let mut unused_ids: rustc_hash::FxHashSet<DataFlowNodeId> = rustc_hash::FxHashSet::default();
    for node in unused_nodes
        .iter()
        .chain(unused_but_referenced_nodes.iter())
    {
        unused_ids.insert(node.id.clone());

        let DataFlowNodeKind::VariableUseSource { kind, pos, .. } = &node.kind else {
            continue;
        };

        if !matches!(kind, VariableSourceKind::Default) {
            // Parameters are handled with Psalm's trailing rule below.
            continue;
        }

        let label = match &node.id {
            DataFlowNodeId::Var(var_id, ..) | DataFlowNodeId::Param(var_id, ..) => {
                interner.lookup(var_id.0).to_string()
            }
            other => other.to_label(interner),
        };

        // Psalm skips `$_`-prefixed variables entirely.
        if label.starts_with("$_") {
            continue;
        }

        let is_foreach_value = analysis_data
            .foreach_var_positions
            .iter()
            .any(|(start, _)| *start == pos.start_offset);

        let (line, col) = stmt_analyzer.get_line_column(pos.start_offset);
        new_issues.push(Issue::new(
            if is_foreach_value {
                IssueKind::UnusedForeachValue
            } else {
                IssueKind::UnusedVariable
            },
            format!("{} is never referenced or the value is not used", label),
            file_path,
            pos.start_offset,
            pos.end_offset,
            line,
            col,
        ));
    }

    // Parameters, grouped per enclosing function-like.
    let mut param_groups: std::collections::BTreeMap<
        u32,
        Vec<&crate::function_analysis_data::ParamSourceInfo>,
    > = std::collections::BTreeMap::new();
    for param_source in &analysis_data.param_sources {
        param_groups
            .entry(param_source.function_key)
            .or_default()
            .push(param_source);
    }

    // Codebase-wide param-use bookkeeping (Psalm's method_param_uses) and the
    // deferred unused-param candidates, drained into `analysis_data` once the
    // immutable borrow held by `param_groups` ends.
    let mut local_used_method_params: Vec<(StrId, StrId, usize)> = Vec::new();
    let mut local_param_candidates: Vec<crate::function_analysis_data::ParamUnusedCandidate> =
        Vec::new();

    for (_, mut params) in param_groups {
        params.sort_by_key(|param| param.param_index);

        if params.iter().any(|param| !param.reportable) {
            // Non-private method params: Psalm's checkMethodParamReferences
            // (find_unused_code) reports each unused param as
            // PossiblyUnusedParam (UnusedParam when the method or class is
            // final), skipping interfaces, overriding methods and promoted
            // properties. No trailing-position rule applies.
            //
            // Unlike Psalm's single-file passes this verdict is codebase-wide:
            // an override that uses the param marks the parent's param used
            // (Psalm's addMethodParamUse parent-propagation), so the actual
            // report is deferred to `find_unused_definitions`. Here we only
            // record which params are used (with propagation) and stash the
            // candidates.
            if stmt_analyzer.config.find_unused_code {
                for param in &params {
                    let Some((method_final, in_interface, _has_overrides)) =
                        param.method_param_meta
                    else {
                        continue;
                    };
                    let (Some(class_id), Some(method_name_id)) =
                        (param.method_class_id, param.method_name_id)
                    else {
                        continue;
                    };
                    if in_interface {
                        continue;
                    }
                    let method_lc_name = interner.lookup(method_name_id).to_lowercase();
                    let method_lc = interner.intern(&method_lc_name);

                    // A `$_`-prefixed param is treated as intentionally unused
                    // (Psalm's isIgnoredForUnusedParam path marks it used), and
                    // a param referenced in this body is used. Either way record
                    // the use and propagate it up the override chain so the
                    // ancestor declarations stay "used" too.
                    let is_used = is_ignored_for_unused_param(&param.name)
                        || !unused_ids.contains(&param.node_id);
                    if is_used {
                        local_used_method_params.push((class_id, method_lc, param.param_index));
                        if let Some(class_info) = stmt_analyzer.codebase.get_class(class_id)
                            && let Some(parents) =
                                class_info.overridden_method_ids.get(&method_name_id)
                        {
                            for parent_id in parents {
                                local_used_method_params.push((
                                    *parent_id,
                                    method_lc,
                                    param.param_index,
                                ));
                            }
                        }
                        continue;
                    }

                    // Locally unused. Psalm skips params of a method that
                    // overrides an ancestor (`empty(overridden_method_ids)`) and
                    // promoted-constructor params. The remaining ones become
                    // candidates, resolved once every override has been seen.
                    //
                    // A trait method's own `overridden_method_ids` reflect the
                    // trait, not the using class (where the method may well
                    // override a parent). Psalm checks each param in the context
                    // of the using class, so candidates for trait methods are
                    // produced there (class_analyzer::analyze_methods_from_trait),
                    // not here against the bare trait.
                    let class_is_trait = stmt_analyzer.codebase.get_class(class_id).is_some_and(
                        |class_info| {
                            class_info.kind
                                == pzoom_code_info::class_like_info::ClassLikeKind::Trait
                        },
                    );
                    if _has_overrides || param.is_promoted || class_is_trait {
                        continue;
                    }
                    let (line, col) = stmt_analyzer.get_line_column(param.span.0);
                    local_param_candidates.push(
                        crate::function_analysis_data::ParamUnusedCandidate {
                            file_path,
                            class_id,
                            method_lc,
                            offset: param.param_index,
                            is_final: method_final,
                            span: (param.span.0, param.span.1),
                            line,
                            col,
                        },
                    );
                }
            }
            continue;
        }

        // func_get_args() reads every param.
        if params.first().is_some_and(|param| {
            analysis_data
                .func_get_args_functions
                .contains(&param.function_key)
        }) {
            continue;
        }

        // Psalm's detectPreviousUnusedArgumentPosition: the next non-ignored
        // param position at or below `position`.
        let previous_position = |position: isize| -> usize {
            params
                .iter()
                .rev()
                .find(|param| {
                    (param.param_index as isize) <= position
                        && !is_ignored_for_unused_param(&param.name)
                })
                .map(|param| param.param_index)
                .unwrap_or(0)
        };

        let mut unused_positions: Vec<&crate::function_analysis_data::ParamSourceInfo> = params
            .iter()
            .filter(|param| {
                unused_ids.contains(&param.node_id)
                    && !param.is_promoted
                    && !is_ignored_for_unused_param(&param.name)
                    // A by-ref param the body writes is an out-param, not
                    // unused (Psalm reports `&$arg` only when never touched).
                    && !(param.by_ref
                        && body_writes_variable(
                            &analysis_data.data_flow_graph,
                            interner,
                            &param.name,
                            param.function_key,
                            param.function_end,
                        ))
            })
            .copied()
            .collect();
        unused_positions.sort_by_key(|param| std::cmp::Reverse(param.param_index));

        let mut last_unused_argument_position = previous_position(params.len() as isize - 1);

        for param in unused_positions {
            // Do not report unused required parameters (ones followed by a
            // used parameter).
            if param.param_index != last_unused_argument_position {
                break;
            }
            last_unused_argument_position = previous_position(param.param_index as isize - 1);

            let (line, col) = stmt_analyzer.get_line_column(param.span.0);
            new_issues.push(Issue::new(
                if param.is_closure {
                    IssueKind::UnusedClosureParam
                } else {
                    IssueKind::UnusedParam
                },
                format!(
                    "Param {} is never referenced in this method",
                    param.name.trim_start_matches('$')
                ),
                file_path,
                param.span.0,
                param.span.1,
                line,
                col,
            ));
        }
    }

    // Stable order for deterministic output.
    new_issues.sort_by_key(|issue| issue.location.start_offset);
    analysis_data.issues.extend(new_issues);

    analysis_data
        .used_method_params
        .extend(local_used_method_params);
    analysis_data
        .param_unused_candidates
        .extend(local_param_candidates);
}

/// Byte offset of the suppression token that suppresses `issue`, if any.
/// Match an issue against statement-level `@psalm-suppress` docblocks whose
/// statement span contains the issue (Psalm activates a statement docblock's
/// suppressions for the whole statement analysis, nested statements included).
/// Returns the matching token's byte offset.
fn stmt_docblock_suppression_match_for_issue(
    contents: &str,
    stmt_suppression_ranges: &[(u32, u32, u32, u32)],
    issue: &Issue,
) -> Option<usize> {
    let issue_names = suppression_candidate_names(issue);
    let issue_offset = issue.location.start_offset;

    for &(docblock_start, docblock_end, stmt_start, stmt_end) in stmt_suppression_ranges {
        // Issues that point into the suppressing docblock itself (e.g. an
        // InvalidReturnType at the @return annotation's location) count as
        // within the statement's reach, like Psalm's storage-level
        // suppressed_issues.
        let _ = stmt_start;
        if issue_offset < docblock_start || issue_offset > stmt_end {
            continue;
        }
        let docblock = contents.get(docblock_start as usize..docblock_end as usize)?;
        for issue_name in &issue_names {
            if let Some(token_offset) =
                crate::issue_suppression::docblock_suppression_match(docblock, issue_name)
            {
                return Some(docblock_start as usize + token_offset);
            }
        }
    }

    None
}

/// A class-level docblock `@psalm-suppress` covering the issue's position
/// (Psalm propagates class suppressions to every member analysis).
pub(crate) fn class_docblock_suppression_match_for_issue(
    contents: &str,
    class_spans: &[(u32, u32)],
    issue: &Issue,
) -> Option<usize> {
    let issue_names = suppression_candidate_names(issue);
    let issue_offset = issue.location.start_offset;

    for &(class_start, class_end) in class_spans {
        if issue_offset < class_start || issue_offset > class_end {
            continue;
        }
        let Some(prefix) = contents.get(..class_start as usize) else {
            continue;
        };
        let Some((docblock_start, docblock)) = crate::issue_suppression::preceding_docblock(prefix)
        else {
            continue;
        };
        for issue_name in &issue_names {
            if let Some(token_offset) =
                crate::issue_suppression::docblock_suppression_match(docblock, issue_name)
            {
                return Some(docblock_start + token_offset);
            }
        }
    }

    None
}

/// Issues Psalm only checks when `find_unused_variables` is enabled (pzoom's
/// `report_unused`). With that mode off they are never emitted, so an inline
/// `@psalm-suppress` of one is exempt from the unused-suppression pass.
fn issue_gated_on_report_unused(issue_name: &str) -> bool {
    matches!(
        issue_name,
        "UnusedVariable"
            | "UnusedForeachValue"
            | "UnnecessaryVarAnnotation"
            | "UnevaluatedCode"
            | "UnusedParam"
            | "UnusedClosureParam"
    )
}

/// Psalm's `Config::getParentIssueType`: suppressing the parent kind also
/// suppresses its derived variant. The reverse never holds — suppressing the
/// child does not suppress the parent (see `suppresses_issue`). This is a
/// single level: an issue has at most one parent.
fn parent_issue_name(issue_name: &str) -> Option<String> {
    match issue_name {
        "PossiblyUndefinedIntArrayOffset" | "PossiblyUndefinedStringArrayOffset" => {
            return Some("PossiblyUndefinedArrayOffset".to_string());
        }
        "PossiblyNullReference" => return Some("NullReference".to_string()),
        "PossiblyFalseReference" | "PossiblyUndefinedArrayOffset" => return None,
        _ => {}
    }

    // `Possibly(False|Null)?Foo` → `Foo`, prefixed with `Invalid` unless it
    // already names an Invalid*/Un* issue (Psalm's preg_replace branch).
    if let Some(stripped) = issue_name.strip_prefix("Possibly") {
        let stripped = stripped
            .strip_prefix("False")
            .or_else(|| stripped.strip_prefix("Null"))
            .unwrap_or(stripped);
        let parent = if !stripped.contains("Invalid") && !stripped.starts_with("Un") {
            format!("Invalid{stripped}")
        } else {
            stripped.to_string()
        };
        return Some(parent);
    }

    // Every Tainted* issue extends TaintedInput.
    if issue_name.starts_with("Tainted") && issue_name != "TaintedInput" {
        return Some("TaintedInput".to_string());
    }

    let direct = match issue_name {
        "UndefinedInterfaceMethod" => "UndefinedMethod",
        "UndefinedMagicPropertyFetch" => "UndefinedPropertyFetch",
        "UndefinedMagicPropertyAssignment" => "UndefinedPropertyAssignment",
        "UndefinedMagicMethod" => "UndefinedMethod",
        "PossibleRawObjectIteration" => "RawObjectIteration",
        "UninitializedProperty" => "PropertyNotSetInConstructor",
        "InvalidDocblockParamName" => "InvalidDocblock",
        "UnusedClosureParam" => "UnusedParam",
        "UnusedConstructor" => "UnusedMethod",
        "StringIncrement" => "InvalidOperand",
        "InvalidLiteralArgument" => "InvalidArgument",
        "RedundantConditionGivenDocblockType" => "RedundantCondition",
        "RedundantFunctionCallGivenDocblockType" => "RedundantFunctionCall",
        "RedundantCastGivenDocblockType" => "RedundantCast",
        "TraitMethodSignatureMismatch" => "MethodSignatureMismatch",
        "ImplementedParamTypeMismatch" => "MoreSpecificImplementedParamType",
        "UndefinedDocblockClass" => "UndefinedClass",
        "UnusedForeachValue" => "UnusedVariable",
        _ => return None,
    };
    Some(direct.to_string())
}

/// The `@psalm-suppress` names that suppress `issue`: the issue's own kind plus
/// its Psalm parent kind (`getParentIssueType`), to be tried in turn.
fn suppression_candidate_names(issue: &Issue) -> Vec<String> {
    let issue_name = format!("{:?}", issue.kind);
    let parent = parent_issue_name(&issue_name);
    let mut names = vec![issue_name];
    if let Some(parent) = parent {
        names.push(parent);
    }
    names
}

pub(crate) fn line_suppression_match_for_issue(
    lines: &[&str],
    line_offsets: &[usize],
    issue: &Issue,
) -> Option<usize> {
    suppression_candidate_names(issue)
        .into_iter()
        .find_map(|name| line_suppression_match_for_issue_named(lines, line_offsets, issue, &name))
}

fn line_suppression_match_for_issue_named(
    lines: &[&str],
    line_offsets: &[usize],
    issue: &Issue,
    issue_name: &str,
) -> Option<usize> {
    let issue_line = issue.location.start_line as usize;

    if issue_line == 0 || issue_line > lines.len() + 1 {
        return None;
    }

    let mut line_no = issue_line;

    while line_no > 0 && line_no <= lines.len() {
        let line = lines[line_no - 1];
        if let Some(col) = line_suppression_match(line, &issue_name) {
            return Some(line_offsets[line_no - 1] + col);
        }

        if line_no == issue_line {
            line_no -= 1;
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            line_no -= 1;
            continue;
        }

        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/");

        if !is_comment {
            break;
        }

        line_no -= 1;
    }

    function_docblock_suppression_match(lines, line_offsets, issue_line, issue_name)
}

/// Byte offset of the start of each line in `contents`.
fn line_start_offsets(contents: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (index, byte) in contents.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(index + 1);
        }
    }
    offsets
}

/// Column of the token within `line` that suppresses `issue_name`, if any.
fn line_suppression_match(line: &str, issue_name: &str) -> Option<usize> {
    let content_start = crate::issue_suppression::suppression_tag_content_start(line)?;

    suppression_tokens(line, content_start)
        .into_iter()
        .find(|(_, token)| token.eq_ignore_ascii_case("all") || suppresses_issue(token, issue_name))
        .map(|(col, _)| col)
}

/// `(byte column, token)` pairs following a `@psalm-suppress` tag at
/// `content_start` within `line`.
/// Every suppression issue token in `line` after the tag, as `(column, token)`
/// pairs. Mirrors Psalm's `DocComment::parseSuppressList`: the issue list is a
/// **comma-separated** run of `[A-Za-z0-9_-]+` names; the first token that is
/// only whitespace-separated (not comma-separated) ends the list and starts the
/// free-text description (e.g. `@psalm-suppress Foo Psalm now knows ...`).
fn suppression_tokens(line: &str, content_start: usize) -> Vec<(usize, &str)> {
    let content = &line[content_start..];
    let bytes = content.as_bytes();
    let is_issue_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';

    let mut tokens = Vec::new();
    let mut index = 0usize;
    loop {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && is_issue_char(bytes[index]) {
            index += 1;
        }
        if index == start {
            break;
        }
        tokens.push((content_start + start, &content[start..index]));

        // Continue only across a comma — a bare whitespace gap is description.
        let mut after = index;
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        if after < bytes.len() && bytes[after] == b',' {
            index = after + 1;
        } else {
            break;
        }
    }

    tokens
}

fn function_docblock_suppression_match(
    lines: &[&str],
    line_offsets: &[usize],
    issue_line: usize,
    issue_name: &str,
) -> Option<usize> {
    if issue_line == 0 || issue_line > lines.len() {
        return None;
    }

    let mut function_line = None;
    for line_no in (1..=issue_line).rev() {
        let line = lines[line_no - 1].trim_start();
        if line.contains("function ") || line.contains(" fn ") {
            function_line = Some(line_no);
            break;
        }
    }

    let function_line = function_line?;

    if !line_is_within_function_scope(lines, function_line, issue_line) {
        return None;
    }

    let mut line_no = function_line.saturating_sub(1);
    while line_no > 0 {
        let line = lines[line_no - 1];
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            line_no -= 1;
            continue;
        }

        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/");

        if !is_comment {
            break;
        }

        if let Some(col) = line_suppression_match(line, issue_name) {
            return Some(line_offsets[line_no - 1] + col);
        }

        line_no -= 1;
    }

    None
}

struct SuppressionCandidate {
    offset: usize,
    name: String,
}

/// All `@psalm-suppress` tokens eligible for unused-suppression reporting,
/// mirroring Psalm's registration rules (FunctionLikeAnalyzer /
/// StatementsAnalyzer): `Tainted*` suppressions are never tracked, and a
/// suppression group containing `UnusedPsalmSuppress` alongside other entries
/// registers nothing (the group's unusedness reports would themselves be
/// suppressed) — `UnusedPsalmSuppress` by itself IS tracked. `InaccessibleMethod`
/// is skipped as in Psalm's statement-level registration.
///
/// Input is the parsed comments as `(base_offset, comment_text)` pairs (Psalm
/// and Hakana both read suppressions from parser comments, never raw text).
/// Each comment is one suppression group — a docblock's `@psalm-suppress` list —
/// and `comment_text` is the exact source slice starting at `base_offset`, so a
/// token's absolute offset is `base_offset + its byte index within the text`.
fn collect_suppression_candidates(comments: &[(usize, &str)]) -> Vec<SuppressionCandidate> {
    let mut candidates = Vec::new();

    for &(base_offset, text) in comments {
        let mut group: Vec<SuppressionCandidate> = Vec::new();
        let mut group_has_unused_suppress = false;

        // A comment may carry several `@psalm-suppress` lines; scan each,
        // tracking the byte offset of the line within the comment text.
        let mut line_offset = 0usize;
        for line in text.split('\n') {
            if let Some(content_start) =
                crate::issue_suppression::suppression_tag_content_start(line)
            {
                for (col, token) in suppression_tokens(line, content_start) {
                    // Tainted* (never tracked) and InaccessibleMethod (skipped
                    // in Psalm's statement-level registration) are not candidates.
                    if token.starts_with("Tainted") || token == "InaccessibleMethod" {
                        continue;
                    }
                    if token == "UnusedPsalmSuppress" {
                        group_has_unused_suppress = true;
                    }
                    group.push(SuppressionCandidate {
                        offset: base_offset + line_offset + col,
                        name: token.to_string(),
                    });
                }
            }
            line_offset += line.len() + 1; // +1 for the consumed '\n'
        }

        if group.len() == 1 || !group_has_unused_suppress {
            candidates.append(&mut group);
        }
    }

    candidates
}

fn line_is_within_function_scope(lines: &[&str], function_line: usize, issue_line: usize) -> bool {
    if issue_line < function_line {
        return false;
    }

    let mut depth: isize = 0;
    for line in &lines[(function_line - 1)..issue_line] {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }

    depth > 0
}

fn suppresses_issue(token: &str, issue_name: &str) -> bool {
    if token == issue_name {
        return true;
    }

    match token {
        "MixedArgument" => {
            return matches!(issue_name, "MixedArgument" | "MixedArgumentTypeCoercion");
        }
        // Psalm's issue class is UnusedParam; the pzoom kind follows Hakana.
        "UnusedParam" => {
            return issue_name == "UnusedParam";
        }
        "MixedReturnStatement" | "MixedInferredReturnType" => {
            return issue_name == "MixedReturnStatement";
        }
        // A `*GivenDocblockType` token suppresses only its own issue, never
        // the base kind. The parent direction (a `RedundantCast` suppress also
        // covering `RedundantCastGivenDocblockType`) is Psalm's
        // getParentIssueType and lives in `parent_issue_name`; without these
        // arms the generic `strip_suffix` rule below would wrongly let the
        // child token suppress the parent issue.
        "RedundantCastGivenDocblockType" => {
            return issue_name == "RedundantCastGivenDocblockType";
        }
        "RedundantConditionGivenDocblockType" => {
            return issue_name == "RedundantConditionGivenDocblockType";
        }
        _ => {}
    }

    if let Some(base) = token.strip_suffix("GivenDocblockType") {
        return issue_name == base;
    }

    false
}
