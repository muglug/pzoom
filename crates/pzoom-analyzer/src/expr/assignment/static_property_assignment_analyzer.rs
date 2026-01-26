//! Static property assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::StaticPropertyAccess;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
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
    let _class_pos = expr_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Try to get the class name
    let class_name = get_class_name(analyzer, access.class, context);

    // Get the property name from the Variable
    let prop_name = match &access.property {
        mago_syntax::ast::ast::variable::Variable::Direct(direct) => Some(direct.name),
        _ => None,
    };

    // Analyze the value expression
    let value_pos = expr_analyzer::analyze(analyzer, value_expr, analysis_data, context);
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
                                        prop_name_str,
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
                                        prop_name_str,
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
    _context: &BlockContext,
) -> Option<String> {
    match expr {
        // Handle special keywords: self, static, parent
        Expression::Self_(_) | Expression::Static(_) => {
            // Get the current class from the analyzer
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                return Some(analyzer.interner.lookup(declaring_class).to_string());
            }
            None
        }
        Expression::Parent(_) => {
            // Get the parent class
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                if let Some(class_info) = analyzer.codebase.get_class(declaring_class) {
                    if let Some(parent) = class_info.parent_class {
                        return Some(analyzer.interner.lookup(parent).to_string());
                    }
                }
            }
            None
        }
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer.get_resolved_name(offset).map(|id| {
                analyzer.interner.lookup(id).to_string()
            })
        }
        _ => None,
    }
}
