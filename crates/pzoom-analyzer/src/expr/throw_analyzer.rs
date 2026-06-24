//! Throw expression analyzer.
//!
//! Modeled after Psalm's ThrowAnalyzer - handles throw expressions and sets
//! the appropriate context flags for control flow analysis.

use mago_syntax::ast::ast::throw::Throw;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;
use std::rc::Rc;

/// Analyze a throw expression.
///
/// This sets `context.has_returned = true` to indicate that control flow
/// will exit at this point (similar to a return statement).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    throw: &Throw<'_>,
    pos: (u32, u32),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Set inside_throw flag while analyzing the thrown expression
    context.inside_throw = true;

    // Analyze the exception expression
    let exception_pos =
        expression_analyzer::analyze(analyzer, throw.exception, analysis_data, context);

    context.inside_throw = false;

    if let Some(throw_type) = analysis_data.expr_types.get(&exception_pos).cloned() {
        let is_valid_throw = !throw_type.types.is_empty()
            && throw_type
                .types
                .iter()
                .all(|atomic| atomic_is_throwable(analyzer, atomic));

        if !is_valid_throw {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidThrow,
                format!(
                    "Cannot throw {}",
                    throw_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Mark that control flow has exited (like Psalm's $context->has_returned = true)
    context.has_returned = true;

    // TODO: Handle finally_scope - combine types with finally scope vars
    // if context.finally_scope.is_some() { ... }

    // TODO: Validate that the thrown expression is Throwable
    // if let Some(throw_type) = analysis_data.expr_types.get(&exception_pos).cloned() {
    //     // Check if throw_type is a subtype of Throwable
    // }

    // Throw expression has type `never` (nothing)
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::nothing()));
}

fn atomic_is_throwable(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            if *name == StrId::THROWABLE
                || object_type_comparator::is_class_subtype_of(
                    *name,
                    StrId::THROWABLE,
                    analyzer.codebase,
                )
            {
                return true;
            }

            let normalized = analyzer.interner.lookup(*name);
            if let Some(stripped) = normalized.strip_prefix('\\') {
                let stripped_id = analyzer
                    .interner
                    .find(stripped)
                    .unwrap_or(pzoom_str::StrId::EMPTY);
                stripped_id == StrId::THROWABLE
                    || object_type_comparator::is_class_subtype_of(
                        stripped_id,
                        StrId::THROWABLE,
                        analyzer.codebase,
                    )
            } else {
                false
            }
        }
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|nested| atomic_is_throwable(analyzer, nested)),
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(|nested| atomic_is_throwable(analyzer, nested)),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_is_throwable(analyzer, as_type),
        _ => false,
    }
}
