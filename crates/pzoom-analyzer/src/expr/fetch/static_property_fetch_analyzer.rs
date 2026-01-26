//! Static property fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::StaticPropertyAccess;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a static property access expression (Foo::$bar).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &StaticPropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let _class_pos = expr_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Try to get the resolved class ID
    let class_id = get_resolved_class_id(analyzer, access.class);
    let class_name = class_id.map(|id| analyzer.interner.lookup(id));

    // Get the property name from the Variable
    let prop_name = match &access.property {
        Variable::Direct(direct) => Some(direct.name),
        _ => None,
    };

    // Try to look up static property type
    if let (Some(class_id), Some(class_name), Some(prop_name)) = (class_id, class_name, prop_name) {
        let prop_id = analyzer.interner.intern(prop_name);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                // Check that property is static
                if !prop_info.is_static {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidStaticPropertyFetch,
                        format!(
                            "Cannot access non-static property {}::${} statically",
                            class_name, prop_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check visibility - private properties are only accessible within the same class
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

                // Check for deprecated properties
                if prop_info.is_deprecated {
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
                if let Some(prop_type) = prop_info.get_type() {
                    analysis_data.set_expr_type(pos, prop_type.clone());
                }
                return;
            } else {
                // Property not found
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedProperty,
                    format!("Property {}::${} does not exist", class_name, prop_name),
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
