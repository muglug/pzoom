//! Instance property assignment analyzer.

use mago_syntax::ast::ast::access::PropertyAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
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
    // Analyze the object expression
    let obj_pos = expr_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

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

    // Analyze the value expression
    let value_pos = expr_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Verify property type if we can resolve it
    if let Some(obj_type) = obj_type {
        if let Some(prop_name) = prop_name {
            // Check for null/invalid types in the union
            let has_object_type = obj_type.types.iter().any(|t| {
                matches!(t, TAtomic::TNamedObject { .. } | TAtomic::TObject)
            });
            let has_null = obj_type.types.iter().any(|t| matches!(t, TAtomic::TNull));
            let has_invalid_type = obj_type.types.iter().any(|t| {
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
                        obj_type.get_id()
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

            for atomic in &obj_type.types {
                match atomic {
                    TAtomic::TNamedObject { name, .. } => {
                        // Look up the class and property
                        if let Some(class_info) = analyzer.codebase.get_class(*name) {
                            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                                // Check property visibility - private properties are only accessible within the same class
                                if prop_info.visibility == Visibility::Private {
                                    let is_same_class = analyzer
                                        .get_declaring_class()
                                        .is_some_and(|calling_class| calling_class == *name);

                                    if !is_same_class {
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
                                if prop_info.is_readonly {
                                    let class_name = analyzer.interner.lookup(*name);
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::InvalidPropertyAssignmentValue,
                                        format!(
                                            "Cannot assign to readonly property {}::${}",
                                            class_name, prop_name
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
                                                        prop_type.get_id(),
                                                        value_type.get_id()
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
                                                        prop_type.get_id(),
                                                        value_type.get_id()
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
                                            let can_be_contained =
                                                union_type_comparator::can_be_contained_by(
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
                                                        prop_name,
                                                        prop_type.get_id(),
                                                        value_type.get_id()
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
                                                        prop_type.get_id(),
                                                        value_type.get_id()
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
                                // Property not found - emit appropriate issue
                                let class_name = analyzer.interner.lookup(*name);
                                let (line, col) = analyzer.get_line_column(pos.0);

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

    // The assignment expression returns the assigned value
    analysis_data.set_expr_type(pos, value_type);
}
