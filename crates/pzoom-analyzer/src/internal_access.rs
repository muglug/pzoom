use crate::context::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use pzoom_str::StrId;

pub(crate) fn can_access_internal(
    analyzer: &StatementsAnalyzer<'_>,
    internal_scopes: &[StrId],
    context: Option<&BlockContext>,
) -> bool {
    if internal_scopes.is_empty() {
        return true;
    }

    let caller_class = analyzer
        .get_declaring_class()
        .map(|class_id| analyzer.interner.lookup(class_id).to_string());
    let caller_namespace = context
        .and_then(|ctx| ctx.namespace)
        .map(|namespace_id| analyzer.interner.lookup(namespace_id).to_string())
        .or_else(|| caller_class.as_deref().map(extract_namespace));
    let caller_method = analyzer.function_info.and_then(|function_info| {
        function_info.declaring_class.map(|class_id| {
            format!(
                "{}::{}",
                analyzer.interner.lookup(class_id),
                analyzer.interner.lookup(function_info.name)
            )
        })
    });

    for scope_id in internal_scopes {
        let scope = analyzer.interner.lookup(*scope_id);
        if scope_matches_caller(
            scope.as_ref(),
            caller_namespace.as_deref(),
            caller_class.as_deref(),
            caller_method.as_deref(),
        ) {
            return true;
        }
    }

    false
}

pub(crate) fn format_internal_scope_phrase(
    analyzer: &StatementsAnalyzer<'_>,
    internal_scopes: &[StrId],
) -> String {
    if internal_scopes.is_empty() {
        return "root namespace".to_string();
    }

    let mut scopes: Vec<String> = internal_scopes
        .iter()
        .map(|scope_id| {
            let scope = analyzer.interner.lookup(*scope_id);
            if scope.is_empty() {
                "root namespace".to_string()
            } else {
                scope.to_string()
            }
        })
        .collect();
    scopes.sort();
    scopes.dedup();

    match scopes.len() {
        0 => "root namespace".to_string(),
        1 => scopes.pop().unwrap(),
        2 => format!("{} or {}", scopes[0], scopes[1]),
        _ => {
            let last = scopes.pop().unwrap();
            format!("{} or {}", scopes.join(", "), last)
        }
    }
}

pub(crate) fn format_caller_context(
    analyzer: &StatementsAnalyzer<'_>,
    context: Option<&BlockContext>,
) -> String {
    if let Some(class_id) = analyzer.get_declaring_class() {
        return analyzer.interner.lookup(class_id).to_string();
    }

    if let Some(namespace_id) = context.and_then(|ctx| ctx.namespace) {
        let namespace = analyzer.interner.lookup(namespace_id);
        if !namespace.is_empty() {
            return namespace.to_string();
        }
    }

    "root namespace".to_string()
}

pub(crate) fn can_class_access_internal(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    internal_scopes: &[StrId],
) -> bool {
    if internal_scopes.is_empty() {
        return true;
    }

    let class_name = analyzer.interner.lookup(class_id);
    let class_name = class_name.as_ref();
    let class_namespace = extract_namespace(class_name);

    for scope_id in internal_scopes {
        let scope = analyzer.interner.lookup(*scope_id);
        if scope_matches_caller(
            scope.as_ref(),
            Some(class_namespace.as_str()),
            Some(class_name),
            None,
        ) {
            return true;
        }
    }

    false
}

fn scope_matches_caller(
    scope: &str,
    caller_namespace: Option<&str>,
    caller_class: Option<&str>,
    caller_method: Option<&str>,
) -> bool {
    let normalized_scope = scope.trim().trim_start_matches('\\');
    if normalized_scope.is_empty() {
        return caller_namespace.is_none_or(str::is_empty);
    }

    if normalized_scope.contains("::") {
        return caller_method.is_some_and(|method| method.eq_ignore_ascii_case(normalized_scope));
    }

    if let Some(class_name) = caller_class {
        if class_name.eq_ignore_ascii_case(normalized_scope) {
            return true;
        }
    }

    if let Some(namespace) = caller_namespace {
        return namespace_equals_or_contains(namespace, normalized_scope);
    }

    false
}

fn namespace_equals_or_contains(namespace: &str, scope: &str) -> bool {
    if namespace.eq_ignore_ascii_case(scope) {
        return true;
    }

    let lower_namespace = namespace.to_ascii_lowercase();
    let lower_scope = scope.to_ascii_lowercase();
    lower_namespace.starts_with(&(lower_scope + "\\"))
}

fn extract_namespace(class_name: &str) -> String {
    class_name
        .rsplit_once('\\')
        .map(|(namespace, _)| namespace.to_string())
        .unwrap_or_default()
}
