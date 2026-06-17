//! Method-call purity checks. Mirrors Psalm `MethodCallPurityAnalyzer`, which
//! reports an `ImpureMethodCall` when a non-mutation-free method is called from
//! a pure / mutation-free context.
//!
//! pzoom models the calling context's purity with a single
//! `enforce_mutation_free` flag rather than Psalm's separate
//! `pure`/`mutation_free`/`external_mutation_free` context modes, so it emits
//! the single mutation-free message.

use pzoom_code_info::{FunctionLikeInfo, Issue, IssueKind};
use pzoom_str::StrId;

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Report `ImpureMethodCall` for a possibly-mutating method called from a
/// mutation-free context.
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_info: &FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    pos: Pos,
    enforce_mutation_free: bool,
    receiver_is_pure_compatible: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    // An `@psalm-immutable` / `@psalm-external-mutation-free` class runs its
    // constructor in Psalm's `external_mutation_free` context (FunctionLikeAnalyzer
    // sets `$context->external_mutation_free` with no `__construct` exemption,
    // unlike `mutation_free`). `is_mutation_free_context` exempts every
    // constructor, so detect this case here: it enforces method-call purity but,
    // per Psalm's external-mutation-free branch, allows calls to the class's own
    // methods (`$method_id->fq_class_name !== $context->self`).
    let external_mutation_free_constructor = analyzer.function_info.is_some_and(|fi| {
        fi.name == StrId::CONSTRUCT
            && !fi.is_static
            && !fi.mutation_free_inferred
            && fi
                .declaring_class
                .and_then(|class_id| analyzer.codebase.get_class(class_id))
                .is_some_and(|class_info| {
                    class_info.is_immutable || class_info.is_external_mutation_free
                })
    });

    if !enforce_mutation_free && !external_mutation_free_constructor {
        return;
    }

    let Some(class_info) = analyzer.codebase.get_class(class_id) else {
        return;
    };

    // Psalm's `$method_pure_compatible`: calling an externally-mutation-free
    // class's method on a reference-free receiver can't mutate external state,
    // so it's allowed even though the method itself isn't mutation-free.
    if method_info.is_external_mutation_free && receiver_is_pure_compatible {
        return;
    }

    // External-mutation-free mode (here, the immutable constructor) permits
    // mutating `$this`: a call to a method declared on the same class is fine.
    if external_mutation_free_constructor
        && !enforce_mutation_free
        && method_info.declaring_class == analyzer.function_info.and_then(|fi| fi.declaring_class)
    {
        return;
    }

    if !method_is_mutation_free(method_info, class_info) {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ImpureMethodCall,
            format!(
                "Cannot call a possibly-mutating method {}::{} from a mutation-free context",
                class_name, method_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }
}

pub(crate) fn method_is_mutation_free(
    method_info: &pzoom_code_info::FunctionLikeInfo,
    _class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    // Class-level @psalm-immutable already propagated onto eligible methods
    // in the populator, so the method flags are authoritative here.
    method_info.is_pure || method_info.is_mutation_free
}
