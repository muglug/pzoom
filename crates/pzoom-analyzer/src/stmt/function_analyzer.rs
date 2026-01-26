//! Function declaration analyzer.
//!
//! Analyzes function bodies with proper return type context.

use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer;

/// Analyze a function declaration.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func: &Function<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_with_namespace(analyzer, func, None, analysis_data, context)
}

/// Analyze a function declaration with a namespace context.
pub fn analyze_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    func: &Function<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the function name - use FQN if in a namespace
    let func_name = func.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, func_name)
    } else {
        func_name.to_string()
    };

    // Look up the function info from the codebase
    let func_name_id = analyzer.interner.intern(&fqn);
    let function_info = analyzer.codebase.get_function(func_name_id);

    // Create a new analyzer with the function context
    let func_analyzer = if let Some(info) = function_info {
        StatementsAnalyzer {
            codebase: analyzer.codebase,
            interner: analyzer.interner,
            function_info: Some(info),
            file_path: analyzer.file_path,
            source: analyzer.source,
            resolved_names: analyzer.resolved_names,
        }
    } else {
        // Function not found in codebase - use analyzer without function context
        StatementsAnalyzer {
            codebase: analyzer.codebase,
            interner: analyzer.interner,
            function_info: None,
            file_path: analyzer.file_path,
            source: analyzer.source,
            resolved_names: analyzer.resolved_names,
        }
    };

    // Create a new context for the function body, preserving namespace
    let mut func_context = BlockContext::new();
    func_context.namespace = context.namespace;

    // Add parameters to context
    for param in func.parameter_list.parameters.iter() {
        let param_name = param.variable.name;
        let param_name_id = analyzer.interner.intern(param_name);

        // Get parameter info from function info
        let param_info = function_info.and_then(|info| {
            info.params.iter().find(|p| p.name == param_name_id)
        });

        // Get parameter type - for variadic params, wrap in array type
        let param_type = if let Some(info) = param_info {
            let base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);
            if info.is_variadic {
                // Variadic parameters become arrays inside the function body
                TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::int()),
                    value_type: Box::new(base_type),
                })
            } else {
                base_type
            }
        } else {
            TUnion::mixed()
        };

        func_context.set_var_type(param_name_id, param_type);
    }

    // Analyze the function body
    stmt_analyzer::analyze_stmts(
        &func_analyzer,
        func.body.statements.as_slice(),
        analysis_data,
        &mut func_context,
    )?;

    Ok(())
}
