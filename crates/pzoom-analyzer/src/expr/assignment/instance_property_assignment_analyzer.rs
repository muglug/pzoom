//! Instance property assignment analyzer.

use mago_syntax::ast::ast::access::PropertyAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze an instance property assignment ($obj->prop = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the value expression
    let value_pos = expression_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    analyze_with_known_type(analyzer, access, value_type, pos, analysis_data, context);
}

/// Analyze an instance property assignment using a precomputed assigned value type.
///
/// This is used by destructuring assignments where each target receives a value type
/// inferred from the RHS container.
pub fn analyze_with_known_type(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    value_type: TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let explicit_mutation_free_context = is_explicit_mutation_free_context(analyzer);

    // Analyze the object expression
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data
        .get_expr_type(obj_pos)
        .map(|obj_type| expand_template_object_union(&obj_type));

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    };

    // Check if this is $this->prop
    let is_this_assignment = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    if explicit_mutation_free_context && is_this_assignment && !is_special_write_method(analyzer) {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ImpurePropertyAssignment,
            "Cannot assign to a property from a mutation-free context",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Verify property type if we can resolve it
    if let Some(obj_type) = obj_type {
        if let Some(prop_name) = prop_name {
            let lookup_types = expand_intersection_lookup_types(&obj_type);

            // Check for null/invalid types in the union
            let has_object_type = lookup_types
                .iter()
                .any(|t| matches!(t, TAtomic::TNamedObject { .. } | TAtomic::TObject));
            let has_null = lookup_types.iter().any(|t| matches!(t, TAtomic::TNull));
            let has_invalid_type = lookup_types.iter().any(|t| {
                !matches!(
                    t,
                    TAtomic::TNamedObject { .. }
                        | TAtomic::TObject
                        | TAtomic::TNull
                        | TAtomic::TMixed
                )
            });

            // Check for purely null type (NullPropertyAssignment)
            if obj_type.is_null() {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullPropertyAssignment,
                    format!("Cannot assign to property ${} on null", prop_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, value_type);
                return;
            }

            // Check for nullable type (PossiblyNullPropertyAssignment)
            if has_null && has_object_type {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyNullPropertyAssignment,
                    format!(
                        "Cannot assign to property ${} on possibly null type",
                        prop_name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Check for invalid (non-object) types
            if has_invalid_type && !has_object_type {
                // Purely invalid type
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidPropertyAssignment,
                    format!(
                        "Cannot assign to property ${} on {}",
                        prop_name,
                        obj_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, value_type);
                return;
            }

            let prop_id = analyzer.interner.intern(prop_name);
            let has_concrete_property_candidate = lookup_types.iter().any(|atomic| {
                let TAtomic::TNamedObject { name, .. } = atomic else {
                    return false;
                };

                analyzer
                    .codebase
                    .get_class(*name)
                    .is_some_and(|class_info| class_info.properties.contains_key(&prop_id))
            });

            for atomic in &lookup_types {
                match atomic {
                    TAtomic::TNamedObject { name, type_params } => {
                        // Look up the class and property
                        if let Some(class_info) = analyzer.codebase.get_class(*name) {
                            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                                // Check property visibility - private properties are only accessible within the same class
                                if prop_info.visibility == Visibility::Private {
                                    let is_same_class = analyzer
                                        .get_declaring_class()
                                        .is_some_and(|calling_class| calling_class == *name);

                                    if !is_same_class
                                        && !receiver_allows_property_visibility_override(
                                            analyzer, &obj_type, *name,
                                        )
                                    {
                                        let class_name = analyzer.interner.lookup(*name);
                                        let (line, col) = analyzer.get_line_column(pos.0);
                                        analysis_data.add_issue(Issue::new(
                                            IssueKind::InaccessibleProperty,
                                            format!(
                                                "Cannot access private property {}::${}",
                                                class_name, prop_name
                                            ),
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                    }
                                }

                                // Check if property is readonly
                                if prop_info.is_readonly || class_info.is_immutable {
                                    let class_name = analyzer.interner.lookup(*name);
                                    let same_class_private_mutation_allowed = prop_info
                                        .readonly_allow_private_mutation
                                        && analyzer
                                            .get_declaring_class()
                                            .is_some_and(|calling_class| calling_class == *name);
                                    let can_write_restricted_property =
                                        is_special_write_method(analyzer)
                                            || same_class_private_mutation_allowed
                                            || (class_info.is_immutable
                                                && analyzer.get_declaring_class().is_some_and(
                                                    |calling_class| {
                                                        calling_class == *name
                                                            && !is_this_assignment
                                                    },
                                                ));

                                    if !can_write_restricted_property {
                                        let (line, col) = analyzer.get_line_column(pos.0);
                                        let message = if prop_info.is_readonly {
                                            format!(
                                                "Cannot assign to readonly property {}::${}",
                                                class_name, prop_name
                                            )
                                        } else {
                                            format!(
                                                "Property {}::${} is defined on an immutable class",
                                                class_name, prop_name
                                            )
                                        };
                                        analysis_data.add_issue(Issue::new(
                                            IssueKind::InaccessibleProperty,
                                            message,
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                        continue;
                                    }
                                }

                                if is_unserialize_method(analyzer) && is_this_assignment {
                                    continue;
                                }

                                if prop_info.is_deprecated {
                                    let class_name = analyzer.interner.lookup(*name);
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::DeprecatedProperty,
                                        format!(
                                            "Property {}::${} is deprecated",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }

                                if !can_access_internal(
                                    analyzer,
                                    &class_info.internal,
                                    Some(context),
                                ) {
                                    let scope_phrase = format_internal_scope_phrase(
                                        analyzer,
                                        &class_info.internal,
                                    );
                                    let class_name = analyzer.interner.lookup(*name);
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::InternalProperty,
                                        format!(
                                            "{}::${} is internal to {}",
                                            class_name, prop_name, scope_phrase
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }

                                if !can_access_internal(
                                    analyzer,
                                    &prop_info.internal,
                                    Some(context),
                                ) {
                                    let scope_phrase =
                                        format_internal_scope_phrase(analyzer, &prop_info.internal);
                                    let class_name = analyzer.interner.lookup(*name);
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::InternalProperty,
                                        format!(
                                            "{}::${} is internal to {}",
                                            class_name, prop_name, scope_phrase
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }

                                // Verify type compatibility using proper type comparator
                                // Only check if property has a declared type
                                if let Some(prop_type) = prop_info.get_type() {
                                    let prop_type = substitute_class_template_params(
                                        class_info,
                                        type_params.as_deref(),
                                        prop_type,
                                    );
                                    let localized_value_type = substitute_class_template_params(
                                        class_info,
                                        type_params.as_deref(),
                                        &value_type,
                                    );
                                    let mut comparison_result = TypeComparisonResult::new();
                                    let is_contained = union_type_comparator::is_contained_by(
                                        analyzer.codebase,
                                        &localized_value_type,
                                        &prop_type,
                                        false,
                                        false,
                                        &mut comparison_result,
                                    );

                                    if !is_contained {
                                        let class_name = analyzer.interner.lookup(*name);
                                        let (line, col) = analyzer.get_line_column(pos.0);

                                        // Check for type coercion
                                        if comparison_result.type_coerced.unwrap_or(false) {
                                            if comparison_result
                                                .type_coerced_from_nested_mixed
                                                .unwrap_or(false)
                                            {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::MixedPropertyTypeCoercion,
                                                    format!(
                                                        "Property {}::${} expects {}, parent type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                            } else {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::PropertyTypeCoercion,
                                                    format!(
                                                        "Property {}::${} expects {}, parent type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                            }
                                        } else {
                                            // Check if there's a partial match (possibly invalid)
                                            if has_mixed_array_key_property_coercion(
                                                analyzer,
                                                &localized_value_type,
                                                &prop_type,
                                            ) {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::MixedPropertyTypeCoercion,
                                                    format!(
                                                        "Property {}::${} expects {}, parent type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                                continue;
                                            }

                                            let can_be_contained =
                                                union_type_comparator::can_be_contained_by(
                                                    analyzer.codebase,
                                                    &localized_value_type,
                                                    &prop_type,
                                                );

                                            if can_be_contained {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::PossiblyInvalidPropertyAssignmentValue,
                                                    format!(
                                                        "Property {}::${} expects {}, possibly different type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                            } else {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::InvalidPropertyAssignmentValue,
                                                    format!(
                                                        "Property {}::${} expects {}, got {}",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                            }
                                        }
                                    }
                                }
                            } else {
                                let class_name = analyzer.interner.lookup(*name);
                                let (line, col) = analyzer.get_line_column(pos.0);

                                if class_info.kind == ClassLikeKind::Interface {
                                    if !has_concrete_property_candidate {
                                        analysis_data.add_issue(Issue::new(
                                            IssueKind::NoInterfaceProperties,
                                            "Interfaces cannot have properties",
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                    }

                                    if has_concrete_property_candidate {
                                        continue;
                                    }
                                }

                                if class_has_magic_setter(class_info) {
                                    if let Some(pseudo_type) = get_pseudo_property_set_type(
                                        class_info,
                                        type_params.as_deref(),
                                        prop_id,
                                    ) {
                                        let localized_value_type = substitute_class_template_params(
                                            class_info,
                                            type_params.as_deref(),
                                            &value_type,
                                        );
                                        let mut comparison_result = TypeComparisonResult::new();
                                        let is_contained = union_type_comparator::is_contained_by(
                                            analyzer.codebase,
                                            &localized_value_type,
                                            &pseudo_type,
                                            false,
                                            false,
                                            &mut comparison_result,
                                        );

                                        if !is_contained {
                                            let can_be_contained =
                                                union_type_comparator::can_be_contained_by(
                                                    analyzer.codebase,
                                                    &localized_value_type,
                                                    &pseudo_type,
                                                );

                                            let issue_kind = if can_be_contained {
                                                IssueKind::PossiblyInvalidPropertyAssignmentValue
                                            } else {
                                                IssueKind::InvalidPropertyAssignmentValue
                                            };
                                            let message = if can_be_contained {
                                                format!(
                                                    "Property {}::${} expects {}, possibly different type {} provided",
                                                    class_name,
                                                    prop_name,
                                                    pseudo_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                )
                                            } else {
                                                format!(
                                                    "Property {}::${} expects {}, got {}",
                                                    class_name,
                                                    prop_name,
                                                    pseudo_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                )
                                            };

                                            analysis_data.add_issue(Issue::new(
                                                issue_kind,
                                                message,
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        }

                                        continue;
                                    }

                                    if class_has_sealed_properties(class_info) {
                                        let kind = if is_this_assignment {
                                            IssueKind::UndefinedThisPropertyAssignment
                                        } else {
                                            IssueKind::UndefinedMagicPropertyAssignment
                                        };
                                        let message = if is_this_assignment {
                                            format!(
                                                "Property {}::${} does not exist",
                                                class_name, prop_name
                                            )
                                        } else {
                                            format!(
                                                "Magic property {}::${} does not exist",
                                                class_name, prop_name
                                            )
                                        };

                                        analysis_data.add_issue(Issue::new(
                                            kind,
                                            message,
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                        continue;
                                    }

                                    continue;
                                }

                                if class_allows_dynamic_property_assignment(analyzer, class_info) {
                                    continue;
                                }

                                if is_this_assignment {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::UndefinedThisPropertyAssignment,
                                        format!(
                                            "Property {}::${} does not exist",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                } else {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::UndefinedPropertyAssignment,
                                        format!(
                                            "Property {}::${} does not exist",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }
                            }
                        }
                    }
                    TAtomic::TNull => {
                        // Already handled above
                    }
                    TAtomic::TMixed => {
                        if is_this_assignment && !context.has_this {
                            continue;
                        }
                        if should_suppress_issue(
                            analyzer,
                            pos.0,
                            &["MixedPropertyAssignment", "MixedAssignment"],
                        ) {
                            continue;
                        }
                        // Emit MixedPropertyAssignment issue
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::MixedAssignment,
                            format!("Cannot assign to property ${} on mixed type", prop_name),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(object_key) = expression_identifier::get_expression_var_key(access.object) {
        clear_object_member_tracking(analyzer, context, &object_key);

        if let Some(prop_name) = prop_name {
            let property_key = format!("{}->{}", object_key, prop_name);
            let property_id = analyzer.interner.intern(&property_key);
            context.set_var_type(property_id, value_type.clone());
        }
    }

    // The assignment expression returns the assigned value
    analysis_data.set_expr_type(pos, value_type);
}

fn is_special_write_method(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let method_name = analyzer.interner.lookup(function_info.name);
    matches!(
        method_name.as_ref(),
        "__construct" | "unserialize" | "__unserialize" | "__clone"
    )
}

fn clear_object_member_tracking(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    object_key: &str,
) {
    let property_prefix = format!("{object_key}->");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            analyzer
                .interner
                .lookup(*var_id)
                .as_ref()
                .starts_with(&property_prefix)
        })
        .collect();

    for var_id in keys_to_clear {
        context.locals.remove(&var_id);
        context.assigned_var_ids.remove(&var_id);
        context.possibly_assigned_var_ids.remove(&var_id);
        context.class_string_origins.remove(&var_id);
    }
}

fn is_unserialize_method(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let method_name = analyzer.interner.lookup(function_info.name);
    matches!(method_name.as_ref(), "unserialize" | "__unserialize")
}

fn is_explicit_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    analyzer
        .function_info
        .is_some_and(|function_info| function_info.is_pure || function_info.is_mutation_free)
}

fn class_allows_dynamic_property_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    if class_info.no_seal_properties || class_info.sealed_properties == Some(false) {
        return true;
    }

    for parent_id in &class_info.all_parent_classes {
        let Some(parent_info) = analyzer.codebase.get_class(*parent_id) else {
            continue;
        };

        if parent_info.no_seal_properties || parent_info.sealed_properties == Some(false) {
            return true;
        }
    }

    false
}

fn should_suppress_issue(
    analyzer: &StatementsAnalyzer<'_>,
    issue_offset: u32,
    issue_names: &[&str],
) -> bool {
    if issue_names
        .iter()
        .any(|issue_name| analyzer.config.is_issue_suppressed(issue_name))
    {
        return true;
    }

    let source = analyzer.source;
    let offset = issue_offset as usize;
    if offset == 0 || offset > source.len() {
        return false;
    }

    let bytes = source.as_bytes();
    let mut cursor = offset;
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }

    if cursor < 2 || &source[cursor - 2..cursor] != "*/" {
        return false;
    }

    let doc_end = cursor;
    let Some(doc_start) = source[..doc_end - 2].rfind("/**") else {
        return false;
    };

    let docblock = &source[doc_start..doc_end];
    docblock
        .split('\n')
        .filter(|line| line.contains("@psalm-suppress"))
        .flat_map(|line| {
            line.split_whitespace()
                .skip_while(|part| *part != "@psalm-suppress")
                .skip(1)
                .flat_map(|part| part.split(','))
                .map(|part| part.trim().trim_end_matches(','))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .any(|suppressed| {
            issue_names
                .iter()
                .any(|issue_name| suppressed == *issue_name)
        })
}

fn class_has_magic_setter(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::SET)
}

fn class_has_sealed_properties(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.sealed_properties.unwrap_or(false) && !class_info.no_seal_properties
}

fn get_pseudo_property_set_type(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    prop_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let pseudo_type = class_info.pseudo_property_set_types.get(&prop_id)?;
    Some(substitute_class_template_params(
        class_info,
        type_params,
        pseudo_type,
    ))
}

fn substitute_class_template_params(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    property_type: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return property_type.clone();
    }

    let template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            type_params,
        ),
    );

    if template_defaults.is_empty() && template_replacements.is_empty() {
        return property_type.clone();
    }

    function_call_analyzer::replace_templates_in_union(
        property_type,
        &template_replacements,
        &template_defaults,
    )
}

fn expand_template_object_union(obj_type: &TUnion) -> TUnion {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TTemplateParam { as_type, .. } => {
                if as_type.is_mixed() {
                    expanded_types.push(TAtomic::TMixed);
                } else {
                    expanded_types.extend(as_type.types.iter().cloned());
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                expanded_types.push((**as_type).clone());
            }
            TAtomic::TObjectIntersection { types } => {
                let mut expanded_intersection = Vec::new();
                for nested in types {
                    match nested {
                        TAtomic::TTemplateParam { as_type, .. } => {
                            if as_type.is_mixed() {
                                expanded_intersection.push(TAtomic::TMixed);
                            } else {
                                expanded_intersection.extend(as_type.types.iter().cloned());
                            }
                        }
                        TAtomic::TTemplateParamClass { as_type, .. } => {
                            expanded_intersection.push((**as_type).clone());
                        }
                        _ => expanded_intersection.push(nested.clone()),
                    }
                }
                expanded_types.push(TAtomic::TObjectIntersection {
                    types: expanded_intersection,
                });
            }
            _ => expanded_types.push(atomic.clone()),
        }
    }

    TUnion::from_types(expanded_types)
}

fn expand_intersection_lookup_types(obj_type: &TUnion) -> Vec<TAtomic> {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if !expanded_types.contains(nested) {
                        expanded_types.push(nested.clone());
                    }
                }
            }
            _ => {
                if !expanded_types.contains(atomic) {
                    expanded_types.push(atomic.clone());
                }
            }
        }
    }

    expanded_types
}

fn receiver_allows_property_visibility_override(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_type: &TUnion,
    target_class: StrId,
) -> bool {
    let mut has_target_class = false;
    let mut has_override_interface = false;

    let mut track_named = |name: StrId| {
        if name == target_class {
            has_target_class = true;
        }

        if analyzer.codebase.get_class(name).is_some_and(|info| {
            info.kind == ClassLikeKind::Interface && info.override_property_visibility
        }) {
            has_override_interface = true;
        }
    };

    for atomic in &receiver_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => track_named(*name),
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if let TAtomic::TNamedObject { name, .. } = nested {
                        track_named(*name);
                    }
                }
            }
            _ => {}
        }
    }

    has_target_class && has_override_interface
}

fn has_mixed_array_key_property_coercion(
    analyzer: &StatementsAnalyzer<'_>,
    value_type: &TUnion,
    property_type: &TUnion,
) -> bool {
    for value_atomic in &value_type.types {
        let Some((value_key_type, value_value_type)) = get_array_key_value_union(value_atomic)
        else {
            continue;
        };

        if !is_broad_array_key_union(value_key_type) {
            continue;
        }

        for property_atomic in &property_type.types {
            let Some((property_key_type, property_value_type)) =
                get_array_key_value_union(property_atomic)
            else {
                continue;
            };

            if property_key_type.is_mixed() {
                continue;
            }

            let mut value_comparison = TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                analyzer.codebase,
                value_value_type,
                property_value_type,
                false,
                false,
                &mut value_comparison,
            ) {
                continue;
            }

            let mut key_comparison = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                value_key_type,
                property_key_type,
                false,
                false,
                &mut key_comparison,
            ) {
                continue;
            }

            if union_type_comparator::can_be_contained_by(
                analyzer.codebase,
                property_key_type,
                value_key_type,
            ) {
                return true;
            }
        }
    }

    false
}

fn get_array_key_value_union(atomic: &TAtomic) -> Option<(&TUnion, &TUnion)> {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => Some((key_type.as_ref(), value_type.as_ref())),
        _ => None,
    }
}

fn is_broad_array_key_union(key_type: &TUnion) -> bool {
    key_type.is_mixed()
        || key_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TArrayKey | TAtomic::TMixed))
}
