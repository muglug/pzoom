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
use pzoom_str::StrId;

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

    // Psalm analyses each trait method body once per using class, with
    // `$this`/`self` bound to that class, and emits the resulting diagnostics at
    // the trait-file location (its IssueBuffer deduplicates the per-class copies).
    // Drive the body with each direct user's context so member accesses resolve
    // against the real property / method types — analysing the bare trait once
    // would type `$this` as the trait, where members are unknown, which forced a
    // lossy kind allowlist that dropped ordinary local diagnostics. Emission
    // stays in the trait file's own analysis so the trait's `@psalm-suppress` /
    // findUnusedPsalmSuppress accounting still applies, and `add_issue` collapses
    // the duplicate issues each user produces at the same trait-file position.
    let mut direct_users: Vec<StrId> = analyzer
        .codebase
        .all_classlike_descendants
        .get(&trait_name_id)
        .into_iter()
        .flatten()
        .copied()
        .filter(|user| {
            let Some(info) = analyzer.codebase.get_class(*user) else {
                return false;
            };
            if !info.used_traits.contains(&trait_name_id) {
                return false;
            }
            // `used_traits` is inherited-inclusive, but Psalm analyses a trait
            // method body only in the class that *directly* uses the trait — a
            // subclass inherits the already-analysed method. Exclude users whose
            // parent already pulls the trait in.
            info.parent_class.is_none_or(|parent| {
                analyzer
                    .codebase
                    .get_class(parent)
                    .is_none_or(|parent_info| !parent_info.used_traits.contains(&trait_name_id))
            })
        })
        .collect();
    direct_users.sort_by(|a, b| {
        analyzer
            .interner
            .lookup(*a)
            .as_ref()
            .cmp(analyzer.interner.lookup(*b).as_ref())
    });

    let body_issues_start = analysis_data.issues.len();

    for member in trait_stmt.members.iter() {
        let ClassLikeMember::Method(method) = member else {
            continue;
        };
        let method_name_id = analyzer.interner.intern(method.name.value);

        if direct_users.is_empty() {
            // A trait with no user still has its body checked once, against
            // itself, so local diagnostics aren't lost entirely.
            analyze_method(
                analyzer,
                method,
                trait_name_id,
                trait_info,
                context.namespace,
                analysis_data,
            )?;
            continue;
        }

        for user_id in &direct_users {
            let user_info = analyzer.codebase.get_class(*user_id);
            // A user that redeclares the method analyses its own copy as part of
            // its class body, so don't analyse the trait's copy for it here.
            if user_info.is_some_and(|info| {
                info.methods
                    .get(&method_name_id)
                    .is_some_and(|method_info| method_info.declaring_class == Some(*user_id))
            }) {
                continue;
            }
            analyze_method(
                analyzer,
                method,
                *user_id,
                user_info,
                context.namespace,
                analysis_data,
            )?;
        }
    }

    // Psalm guards the entire return-STATEMENT analysis block behind
    // `!($source->getSource() instanceof TraitAnalyzer)` (ReturnAnalyzer), so a
    // trait body never reports the per-`return` diagnostics — only the overall
    // declared-vs-inferred return-TYPE check (ReturnTypeAnalyzer) runs, which is
    // emitted elsewhere and survives. Drop that guarded set from the trait body.
    let body_issues = analysis_data.issues.split_off(body_issues_start);
    analysis_data
        .issues
        .extend(body_issues.into_iter().filter(|issue| {
            !matches!(
                issue.kind,
                IssueKind::InvalidReturnStatement
                    | IssueKind::NullableReturnStatement
                    | IssueKind::FalsableReturnStatement
                    | IssueKind::MixedReturnStatement
                    | IssueKind::MixedReturnTypeCoercion
                    | IssueKind::LessSpecificReturnStatement
                    | IssueKind::NonVariableReferenceReturn
            )
        }));

    Ok(())
}
