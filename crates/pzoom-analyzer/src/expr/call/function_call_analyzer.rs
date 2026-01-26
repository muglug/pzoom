//! Function call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator;

use super::argument_analyzer;

/// Analyze a function call expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the callee expression to get the function name
    let _callee_pos = expr_analyzer::analyze(analyzer, func_call.function, analysis_data, context);

    // Collect argument positions first
    let mut arg_positions = Vec::new();
    for arg in func_call.argument_list.arguments.iter() {
        let arg_pos = argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        arg_positions.push(arg_pos);
    }

    // Try to get the function name and whether it's fully qualified
    let (func_name, is_fq) = get_function_name(func_call.function);

    // Try to look up function return type
    if let Some(name) = func_name {
        // Resolve the function name considering namespace context
        let func_info = resolve_function(analyzer, name, is_fq, context);

        if let Some(func_info) = func_info {
            // Check for deprecated functions
            if func_info.is_deprecated {
                let (line, col) = analyzer.get_line_column(pos.0);
                let message = func_info
                    .deprecation_message
                    .as_ref()
                    .map(|m| format!("Function {} is deprecated: {}", name, m))
                    .unwrap_or_else(|| format!("Function {} is deprecated", name));
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedFunction,
                    message,
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Verify argument types against function parameters
            verify_arguments(
                analyzer,
                func_call,
                &arg_positions,
                func_info,
                name,
                analysis_data,
                context,
            );

            // Use the function's return type if available
            if let Some(return_type) = &func_info.return_type {
                analysis_data.set_expr_type(pos, return_type.clone());
                return;
            }
        } else {
            // Function not found in codebase
            // Don't emit error for language constructs that look like functions
            if !is_language_construct(name) {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedFunction,
                    format!("Function {} is not defined", name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Verify argument types against function parameter types.
fn verify_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    func_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
) {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();

    // Check if any argument is unpacked (spread operator)
    let has_spread = args.iter().any(|arg| arg.is_unpacked());

    // Count required parameters (non-optional, non-variadic)
    let required_params = func_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    // Check if we have enough arguments (skip if there's a spread, as we can't know statically)
    if !has_spread && args.len() < required_params {
        let span = func_call.argument_list.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to function {}, {} expected, {} provided",
                func_name,
                required_params,
                args.len()
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    // Verify each argument type
    for (idx, arg) in args.iter().enumerate() {
        // Skip type checking for spread arguments - they unpack into multiple arguments
        if arg.is_unpacked() {
            continue;
        }

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        // Get the corresponding parameter (handle variadic params)
        let param = if idx < func_info.params.len() {
            Some(&func_info.params[idx])
        } else {
            // Check if last param is variadic
            func_info.params.last().filter(|p| p.is_variadic)
        };

        if let Some(param) = param {
            // Check by-reference arguments
            if param.by_ref {
                if !is_valid_by_ref_arg(arg) {
                    let span = arg.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidPassByReference,
                        format!(
                            "Argument {} of {} is passed by reference, but the provided value is not a variable",
                            idx + 1,
                            func_name
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }

            // Check argument type compatibility
            // Only check if param has a declared type
            if let (Some(arg_type), Some(param_type)) = (analysis_data.get_expr_type(arg_pos), param.get_type()) {
                // Skip if param accepts mixed
                if param_type.is_mixed() {
                    continue;
                }

                // Skip validation if argument is mixed (would produce MixedArgument)
                // For now, suppress mixed issues as they create too many false positives
                // without docblock parsing
                if arg_type.is_mixed() {
                    continue;
                }

                // Check for null being passed to non-nullable param
                if arg_type.is_nullable && !param_type.is_nullable && !param_type.is_mixed() {
                    // Check if arg is only null
                    if arg_type.types.len() == 1
                        && matches!(arg_type.types.first(), Some(pzoom_code_info::TAtomic::TNull))
                    {
                        let span = arg.span();
                        let (line, col) = analyzer.get_line_column(span.start.offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::NullArgument,
                            format!(
                                "Argument {} of {} cannot be null, {} expected",
                                idx + 1,
                                func_name,
                                param_type.get_id()
                            ),
                            analyzer.file_path,
                            span.start.offset,
                            span.end.offset,
                            line,
                            col,
                        ));
                        continue;
                    }
                }

                // Check type compatibility with class hierarchy awareness
                if !type_comparator::is_contained_by_with_codebase(&arg_type, param_type, analyzer.codebase) {
                    let span = arg.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidArgument,
                        format!(
                            "Argument {} of {} expects {}, {} provided",
                            idx + 1,
                            func_name,
                            param_type.get_id(),
                            arg_type.get_id()
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }
        }
    }
}

/// Check if an argument can be passed by reference.
fn is_valid_by_ref_arg(arg: &Argument<'_>) -> bool {
    let expr = arg.value();
    // Assignment expressions like `$x = null` evaluate to a variable reference,
    // so they're valid for by-ref params
    matches!(
        expr,
        Expression::Variable(_)
            | Expression::ArrayAccess(_)
            | Expression::Access(_)
            | Expression::Assignment(_)
    )
}

/// Resolve a function by name, considering namespace context.
///
/// PHP function resolution:
/// 1. If fully qualified (starts with \), use it directly
/// 2. If unqualified, first try current_namespace\function_name
/// 3. Fall back to global namespace
fn resolve_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    is_fully_qualified: bool,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    if is_fully_qualified {
        // Strip leading backslash and look up directly
        let clean_name = name.strip_prefix('\\').unwrap_or(name);
        let func_id = analyzer.interner.intern(clean_name);
        return analyzer.codebase.get_function(func_id);
    }

    // Try namespace-qualified lookup first
    if let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, name);
        let func_id = analyzer.interner.intern(&qualified_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }
    }

    // Fall back to global namespace
    let func_id = analyzer.interner.intern(name);
    analyzer.codebase.get_function(func_id)
}

/// Extract the function name from a function call expression.
/// Returns (name, is_fully_qualified).
fn get_function_name<'a>(expr: &'a Expression<'a>) -> (Option<&'a str>, bool) {
    match expr {
        Expression::Identifier(id) => (Some(id.value()), id.is_fully_qualified()),
        _ => (None, false),
    }
}

/// Check if a name is a PHP language construct (not a real function).
///
/// These are special syntax that look like function calls but are actually
/// language constructs handled by the parser/compiler. They won't be in stubs.
fn is_language_construct(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        // Output constructs
        "echo"
            | "print"
            // Program termination
            | "die"
            | "exit"
            // Variable inspection (actually functions, but often not in stubs)
            | "isset"
            | "unset"
            | "empty"
            // Include/require (handled separately but can appear as function-like)
            | "include"
            | "include_once"
            | "require"
            | "require_once"
            // Evaluation
            | "eval"
            // List assignment
            | "list"
            // Array literal (not really a function)
            | "array"
    )
}
