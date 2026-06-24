//! Casing hints for case-sensitive name resolution.
//!
//! pzoom resolves class, function and method names case-sensitively
//! (deliberately stricter than PHP and Psalm, which resolve them
//! case-insensitively). A wrong-cased reference is therefore undefined — but
//! the diagnostics should name the correctly-cased symbol so the fix is
//! obvious. The lookups here are O(1) via the lowercase-name maps built during
//! the populate phase.

use pzoom_str::StrId;

use crate::statements_analyzer::StatementsAnalyzer;

/// The correctly-cased classlike for a reference that failed exact lookup,
/// when one exists differing only by case.
pub fn class_casing_hint(analyzer: &StatementsAnalyzer<'_>, requested: StrId) -> Option<StrId> {
    analyzer
        .codebase
        .cased_classlike_for(analyzer.interner, requested)
}

/// "Class `foo` does not exist" message, naming the correctly-cased classlike
/// when the reference differs from a declaration only by case.
pub fn undefined_class_message(
    analyzer: &StatementsAnalyzer<'_>,
    requested_name: impl AsRef<str>,
) -> String {
    let requested_name = requested_name.as_ref();
    let requested = analyzer
        .interner
        .find(requested_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    match class_casing_hint(analyzer, requested) {
        Some(cased) => format!(
            "Class {} does not exist (incorrect casing of {})",
            requested_name,
            analyzer.interner.lookup(cased)
        ),
        None => format!("Class {} does not exist", requested_name),
    }
}

/// "Docblock class `foo` does not exist" message, naming the correctly-cased
/// classlike when the reference differs from a declaration only by case.
pub fn undefined_docblock_class_message(
    analyzer: &StatementsAnalyzer<'_>,
    requested: StrId,
) -> String {
    let requested_name = analyzer.interner.lookup(requested);
    match class_casing_hint(analyzer, requested) {
        Some(cased) => format!(
            "Docblock class {} does not exist (incorrect casing of {})",
            requested_name,
            analyzer.interner.lookup(cased)
        ),
        None => format!("Docblock class {} does not exist", requested_name),
    }
}

/// "Function `BAZ` is not defined" message, naming the correctly-cased
/// function when the reference differs from a declaration only by case.
pub fn undefined_function_message(
    analyzer: &StatementsAnalyzer<'_>,
    name: impl AsRef<str>,
    namespace: Option<StrId>,
) -> String {
    let name = name.as_ref();
    let clean = name.trim_start_matches('\\');

    let mut hint = namespace.and_then(|ns_id| {
        let qualified = format!("{}\\{}", analyzer.interner.lookup(ns_id), clean);
        analyzer.codebase.cased_functionlike_for(
            analyzer.interner,
            analyzer
                .interner
                .find(&qualified)
                .unwrap_or(pzoom_str::StrId::EMPTY),
        )
    });
    if hint.is_none() {
        hint = analyzer.codebase.cased_functionlike_for(
            analyzer.interner,
            analyzer
                .interner
                .find(clean)
                .unwrap_or(pzoom_str::StrId::EMPTY),
        );
    }

    match hint {
        Some(cased) => format!(
            "Function {} is not defined (incorrect casing of {})",
            name,
            analyzer.interner.lookup(cased)
        ),
        None => format!("Function {} is not defined", name),
    }
}

/// "Method `Foo::BAR` does not exist" message, naming the correctly-cased
/// method when the reference differs from a declaration only by case.
pub fn undefined_method_message(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: impl AsRef<str>,
    method_name: impl AsRef<str>,
) -> String {
    let class_name = class_name.as_ref();
    let method_name = method_name.as_ref();

    let hint = analyzer
        .codebase
        .get_class(
            analyzer
                .interner
                .find(class_name)
                .unwrap_or(pzoom_str::StrId::EMPTY),
        )
        .and_then(|class_info| {
            class_info.cased_method_for(
                analyzer.interner,
                analyzer
                    .interner
                    .find(method_name)
                    .unwrap_or(pzoom_str::StrId::EMPTY),
            )
        });

    match hint {
        Some(cased) => format!(
            "Method {}::{} does not exist (incorrect casing of {})",
            class_name,
            method_name,
            analyzer.interner.lookup(cased)
        ),
        None => format!("Method {}::{} does not exist", class_name, method_name),
    }
}
