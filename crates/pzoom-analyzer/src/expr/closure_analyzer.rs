//! Closure and arrow function analyzer.

use mago_syntax::ast::ast::function_like::arrow_function::ArrowFunction;
use mago_syntax::ast::ast::function_like::closure::Closure;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter as MagoParameter;

use pzoom_code_info::{FunctionLikeParameter, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a closure expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    closure: &Closure<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Create a new scope for the closure body
    let mut closure_context = context.clone();

    // Handle use() clause for captured variables
    if let Some(ref use_clause) = closure.use_clause {
        for use_var in use_clause.variables.iter() {
            let var_name = use_var.variable.name;
            let var_id = analyzer.interner.intern(var_name);

            // Copy the variable's type from the outer context
            if let Some(var_type) = context.locals.get(&var_id) {
                // For &$var (by reference), the inner changes affect outer
                // For $var (by value), it's a copy
                closure_context.locals.insert(var_id, var_type.clone());
            }
        }
    }

    // Extract parameter types
    let params = extract_param_types(analyzer, &closure.parameter_list.parameters);

    // Add parameters to the closure context
    for param in &closure.parameter_list.parameters {
        let param_name = param.variable.name;
        let param_id = analyzer.interner.intern(param_name);

        // For now, use mixed for untyped params
        // Full type hint resolution would require more infrastructure
        let param_type = TUnion::mixed();
        closure_context.locals.insert(param_id, param_type);
    }

    // For now, use mixed for return type if no hint
    // Full type hint resolution would require more infrastructure
    let return_type = if closure.return_type_hint.is_some() {
        Some(TUnion::mixed())
    } else {
        None
    };

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: if params.is_empty() { None } else { Some(params) },
        return_type: return_type.map(Box::new),
    });

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze an arrow function expression.
pub fn analyze_arrow_function(
    analyzer: &StatementsAnalyzer<'_>,
    arrow: &ArrowFunction<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Arrow functions have implicit variable capture - all variables from outer scope
    // are captured by value automatically

    // Add parameters to context
    let mut arrow_context = context.clone();

    // Extract parameter types
    let params = extract_param_types(analyzer, &arrow.parameter_list.parameters);

    for param in &arrow.parameter_list.parameters {
        let param_name = param.variable.name;
        let param_id = analyzer.interner.intern(param_name);

        // For now, use mixed for untyped params
        let param_type = TUnion::mixed();
        arrow_context.locals.insert(param_id, param_type);
    }

    // Analyze the body expression to infer return type
    let body_pos = expr_analyzer::analyze(analyzer, arrow.expression, analysis_data, &mut arrow_context);
    let inferred_return_type = analysis_data.get_expr_type(body_pos).map(|t| (*t).clone());

    // Use inferred type if available
    let return_type = inferred_return_type;

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: if params.is_empty() { None } else { Some(params) },
        return_type: return_type.map(Box::new),
    });

    analysis_data.set_expr_type(pos, expr_type);
}

/// Extract parameter type information from a list of parameters.
fn extract_param_types<'a, I>(
    analyzer: &StatementsAnalyzer<'_>,
    parameters: I,
) -> Vec<FunctionLikeParameter>
where
    I: IntoIterator<Item = &'a MagoParameter<'a>>,
{
    parameters
        .into_iter()
        .map(|param| {
            FunctionLikeParameter {
                name: Some(analyzer.interner.intern(param.variable.name)),
                param_type: TUnion::mixed(), // Type hint resolution would go here
                is_optional: param.default_value.is_some(),
                is_variadic: param.ellipsis.is_some(),
                by_ref: param.ampersand.is_some(),
            }
        })
        .collect()
}
