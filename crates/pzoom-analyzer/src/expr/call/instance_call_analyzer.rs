//! Instance method call analyzer.

use mago_syntax::ast::ast::call::{MethodCall, NullSafeMethodCall};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::arguments_analyzer;

/// Analyze a method call expression ($obj->method()).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    method_call: &MethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the object expression
    let obj_pos = expr_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Analyze arguments
    arguments_analyzer::analyze(analyzer, &method_call.argument_list, analysis_data, context);

    // Get the method name
    let method_name = get_method_name(&method_call.method);

    // Try to look up method return type from each atomic type in the union
    if let (Some(obj_t), Some(method_name)) = (obj_type, method_name) {
        let return_type = get_method_return_type(analyzer, &obj_t, method_name, pos, analysis_data);
        if let Some(return_type) = return_type {
            analysis_data.set_expr_type(pos, return_type);
            return;
        }
    }

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Analyze a null-safe method call expression ($obj?->method()).
pub fn analyze_nullsafe(
    analyzer: &StatementsAnalyzer<'_>,
    method_call: &NullSafeMethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the object expression
    let obj_pos = expr_analyzer::analyze(analyzer, method_call.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Analyze arguments
    arguments_analyzer::analyze(analyzer, &method_call.argument_list, analysis_data, context);

    // Get the method name
    let method_name = get_method_name(&method_call.method);

    // Try to look up method return type
    if let (Some(obj_t), Some(method_name)) = (obj_type, method_name) {
        // For null-safe calls, get the return type and add null to it
        if let Some(mut return_type) =
            get_method_return_type(analyzer, &obj_t, method_name, pos, analysis_data)
        {
            // If the object could be null, the result could be null
            if obj_t.is_nullable {
                return_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, return_type);
            return;
        }
    }

    // Fall back to mixed|null
    let mut result = TUnion::mixed();
    result.add_type(TAtomic::TNull);
    analysis_data.set_expr_type(pos, result);
}

/// Get the method name from a method selector.
fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

/// Look up the return type of a method on a type.
fn get_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    method_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let method_id = analyzer.interner.intern(method_name);

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                // Look up the class
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    // Look up the method
                    if let Some(method_info) = class_info.methods.get(&method_id) {
                        // Check visibility - private methods are only accessible within the same class
                        if method_info.visibility == Visibility::Private {
                            let is_same_class = analyzer
                                .get_declaring_class()
                                .is_some_and(|calling_class| calling_class == *name);

                            if !is_same_class {
                                let (line, col) = analyzer.get_line_column(pos.0);
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::InaccessibleMethod,
                                    format!(
                                        "Cannot access private method {}::{}",
                                        analyzer.interner.lookup(*name),
                                        method_name
                                    ),
                                    analyzer.file_path,
                                    pos.0,
                                    pos.1,
                                    line,
                                    col,
                                ));
                            }
                        }

                        // Check for deprecated methods
                        if method_info.is_deprecated {
                            let class_name = analyzer.interner.lookup(*name);
                            let message = method_info
                                .deprecation_message
                                .as_ref()
                                .map(|m| {
                                    format!(
                                        "Method {}::{} is deprecated: {}",
                                        class_name, method_name, m
                                    )
                                })
                                .unwrap_or_else(|| {
                                    format!("Method {}::{} is deprecated", class_name, method_name)
                                });
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::DeprecatedMethod,
                                message,
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        // Return the method's return type
                        if let Some(return_type) = &method_info.return_type {
                            return Some(return_type.clone());
                        }
                    } else {
                        // Method not found - check for __call magic method
                        if class_info.methods.contains_key(&analyzer.interner.intern("__call")) {
                            // Has __call magic method - return mixed
                            return Some(TUnion::mixed());
                        }
                        // Method not found
                        let class_name = analyzer.interner.lookup(*name);
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::UndefinedMethod,
                            format!("Method {}::{} does not exist", class_name, method_name),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
            }
            TAtomic::TObject => {
                // Generic object - can't look up method, just return mixed
            }
            TAtomic::TMixed => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedMethodCall,
                    format!("Cannot call method {} on mixed type", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            TAtomic::TNull | TAtomic::TVoid => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullReference,
                    format!("Cannot call method {} on null", method_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            _ => {
                let type_desc = atomic.get_id();
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidMethodCall,
                    format!("Cannot call method {} on {}", method_name, type_desc),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    }

    None
}
