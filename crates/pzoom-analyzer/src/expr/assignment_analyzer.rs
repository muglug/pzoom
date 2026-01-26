//! Assignment expression analyzer.
//!
//! This module handles various forms of PHP assignments:
//! - Simple variable assignment: $x = value
//! - Property assignment: $obj->prop = value (handled by instance_property_assignment_analyzer)
//! - Static property assignment: Class::$prop = value (handled by static_property_assignment_analyzer)
//! - Array assignment: $arr[key] = value (handled by array_assignment_analyzer)
//! - Destructuring: list($a, $b) = $arr or [$a, $b] = $arr

use mago_syntax::ast::ast::assignment::Assignment;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr::assignment::{array_assignment_analyzer, instance_property_assignment_analyzer, static_property_assignment_analyzer};
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an assignment expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    assignment: &Assignment<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Check if this is a property assignment - handle specially
    if let Expression::Access(access) = assignment.lhs {
        use mago_syntax::ast::ast::access::Access;

        match access {
            Access::Property(prop_access) => {
                instance_property_assignment_analyzer::analyze(
                    analyzer,
                    prop_access,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                );
                return;
            }
            Access::StaticProperty(static_prop) => {
                static_property_assignment_analyzer::analyze(
                    analyzer,
                    static_prop,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                );
                return;
            }
            _ => {}
        }
    }

    // Check if this is an array element assignment
    if let Expression::ArrayAccess(array_access) = assignment.lhs {
        array_assignment_analyzer::analyze(
            analyzer,
            array_access,
            assignment.rhs,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    // Analyze the right-hand side first
    let rhs_pos = expr_analyzer::analyze(analyzer, assignment.rhs, analysis_data, context);
    let rhs_type = analysis_data
        .get_expr_type(rhs_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // Handle the left-hand side
    analyze_assignment_lhs(analyzer, assignment.lhs, &rhs_type, pos, analysis_data, context);

    // The assignment expression itself has the type of the RHS
    analysis_data.set_expr_type(pos, rhs_type);
}

/// Analyze the left-hand side of an assignment and set variable types.
fn analyze_assignment_lhs(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_type: &TUnion,
    _pos: Pos,
    _analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match lhs {
        Expression::Variable(var) => {
            if let Variable::Direct(direct) = var {
                let var_name = direct.name;

                // Intern the variable name
                let var_id = analyzer.interner.intern(var_name);

                // Set the variable's type in context (this also tracks assignment)
                context.set_var_type(var_id, rhs_type.clone());
            }
        }
        Expression::Access(access) => {
            use mago_syntax::ast::ast::access::Access;

            match access {
                Access::Property(_) | Access::NullSafeProperty(_) | Access::StaticProperty(_) => {
                    // Property assignments are handled by the specialized analyzers
                    // which are dispatched at the top of analyze() before reaching here
                }
                Access::ClassConstant(_) => {
                    // Cannot assign to class constants - this would be a parse error
                    // The PHP parser would reject this before we get here
                }
            }
        }
        Expression::List(list) => {
            // list() assignment - destructuring
            // For full implementation:
            // 1. Check RHS is array-like
            // 2. Extract element types from RHS array
            // 3. Assign to each list element variable
            // For now, we assign mixed to captured variables
            for element in list.elements.iter() {
                if let mago_syntax::ast::ast::array::ArrayElement::Value(value_element) = element {
                    if let Expression::Variable(var) = value_element.value {
                        if let Variable::Direct(direct) = var {
                            let var_id = analyzer.interner.intern(direct.name);
                            context.set_var_type(var_id, TUnion::mixed());
                        }
                    }
                }
            }
        }
        Expression::Array(array) => {
            // Short list syntax: [$a, $b] = $arr
            // Similar to list() handling
            for element in array.elements.iter() {
                if let mago_syntax::ast::ast::array::ArrayElement::Value(value_element) = element {
                    if let Expression::Variable(var) = value_element.value {
                        if let Variable::Direct(direct) = var {
                            let var_id = analyzer.interner.intern(direct.name);
                            context.set_var_type(var_id, TUnion::mixed());
                        }
                    }
                }
            }
        }
        Expression::ArrayAccess(_) => {
            // Array element assignment - $arr[key] = value
            // Handled by array_assignment_analyzer when dispatched from expr_analyzer
        }
        Expression::ArrayAppend(_) => {
            // Array append - $arr[] = value
            // Handled by array_assignment_analyzer when dispatched from expr_analyzer
        }
        _ => {
            // Other expressions on LHS (invalid in most cases)
        }
    }
}
