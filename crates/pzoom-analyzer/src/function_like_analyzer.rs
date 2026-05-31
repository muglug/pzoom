//! Function-like analyzer.
//!
//! Shared home for the cross-cutting concerns of analyzing a function, method,
//! closure or arrow function body, mirroring Psalm's `FunctionLikeAnalyzer`.
//!
//! Psalm's `FunctionLikeAnalyzer` carries inferred purity/mutation state
//! (`inferred_impure`, `inferred_has_mutation`) that the statement analyzers set
//! as they walk the body, then finalizes a closure's `is_pure` from it
//! (`FunctionLikeAnalyzer.php:672` — `$new_closure_is_pure = !$this->inferred_impure`).
//! pzoom doesn't yet thread those flags through every impure-operation site, so
//! it reconstructs the same signal from the impurity *issues* emitted while
//! analyzing the body. This module owns that reconstruction so closures, arrow
//! functions and (eventually) named functions/methods share one implementation.

use mago_syntax::ast::ast::statement::Statement;

use pzoom_code_info::IssueKind;

use crate::function_analysis_data::FunctionAnalysisData;

/// Tracks whether the function-like body analyzed under this analyzer performed
/// an operation that makes it impure. Mirrors the relevant subset of Psalm's
/// `FunctionLikeAnalyzer` mutation/purity bookkeeping.
#[derive(Debug, Default)]
pub struct InferredPurity {
    /// Whether the body was observed to perform an impure operation.
    pub inferred_impure: bool,
}

/// Returns true when an issue kind denotes an impure operation in a body that
/// is being purity-inferred (i.e. one that would make the enclosing closure
/// impure). Mirrors the impurity issues Psalm raises while `track_mutations`.
pub(crate) fn is_impure_issue_kind(kind: IssueKind) -> bool {
    matches!(
        kind,
        IssueKind::ImpureFunctionCall
            | IssueKind::ImpureMethodCall
            | IssueKind::ImpurePropertyAssignment
            | IssueKind::ImpurePropertyFetch
            | IssueKind::ImpureStaticProperty
            | IssueKind::ImpureStaticVariable
            | IssueKind::ImpureVariable
            | IssueKind::ImpureByReferenceAssignment
    )
}

/// Inspect (and optionally remove) the issues emitted while analyzing a
/// function-like body to determine whether it performed an impure operation.
///
/// `issue_count_before` is the issue count captured just before the body was
/// analyzed. When `retain_impure_issues` is false (we are *inferring* purity
/// rather than enforcing a declared `@psalm-pure`), the impurity issues are
/// dropped so they don't surface on an un-annotated closure — only the
/// resulting `inferred_impure` signal is kept. Returns whether an impure
/// operation was observed.
pub(crate) fn strip_inferred_impure_issues(
    analysis_data: &mut FunctionAnalysisData,
    issue_count_before: usize,
    retain_impure_issues: bool,
) -> bool {
    if analysis_data.issues.len() == issue_count_before {
        return false;
    }

    let new_issues = analysis_data.issues.split_off(issue_count_before);
    let mut filtered = Vec::with_capacity(new_issues.len());
    let mut saw_impure_issue = false;

    for issue in new_issues {
        if is_impure_issue_kind(issue.kind) {
            saw_impure_issue = true;
            if retain_impure_issues {
                filtered.push(issue);
            }
        } else {
            filtered.push(issue);
        }
    }

    analysis_data.issues.extend(filtered);
    saw_impure_issue
}

/// Whether a function-like body contains statements with obvious side effects
/// (echo, unset, global, static) that make it impure regardless of the issues
/// emitted. Mirrors the side-effecting statement checks Psalm performs.
pub(crate) fn body_has_obvious_side_effect_statements(statements: &[Statement<'_>]) -> bool {
    statements.iter().any(|statement| {
        matches!(
            statement,
            Statement::Echo(_)
                | Statement::EchoTag(_)
                | Statement::Unset(_)
                | Statement::Global(_)
                | Statement::Static(_)
        )
    })
}
