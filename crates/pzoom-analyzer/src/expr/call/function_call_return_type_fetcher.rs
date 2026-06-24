//! Function call return type fetcher.
//!
//! Mirrors Psalm/Hakana's dedicated function return-type fetcher flow:
//! special-case builtins first, then function storage return type.

use mago_syntax::cst::cst::argument::Argument;
use rustc_hash::FxHashMap;

use pzoom_code_info::data_flow::node::SinkType;
use pzoom_code_info::{
    ArrayDataKind, ArrayKey, DataFlowGraph, DataFlowNode, FunctionLikeIdentifier, FunctionLikeInfo,
    GraphKind, PathKind, TAtomic, TUnion,
};
use pzoom_str::StrId;

use crate::config::parse_php_version_tuple;
use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::function_call_analyzer;
use pzoom_code_info::TemplateResult;

pub(crate) fn fetch(
    analyzer: &StatementsAnalyzer<'_>,
    normalized_name: &str,
    function_info: Option<&FunctionLikeInfo>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    template_result: Option<&TemplateResult>,
) -> Option<TUnion> {
    let normalized_name = normalized_name
        .strip_prefix('\\')
        .unwrap_or(normalized_name);

    // Psalm's ignoreInternalFunctionFalseReturn/NullableReturn (default true,
    // and on in its test harness): the false/null in an internal (stub)
    // function's return type does not raise Possibly* issues downstream.
    let is_stub_function = function_info.is_some_and(|info| {
        analyzer
            .codebase
            .files
            .get(&info.file_path)
            .is_some_and(|file_info| file_info.is_stub)
    });
    // Psalm's FunctionCallReturnTypeFetcher exempts the strpos family from
    // the internal-falsable ignore: their `false` ("needle not found") is
    // always reportable, even when internal falsable returns are ignored.
    let falsable_always_reported = matches!(
        normalized_name.to_ascii_lowercase().as_str(),
        "mb_strpos"
            | "mb_strrpos"
            | "mb_stripos"
            | "mb_strripos"
            | "strpos"
            | "strrpos"
            | "stripos"
            | "strripos"
            | "strstr"
            | "stristr"
            | "strrchr"
            | "strpbrk"
            | "array_search"
    );
    let apply_internal_ignores = |return_type: &mut TUnion| {
        if is_stub_function {
            if return_type.is_falsable() && !falsable_always_reported {
                return_type.ignore_falsable_issues = true;
            }
            if return_type.is_nullable() {
                return_type.ignore_nullable_issues = true;
            }
        }
    };

    // Function return-type providers (Psalm-style extension point).
    if let Some(mut return_type) = crate::return_type_provider::dispatch_function_return_type(
        &crate::return_type_provider::FunctionReturnTypeProviderEvent {
            analyzer,
            function_id: normalized_name,
            args,
            arg_positions,
            context,
        },
        analysis_data,
    ) {
        apply_internal_ignores(&mut return_type);
        return Some(return_type);
    }

    let Some(function_info) = function_info else {
        return None;
    };

    if function_info.get_return_type().is_none() {
        return None;
    }

    let empty_template_result = TemplateResult::default();
    let template_result = template_result.unwrap_or(&empty_template_result);

    let param_arg_types =
        collect_param_arg_types(&function_info.params, arg_positions, analysis_data);

    let mut resolved_return_type = resolve_functionlike_return_type(
        analyzer,
        function_info,
        template_result,
        &param_arg_types,
        args.len(),
    )
    .or_else(|| Some(TUnion::mixed()));

    if let Some(return_type) = resolved_return_type.as_mut() {
        apply_internal_ignores(return_type);
    }

    resolved_return_type
}

