//! Expression statement analyzer.

use mago_allocator::Arena;
use mago_span::HasSpan;
use mago_syntax::cst::cst::binary::BinaryOperator;
use mago_syntax::cst::cst::control_flow::r#if::{If, IfBody, IfStatementBody};
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::keyword::Keyword;
use mago_syntax::cst::cst::statement::{ExpressionStatement, Statement};
use mago_syntax::cst::cst::terminator::Terminator;
use mago_syntax::cst::cst::unary::{UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::cst::sequence::Sequence;

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
                        value: b"if",
                    },
                    left_parenthesis: cond_span,
                    condition,
                    right_parenthesis: cond_span,
                    body: IfBody::Statement(IfStatementBody {
                        statement: body_statement,
                        else_if_clauses: Sequence::empty(),
                        else_clause: None,
                    }),
                };
                return if_else_analyzer::analyze(analyzer, &synthetic_if, analysis_data, context);
            }
        }
    }

    // Analyze the expression - the result type is discarded
    let pos = expression_analyzer::analyze(analyzer, expr_stmt.expression, analysis_data, context);

    // A statement-level `never` ends control flow only when it originates from a
    // never-returning call or `exit`/`die` — mirroring Psalm, where
    // `$context->has_returned` is set by FunctionCall/MethodCallAnalyzer and
    // ExitAnalyzer, not by an assignment or a bare arithmetic expression. In
    // particular, a modulo-by-zero (`$x = 3 % 0`) yields `never` but does NOT
    // make the following statements unreachable, so each such site still gets its
    // own NoValue. (`throw` is handled in throw_analyzer; a bare `yield;` keeps
    // resuming, as in Psalm's YieldAnalyzer.)
    if matches!(
        expr_stmt.expression.unparenthesized(),
        Expression::Call(_) | Expression::Construct(_)
    ) && analysis_data
        .expr_types
        .get(&pos)
        .cloned()
        .is_some_and(|t| t.is_nothing())
    {
        context.has_returned = true;
    }

    Ok(())
}
