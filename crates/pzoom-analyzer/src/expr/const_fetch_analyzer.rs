//! Constant fetch analyzer.

use mago_syntax::ast::ast::access::ConstantAccess;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

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

    // Check for built-in constants first (case-insensitive for true/false/null)
    let result_type = match const_name.to_lowercase().as_str() {
        "true" => {
            analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TTrue));
            return;
        }
        "false" => {
            analysis_data.set_expr_type(pos, TUnion::new(TAtomic::TFalse));
            return;
        }
        "null" => {
            analysis_data.set_expr_type(pos, TUnion::null());
            return;
        }

        // PHP version constants
        "php_version" => TUnion::string(),
        "php_major_version" | "php_minor_version" | "php_release_version" | "php_version_id"
        | "php_int_max" | "php_int_min" | "php_int_size" | "php_float_dig" | "php_maxpathlen" => {
            TUnion::int()
        }
        "php_float_epsilon" | "php_float_max" | "php_float_min" => TUnion::float(),

        // OS constants
        "php_os" | "php_os_family" | "php_eol" | "directory_separator" | "path_separator" => {
            TUnion::string()
        }

        // Boolean-ish constants
        "php_debug" | "php_zts" => TUnion::bool(),

        // Common constants from stubs - check hardcoded list first
        "e_all" | "e_error" | "e_warning" | "e_parse" | "e_notice" | "e_strict" | "e_deprecated"
        | "e_core_error" | "e_core_warning" | "e_compile_error" | "e_compile_warning"
        | "e_user_error" | "e_user_warning" | "e_user_notice" | "e_user_deprecated"
        | "e_recoverable_error" => TUnion::int(),

        // For other constants, try lookup
        _ => {
            // Resolve constant name considering namespace
            if let Some(const_info) = resolve_constant(analyzer, const_name, context) {
                analysis_data.set_expr_type(pos, const_info);
                return;
            }

            // Constant not found - for now, just return mixed without error
            // TODO: Enable this once @psalm-suppress annotation parsing is implemented
            // analysis_data.add_issue(Issue::new(
            //     IssueKind::UndefinedConstant,
            //     format!("Constant {} is not defined", const_name),
            //     analyzer.file_path,
            //     pos.0,
            //     pos.1,
            //     0,
            //     0,
            // ));
            TUnion::mixed()
        }
    };

    analysis_data.set_expr_type(pos, result_type);
}

/// Resolve a constant by name, considering namespace context.
fn resolve_constant(
    analyzer: &StatementsAnalyzer<'_>,
    name: &str,
    context: &BlockContext,
) -> Option<TUnion> {
    // Try namespace-qualified lookup first
    if let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, name);
        let const_id = analyzer.interner.intern(&qualified_name);
        if let Some(const_info) = analyzer.codebase.constants.get(&const_id) {
            return Some(const_info.constant_type.clone());
        }
    }

    // Fall back to global namespace
    let const_id = analyzer.interner.intern(name);
    analyzer
        .codebase
        .constants
        .get(&const_id)
        .map(|c| c.constant_type.clone())
}