/// Port of Hakana `function_call_return_type_fetcher::add_dataflow`.
///
/// Builds the call's return node (`CallTo`, specialized to the call site), adds
/// argument→return edges for known builtins (`get_special_argument_nodes`), and
/// makes the returned union's `parent_nodes` flow from the call node.
///
/// Whole-program (taint) additions, data-driven where Hakana hardcodes:
/// - storage `added_taints`/`removed_taints` (Psalm stub
///   `@psalm-taint-unescape`/`@psalm-taint-escape`) ride the special
///   argument→return edges (Hakana: `get_special_added_removed_taints`);
/// - storage `return_source_params` (Psalm `@psalm-flow`) add argument→return
///   edges for user-defined and stub functions alike (Psalm
///   `taintUsingFlows`; no Hakana equivalent - it has no flow docblocks);
/// - storage `taint_source_types` (Psalm `@psalm-taint-source`) re-add the
///   call node as a `TaintSource` (Hakana does the same with attributes).
///
/// Conservative deviations:
/// - `specialize_call` is treated as always-true (Hakana enables it for nearly
///   every function), so the return node is always call-site specialized.
/// - In whole-program mode Hakana prefers `return_type_location`/`name_location`
///   for the node position; pzoom does not store those, so `None` is used.
/// - `context.allow_taints` and per-param `propagate_taint` are not ported.
/// - Hack-only `HH\Asio\join` early-return is skipped.
pub(crate) fn add_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    function_info: Option<&FunctionLikeInfo>,
    arg_positions: &[Pos],
    pos: Pos,
    mut stmt_type: TUnion,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    let call_node_pos = make_data_flow_node_position(analyzer, pos);

    // Psalm positions the return node at the declared return type when one
    // exists (FunctionCallReturnTypeFetcher passes
    // $function_storage->return_type_location), which becomes the "Consider
    // improving the type at …" origin for mixed values. Same-file only —
    // line/column derivation needs the current file's source.
    let return_decl_pos = function_info
        .filter(|info| info.file_path == analyzer.file_path)
        .and_then(|info| info.return_type_location)
        .map(|(start, end)| make_data_flow_node_position(analyzer, (start, end)));

    let function_call_node = if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        DataFlowNode::get_for_method_return(
            functionlike_id,
            Some(return_decl_pos.unwrap_or(call_node_pos)),
            Some(call_node_pos),
        )
    } else {
        // Whole-program (taint) mode: per-call-site nodes only for
        // `specialize_call` storages (plain functions specialize at scan
        // time; an unknown callee defaults to specialized, Psalm's `?? true`).
        let specialize_call = function_info
            .map(|info| info.taints.specialize_call)
            .unwrap_or(true);
        DataFlowNode::get_for_method_return(
            functionlike_id,
            None,
            specialize_call.then_some(call_node_pos),
        )
    };

    analysis_data
        .data_flow_graph
        .add_node(function_call_node.clone());

    // Hakana: only non-user-defined (builtin) functions get summarized
    // argument→return edges; user-defined bodies are analyzed separately.
    let user_defined = function_info.is_some_and(|info| {
        !analyzer
            .codebase
            .files
            .get(&info.file_path)
            .is_some_and(|file_info| file_info.is_stub)
    });

    let (param_offsets, variadic_path) = if !user_defined && !arg_positions.is_empty() {
        get_special_argument_nodes(functionlike_id)
    } else {
        (vec![], None)
    };

    // Whole-program mode: storage added/removed taints (from Psalm's stub
    // `@psalm-taint-unescape`/`@psalm-taint-escape` docblocks) ride every
    // argument→return edge, plus the hardcoded html-codec taints (Psalm's
    // built-in `HtmlFunctionTainter` plugin; Hakana hardcodes the same data
    // in `get_special_added_removed_taints`).
    let (added_taints, removed_taints) = if matches!(
        analysis_data.data_flow_graph.kind,
        GraphKind::WholeProgram(_)
    ) {
        let (mut added, mut removed) = function_info
            .map(|info| {
                (
                    info.taints.added_taints.clone(),
                    info.taints.removed_taints.clone(),
                )
            })
            .unwrap_or_default();

        let (event_added, event_removed) =
            get_html_codec_taints(analyzer, functionlike_id, arg_positions, analysis_data);
        for taint in event_added {
            if !added.contains(&taint) {
                added.push(taint);
            }
        }
        for taint in event_removed {
            if !removed.contains(&taint) {
                removed.push(taint);
            }
        }

        // Psalm `taintReturnType`: a preg_replace whose literal pattern is a
        // simple character-class exclusion (e.g. `/[^_a-z\/\.A-Z0-9]/`, with
        // a literal replacement) strips every html/quote/sql metacharacter
        // from the value, so those taints drop from the argument→return flow.
        for taint in
            get_preg_replace_exclusion_removed_taints(functionlike_id, arg_positions, analysis_data)
        {
            if !removed.contains(&taint) {
                removed.push(taint);
            }
        }

        (added, removed)
    } else {
        (vec![], vec![])
    };

    let mut last_arg = usize::MAX;

    for (param_offset, path_kind) in param_offsets {
        if let Some(arg_pos) = arg_positions.get(param_offset) {
            add_special_param_dataflow(
                analyzer,
                functionlike_id,
                true,
                param_offset,
                *arg_pos,
                pos,
                &mut analysis_data.data_flow_graph,
                &function_call_node,
                path_kind,
                added_taints.clone(),
                removed_taints.clone(),
            );
        }

        last_arg = param_offset;
    }

    if let Some(path_kind) = &variadic_path {
        for (param_offset, arg_pos) in arg_positions.iter().enumerate() {
            if last_arg == usize::MAX || param_offset > last_arg {
                add_special_param_dataflow(
                    analyzer,
                    functionlike_id,
                    true,
                    param_offset,
                    *arg_pos,
                    pos,
                    &mut analysis_data.data_flow_graph,
                    &function_call_node,
                    path_kind.clone(),
                    added_taints.clone(),
                    removed_taints.clone(),
                );
            }
        }
    }

    add_storage_taint_dataflow(
        analyzer,
        functionlike_id,
        function_info,
        arg_positions,
        pos,
        &function_call_node,
        analysis_data,
        &added_taints,
        &removed_taints,
    );

    if let Some(escaped_node) = apply_conditionally_escaped_taints(
        analyzer,
        functionlike_id,
        function_info,
        arg_positions,
        analysis_data,
        &function_call_node,
        pos,
    ) {
        stmt_type.parent_nodes.push(escaped_node);
    } else {
        stmt_type.parent_nodes.push(function_call_node);
    }

    stmt_type
}

