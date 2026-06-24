//! Interface declaration analyzer.
//!
//! Mirrors Psalm's `Internal\Analyzer\InterfaceAnalyzer` (a sibling of
//! `ClassAnalyzer`): it runs the interface-specific declaration checks and, like
//! Psalm's `InterfaceAnalyzer::analyze`, analyses each method — including the
//! bodyless ones — so their declared param/return types still get the signature
//! checks. The shared class-like checks live in [`crate::stmt::class_analyzer`]
//! (pzoom's de-facto `ClassLikeAnalyzer` base) and are reused here.

use mago_syntax::ast::ast::class_like::Interface;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;

use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::{Issue, IssueKind};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::attribute_analyzer;
use crate::stmt::class_analyzer::{
    analyze_method, check_class_constant_overrides, check_deprecated_and_internal_relationships,
    check_docblock_issues, check_duplicate_constant_declarations,
    check_duplicate_method_declarations, check_extended_template_param_bounds,
    check_inheritor_violations, check_invalid_override_attributes,
    check_method_docblock_param_type_mismatches, check_missing_interface_method_typehints,
    check_missing_template_params, check_pseudo_method_annotations,
    check_pseudo_method_compatibility, check_template_variance, check_undefined_docblock_mixins,
    check_undefined_docblock_template_extends_classes, class_issue_pos, resolve_alias_in_context,
};

/// Analyze an interface declaration. The enclosing namespace (if any) is read
/// from `context`, so the same entry point serves a top-level or a namespaced
/// interface.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    interface_stmt: &Interface<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let interface_name = interface_stmt.name.value;
    let fqn = match context.namespace {
        Some(namespace) => {
            format!(
                "{}\\{}",
                analyzer.interner.lookup(namespace),
                interface_name
            )
        }
        None => interface_name.to_string(),
    };
    let interface_name_id = analyzer
        .interner
        .find(&fqn)
        .unwrap_or(pzoom_str::StrId::EMPTY);

    let interface_info = analyzer.codebase.get_class(interface_name_id);
    attribute_analyzer::analyze_interface_or_trait_attributes(
        analyzer,
        interface_stmt.attribute_lists.as_slice(),
        interface_stmt.members.as_slice(),
        interface_info,
        interface_name_id,
        context,
        analysis_data,
    );

    check_interface_property_declarations(analyzer, interface_stmt, analysis_data);

    if let Some(info) = interface_info {
        check_interface_extends_targets(analyzer, info, context, analysis_data);
        check_inheritor_violations(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_undefined_docblock_template_extends_classes(analyzer, info, analysis_data);
        check_template_variance(analyzer, info, analysis_data);
        check_missing_interface_method_typehints(analyzer, info, analysis_data);
        check_invalid_override_attributes(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_duplicate_method_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
    }

    // Mirror Psalm's InterfaceAnalyzer::analyze: a MethodAnalyzer is created for
    // every interface method (all bodyless), so their declared param/return
    // types still get the signature checks (deprecation, undefined classes,
    // wrong casing). analyze_method skips body analysis for a non-concrete body,
    // exactly as it already does for `abstract` class methods.
    for member in interface_stmt.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            analyze_method(
                analyzer,
                method,
                interface_name_id,
                interface_info,
                context.namespace,
                analysis_data,
            )?;
        }
    }

    Ok(())
}

/// PHP 8.4 interface property rules: an interface property must be hooked,
/// explicitly public, and non-static; hooks require PHP >= 8.4. These are
/// parse errors in PHP (Psalm's parser reports them; mago accepts them, so
/// re-check here with the configured version).
fn check_interface_property_declarations(
    analyzer: &StatementsAnalyzer<'_>,
    interface_stmt: &Interface<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_span::HasSpan;
    use mago_syntax::ast::ast::class_like::property::Property;
    use mago_syntax::ast::ast::modifier::Modifier;

    for member in interface_stmt.members.iter() {
        let ClassLikeMember::Property(property) = member else {
            continue;
        };

        let span = property.span();
        let emit = |message: &str, analysis_data: &mut FunctionAnalysisData| {
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ParseError,
                message.to_string(),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        };

        match property {
            Property::Plain(_) => {
                emit(
                    "Interfaces may only include hooked properties",
                    analysis_data,
                );
            }
            Property::Hooked(hooked) => {
                if analyzer.config.php_version_id() < 80400 {
                    emit(
                        "Property hooks are not available before PHP 8.4",
                        analysis_data,
                    );
                    continue;
                }

                let mut has_public = false;
                let mut bad_modifier = None;
                for modifier in hooked.modifiers.iter() {
                    match modifier {
                        Modifier::Public(_) => has_public = true,
                        Modifier::Private(_) => {
                            bad_modifier = Some("Interface properties cannot be private")
                        }
                        Modifier::Protected(_) => {
                            bad_modifier = Some("Interface properties cannot be protected")
                        }
                        Modifier::Static(_) => {
                            bad_modifier = Some("Interface properties cannot be static")
                        }
                        _ => {}
                    }
                }
                if let Some(message) = bad_modifier {
                    emit(message, analysis_data);
                } else if !has_public {
                    emit(
                        "Interface properties must be declared public",
                        analysis_data,
                    );
                }
            }
        }
    }
}

/// `interface I extends X` where X is not an interface (Psalm's
/// UndefinedInterface "X is not an interface").
fn check_interface_extends_targets(
    analyzer: &StatementsAnalyzer<'_>,
    interface_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    for extended_id in &interface_info.interfaces {
        let resolved = resolve_alias_in_context(*extended_id, context);
        if let Some(extended_info) = analyzer.codebase.get_class(resolved)
            && extended_info.kind != ClassLikeKind::Interface
        {
            let (issue_start, issue_end) = class_issue_pos(interface_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedInterface,
                format!(
                    "{} is not an interface",
                    analyzer.interner.lookup(*extended_id)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        }
    }
}
