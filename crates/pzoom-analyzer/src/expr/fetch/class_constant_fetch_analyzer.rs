//! Class constant fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::ClassConstantAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassConstantInfo, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a class constant access expression (Foo::BAR).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ClassConstantAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let _class_pos = expr_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Try to get the resolved class ID
    let class_id = get_resolved_class_id(analyzer, access.class);
    let classlike_name = class_id.map(|id| analyzer.interner.lookup(id));

    // Get the constant name
    let const_name = match &access.constant {
        ClassLikeConstantSelector::Identifier(id) => Some(id.value),
        ClassLikeConstantSelector::Expression(_) => None,
    };

    // Handle ::class pseudo-constant
    if let Some(const_name) = const_name {
        if const_name.eq_ignore_ascii_case("class") {
            if let Some(class_name) = classlike_name {
                // Return a literal class string
                analysis_data.set_expr_type(
                    pos,
                    TUnion::new(TAtomic::TLiteralClassString { name: class_name.to_string() }),
                );
                return;
            }
        }
    }

    // Try to look up class constant type
    if let (Some(class_id), Some(class_name), Some(const_name)) = (class_id, classlike_name, const_name) {
        let const_id = analyzer.interner.intern(const_name);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            // Look for constant in class hierarchy (class, parents, interfaces)
            if let Some(const_info) =
                find_constant_in_hierarchy(analyzer, class_id, const_id)
            {
                // Check visibility
                if const_info.visibility == Visibility::Private {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleClassConstant,
                        format!(
                            "Cannot access private constant {}::{}",
                            class_name, const_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check for deprecated constants
                if const_info.is_deprecated {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedConstant,
                        format!("Constant {}::{} is deprecated", class_name, const_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Return the constant's type
                analysis_data.set_expr_type(pos, const_info.constant_type.clone());
                return;
            } else {
                // Constant not found in class hierarchy
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedConstant,
                    format!("Constant {}::{} does not exist", class_name, const_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            // Silence unused variable warning
            let _ = class_info;
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

/// Find a constant in a class's hierarchy (class, parent classes, interfaces).
fn find_constant_in_hierarchy<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    class_id: StrId,
    const_id: StrId,
) -> Option<&'a ClassConstantInfo> {
    // Check the class itself
    if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        if let Some(const_info) = class_info.constants.get(&const_id) {
            return Some(const_info);
        }

        // Check parent class
        if let Some(parent_id) = class_info.parent_class {
            if let Some(const_info) = find_constant_in_hierarchy(analyzer, parent_id, const_id) {
                return Some(const_info);
            }
        }

        // Check interfaces
        for iface_id in &class_info.interfaces {
            if let Some(const_info) = find_constant_in_hierarchy(analyzer, *iface_id, const_id) {
                return Some(const_info);
            }
        }
    }

    None
}