/// Psalm `FunctionCallReturnTypeFetcher::taintReturnType` /
/// `StaticCallAnalyzer`: evaluate `@psalm-taint-escape (<conditional>)`
/// against the call's arguments. When the conditional resolves to a
/// non-nullable type with literal taint names, the call's return flows into
/// a `<name>-escaped` assignment node instead of the call node.
///
/// Psalm never registers that node in the graph (the `addPath` has no
/// matching `addNode`), so its taint BFS — which skips edges whose target is
/// not a known node — dead-ends there: a resolved conditional escape stops
/// EVERY taint through the return value, not only the named kinds (verified
/// against Psalm: escaping only `has_quotes`, or even a bogus name, silences
/// TaintedHtml too). pzoom's BFS has the same target-must-exist rule, so the
/// node is deliberately not added here either.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_conditionally_escaped_taints(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    function_info: Option<&FunctionLikeInfo>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    function_call_node: &DataFlowNode,
    call_pos: Pos,
) -> Option<DataFlowNode> {
    if !matches!(
        analysis_data.data_flow_graph.kind,
        GraphKind::WholeProgram(_)
    ) {
        return None;
    }

    let info = function_info?;
    if info.taints.conditionally_removed_taints.is_empty() {
        return None;
    }

    let param_arg_types = collect_param_arg_types(&info.params, arg_positions, analysis_data);

    let mut removed_taints: Vec<SinkType> = Vec::new();
    let mut any_resolved = false;

    for conditional in &info.taints.conditionally_removed_taints {
        // Bind the conditional's `$param` subject from the matching argument
        // (or the param default via collect_param_arg_types), then collapse
        // the conditional through the template layer (Psalm's
        // TemplateInferredTypeReplacer::replaceConditional).
        let mut template_result = TemplateResult::default();
        if let Some(subject_type) = param_arg_types.get(&conditional.param_name) {
            crate::template::lower_bounds_insert(
                &mut template_result,
                conditional.param_name,
                conditional.defining_entity,
                subject_type.clone(),
            );
        }

        let resolved = function_call_analyzer::replace_templates_in_union_in(
            Some(analyzer.codebase),
            &TUnion::new(TAtomic::TConditional(Box::new(conditional.clone()))),
            &template_result,
        );

        // Psalm: a nullable result means the null branch may apply — no escape.
        if resolved.is_nullable()
            || resolved
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TNull))
        {
            continue;
        }

        for atomic in &resolved.types {
            if let TAtomic::TLiteralString { value } = atomic {
                any_resolved = true;
                for kind in SinkType::kinds_from_name(value) {
                    if !removed_taints.contains(&kind) {
                        removed_taints.push(kind);
                    }
                }
            }
        }
    }

    if !any_resolved {
        return None;
    }

    let label = match functionlike_id {
        FunctionLikeIdentifier::Function(name) => {
            format!("{}-escaped", analyzer.interner.lookup(*name))
        }
        FunctionLikeIdentifier::Method(classlike_name, method_name) => format!(
            "{}::{}-escaped",
            analyzer.interner.lookup(*classlike_name),
            analyzer.interner.lookup(*method_name)
        ),
        FunctionLikeIdentifier::Closure(..) => "closure-escaped".to_string(),
    };

    let escaped_node =
        DataFlowNode::get_for_local_string(label, make_data_flow_node_position(analyzer, call_pos));

    analysis_data.data_flow_graph.add_path(
        &function_call_node.id,
        &escaped_node.id,
        PathKind::Default,
        vec![],
        removed_taints,
    );
    // Deliberately not add_node'd — see the doc comment above.

    Some(escaped_node)
}

/// Psalm `FunctionCallReturnTypeFetcher::taintReturnType`'s preg_replace
/// special case: when the pattern and replacement are single string literals
/// and the pattern body is `[^…]` whose excluded-set complement passes
/// `simpleExclusion`, the result cannot carry html/quote/sql
/// metacharacters.
fn get_preg_replace_exclusion_removed_taints(
    functionlike_id: &FunctionLikeIdentifier,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Vec<SinkType> {
    let FunctionLikeIdentifier::Function(function_name) = functionlike_id else {
        return vec![];
    };
    if *function_name != StrId::PREG_REPLACE || arg_positions.len() <= 2 {
        return vec![];
    }

    let get_literal = |pos: Pos| -> Option<String> {
        let arg_type = analysis_data.expr_types.get(&pos).cloned()?;
        if arg_type.types.len() != 1 {
            return None;
        }
        match arg_type.types.first() {
            Some(TAtomic::TLiteralString { value })
                if value != pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE =>
            {
                Some(value.clone())
            }
            _ => None,
        }
    };

    let (Some(pattern_literal), Some(_replacement_literal)) = (
        arg_positions.first().copied().and_then(get_literal),
        arg_positions.get(1).copied().and_then(get_literal),
    ) else {
        return vec![];
    };

    // Strip the delimiters, mirroring Psalm's `substr($value, 1, -1)` + trim.
    if pattern_literal.len() < 2 {
        return vec![];
    }
    let escape_char = pattern_literal.as_bytes()[0];
    let pattern = pattern_literal[1..pattern_literal.len() - 1].trim();
    let pattern_bytes = pattern.as_bytes();
    if pattern_bytes.len() < 3
        || pattern_bytes[0] != b'['
        || pattern_bytes[1] != b'^'
        || pattern_bytes[pattern_bytes.len() - 1] != b']'
    {
        return vec![];
    }

    if simple_exclusion(&pattern[2..pattern.len() - 1], escape_char) {
        vec![SinkType::Html, SinkType::HasQuotes, SinkType::Sql]
    } else {
        vec![]
    }
}

/// Byte-for-byte port of Psalm's `simpleExclusion`: whether a `[^…]`
/// character-class body only re-admits known-safe characters and ranges
/// (`a-z`, `a-Z`, `A-Z`, `0-9`, `_-|:#. ` and a few escaped literals).
fn simple_exclusion(pattern: &str, escape_char: u8) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        let current = bytes[i];
        let next = bytes.get(i + 1).copied();

        if current == b'\\' {
            match next {
                None | Some(b'x') | Some(b'u') => return false,
                Some(b'.') | Some(b'(') | Some(b')') | Some(b'[') | Some(b']') | Some(b's')
                | Some(b'w') => {
                    i += 2;
                    continue;
                }
                Some(other) if other == escape_char => {
                    i += 2;
                    continue;
                }
                _ => return false,
            }
        }

        if next != Some(b'-') {
            if matches!(current, b'_' | b'-' | b'|' | b':' | b'#' | b'.' | b' ') {
                i += 1;
                continue;
            }
            return false;
        }

        if current == b']' {
            return false;
        }

        let Some(range_end) = bytes.get(i + 2).copied() else {
            return false;
        };

        if (current == b'a' && range_end == b'z')
            || (current == b'a' && range_end == b'Z')
            || (current == b'A' && range_end == b'Z')
            || (current == b'0' && range_end == b'9')
        {
            i += 3;
            continue;
        }

        return false;
    }

    true
}

