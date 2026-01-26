//! Variable fetch analyzer.

use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a variable fetch expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    var: &Variable<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match var {
        Variable::Direct(direct) => {
            // Get the variable name from the identifier
            let var_name = direct.name;

            // Look up the interned string ID for this variable
            if let Some(var_id) = analyzer.interner.find(var_name) {
                // Check if we have a type for this variable in context
                if let Some(var_type) = context.get_var_type(var_id) {
                    analysis_data.set_expr_type(pos, var_type.clone());
                } else {
                    // Variable not yet assigned - could be undefined
                    // For now, treat as mixed
                    analysis_data.set_expr_type(pos, TUnion::mixed());
                }
            } else {
                // Variable name not interned yet - treat as mixed
                analysis_data.set_expr_type(pos, TUnion::mixed());
            }
        }
        Variable::Indirect(_indirect) => {
            // Variable variables ($$name) - type is unknown at static analysis time
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
        Variable::Nested(_nested) => {
            // Nested variables - type is unknown at static analysis time
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    }
}
