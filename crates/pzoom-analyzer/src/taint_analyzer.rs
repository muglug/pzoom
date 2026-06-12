//! Whole-program taint analysis: BFS from taint sources to sinks over the
//! merged data-flow graph. Port of Hakana's
//! `analyzer/dataflow/program_analyzer.rs` (`find_tainted_data`), with Psalm's
//! taint taxonomy and issue kinds (the test corpus is Psalm's TaintTest).

use std::rc::Rc;

use rustc_hash::FxHashSet;

use pzoom_code_info::data_flow::graph::DataFlowGraph;
use pzoom_code_info::data_flow::node::{DataFlowNodeKind, SinkType};
use pzoom_code_info::data_flow::path::{ArrayDataKind, PathKind};
use pzoom_code_info::data_flow::tainted_node::TaintedNode;
use pzoom_code_info::issue::Issue;
use pzoom_str::Interner;

use crate::unused_variable_analyzer::{should_ignore_array_fetch, should_ignore_property_fetch};

/// Psalm's default taint max depth (`ProjectAnalyzer::trackTaintedInputs`
/// walks until exhaustion; Hakana caps at a configured depth).
const MAX_DEPTH: usize = 40;

pub(crate) fn find_tainted_data(
    graph: &DataFlowGraph,
    interner: &Interner,
    suppressed_ranges: &[(u32, u32)],
) -> Vec<Issue> {
    let mut new_issues = vec![];

    if std::env::var("PZOOM_TAINT_DEBUG").is_ok() {
        eprintln!("=== taint graph ===");
        for (id, node) in &graph.sources {
            eprintln!("SOURCE {:?} kind={:?}", id.to_label(interner), node.kind);
        }
        for (id, node) in &graph.sinks {
            eprintln!("SINK {:?} kind={:?}", id.to_label(interner), node.kind);
        }
        let mut edges: Vec<String> = graph
            .forward_edges
            .iter()
            .flat_map(|(from, tos)| {
                tos.iter().map(move |(to, path)| {
                    format!(
                        "EDGE {} -> {} [{:?}]",
                        from.to_label(interner),
                        to.to_label(interner),
                        path.kind
                    )
                })
            })
            .collect();
        edges.sort();
        for edge in edges {
            eprintln!("{}", edge);
        }
    }

    let sources = graph
        .sources
        .values()
        .filter(|node| matches!(node.kind, DataFlowNodeKind::TaintSource { .. }))
        .map(|node| Rc::new(TaintedNode::from(node)))
        .collect::<Vec<_>>();

    find_paths_to_sinks(sources, graph, &mut new_issues, interner, suppressed_ranges);

    new_issues
}

fn find_paths_to_sinks(
    mut sources: Vec<Rc<TaintedNode>>,
    graph: &DataFlowGraph,
    new_issues: &mut Vec<Issue>,
    interner: &Interner,
    suppressed_ranges: &[(u32, u32)],
) {
    let mut seen_sources = FxHashSet::default();

    for source in &sources {
        seen_sources.insert(source.get_unique_source_id(interner));
    }

    if graph.sinks.is_empty() {
        return;
    }

    for i in 0..MAX_DEPTH {
        if sources.is_empty() {
            break;
        }

        let mut new_sources = Vec::new();

        for source in sources {
            let source_taints = source.taint_sinks.clone();

            for generated_source in get_specialized_sources(graph, source) {
                new_sources.extend(get_child_nodes(
                    graph,
                    &generated_source,
                    &source_taints,
                    &mut seen_sources,
                    new_issues,
                    i == MAX_DEPTH - 1,
                    interner,
                    suppressed_ranges,
                ));
            }
        }

        sources = new_sources;
    }
}

