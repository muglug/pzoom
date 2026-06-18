//! Trait declaration analyzer.
//!
//! Mirrors Psalm's `Internal\Analyzer\TraitAnalyzer` (a sibling of
//! `ClassAnalyzer` / `InterfaceAnalyzer`): it runs the trait-specific
//! declaration checks and analyses each trait method. The shared class-like
//! checks live in [`crate::stmt::class_analyzer`] (pzoom's de-facto
//! `ClassLikeAnalyzer` base) and are reused here.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::Trait;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;

use pzoom_code_info::{Issue, IssueKind};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::attribute_analyzer;
use crate::stmt::class_analyzer::{
    analyze_method, check_class_constant_overrides, check_deprecated_and_internal_relationships,
    check_docblock_issues, check_duplicate_constant_declarations,
    check_duplicate_method_declarations, check_duplicate_property_declarations,
    check_extended_template_param_bounds, check_method_docblock_param_type_mismatches,
    check_missing_dependencies, check_missing_property_types, check_pseudo_method_annotations,
    check_pseudo_method_compatibility, check_undefined_docblock_mixins,
    check_undefined_docblock_property_types, collect_dependency_name_spans,
    union_contains_special_class_names,
};

/// Analyze a trait declaration. The enclosing namespace (if any) is read from
/// `context`, so the same entry point serves a top-level or a namespaced trait.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    trait_stmt: &Trait<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the trait name - use FQN if in a namespace
    let trait_name = trait_stmt.name.value;
    let fqn = match context.namespace {
        Some(namespace) => format!("{}\\{}", analyzer.interner.lookup(namespace), trait_name),
        None => trait_name.to_string(),
    };
    let trait_name_id = analyzer.interner.intern(&fqn);

    // Look up the trait info from the codebase
    let trait_info = analyzer.codebase.get_class(trait_name_id);

    attribute_analyzer::analyze_interface_or_trait_attributes(
        analyzer,
        trait_stmt.attribute_lists.as_slice(),
        trait_stmt.members.as_slice(),
        trait_info,
        trait_name_id,
        context,
        analysis_data,
    );

    // PHP < 8.2: traits cannot declare constants (Psalm's
    // ConstantDeclarationInTrait).
    if analyzer.config.php_version_id() < 80200
        && let Some(info) = trait_info
    {
        for const_info in info.constants.values() {
            if const_info.declaring_class != info.name {
                continue;
            }
            let (line, col) = analyzer.get_line_column(const_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ConstantDeclarationInTrait,
                "Traits cannot declare constants until PHP 8.2",
                analyzer.file_path,
                const_info.start_offset,
                const_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }

    // Check for missing property types in trait-declared properties
    if let Some(info) = trait_info {
        let name_span = trait_stmt.name.span();
        let dependency_fallback = (name_span.start.offset, name_span.end.offset);
        let dependency_spans = collect_dependency_name_spans(
            analyzer,
            None,
            None,
            trait_stmt.members.as_slice(),
            context,
        );
        check_missing_dependencies(
            analyzer,
            info,
            context,
            analysis_data,
            &dependency_spans,
            dependency_fallback,
        );
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_duplicate_method_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
        check_missing_property_types(analyzer, &fqn, info, analysis_data);
    }

    for member in trait_stmt.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            let issue_count_before = analysis_data.issues.len();
            analyze_method(
                analyzer,
                method,
                trait_name_id,
                trait_info,
                context.namespace,
                analysis_data,
            )?;

            let method_name_id = analyzer.interner.intern(method.name.value);
            let should_emit_return_mismatch = trait_info
                .and_then(|info| info.methods.get(&method_name_id))
                .and_then(|method_info| method_info.get_return_type())
                .is_some_and(|return_type| !union_contains_special_class_names(return_type));

            let new_issues = analysis_data.issues.split_off(issue_count_before);
            let filtered_issues: Vec<_> = new_issues
                .into_iter()
                .filter_map(|mut issue| {
                    // Purity and readonly/visibility diagnostics follow from the
                    // method's own mutation-free contract and from local / `$this`
                    // writes, not from the using class, so Psalm emits them while
                    // analysing the trait and the trait file's @psalm-suppress
                    // annotations apply here. Surface them directly (the
                    // per-using-class reference pass discards its issue copies).
                    if matches!(
                        issue.kind,
                        IssueKind::ImpureMethodCall
                            | IssueKind::ImpureFunctionCall
                            | IssueKind::ImpurePropertyAssignment
                            | IssueKind::ImpurePropertyFetch
                            | IssueKind::ImpureStaticProperty
                            | IssueKind::ImpureStaticVariable
                            | IssueKind::ImpureVariable
                            | IssueKind::ImpureByReferenceAssignment
                            | IssueKind::InaccessibleProperty
                    ) {
                        return Some(issue);
                    }

                    if !should_emit_return_mismatch {
                        return None;
                    }

                    // Psalm guards the per-`return` diagnostics behind
                    // `!($source->getSource() instanceof TraitAnalyzer)`, so a
                    // trait body reports only the overall declared-vs-inferred
                    // return-TYPE mismatch (InvalidReturnType), never the
                    // statement-level one. Keep InvalidReturnType as-is and drop
                    // InvalidReturnStatement — remapping it produced a duplicate
                    // InvalidReturnType alongside the genuine one.
                    if !matches!(issue.kind, IssueKind::InvalidReturnType) {
                        return None;
                    }

                    Some(issue)
                })
                .collect();

            analysis_data.issues.extend(filtered_issues);
        }
    }

    Ok(())
}
