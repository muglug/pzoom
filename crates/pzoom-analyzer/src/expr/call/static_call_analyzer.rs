//! Static method call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::call::StaticMethodCall;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

use super::arguments_analyzer;

/// Analyze a static method call expression (Foo::bar()).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    static_call: &StaticMethodCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let _class_pos = expr_analyzer::analyze(analyzer, static_call.class, analysis_data, context);

    // Analyze arguments
    arguments_analyzer::analyze(analyzer, &static_call.argument_list, analysis_data, context);

    // Try to get the class name using resolved names
    let class_id = get_resolved_class_id(analyzer, static_call.class);

    // Get the method name
    let method_name = get_method_name(&static_call.method);

    // Try to look up method return type
    if let (Some(class_id), Some(method_name)) = (class_id, method_name) {
        let class_name = analyzer.interner.lookup(class_id);
        let method_id = analyzer.interner.intern(method_name);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if let Some(method_info) = class_info.methods.get(&method_id) {
                // Check that method is static
                if !method_info.is_static {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidStaticMethodCall,
                        format!(
                            "Cannot call non-static method {}::{} statically",
                            class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check visibility
                if method_info.visibility == Visibility::Private {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleMethod,
                        format!(
                            "Cannot access private method {}::{}",
                            class_name, method_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check for deprecated methods
                if method_info.is_deprecated {
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
                    analysis_data.set_expr_type(pos, return_type.clone());
                    return;
                }
            } else {
                // Method not found
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

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(analyzer: &StatementsAnalyzer<'_>, expr: &Expression<'_>) -> Option<StrId> {
    match expr {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer.get_resolved_name(offset)
        }
        _ => None,
    }
}

/// Get the method name from a method selector.
fn get_method_name<'a>(selector: &'a ClassLikeMemberSelector<'a>) -> Option<&'a str> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}