/// Hakana `get_specialized_sources`: a specialized node also flows through its
/// unspecialized id (entering the callee), and an unspecialized node fans out
/// to its recorded specializations (returning to call sites).
fn get_specialized_sources(graph: &DataFlowGraph, source: Rc<TaintedNode>) -> Vec<Rc<TaintedNode>> {
    let mut generated_sources = vec![];

    if graph.forward_edges.contains_key(&source.id) {
        generated_sources.push(source.clone());
    }

    if source.is_specialized {
        let (unspecialized_id, specialization_key) = source.id.unspecialize();
        if graph.forward_edges.contains_key(&unspecialized_id) {
            let mut new_source = (*source).clone();

            new_source.id = unspecialized_id;
            new_source.is_specialized = false;

            new_source
                .specialized_calls
                .entry(specialization_key)
                .or_default()
                .insert(new_source.id.clone());

            generated_sources.push(Rc::new(new_source));
        }
    } else if let Some(specializations) = graph.specializations.get(&source.id) {
        for specialization in specializations {
            if source.specialized_calls.is_empty()
                || source.specialized_calls.contains_key(specialization)
            {
                let new_id = source.id.specialize(specialization.0, specialization.1);

                if graph.forward_edges.contains_key(&new_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_id;

                    new_source.is_specialized = false;
                    new_source.specialized_calls.remove(specialization);

                    generated_sources.push(Rc::new(new_source));
                }
            }
        }
    } else {
        for (key, map) in &source.specialized_calls {
            if map.contains(&source.id) {
                let new_forward_edge_id = source.id.specialize(key.0, key.1);

                if graph.forward_edges.contains_key(&new_forward_edge_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_forward_edge_id;
                    new_source.is_specialized = false;
                    generated_sources.push(Rc::new(new_source));
                }
            }
        }
    }

    generated_sources
}

#[allow(clippy::too_many_arguments)]
fn get_child_nodes(
    graph: &DataFlowGraph,
    generated_source: &Rc<TaintedNode>,
    source_taints: &[SinkType],
    seen_sources: &mut FxHashSet<String>,
    new_issues: &mut Vec<Issue>,
    is_last: bool,
    interner: &Interner,
    suppressed_ranges: &[(u32, u32)],
) -> Vec<Rc<TaintedNode>> {
    let mut new_child_nodes = Vec::new();

    let Some(forward_edges) = graph.forward_edges.get(&generated_source.id) else {
        return new_child_nodes;
    };

    for (to_id, path) in forward_edges {
        let Some(destination_node) = graph
            .vertices
            .get(to_id)
            .or_else(|| graph.sinks.get(to_id))
            .or_else(|| graph.sources.get(to_id))
        else {
            continue;
        };

        if let PathKind::Aggregate = &path.kind {
            continue;
        }

        // Psalm's `getTaintFlowGraphWithSuppressed`: a statement whose
        // docblock has `@psalm-suppress TaintedInput` adds no taint paths.
        // pzoom adds paths unconditionally and instead refuses to traverse
        // into nodes positioned inside a suppressed statement.
        if !suppressed_ranges.is_empty()
            && let Some(dest_pos) = destination_node.get_pos()
            && suppressed_ranges
                .iter()
                .any(|(start, end)| dest_pos.start_offset >= *start && dest_pos.start_offset <= *end)
        {
            continue;
        }

        // Going through a scalar type guard right after an array/property
        // assignment can't carry the tainted contents.
        if let PathKind::ScalarTypeGuard = &path.kind
            && has_recent_assignment(&generated_source.path_types)
        {
            continue;
        }

        if let PathKind::RefineSymbol(symbol_id) = &path.kind
            && has_unmatched_property_assignment(symbol_id, &generated_source.path_types)
        {
            continue;
        }

        if should_ignore_array_fetch(
            &path.kind,
            &ArrayDataKind::ArrayKey,
            &generated_source.path_types,
        ) {
            continue;
        }

        if should_ignore_array_fetch(
            &path.kind,
            &ArrayDataKind::ArrayValue,
            &generated_source.path_types,
        ) {
            continue;
        }

        if should_ignore_property_fetch(&path.kind, &generated_source.path_types) {
            continue;
        }

        let mut new_taints = source_taints.to_vec();
        for added in &path.added_taints {
            if !new_taints.contains(added) {
                new_taints.push(added.clone());
            }
        }
        new_taints.retain(|t| !path.removed_taints.contains(t));

        if new_taints.is_empty() {
            continue;
        }

        let mut new_destination = TaintedNode::from(destination_node);

        new_destination.previous = Some(generated_source.clone());
        new_destination.taint_sinks.clone_from(&new_taints);
        new_destination
            .specialized_calls
            .clone_from(&generated_source.specialized_calls);

        let mut new_path_types = generated_source.path_types.clone();
        new_path_types.push(match &path.kind {
            PathKind::RemoveDictKey(_) => PathKind::Default,
            _ => path.kind.clone(),
        });
        new_destination.path_types = new_path_types;

        // Hakana sink matching, with one pzoom accommodation: sinks made from
        // params have no stored location (`ParamInfo` lacks `name_location`),
        // so the issue position falls back to the predecessor node (the
        // argument value), which is where Psalm reports these issues too.
        if let Some(sink) = graph.sinks.get(to_id)
            && let DataFlowNodeKind::TaintSink {
                types,
                pos: sink_pos,
            } = &sink.kind
            && let Some(issue_pos) = sink_pos.or_else(|| generated_source.pos.as_deref().copied())
        {
            let mut matching_sinks = types.clone();
            matching_sinks.retain(|t| new_taints.contains(t));

            for matching_sink in &matching_sinks {
                new_destination.taint_sinks.retain(|s| s != matching_sink);

                let message = format!(
                    "Detected tainted {} in path: {}",
                    matching_sink.label(),
                    new_destination.get_trace(interner)
                );
                let mut issue = Issue::new(
                    matching_sink.issue_kind(),
                    message,
                    issue_pos.file_path,
                    issue_pos.start_offset,
                    issue_pos.end_offset,
                    issue_pos.start_line,
                    issue_pos.start_column as u32,
                );
                issue.taint_trace = new_destination
                    .get_trace_nodes(interner)
                    .into_iter()
                    .map(|(label, pos)| pzoom_code_info::TraceNode {
                        label,
                        location: pos.map(|p| {
                            pzoom_code_info::CodeLocation::new(
                                p.file_path,
                                p.start_offset,
                                p.end_offset,
                                p.start_line,
                                p.start_column as u32,
                            )
                        }),
                    })
                    .collect();
                new_issues.push(issue);
            }
        }

        let source_id = new_destination.get_unique_source_id(interner);

        if seen_sources.contains(&source_id) {
            continue;
        }

        seen_sources.insert(source_id);

        if !is_last {
            new_child_nodes.push(Rc::new(new_destination));
        }
    }

    new_child_nodes
}