/// Port of Psalm's built-in `HtmlFunctionTainter` plugin (registered
/// unconditionally in `Config`), Hakana-style as a hardcoded table:
/// `html_entity_decode`/`htmlspecialchars_decode` ADD html (and has_quotes
/// when ENT_QUOTES is passed, or by default on PHP >= 8.1);
/// `htmlentities`/`htmlspecialchars` REMOVE the same kinds.
fn get_html_codec_taints(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> (Vec<SinkType>, Vec<SinkType>) {
    const ENT_QUOTES: i64 = 3;

    let FunctionLikeIdentifier::Function(function_name) = functionlike_id else {
        return (vec![], vec![]);
    };

    if arg_positions.is_empty() {
        return (vec![], vec![]);
    }

    let name = analyzer.interner.lookup(*function_name).to_lowercase();
    let decodes = matches!(
        name.as_str(),
        "html_entity_decode" | "htmlspecialchars_decode"
    );
    let encodes = matches!(name.as_str(), "htmlentities" | "htmlspecialchars");

    if !decodes && !encodes {
        return (vec![], vec![]);
    }

    let kinds = match arg_positions.get(1) {
        None => {
            if get_configured_php_version_id(analyzer) >= 80100 {
                vec![SinkType::Html, SinkType::HasQuotes]
            } else {
                vec![SinkType::Html]
            }
        }
        Some(flags_pos) => {
            let flags_value = analysis_data
                .expr_types
                .get(&*flags_pos)
                .cloned()
                .and_then(|t| get_single_literal_int(&t));
            match flags_value {
                Some(value) if (value & ENT_QUOTES) == ENT_QUOTES => {
                    vec![SinkType::Html, SinkType::HasQuotes]
                }
                _ => vec![SinkType::Html],
            }
        }
    };

    if decodes {
        (kinds, vec![])
    } else {
        (vec![], kinds)
    }
}

/// Whole-program taint dataflow driven by function-like storage, shared by
/// function and method calls:
/// - Psalm `FunctionCallReturnTypeFetcher::taintUsingFlows`: `@psalm-flow
///   ($a) -> return` adds an argument→return edge regardless of whether the
///   function is user-defined (the body of a stubbed/user flow function may
///   not exhibit the flow itself);
/// - Psalm `taintUsingStorage` / Hakana `taint_source_types`: calling a
///   `@psalm-taint-source` function introduces taints on its return value.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_storage_taint_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    function_info: Option<&FunctionLikeInfo>,
    arg_positions: &[Pos],
    pos: Pos,
    function_call_node: &DataFlowNode,
    analysis_data: &mut FunctionAnalysisData,
    added_taints: &[SinkType],
    removed_taints: &[SinkType],
) {
    if !matches!(
        analysis_data.data_flow_graph.kind,
        GraphKind::WholeProgram(_)
    ) {
        return;
    }

    let Some(info) = function_info else {
        return;
    };

    for (param_index, path_type) in &info.taints.return_source_params {
        let arg_indices = if info.params.get(*param_index).is_some_and(|p| p.is_variadic) {
            (*param_index..arg_positions.len()).collect::<Vec<_>>()
        } else {
            vec![*param_index]
        };

        for arg_index in arg_indices {
            let Some(arg_pos) = arg_positions.get(arg_index) else {
                continue;
            };

            let path_kind = match path_type.as_str() {
                "array-fetch" => PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue),
                "array-assignment" => PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                _ => PathKind::Default,
            };

            add_special_param_dataflow(
                analyzer,
                functionlike_id,
                true,
                arg_index,
                *arg_pos,
                pos,
                &mut analysis_data.data_flow_graph,
                function_call_node,
                path_kind,
                added_taints.to_vec(),
                removed_taints.to_vec(),
            );
        }
    }

    // Psalm `@psalm-flow proxy other_fn($a) [-> return]`: Psalm analyzes a
    // synthesized call to the proxied function; pzoom wires the dataflow that
    // call would create - argument edges into the proxied function's
    // argument nodes (with its sinks) and, for `-> return`, an edge from the
    // proxied call's return node into this call's return node.
    for proxy_call in &info.taints.proxy_calls {
        let proxy_id = if let Some((class_name, method_name)) = proxy_call.fqn.split_once("::") {
            FunctionLikeIdentifier::Method(
                analyzer
                    .interner
                    .find(class_name)
                    .unwrap_or(pzoom_str::StrId::EMPTY),
                analyzer
                    .interner
                    .find(method_name)
                    .unwrap_or(pzoom_str::StrId::EMPTY),
            )
        } else {
            FunctionLikeIdentifier::Function(
                analyzer
                    .interner
                    .find(&proxy_call.fqn)
                    .unwrap_or(pzoom_str::StrId::EMPTY),
            )
        };

        let proxied_info: Option<&FunctionLikeInfo> = match proxy_id {
            FunctionLikeIdentifier::Function(name) => analyzer.codebase.get_function(name),
            FunctionLikeIdentifier::Method(class_name, method_name) => analyzer
                .codebase
                .get_class(class_name)
                .and_then(|class_info| class_info.methods.get(&method_name))
                .map(|method_info| method_info.as_ref()),
            FunctionLikeIdentifier::Closure(..) => None,
        };

        let call_node_pos = make_data_flow_node_position(analyzer, pos);

        for (proxied_offset, own_param_index) in proxy_call.params.iter().enumerate() {
            let Some(arg_pos) = arg_positions.get(*own_param_index) else {
                continue;
            };
            let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned() else {
                continue;
            };

            let proxied_method_node = DataFlowNode::get_for_method_argument(
                &proxy_id,
                proxied_offset,
                None,
                Some(call_node_pos),
            );

            let mut sinks = proxied_info
                .and_then(|p| p.params.get(proxied_offset))
                .map(|param| param.sinks.clone())
                .unwrap_or_default();
            for taint in super::argument_analyzer::get_builtin_argument_taints(
                &proxy_id,
                proxied_offset,
                analyzer.interner,
            ) {
                if !sinks.contains(&taint) {
                    sinks.push(taint);
                }
            }
            if !sinks.is_empty() {
                analysis_data.data_flow_graph.add_node(DataFlowNode {
                    id: proxied_method_node.id.clone(),
                    kind: pzoom_code_info::data_flow::node::DataFlowNodeKind::TaintSink {
                        pos: Some(make_data_flow_node_position(analyzer, *arg_pos)),
                        types: sinks,
                    },
                });
            }

            for parent_node in &arg_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &proxied_method_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            analysis_data.data_flow_graph.add_node(proxied_method_node);
        }

        if proxy_call.returns {
            let proxied_call_node =
                DataFlowNode::get_for_method_return(&proxy_id, None, Some(call_node_pos));
            analysis_data.data_flow_graph.add_path(
                &proxied_call_node.id,
                &function_call_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
            analysis_data.data_flow_graph.add_node(proxied_call_node);
        }
    }

    let mut source_types = if !info.taints.taint_source_types.is_empty() {
        info.taints.taint_source_types.clone()
    } else {
        info.taints.added_taints.clone()
    };
    source_types.retain(|t| !info.taints.removed_taints.contains(t));

    if !source_types.is_empty() {
        let function_call_node_source = DataFlowNode {
            id: function_call_node.id.clone(),
            kind: pzoom_code_info::data_flow::node::DataFlowNodeKind::TaintSource {
                pos: function_call_node.get_pos(),
                types: source_types,
            },
        };
        analysis_data
            .data_flow_graph
            .add_node(function_call_node_source);
    }
}

