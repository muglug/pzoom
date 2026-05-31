//! Static property assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::StaticPropertyAccess;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a static property assignment (Foo::$bar = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &StaticPropertyAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression to get the class name
    let _class_pos = expression_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Psalm: writing a static property is global mutable state, so it is impure from a
    // `@psalm-pure` context.
    crate::expr::fetch::static_property_fetch_analyzer::emit_impure_static_property(
        analyzer,
        pos,
        analysis_data,
    );

    // Try to get the class name
    let class_name = get_class_name(analyzer, access.class, context);

    // Get the property name from the Variable
    let prop_name = match &access.property {
        mago_syntax::ast::ast::variable::Variable::Direct(direct) => Some(direct.name),
        _ => None,
    };

    // Analyze the value expression
    let value_pos = expression_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Verify property type if we can resolve it
    if let (Some(class_name), Some(prop_name)) = (class_name, prop_name) {
        let class_id = analyzer.interner.intern(&class_name);
        // Strip the leading $ from property name
        let prop_name_str = prop_name.trim_start_matches('$');
        let prop_id = analyzer.interner.intern(prop_name_str);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                // Check that property is static
                if !prop_info.is_static {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidStaticPropertyFetch,
                        format!(
                            "Cannot access non-static property {}::${} statically",
                            class_name, prop_name_str
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check property visibility - private properties are only accessible within the same class
                if prop_info.visibility == Visibility::Private {
                    let is_same_class = analyzer
                        .get_declaring_class()
                        .is_some_and(|calling_class| calling_class == class_id);

                    if !is_same_class {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InaccessibleProperty,
                            format!(
                                "Cannot access private property {}::${}",
                                class_name, prop_name_str
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
                if prop_info.is_readonly {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidPropertyAssignmentValue,
                        format!(
                            "Cannot assign to readonly property {}::${}",
                            class_name, prop_name_str
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if prop_info.is_deprecated {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedProperty,
                        format!("Property {}::${} is deprecated", class_name, prop_name_str),
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
                    let mut comparison_result = TypeComparisonResult::new();
                    let is_contained = union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &value_type,
                        prop_type,
                        false,
                        false,
                        &mut comparison_result,
                    );

                    if !is_contained {
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
                                        prop_name_str,
                                        prop_type.get_id(Some(analyzer.interner)),
                                        value_type.get_id(Some(analyzer.interner))
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
                                        prop_name_str,
                                        prop_type.get_id(Some(analyzer.interner)),
                                        value_type.get_id(Some(analyzer.interner))
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
                                &value_type,
                                prop_type,
                            ) {
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::MixedPropertyTypeCoercion,
                                    format!(
                                        "Property {}::${} expects {}, parent type {} provided",
                                        class_name,
                                        prop_name_str,
                                        prop_type.get_id(Some(analyzer.interner)),
                                        value_type.get_id(Some(analyzer.interner))
                                    ),
                                    analyzer.file_path,
                                    pos.0,
                                    pos.1,
                                    line,
                                    col,
                                ));
                            } else {
                                let can_be_contained = union_type_comparator::can_be_contained_by(
                                    analyzer.codebase,
                                    &value_type,
                                    prop_type,
                                );

                                if can_be_contained {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::PossiblyInvalidPropertyAssignmentValue,
                                        format!(
                                            "Property {}::${} expects {}, possibly different type {} provided",
                                            class_name,
                                            prop_name_str,
                                            prop_type.get_id(Some(analyzer.interner)),
                                            value_type.get_id(Some(analyzer.interner))
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
                                            prop_name_str,
                                            prop_type.get_id(Some(analyzer.interner)),
                                            value_type.get_id(Some(analyzer.interner))
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
                }
            } else {
                // Property not found
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedPropertyAssignment,
                    format!("Property {}::${} does not exist", class_name, prop_name_str),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        } else {
            // Class not found
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!("Class {} does not exist", class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // The assignment expression returns the assigned value
    analysis_data.set_expr_type(pos, value_type);
}

/// Try to extract a class name from an expression, resolving self/static/parent.
fn get_class_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<String> {
    let resolve_alias = |class_id| {
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .filter(|alias_target| analyzer.codebase.get_class(*alias_target).is_some())
            .unwrap_or(class_id)
    };

    match expr {
        // Handle special keywords: self, static, parent
        Expression::Self_(_) | Expression::Static(_) => {
            // Get the current class from the analyzer
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                let class_id = resolve_alias(declaring_class);
                return Some(analyzer.interner.lookup(class_id).to_string());
            }
            None
        }
        Expression::Parent(_) => {
            // Get the parent class
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                if let Some(class_info) = analyzer.codebase.get_class(declaring_class) {
                    if let Some(parent) = class_info.parent_class {
                        let class_id = resolve_alias(parent);
                        return Some(analyzer.interner.lookup(class_id).to_string());
                    }
                }
            }
            None
        }
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            let class_id = analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(id.value()));
            let class_id = resolve_alias(class_id);
            Some(analyzer.interner.lookup(class_id).to_string())
        }
        _ => None,
    }
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
