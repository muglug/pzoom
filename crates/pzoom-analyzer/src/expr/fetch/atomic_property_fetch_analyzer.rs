//! Atomic property fetch analyzer - handles property lookups on specific types.

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a property fetch on a known class type.
pub fn analyze_property(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
    _in_assignment: bool,
) -> Option<TUnion> {
    let class_id = analyzer.interner.intern(class_name);
    let prop_id = analyzer.interner.intern(prop_name);

    // Look up the class in the codebase
    if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        // Look up the property
        if let Some(prop_info) = class_info.properties.get(&prop_id) {
            // Check visibility - private properties are only accessible within the same class
            if prop_info.visibility == Visibility::Private {
                let is_same_class = analyzer
                    .get_declaring_class()
                    .is_some_and(|calling_class| calling_class == class_id);

                if !is_same_class {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleProperty,
                        format!("Cannot access private property {}::${}", class_name, prop_name),
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

            // Return the property's declared type
            return prop_info.get_type().cloned();
        } else {
            // Property not found - might be a magic property via __get
            // For now, report as undefined unless the class has __get
            let has_magic_get = class_info
                .methods
                .keys()
                .any(|m| analyzer.interner.lookup(*m).as_ref() == "__get");

            if !has_magic_get {
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
        }
    }

    // Couldn't determine property type - return None to fall back to mixed
    None
}