/// Port of Hakana `add_special_param_dataflow`: connect a call argument's
/// `FunctionLikeArg` node to the call's return node with the given path kind
/// and the edge's added/removed taints (storage-driven in pzoom; Hakana uses
/// the hardcoded `get_special_added_removed_taints` map).
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_special_param_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    specialize_call: bool,
    param_offset: usize,
    arg_pos: Pos,
    call_pos: Pos,
    data_flow_graph: &mut DataFlowGraph,
    function_call_node: &DataFlowNode,
    path_kind: PathKind,
    added_taints: Vec<pzoom_code_info::data_flow::node::SinkType>,
    removed_taints: Vec<pzoom_code_info::data_flow::node::SinkType>,
) {
    let argument_node = DataFlowNode::get_for_method_argument(
        functionlike_id,
        param_offset,
        Some(make_data_flow_node_position(analyzer, arg_pos)),
        if specialize_call {
            Some(make_data_flow_node_position(analyzer, call_pos))
        } else {
            None
        },
    );

    data_flow_graph.add_path(
        &argument_node.id,
        &function_call_node.id,
        path_kind,
        added_taints,
        removed_taints,
    );
    data_flow_graph.add_node(argument_node);
}

/// Port of Hakana `get_special_argument_nodes`, restricted to the PHP builtins
/// in the table (Hack `HH\Lib\*` entries are dropped; PHP spellings are used
/// where Hakana models a `_with_matches`/`_l` Hack variant).
///
/// Returns a list of `(input_argument_position, path-to-return-output)` pairs;
/// the optional second member applies to all remaining arguments (variadic-ish
/// builtins). Unknown builtins fall back to flowing every argument to the
/// return value (Hakana's cop-out default, guaranteeing false positives over
/// false negatives).
fn get_special_argument_nodes(
    functionlike_id: &FunctionLikeIdentifier,
) -> (Vec<(usize, PathKind)>, Option<PathKind>) {
    match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => match *function_name {
            StrId::VAR_EXPORT
            | StrId::PRINT_R
            | StrId::HIGHLIGHT_STRING
            | StrId::STRTOLOWER
            | StrId::STRTOUPPER
            | StrId::TRIM
            | StrId::LTRIM
            | StrId::RTRIM
            | StrId::STRIP_TAGS
            | StrId::STRIPSLASHES
            | StrId::STRIPCSLASHES
            | StrId::HTMLENTITIES
            | StrId::HTML_ENTITY_DECODE
            | StrId::HTMLSPECIALCHARS
            | StrId::HTMLSPECIALCHARS_DECODE
            | StrId::STR_ROT13
            | StrId::STR_SHUFFLE
            | StrId::STRSTR
            | StrId::STRISTR
            | StrId::STRCHR
            | StrId::STRPBRK
            | StrId::STRRCHR
            | StrId::STRREV
            | StrId::PREG_QUOTE
            | StrId::WORDWRAP
            | StrId::REALPATH
            | StrId::STRVAL
            | StrId::STR_GETCSV
            | StrId::ADDCSLASHES
            | StrId::ADDSLASHES
            | StrId::UCFIRST
            | StrId::UCWORDS
            | StrId::LCFIRST
            | StrId::NL2BR
            | StrId::QUOTED_PRINTABLE_DECODE
            | StrId::QUOTED_PRINTABLE_ENCODE
            | StrId::QUOTEMETA
            | StrId::CHOP
            | StrId::CONVERT_UUDECODE
            | StrId::CONVERT_UUENCODE
            | StrId::BASE64_ENCODE
            | StrId::BASE64_DECODE
            | StrId::URLENCODE
            | StrId::URLDECODE
            | StrId::GZINFLATE
            | StrId::GET_OBJECT_VARS
            | StrId::RAWURLENCODE
            | StrId::ORD
            | StrId::LOG
            | StrId::IP2LONG
            | StrId::BIN2HEX
            | StrId::HEX2BIN
            | StrId::ESCAPESHELLARG
            | StrId::CHR
            | StrId::DECBIN
            | StrId::DECHEX
            | StrId::HEXDEC
            | StrId::RAWURLDECODE
            | StrId::UTF8_DECODE
            | StrId::UTF8_ENCODE
            | StrId::STREAM_GET_META_DATA
            | StrId::DIRNAME => (vec![(0, PathKind::Default)], None),
            StrId::ARRAY_MERGE | StrId::PACK | StrId::UNPACK | StrId::JSON_DECODE => {
                (vec![(0, PathKind::Default)], Some(PathKind::Default))
            }
            StrId::NUMBER_FORMAT
            | StrId::SUBSTR
            | StrId::GZCOMPRESS
            | StrId::GZDECODE
            | StrId::GZDEFLATE
            | StrId::GZUNCOMPRESS
            | StrId::STR_REPEAT
            | StrId::BASENAME => (vec![(0, PathKind::Default)], Some(PathKind::Aggregate)),
            StrId::COUNT
            | StrId::INTVAL
            | StrId::GET_CLASS
            | StrId::CTYPE_LOWER
            | StrId::SHA1
            | StrId::MD5
            | StrId::CRC32
            | StrId::FILTER_VAR
            | StrId::IS_A
            | StrId::IS_BOOL
            | StrId::IS_CALLABLE
            | StrId::IS_FINITE
            | StrId::IS_FLOAT
            | StrId::IS_INFINITE
            | StrId::IS_INT
            | StrId::IS_NAN
            | StrId::IS_NULL
            | StrId::IS_NUMERIC
            | StrId::IS_OBJECT
            | StrId::IS_RESOURCE
            | StrId::IS_SCALAR
            | StrId::IS_STRING
            | StrId::CTYPE_ALNUM
            | StrId::CTYPE_ALPHA
            | StrId::CTYPE_DIGIT
            | StrId::CTYPE_PUNCT
            | StrId::CTYPE_SPACE
            | StrId::CTYPE_UPPER
            | StrId::CTYPE_XDIGIT
            | StrId::ASIN
            | StrId::CEIL
            | StrId::ABS
            | StrId::DEG2RAD
            | StrId::FLOOR
            | StrId::CLASS_EXISTS
            | StrId::LONG2IP
            | StrId::RAD2DEG
            | StrId::ROUND
            | StrId::GETTYPE
            | StrId::FUNCTION_EXISTS
            | StrId::GET_PARENT_CLASS
            | StrId::GET_RESOURCE_TYPE
            | StrId::FLOATVAL => (vec![(0, PathKind::Aggregate)], None),
            StrId::HASH_EQUALS
            | StrId::RANGE
            | StrId::STRPOS
            | StrId::SUBSTR_COUNT
            | StrId::STRCMP
            | StrId::STRNATCASECMP
            | StrId::IS_SUBCLASS_OF
            | StrId::STRIPOS
            | StrId::STRLEN
            | StrId::STRNATCMP
            | StrId::STRNCMP
            | StrId::STRRPOS
            | StrId::STRSPN
            | StrId::LEVENSHTEIN
            | StrId::INTDIV
            | StrId::STRCASECMP
            | StrId::STRCSPN
            | StrId::SUBSTR_COMPARE
            | StrId::VERSION_COMPARE
            | StrId::FMOD
            | StrId::POW
            | StrId::ATAN2
            | StrId::MB_DETECT_ENCODING => (vec![], Some(PathKind::Aggregate)),
            StrId::IN_ARRAY | StrId::PREG_MATCH | StrId::PREG_MATCH_ALL | StrId::HASH => (
                vec![
                    (0, PathKind::Aggregate),
                    (1, PathKind::Aggregate),
                    (3, PathKind::Aggregate),
                    (4, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::JSON_ENCODE | StrId::SERIALIZE => (vec![(0, PathKind::Serialize)], None),
            StrId::VAR_DUMP | StrId::PRINTF => {
                (vec![(0, PathKind::Serialize)], Some(PathKind::Serialize))
            }
            StrId::SSCANF | StrId::SUBSTR_REPLACE => {
                (vec![(0, PathKind::Default), (1, PathKind::Default)], None)
            }
            StrId::STR_REPLACE | StrId::STR_IREPLACE | StrId::PREG_FILTER | StrId::PREG_REPLACE => {
                (
                    vec![
                        (0, PathKind::Aggregate),
                        (1, PathKind::Default),
                        (2, PathKind::Default),
                    ],
                    None,
                )
            }
            StrId::PREG_GREP => (vec![(0, PathKind::Aggregate), (1, PathKind::Default)], None),
            StrId::VSPRINTF | StrId::IMPLODE | StrId::JOIN => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                ],
                None,
            ),
            StrId::STR_PAD | StrId::CHUNK_SPLIT => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Default),
                ],
                None,
            ),
            // Psalm's StrTrReturnTypeProvider flows every argument into the
            // return value (the replacement table's contents end up in the
            // result), unlike Hakana which treats the table as an aggregate.
            StrId::STRTR => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::Default),
                    (2, PathKind::Default),
                ],
                None,
            ),
            StrId::EXPLODE | StrId::PREG_SPLIT => (
                vec![
                    (0, PathKind::Aggregate),
                    (
                        1,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                ],
                None,
            ),
            StrId::HTTP_BUILD_QUERY => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue))],
                None,
            ),
            StrId::PATHINFO => (
                vec![
                    (
                        0,
                        PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, "dirname".to_string()),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "basename".to_string(),
                        ),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "extension".to_string(),
                        ),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "filename".to_string(),
                        ),
                    ),
                ],
                None,
            ),
            StrId::STR_SPLIT => (
                vec![
                    (
                        0,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Aggregate),
                ],
                None,
            ),
            // Hakana handles `sprintf` separately via its format-string concat
            // analysis; pzoom has no such pass, so use the catch-all default.
            StrId::SPRINTF => (vec![], Some(PathKind::Default)),
            _ => {
                // this is a cop-out, but will guarantee false-positives vs
                // false-negatives in taint analysis
                (vec![], Some(PathKind::Default))
            }
        },
        _ => (vec![], Some(PathKind::Default)),
    }
}

