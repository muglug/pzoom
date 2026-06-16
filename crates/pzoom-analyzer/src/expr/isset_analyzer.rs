//! Isset expression analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::construct::IssetConstruct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{Issue, IssueKind, TUnion, VarName};

use crate::context::BlockContext;
use crate::expr::fetch::instance_property_fetch_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze an isset() expression.
///
/// isset() returns true if the variable exists and is not null.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    isset: &IssetConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for value in isset.values.iter() {
        // Psalm matches the raw php-parser node, which has no parenthesis
        // nodes; unwrap parens to mirror that.
        let operand = value.unparenthesized();

        if let Some(property_name) = as_this_property_fetch(operand) {
            // Psalm: a `$this->prop` operand not yet in scope is analyzed as
            // a regular property fetch first — outside inside_isset, so an
            // undeclared property still reports UndefinedThisPropertyFetch —
            // and falls back to a mixed seed so the isset-true branch sees
            // the variable.
            let var_id = VarName::from(format!("$this->{}", property_name));
            if !context.has_variable(&var_id) {
                if context.has_variable("$this")
                    && let Expression::Access(Access::Property(prop_access)) = operand
                {
                    let span = operand.span();
                    instance_property_fetch_analyzer::analyze(
                        analyzer,
                        prop_access,
                        (span.start.offset, span.end.offset),
                        analysis_data,
                        context,
                        false,
                    );
                }
                if !context.has_variable(&var_id) {
                    context.set_var_type(var_id.clone(), TUnion::mixed());
                }
                context.vars_possibly_in_scope.insert(var_id);
            }
        } else if !is_valid_statement(operand) {
            let span = operand.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                "Isset only works with variables and array elements",
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }

        let _value_pos = analyze_isset_var(analyzer, value, analysis_data, context);
    }

    // isset() always returns bool
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::bool()));
}

/// Psalm's `IssetAnalyzer::analyzeIssetVar`: analyze the inner expression
/// with `inside_isset` set, suppressing undefined-variable and
/// possibly-undefined-fetch reporting. Also used by the empty() analyzer.
pub(crate) fn analyze_isset_var(
    analyzer: &StatementsAnalyzer<'_>,
    value: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);

    context.inside_isset = was_inside_isset;
    value_pos
}

/// A `$this->name` property fetch with a literal name (Psalm's
/// PropertyFetch-on-$this special case).
fn as_this_property_fetch<'a>(expr: &'a Expression<'_>) -> Option<&'a str> {
    let Expression::Access(Access::Property(prop_access)) = expr else {
        return None;
    };
    let Expression::Variable(Variable::Direct(object_var)) = prop_access.object.unparenthesized()
    else {
        return None;
    };
    if object_var.name != "$this" {
        return None;
    }
    match &prop_access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

/// Psalm's `IssetAnalyzer::isValidStatement`: the operand kinds isset()
/// accepts; everything else is "Isset only works with variables and array
/// elements".
fn is_valid_statement(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Variable(_)
        | Expression::ArrayAccess(_)
        | Expression::ArrayAppend(_)
        | Expression::Access(
            Access::Property(_)
            | Access::NullSafeProperty(_)
            | Access::StaticProperty(_)
            | Access::ClassConstant(_),
        ) => true,
        // Psalm's AssignRef (`isset($x = &$y)`).
        Expression::Assignment(assignment) => matches!(
            assignment.rhs.unparenthesized(),
            Expression::UnaryPrefix(unary)
                if matches!(unary.operator, UnaryPrefixOperator::Reference(_))
        ),
        _ => false,
    }
}
