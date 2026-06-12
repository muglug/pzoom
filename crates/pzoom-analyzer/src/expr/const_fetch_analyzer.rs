//! Constant fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::ConstantAccess;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze a constant fetch expression.
///
/// Handles global constants like `PHP_VERSION`, `true`, `false`, `null`, etc.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    constant: &ConstantAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let const_name = constant.name.value();
    let name_offset = constant.name.span().start.offset;

    // The standard streams are hardcoded as resources (Psalm's
    // ConstFetchAnalyzer::getGlobalConstType) — their stub declarations are
    // self-referential placeholders (`const STDERR = STDERR;`).
    if matches!(const_name, "STDERR" | "STDOUT" | "STDIN") {
        analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TResource)));
        return;
    }

    // Check for built-in constants first (case-insensitive for true/false/null)
    let result_type = match const_name.to_lowercase().as_str() {
        "true" => {
            analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TTrue)));
            return;
        }
        "false" => {
            analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TFalse)));
            return;
        }
        "null" => {
            analysis_data.expr_types.insert(pos, Rc::new(TUnion::null()));
            return;
        }

        // For other constants (the runtime constants are typed at
        // collection via runtime_global_constant_type; E_* error levels
        // carry literal values from the stubs), try lookup
        _ => {
            // Resolve constant name considering namespace
            if let Some(const_info) = resolve_constant(analyzer, const_name, name_offset, context) {
                analysis_data.expr_types.insert(pos, Rc::new(const_info));
                return;
            }

            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedConstant,
                format!("Constant {} is not defined", const_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            TUnion::mixed()
        }
    };

    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}

/// Resolve a constant by name, considering namespace context.
fn resolve_constant(
    analyzer: &StatementsAnalyzer<'_>,
    name: &str,
    name_offset: u32,
    context: &BlockContext,
) -> Option<TUnion> {
    let normalized_name = name.trim_start_matches('\\');
    let runtime_const_id = analyzer.interner.intern(normalized_name);
    if let Some(runtime_type) = context.defined_constants.get(&runtime_const_id) {
        return Some(runtime_type.clone());
    }

    if let Some(resolved_name) = analyzer.get_resolved_name(name_offset) {
        if let Some(const_info) = analyzer.codebase.constants.get(&resolved_name) {
            return Some(const_info.constant_type.clone());
        }

        let resolved_name_str = analyzer.interner.lookup(resolved_name);
        let normalized_resolved = resolved_name_str.trim_start_matches('\\');
        if normalized_resolved != resolved_name_str.as_ref() {
            let normalized_id = analyzer.interner.intern(normalized_resolved);
            if let Some(const_info) = analyzer.codebase.constants.get(&normalized_id) {
                return Some(const_info.constant_type.clone());
            }
        }
    }

    let is_fully_qualified = name.starts_with('\\');
    let normalized_name = name.trim_start_matches('\\');

    // Try namespace-qualified lookup first
    if !is_fully_qualified && let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, normalized_name);
        let const_id = analyzer.interner.intern(&qualified_name);
        if let Some(const_info) = analyzer.codebase.constants.get(&const_id) {
            return Some(const_info.constant_type.clone());
        }
    }

    // Fall back to the global namespace. PHP falls back only for
    // UNQUALIFIED names: a relative-qualified `A\B` inside `namespace C`
    // resolves solely to `C\A\B` (Psalm's getGlobalConstType fallback uses
    // just the last name part, which likewise never matches `A\B`).
    if !is_fully_qualified && normalized_name.contains('\\') && context.namespace.is_some() {
        return None;
    }
    let const_id = analyzer.interner.intern(normalized_name);
    analyzer
        .codebase
        .constants
        .get(&const_id)
        .map(|c| c.constant_type.clone())
}
