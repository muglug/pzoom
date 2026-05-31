//! Static statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::r#static::Static;
use pzoom_code_info::{Issue, IssueKind, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a `static $var` statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    static_stmt: &Static<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let stmt_offset = static_stmt.span().start.offset;

    // Mirrors Psalm `StaticAnalyzer`: a static variable is persistent state across calls,
    // so declaring one in a mutation-free context (`$context->mutation_free`) is impure.
    if crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer) {
        let span = static_stmt.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::ImpureStaticVariable,
            "Cannot use a static variable in a mutation-free context",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    for item in static_stmt.items.iter() {
        let var_id = analyzer.interner.intern(item.variable().name);
        context.static_var_ids.insert(var_id);

        let default_type = if let Some(default_expr) = item.value() {
            let expr_pos =
                expression_analyzer::analyze(analyzer, default_expr, analysis_data, context);
            analysis_data
                .get_expr_type(expr_pos)
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed)
        } else {
            TUnion::mixed()
        };

        let mut var_type = TUnion::mixed();
        if let Some(annotation_type) =
            get_static_var_annotation_type(analyzer, stmt_offset as u32, var_id)
        {
            if let Some(default_expr) = item.value() {
                let mut comparison = TypeComparisonResult::new();
                let default_is_contained = union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &default_type,
                    &annotation_type,
                    false,
                    false,
                    &mut comparison,
                );

                if !default_is_contained {
                    let default_span = default_expr.span();
                    let (line, col) = analyzer.get_line_column(default_span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ReferenceConstraintViolation,
                        format!(
                            "${} violates a by-reference type constraint",
                            analyzer.interner.lookup(var_id)
                        ),
                        analyzer.file_path,
                        default_span.start.offset,
                        default_span.end.offset,
                        line,
                        col,
                    ));
                }
            }

            context.add_reference_constraint(var_id, annotation_type.clone());
            var_type = annotation_type;
        }

        context.set_var_type(var_id, var_type);
    }
}

fn get_static_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    offset: u32,
    var_id: StrId,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(offset)?;

    for annotation in annotations {
        match annotation.var_name {
            Some(name) if name == var_id => return Some(annotation.var_type.clone()),
            None => return Some(annotation.var_type.clone()),
            _ => {}
        }
    }

    None
}
