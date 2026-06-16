//! Taint metadata scanning: `@psalm-taint-*` / `@psalm-flow` docblock tags
//! and the builtin sink map.
//!
//! Ports the taint handling from Psalm's `FunctionLikeDocblockParser` (tag
//! grammar) + `FunctionLikeDocblockScanner` (application to storage). The
//! builtin sink map (`InternalTaintSinkMap`) is applied at call time in
//! the analyzer, mirroring Hakana's `get_argument_taints`.

use pzoom_code_info::data_flow::node::SinkType;
use pzoom_code_info::functionlike_info::{FunctionLikeTaints, ParamInfo};

use super::DeclarationCollector;
use crate::docblock::ParsedDocblock;

impl DeclarationCollector<'_, '_> {
    /// Parse the `@psalm-taint-*` / `@psalm-flow` tags from a function-like
    /// docblock and apply the param-targeted ones (`taint-sink`,
    /// `assert-untainted`) to `params`. `is_pure` mirrors Psalm forcing
    /// `specialize_call` for `@psalm-pure` functions.
    pub(crate) fn scan_docblock_taints(
        &self,
        parsed: &ParsedDocblock,
        params: &mut [ParamInfo],
        is_pure: bool,
    ) -> (FunctionLikeTaints, Vec<String>) {
        let mut taints = FunctionLikeTaints {
            specialize_call: is_pure || parsed.tags.contains_key("psalm-taint-specialize"),
            ..Default::default()
        };
        let mut raw_conditional_escapes: Vec<String> = Vec::new();

        // @psalm-taint-sink <kind> $param
        if let Some(tags) = parsed.tags.get("psalm-taint-sink") {
            for content in tags.values() {
                let mut parts = content.split_whitespace();
                if let (Some(kind), Some(param_name)) = (parts.next(), parts.next()) {
                    for param in params.iter_mut() {
                        if &*self.interner.lookup(param.name) == param_name {
                            for sink in SinkType::kinds_from_name(kind) {
                                if !param.sinks.contains(&sink) {
                                    param.sinks.push(sink);
                                }
                            }
                        }
                    }
                }
            }
        }

        // @psalm-taint-source <kind>
        if let Some(tags) = parsed.tags.get("psalm-taint-source") {
            for content in tags.values() {
                if let Some(kind) = content.split_whitespace().next() {
                    for sink in SinkType::kinds_from_name(kind) {
                        if !taints.taint_source_types.contains(&sink) {
                            taints.taint_source_types.push(sink);
                        }
                    }
                }
            }
        }

        // @psalm-taint-unescape <kind>
        if let Some(tags) = parsed.tags.get("psalm-taint-unescape") {
            for content in tags.values() {
                if let Some(kind) = content.split_whitespace().next() {
                    for sink in SinkType::kinds_from_name(kind) {
                        if !taints.added_taints.contains(&sink) {
                            taints.added_taints.push(sink);
                        }
                    }
                }
            }
        }

        // @psalm-taint-escape <kind> | @psalm-taint-escape (<conditional>)
        // Conditional escapes are returned as raw strings for the caller to
        // parse with the function-like's full docblock type context
        // (self/parent class, templates, param names).
        if let Some(tags) = parsed.tags.get("psalm-taint-escape") {
            for content in tags.values() {
                let content = content.trim();
                if content.is_empty() {
                    continue;
                }
                if content.starts_with('(') {
                    raw_conditional_escapes.push(content.to_string());
                } else if let Some(kind) = content.split_whitespace().next() {
                    for sink in SinkType::kinds_from_name(kind.trim_matches('\'').trim_matches('"'))
                    {
                        if !taints.removed_taints.contains(&sink) {
                            taints.removed_taints.push(sink);
                        }
                    }
                }
            }
        }

        // @psalm-assert-untainted $param
        if let Some(tags) = parsed.tags.get("psalm-assert-untainted") {
            for content in tags.values() {
                let param_name = content.trim();
                for param in params.iter_mut() {
                    if &*self.interner.lookup(param.name) == param_name {
                        param.assert_untainted = true;
                    }
                }
            }
        }

        // @psalm-flow ($a, $b) -> return  /  @psalm-flow ($a) -(array-fetch)-> return
        if let Some(tags) = parsed.tags.get("psalm-flow") {
            for content in tags.values() {
                self.scan_taint_flow(content, params, &mut taints);
            }
        }

        (taints, raw_conditional_escapes)
    }

    /// Port of Psalm's `FunctionLikeDocblockScanner::handleTaintFlow`.
    fn scan_taint_flow(&self, flow: &str, params: &[ParamInfo], taints: &mut FunctionLikeTaints) {
        let mut path_type = "arg".to_string();
        let mut flow = flow.trim().to_string();

        // `-(array-fetch)->` style fancy paths carry an explicit path type.
        if let Some(open) = flow.find("-(")
            && let Some(close) = flow[open..].find(")->")
        {
            path_type = flow[open + 2..open + close].to_string();
            flow.replace_range(open..open + close + 3, "->");
        }

        let flow_parts: Vec<&str> = flow.split("->").collect();

        if flow_parts.len() > 1 && flow_parts[1].trim() == "return" {
            let source_param_string = flow_parts[0].trim();

            if source_param_string.starts_with('(') && source_param_string.ends_with(')') {
                for source_param in source_param_string[1..source_param_string.len() - 1]
                    .split(',')
                    .map(str::trim)
                {
                    for (i, param) in params.iter().enumerate() {
                        if &*self.interner.lookup(param.name) == source_param {
                            taints.return_source_params.push((i, path_type.clone()));
                        }
                    }
                }
            }
        }

        // Psalm `FunctionLikeDocblockScanner`: `@psalm-flow proxy
        // other_fn($a, $b) [-> return]`.
        if let Some(first_part) = flow_parts.first()
            && let Some(proxy_call) = first_part.trim().strip_prefix("proxy")
            && let Some((fqn, source_param_string)) = proxy_call.trim().split_once('(')
            && !fqn.is_empty()
        {
            let mut call_params = vec![];
            for source_param in source_param_string
                .trim_end_matches(')')
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                for (i, param) in params.iter().enumerate() {
                    if &*self.interner.lookup(param.name) == source_param {
                        call_params.push(i);
                    }
                }
            }

            taints
                .proxy_calls
                .push(pzoom_code_info::functionlike_info::TaintProxyCall {
                    fqn: fqn.trim().trim_start_matches('\\').to_string(),
                    params: call_params,
                    returns: flow_parts.get(1).is_some_and(|p| p.trim() == "return"),
                });
        }
    }
}
