//! Clone expression analyzer.

use pzoom_code_info::class_like_info::{ClassLikeInfo, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator::is_class_subtype_of;
use std::rc::Rc;

/// Analyze clone expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    clone_expr: &mago_syntax::ast::ast::clone::Clone<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let inner_pos =
        expression_analyzer::analyze(analyzer, clone_expr.object, analysis_data, context);
    if let Some(inner_type) = analysis_data.expr_types.get(&inner_pos).cloned() {
        let mut atomic_types = inner_type.types.clone();
        let mut invalid_clones = Vec::new();
        let mut mixed_clone = false;
        let mut immutable_cloned = false;
        let mut possibly_valid = false;

        while let Some(atomic) = atomic_types.pop() {
            let atomic_id = atomic.get_id(Some(analyzer.interner));

            match atomic {
                TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                    mixed_clone = true;
                }
                TAtomic::TObject | TAtomic::TObjectIntersection { .. } => {
                    possibly_valid = true;
                }
                TAtomic::TNamedObject { name, .. } => {
                    let Some(class_id) = resolve_clone_target_class_id(analyzer, name) else {
                        invalid_clones.push(atomic_id);
                        continue;
                    };
                    let Some(class_info) = analyzer.codebase.get_class(class_id) else {
                        invalid_clones.push(atomic_id);
                        continue;
                    };
                    immutable_cloned = true;

                    if let Some(clone_method) = class_info.methods.get(&StrId::CLONE) {
                        if !is_clone_method_visible(analyzer, class_info, clone_method) {
                            invalid_clones.push(atomic_id);
                            continue;
                        }

                        let class_name = analyzer.interner.lookup(class_id);
                        let (line, col) = analyzer.get_line_column(pos.0);

                        if clone_method.is_deprecated {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::DeprecatedMethod,
                                format!("Method {}::__clone is deprecated", class_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        if !can_access_internal(analyzer, &clone_method.internal, Some(context)) {
                            let scope_phrase =
                                format_internal_scope_phrase(analyzer, &clone_method.internal);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InternalMethod,
                                format!(
                                    "The method {}::__clone is internal to {} but called from {}",
                                    class_name,
                                    scope_phrase,
                                    crate::internal_access::format_caller_context(
                                        analyzer,
                                        Some(context),
                                    )
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }
                    }

                    possibly_valid = true;
                }
                TAtomic::TTemplateParam { as_type, .. } => {
                    atomic_types.extend(as_type.types.clone());
                }
                TAtomic::TFalse if inner_type.ignore_falsable_issues => {}
                TAtomic::TNull if inner_type.ignore_nullable_issues => {}
                _ => {
                    invalid_clones.push(atomic_id);
                }
            }
        }

        let issue_offset = analysis_data.current_stmt_start.unwrap_or(pos.0);

        if mixed_clone
            && !crate::issue_suppression::is_issue_suppressed_at(
                analyzer,
                analysis_data,
                issue_offset,
                "MixedClone",
            )
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedClone,
                "Cannot clone mixed",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if let Some(first_invalid_clone) = invalid_clones.first() {
            let (issue_kind, issue_name) = if possibly_valid {
                (IssueKind::PossiblyInvalidClone, "PossiblyInvalidClone")
            } else {
                (IssueKind::InvalidClone, "InvalidClone")
            };

            if !crate::issue_suppression::is_issue_suppressed_at(
                analyzer,
                analysis_data,
                issue_offset,
                issue_name,
            ) {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    issue_kind,
                    format!("Cannot clone {}", first_invalid_clone),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        } else {
            let mut result_type = (*inner_type).clone();
            // Psalm `CloneAnalyzer`: a cloned named object is reference-free
            // and (re-)mutable — the copy is fresh, so property writes on it
            // are pure even in mutation-free contexts.
            if immutable_cloned {
                result_type.reference_free = true;
                result_type.allow_mutations = true;
            }
            analysis_data.expr_types.insert(pos, Rc::new(result_type));
        }
    }
}

fn is_clone_method_visible(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    clone_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let visibility_scope_class_id = class_info
        .appearing_method_ids
        .get(&StrId::CLONE)
        .copied()
        .or(clone_method.declaring_class)
        .unwrap_or(class_info.name);

    match clone_method.visibility {
        Visibility::Public => true,
        Visibility::Private => analyzer
            .get_declaring_class()
            .is_some_and(|calling_class| calling_class == visibility_scope_class_id),
        Visibility::Protected => analyzer.get_declaring_class().is_some_and(|calling_class| {
            calling_class == visibility_scope_class_id
                || is_class_subtype_of(calling_class, visibility_scope_class_id, analyzer.codebase)
        }),
    }
}

fn resolve_clone_target_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> Option<StrId> {
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let class_name = analyzer.interner.lookup(class_id);
    let normalized_name = class_name.trim_start_matches('\\');

    if normalized_name != class_name.as_ref() {
        let normalized_id = analyzer.interner.intern(normalized_name);
        if analyzer.codebase.get_class(normalized_id).is_some() {
            return Some(normalized_id);
        }
    } else {
        let prefixed_name = format!("\\{}", class_name);
        let prefixed_id = analyzer.interner.intern(&prefixed_name);
        if analyzer.codebase.get_class(prefixed_id).is_some() {
            return Some(prefixed_id);
        }
    }

    analyzer
        .codebase
        .classlike_infos
        .keys()
        .copied()
        .find(|candidate_id| {
            analyzer
                .interner
                .lookup(*candidate_id)
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(normalized_name)
        })
}
