//! Instance property fetch analyzer.

use mago_syntax::ast::ast::access::{NullSafePropertyAccess, PropertyAccess};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an instance property access expression ($obj->prop).
use super::atomic_property_fetch_analyzer::*;

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
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        ClassLikeMemberSelector::Variable(var) => {
            let _var_pos = expression_analyzer::analyze(
                analyzer,
                &Expression::Variable(var.clone()),
                analysis_data,
                context,
            );
            None
        }
        ClassLikeMemberSelector::Expression(expr) => {
            let _expr_pos =
                expression_analyzer::analyze(analyzer, expr.expression, analysis_data, context);
            None
        }
    };

    // Check if this is $this->prop
    let is_this_fetch = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    if let Some(prop_name) = prop_name {
        if let Some(keyed_type) =
            get_reconciled_property_type(analyzer, context, access.object, prop_name)
        {
            analysis_data.set_expr_type(pos, keyed_type);
            return;
        }
    }

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(prop_type) = get_property_type(
            analyzer,
            &obj_t,
            prop_name,
            pos,
            analysis_data,
            is_this_fetch,
            context.inside_isset,
            context.has_this,
            context,
        ) {
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
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    };

    if let Some(prop_name) = prop_name {
        if let Some(mut keyed_type) =
            get_reconciled_property_type(analyzer, context, access.object, prop_name)
        {
            if obj_type.is_some_and(|obj_t| obj_t.is_nullable) {
                keyed_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, keyed_type);
            return;
        }
    }

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(mut prop_type) = get_property_type(
            analyzer,
            &obj_t,
            prop_name,
            pos,
            analysis_data,
            false,
            true,
            context.has_this,
            context,
        ) {
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