pub(crate) fn fetch_microtime_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let arg_pos = *arg_positions.first()?;
    let arg_type = analysis_data.expr_types.get(&arg_pos).cloned()?;

    if arg_type.is_always_truthy() {
        Some(TUnion::float())
    } else if arg_type.is_always_falsy() {
        Some(TUnion::string())
    } else {
        None
    }
}

pub(crate) fn fetch_preg_split_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let pattern_pos = *arg_positions.first()?;
    let subject_pos = *arg_positions.get(1)?;
    let pattern_type = analysis_data.expr_types.get(&pattern_pos).cloned()?;
    let subject_type = analysis_data.expr_types.get(&subject_pos).cloned()?;

    if !union_is_string_like(&pattern_type) || !union_is_string_like(&subject_type) {
        return None;
    }

    let list_atomic = if let Some(flags_pos) = arg_positions.get(3).copied() {
        let flags_type = analysis_data.expr_types.get(&flags_pos).cloned()?;
        match get_single_literal_int(&flags_type) {
            Some(0 | 2) => TAtomic::non_empty_list(TUnion::string()),
            Some(1 | 3) => TAtomic::list(TUnion::string()),
            Some(_) => TAtomic::list(TUnion::new(make_offset_capture_shape())),
            None => TAtomic::non_empty_list(TUnion::string()),
        }
    } else {
        TAtomic::non_empty_list(TUnion::string())
    };

    let mut result = TUnion::from_types(vec![list_atomic, TAtomic::TFalse]);
    result.ignore_falsable_issues = true;
    Some(result)
}

