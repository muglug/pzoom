//! Instance property fetch analyzer.

use mago_syntax::ast::ast::access::{NullSafePropertyAccess, PropertyAccess};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an instance property access expression ($obj->prop).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    _in_assignment: bool,
) {
    // Analyze the object expression
    let obj_pos = expr_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        ClassLikeMemberSelector::Variable(var) => {
            let _var_pos = expr_analyzer::analyze(
                analyzer,
                &Expression::Variable(var.clone()),
                analysis_data,
                context,
            );
            None
        }
        ClassLikeMemberSelector::Expression(expr) => {
            let _expr_pos =
                expr_analyzer::analyze(analyzer, expr.expression, analysis_data, context);
            None
        }
    };

    // Check if this is $this->prop
    let is_this_fetch = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(prop_type) =
            get_property_type(analyzer, &obj_t, prop_name, pos, analysis_data, is_this_fetch)
        {
            analysis_data.set_expr_type(pos, prop_type);
            return;
        }
    }

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Analyze a null-safe property access expression ($obj?->prop).
pub fn analyze_nullsafe(
    analyzer: &StatementsAnalyzer<'_>,
    access: &NullSafePropertyAccess<'_>,
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

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(mut prop_type) =
            get_property_type(analyzer, &obj_t, prop_name, pos, analysis_data, false)
        {
            // If the object could be null, the result could be null
            if obj_t.is_nullable {
                prop_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, prop_type);
            return;
        }
    }

    // Fall back to mixed|null
    let mut result = TUnion::mixed();
    result.add_type(TAtomic::TNull);
    analysis_data.set_expr_type(pos, result);
}

/// Look up the type of a property on a type.
fn get_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    is_this_fetch: bool,
) -> Option<TUnion> {
    let prop_id = analyzer.interner.intern(prop_name);

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

    // Check for purely null type (NullPropertyFetch)
    if obj_type.is_null() {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::NullPropertyFetch,
            format!("Cannot access property ${} on null", prop_name),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return None;
    }

    // Check for nullable type (PossiblyNullPropertyFetch)
    if has_null && has_object_type {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullPropertyFetch,
            format!(
                "Cannot access property ${} on possibly null type",
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
    if has_invalid_type {
        if !has_object_type {
            // Purely invalid type
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidPropertyFetch,
                format!(
                    "Cannot access property ${} on {}",
                    prop_name,
                    obj_type.get_id()
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            return None;
        } else {
            // Mixed valid/invalid types
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidPropertyFetch,
                format!(
                    "Cannot access property ${} on possibly non-object type {}",
                    prop_name,
                    obj_type.get_id()
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                // Look up the class
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    // Look up the property
                    if let Some(prop_info) = class_info.properties.get(&prop_id) {
                        // Check visibility - private properties are only accessible within the same class
                        if prop_info.visibility == Visibility::Private {
                            let is_same_class = analyzer
                                .get_declaring_class()
                                .is_some_and(|calling_class| calling_class == *name);

                            if !is_same_class {
                                let (line, col) = analyzer.get_line_column(pos.0);
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::InaccessibleProperty,
                                    format!(
                                        "Cannot access private property {}::${}",
                                        analyzer.interner.lookup(*name),
                                        prop_name
                                    ),
                                    analyzer.file_path,
                                    pos.0,
                                    pos.1,
                                    line,
                                    col,
                                ));
                            }
                        }

                        // Check for deprecated properties
                        if prop_info.is_deprecated {
                            let class_name = analyzer.interner.lookup(*name);
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::DeprecatedProperty,
                                format!("Property {}::${} is deprecated", class_name, prop_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        // Return the property's type
                        return prop_info.get_type().cloned();
                    } else {
                        // Property not found - emit appropriate issue
                        let class_name = analyzer.interner.lookup(*name);
                        let (line, col) = analyzer.get_line_column(pos.0);

                        if is_this_fetch {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedThisPropertyFetch,
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
                                IssueKind::UndefinedPropertyFetch,
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
            TAtomic::TObject => {
                // Generic object - can't look up property
            }
            TAtomic::TMixed => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedPropertyFetch,
                    format!("Cannot access property ${} on mixed type", prop_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            TAtomic::TNull => {
                // Already handled above
            }
            _ => {
                // Already handled in has_invalid_type check above
            }
        }
    }

    None
}
