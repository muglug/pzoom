//! Expression statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::control_flow::r#if::{If, IfBody, IfStatementBody};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::keyword::Keyword;
use mago_syntax::ast::ast::statement::{ExpressionStatement, Statement};
use mago_syntax::ast::ast::terminator::Terminator;
use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::sequence::Sequence;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_else_analyzer;

/// Analyze an expression statement (a statement that is just an expression).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    expr_stmt: &ExpressionStatement<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Mirror Psalm's AndAnalyzer/OrAnalyzer from_stmt path: a statement-level
    // `A && B` is equivalent to `if (A) { B; }`, and `A || B` to `if (!A) { B; }`.
    // Analyzing it as such gives the right operand the if-body's narrowing and
    // produces the correct fallthrough merge for an assignment in the right operand
    // (e.g. `($x === null) && ($x = "")`).
    if let Some(arena) = analyzer.arena {
        if let Expression::Binary(binary) = expr_stmt.expression.unparenthesized() {
            // The condition entering the synthesized `if`: `left` for `&&`, `!left`
            // for `||` (the right operand runs only when the left operand is falsy).
            let condition: Option<&Expression> = match binary.operator {
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => Some(binary.lhs),
                BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
                    let not_left: &Expression = arena.alloc(Expression::UnaryPrefix(UnaryPrefix {
                        operator: UnaryPrefixOperator::Not(binary.lhs.span()),
                        operand: binary.lhs,
                    }));
                    Some(not_left)
                }
                _ => None,
            };

            if let Some(condition) = condition {
                // Synthesize `if (<condition>) { <right>; }` (Psalm's VirtualIf). The
                // operands are already arena-allocated; the wrapper nodes are
                // allocated in the same parse arena.
                let right = binary.rhs;
                let cond_span = binary.lhs.span();
                let body_statement: &Statement =
                    arena.alloc(Statement::Expression(ExpressionStatement {
                        expression: right,
                        terminator: Terminator::Semicolon(right.span()),
                    }));
                let synthetic_if = If {
                    r#if: Keyword {
                        span: cond_span,
                        value: arena.alloc_str("if"),
                    },
                    left_parenthesis: cond_span,
                    condition,
                    right_parenthesis: cond_span,
                    body: IfBody::Statement(IfStatementBody {
                        statement: body_statement,
                        else_if_clauses: Sequence::empty(arena),
                        else_clause: None,
                    }),
                };
                return if_else_analyzer::analyze(analyzer, &synthetic_if, analysis_data, context);
            }
        }
    }

    // Analyze the expression - the result type is discarded
    let pos = expression_analyzer::analyze(analyzer, expr_stmt.expression, analysis_data, context);

    // A statement-level expression of type `never` ends control flow.
    if analysis_data
        .get_expr_type(pos)
        .is_some_and(|t| t.is_nothing())
    {
        context.has_returned = true;
    }

    Ok(())
}