pub(crate) fn fetch_hrtime_return_type(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    // Psalm's HrTimeReturnTypeProvider / CallMap models `hrtime(false)` as the
    // precise 2-element pair `array{0: int, 1: int}` — both offsets are always
    // present, so destructuring `[$s, $n] = hrtime()` does not make the second
    // offset look possibly-undefined (an open `non-empty-list<int>` would only
    // guarantee offset 0).
    fn hrtime_pair_type() -> TAtomic {
        let mut known_values = FxHashMap::default();
        known_values.insert(ArrayKey::Int(0), (false, TUnion::int()));
        known_values.insert(ArrayKey::Int(1), (false, TUnion::int()));
        TAtomic::keyed_array(known_values, true, true, None, None)
    }

    if args.is_empty() {
        return Some(TUnion::new(hrtime_pair_type()));
    }

    let first_arg_pos = *arg_positions.first()?;
    let first_arg_type = analysis_data.expr_types.get(&first_arg_pos).cloned()?;

    match get_single_literal_bool(&first_arg_type) {
        // Psalm's HrTimeReturnTypeProvider: as_number=true can overflow into
        // a float on 32-bit platforms — int|float.
        Some(true) => Some(TUnion::from_types(vec![TAtomic::TInt, TAtomic::TFloat])),
        Some(false) => Some(TUnion::new(hrtime_pair_type())),
        None => Some(TUnion::from_types(vec![
            TAtomic::TInt,
            TAtomic::TFloat,
            hrtime_pair_type(),
        ])),
    }
}

fn union_is_string_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TClassString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
        )
    })
}

fn get_single_literal_int(union: &TUnion) -> Option<i64> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}

