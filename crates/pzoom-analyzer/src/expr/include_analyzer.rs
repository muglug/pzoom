//! Include/require expression analyzer.

use mago_syntax::ast::ast::construct::{
    IncludeConstruct, IncludeOnceConstruct, RequireConstruct, RequireOnceConstruct,
};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze an include expression.
pub fn analyze_include(
    analyzer: &StatementsAnalyzer<'_>,
    include: &IncludeConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, include.value, pos, analysis_data, context, false);
}

/// Analyze an include_once expression.
pub fn analyze_include_once(
    analyzer: &StatementsAnalyzer<'_>,
    include: &IncludeOnceConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, include.value, pos, analysis_data, context, false);
}

/// Analyze a require expression.
pub fn analyze_require(
    analyzer: &StatementsAnalyzer<'_>,
    require: &RequireConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, require.value, pos, analysis_data, context, true);
}

/// Analyze a require_once expression.
pub fn analyze_require_once(
    analyzer: &StatementsAnalyzer<'_>,
    require: &RequireOnceConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    analyze_path(analyzer, require.value, pos, analysis_data, context, true);
}

/// Analyze the path argument of an include/require expression.
fn analyze_path(
    analyzer: &StatementsAnalyzer<'_>,
    path: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    _is_require: bool,
) {
    // Analyze the path expression (general use — Hakana's include_analyzer).
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let path_pos = expression_analyzer::analyze(analyzer, path, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    // Get the path type
    if let Some(path_type) = analysis_data.expr_types.get(&path_pos).cloned() {
        // Psalm `IncludeAnalyzer`: the include path is an `include`
        // taint sink (TaintedInclude when user input reaches it).
        if analyzer.config.taint_analysis {
            crate::expr::output_constructs::add_construct_argument_dataflow(
                analyzer,
                "include",
                &[pzoom_code_info::data_flow::node::SinkType::Include],
                0,
                path_pos,
                &path_type,
                pos,
                analysis_data,
                context,
            );
        }
    }

    // Psalm's IncludeAnalyzer: resolve the path expression to a file statically
    // (getPathTo). A path that cannot be resolved is UnresolvableInclude; a
    // resolvable one that does not exist on disk is MissingFile.
    match get_path_to(analyzer, path, analysis_data) {
        None => {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnresolvableInclude,
                "Cannot resolve the given expression to a file path".to_string(),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
        Some(path_to_file) => {
            if include_path_exists(analyzer, &path_to_file) == Some(false) {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MissingFile,
                    format!("Cannot find file {path_to_file} to include"),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    // include/require returns the return value of the included file,
    // or 1 on success, false on failure (for include)
    // For simplicity, we return mixed since we don't track included file returns
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::mixed()));
}

/// Port of Psalm's `IncludeAnalyzer::getPathTo`: statically resolve an
/// include-path expression to a file path, or `None` if it can't be resolved.
fn get_path_to(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
) -> Option<String> {
    use mago_syntax::ast::ast::binary::BinaryOperator;
    use mago_syntax::ast::ast::call::Call;
    use mago_syntax::ast::ast::magic_constant::MagicConstant;

    let expr = expr.unparenthesized();

    // A statically-known single literal string covers string literals, narrowed
    // variables, folded concatenations, `dirname()` of a literal, and string
    // constants — Psalm's `$stmt_type->isSingleStringLiteral()`.
    let span = mago_span::HasSpan::span(expr);
    if let Some(path_type) = analysis_data
        .expr_types
        .get(&(span.start.offset, span.end.offset))
        && let Some(TAtomic::TLiteralString { value }) = path_type.get_single()
    {
        return Some(value.clone());
    }

    match expr {
        // `$a . $b`
        Expression::Binary(binop) if matches!(binop.operator, BinaryOperator::StringConcat(_)) => {
            let left = get_path_to(analyzer, binop.lhs, analysis_data)?;
            let right = get_path_to(analyzer, binop.rhs, analysis_data)?;
            Some(format!("{left}{right}"))
        }
        // `dirname($path[, $levels])`
        Expression::Call(Call::Function(func_call))
            if matches!(
                func_call.function.unparenthesized(),
                Expression::Identifier(id) if id.value().eq_ignore_ascii_case("dirname")
            ) =>
        {
            let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
            let first = args.first()?;
            let mut levels: i64 = 1;
            if let Some(second) = args.get(1) {
                let level_span = mago_span::HasSpan::span(second.value());
                match analysis_data
                    .expr_types
                    .get(&(level_span.start.offset, level_span.end.offset))
                    .and_then(|t| t.get_single())
                {
                    Some(TAtomic::TLiteralInt { value }) => levels = *value,
                    _ => return None,
                }
            }
            if levels < 1 {
                return None;
            }
            let base = get_path_to(analyzer, first.value(), analysis_data)?;
            Some(apply_dirname(&base, levels as usize))
        }
        // `__DIR__` / `__FILE__` resolve against the including file.
        Expression::MagicConstant(MagicConstant::Directory(_)) => Some(apply_dirname(
            &analyzer.interner.lookup(analyzer.file_path),
            1,
        )),
        Expression::MagicConstant(MagicConstant::File(_)) => {
            Some(analyzer.interner.lookup(analyzer.file_path).to_string())
        }
        _ => None,
    }
}

/// PHP's `dirname($path, $levels)` — strip the last `levels` path components.
fn apply_dirname(path: &str, levels: usize) -> String {
    let mut buf = std::path::PathBuf::from(path);
    for _ in 0..levels {
        if !buf.pop() {
            break;
        }
    }
    let result = buf.to_string_lossy();
    if result.is_empty() {
        ".".to_string()
    } else {
        result.into_owned()
    }
}

/// Whether a resolved include path exists on disk. Returns `None` when
/// existence can't be determined (so MissingFile is not reported on a guess) —
/// a relative path is probed against both the including file's directory and
/// the working directory (PHP's include-path conventions).
fn include_path_exists(analyzer: &StatementsAnalyzer<'_>, path: &str) -> Option<bool> {
    use std::path::Path;

    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Some(candidate.exists());
    }

    if candidate.exists() {
        return Some(true);
    }

    let current = analyzer.interner.lookup(analyzer.file_path);
    let current_dir = Path::new(&*current).parent()?;
    Some(current_dir.join(path).exists())
}