/// Hakana `has_recent_assignment`.
fn has_recent_assignment(generated_path_types: &[PathKind]) -> bool {
    let filtered_paths = generated_path_types
        .iter()
        .rev()
        .filter(|t| !matches!(t, PathKind::Default));

    let mut nesting = 0;

    for filtered_path in filtered_paths {
        match filtered_path {
            PathKind::ArrayAssignment(_, _)
            | PathKind::UnknownArrayAssignment(_)
            | PathKind::PropertyAssignment(_, _)
            | PathKind::UnknownPropertyAssignment => {
                if nesting == 0 {
                    return true;
                }

                nesting -= 1;
            }
            PathKind::ArrayFetch(_, _)
            | PathKind::UnknownArrayFetch(_)
            | PathKind::PropertyFetch(_, _)
            | PathKind::UnknownPropertyFetch => {
                nesting += 1;
            }
            PathKind::Serialize => {
                return false;
            }
            _ => (),
        }
    }

    false
}

/// Hakana `has_unmatched_property_assignment`.
fn has_unmatched_property_assignment(
    symbol: &pzoom_str::StrId,
    generated_path_types: &[PathKind],
) -> bool {
    let filtered_paths = generated_path_types
        .iter()
        .rev()
        .filter(|t| !matches!(t, PathKind::Default));

    let mut nesting = 0;

    for filtered_path in filtered_paths {
        match filtered_path {
            PathKind::PropertyAssignment(assignment_symbol, _) => {
                if assignment_symbol == symbol {
                    if nesting == 0 {
                        return false;
                    }

                    nesting -= 1;
                }
            }
            PathKind::UnknownPropertyAssignment => {
                if nesting == 0 {
                    return false;
                }

                nesting -= 1;
            }
            PathKind::PropertyFetch(fetch_symbol, _) => {
                if fetch_symbol == symbol {
                    nesting += 1;
                }
            }
            PathKind::UnknownPropertyFetch => {
                nesting += 1;
            }
            PathKind::Serialize => {
                return false;
            }
            _ => (),
        }
    }

    true
}