fn get_single_literal_bool(union: &TUnion) -> Option<bool> {
    if union.types.len() != 1 {
        return None;
    }

    match union.types.first() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn make_offset_capture_shape() -> TAtomic {
    let mut known_values = FxHashMap::default();
    known_values.insert(ArrayKey::Int(0), (false, TUnion::string()));
    known_values.insert(ArrayKey::Int(1), (false, TUnion::int()));

    TAtomic::keyed_array(known_values, true, true, None, None)
}

/// Map each parameter to its argument's inferred type, so conditional return
/// types of the form `($param is X ? A : B)` can be evaluated at the call
/// site. An omitted argument takes the declared default (Psalm evaluates
/// defaults too — microtime()'s `($as_float is true ? float : string)`
/// resolves to the no-arg branch).
pub(crate) fn collect_param_arg_types(
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> FxHashMap<StrId, TUnion> {
    let mut param_arg_types: FxHashMap<StrId, TUnion> = FxHashMap::default();
    for (index, param) in params.iter().enumerate() {
        if let Some(arg_pos) = arg_positions.get(index) {
            if let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned() {
                param_arg_types.insert(param.name, (*arg_type).clone());
            }
        } else if let Some(default_type) = &param.default_type {
            param_arg_types.insert(param.name, default_type.clone());
        }
    }
    param_arg_types
}

pub(crate) fn resolve_functionlike_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    template_result: &TemplateResult,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let return_type = function_info.get_return_type()?;
    let mut effective_template_result = template_result.clone();
    inject_fetcher_template_replacements(
        analyzer,
        function_info,
        arg_count,
        param_arg_types,
        &mut effective_template_result,
    );

    // Conditional return types resolve inside the template replacement layer
    // (Psalm's TemplateInferredTypeReplacer::replaceConditional) — the fetcher
    // only injects the synthetic/param-subject bounds above.
    let mut resolved = function_call_analyzer::replace_templates_in_union_in(
        Some(analyzer.codebase),
        return_type,
        &effective_template_result,
    );
    // Template substitution rebuilds the union from its atomics, dropping the
    // docblock suppression flags Psalm stores on the return union itself
    // (`@psalm-ignore-nullable-return` / `@psalm-ignore-falsable-return`). Carry them
    // over from the declared return type.
    resolved.from_docblock |= return_type.from_docblock;
    resolved.ignore_nullable_issues |= return_type.ignore_nullable_issues;
    resolved.ignore_falsable_issues |= return_type.ignore_falsable_issues;

    // Collapse any conditional left nested inside the resolved type (e.g. in
    // a callable's return position, where template replacement does not
    // descend) into the union of its branches.
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut resolved,
        &crate::type_expander::TypeExpansionOptions {
            evaluate_conditional_types: true,
            ..Default::default()
        },
    );

    // Collapse any conditional nested in a non-conditional return type's parameters.
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut resolved,
        &crate::type_expander::TypeExpansionOptions {
            evaluate_conditional_types: true,
            ..Default::default()
        },
    );
    Some(resolved)
}

/// Binds the synthetic templates Psalm's FunctionCallReturnTypeFetcher fills
/// in (TFunctionArgCount, PHP version templates, never for anything left
/// unbound), plus pzoom's `$param`-named conditional-subject templates, which
/// bind from the matching argument (or the param default) — Psalm achieves
/// the same by rewriting such params to generated templates at scan time.
fn inject_fetcher_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    arg_count: usize,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    template_result: &mut TemplateResult,
) {
    inject_subject_template_replacements(
        analyzer,
        &function_info.template_types,
        arg_count,
        param_arg_types,
        template_result,
    );
}

pub(crate) fn inject_subject_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_template_types: &[pzoom_code_info::functionlike_info::FunctionTemplateType],
    arg_count: usize,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    template_result: &mut TemplateResult,
) {
    for template_type in functionlike_template_types {
        if crate::template::lower_bounds_contains_name(template_result, template_type.name) {
            continue;
        }

        let template_name = analyzer.interner.lookup(template_type.name);
        let replacement = if template_name
            .as_ref()
            .eq_ignore_ascii_case("TFunctionArgCount")
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: arg_count as i64,
            })
        } else if template_name
            .as_ref()
            .eq_ignore_ascii_case("TPhpMajorVersion")
            || template_name.as_ref() == "PHP_MAJOR_VERSION"
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_major_version(analyzer),
            })
        } else if template_name.as_ref().eq_ignore_ascii_case("TPhpVersionId")
            || template_name.as_ref() == "PHP_VERSION_ID"
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_version_id(analyzer),
            })
        } else if template_name.as_ref().starts_with('$') {
            match param_arg_types.get(&template_type.name) {
                // Psalm's standin binding keeps only the argument members the
                // template's as-type can contain (matching_input_keys), so a
                // `string|false` subject binds `$subject as string|array`
                // without the false.
                Some(arg_type) => {
                    let matching: Vec<pzoom_code_info::TAtomic> = arg_type
                        .types
                        .iter()
                        .filter(|atomic| {
                            crate::type_comparator::union_type_comparator::can_be_contained_by(
                                analyzer.codebase,
                                &TUnion::new((*atomic).clone()),
                                &template_type.as_type,
                            )
                        })
                        .cloned()
                        .collect();
                    if matching.is_empty() || matching.len() == arg_type.types.len() {
                        arg_type.clone()
                    } else {
                        TUnion::from_types(matching)
                    }
                }
                None => TUnion::nothing(),
            }
        } else {
            TUnion::nothing()
        };

        crate::template::lower_bounds_insert(
            template_result,
            template_type.name,
            template_type.defining_entity,
            replacement,
        );
    }
}

fn get_configured_php_major_version(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, _, _) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    major as i64
}

fn get_configured_php_version_id(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, minor, patch) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    (major as i64) * 10_000 + (minor as i64) * 100 + (patch as i64)
}
