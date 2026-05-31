//! If-conditional helpers.

use std::cell::RefCell;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::scope::if_conditional_scope::IfConditionalScope;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an if/elseif condition, producing the conditional scope.
///
/// Adapted from hakana-core's `if_conditional_analyzer::analyze`. The condition is
/// analyzed once in a dedicated context carrying a shared `if_body_context`; the
/// `&&`/`||` analyzers narrow into that shared context with the type information the
/// if body should see (including right-operand assignments). The returned scope's
/// `if_body_context` is the post-condition fallthrough base with only the
/// operator-narrowed/-assigned locals overlaid, so simple (non-`&&`/`||`) conditions
/// reduce to the fallthrough base unchanged.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    cond: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    outer_context: &BlockContext,
) -> IfConditionalScope {
    let pre_seed_locals = outer_context.locals.clone();

    // The shared body context the &&/|| operators narrow into.
    let mut if_body_seed = outer_context.clone();
    if_body_seed.if_body_context = None;
    let if_body_rc = Rc::new(RefCell::new(if_body_seed));

    // Analyze the condition in a dedicated context. We keep the pre-existing
    // assigned/possibly-assigned tracking intact (unlike Hakana, whose variable
    // checks are locals-based) so already-defined variables stay defined.
    let mut cond_context = outer_context.clone();
    cond_context.if_body_context = Some(if_body_rc.clone());
    let was_inside_conditional = cond_context.inside_conditional;
    cond_context.inside_conditional = true;

    // Psalm analyzes the sub-expression that is definitely evaluated regardless of
    // branch (e.g. the `preg_match(..., $matches)` in `if (!preg_match(...) || ...)`)
    // on its own first, so its assignments are seeded into scope before the rest of
    // the condition narrows things. Only do so when it is a strict sub-expression.
    let externally_applied_if_cond_expr = get_definitely_evaluated_expression_after_if(cond);
    if !std::ptr::eq(externally_applied_if_cond_expr, cond.unparenthesized()) {
        expression_analyzer::analyze(
            analyzer,
            externally_applied_if_cond_expr,
            analysis_data,
            &mut cond_context,
        );
    }

    expression_analyzer::analyze(analyzer, cond, analysis_data, &mut cond_context);

    cond_context.inside_conditional = was_inside_conditional;
    cond_context.if_body_context = None;

    let cond_referenced_var_ids = cond_context.cond_referenced_var_ids.clone();
    let post_if_context = cond_context;

    // Build if_body_context from the post-condition fallthrough base, overlaying only
    // the locals the &&/|| operators actually narrowed or assigned (those that differ
    // from the pre-condition seed). For a simple condition no operator runs, so this
    // leaves the fallthrough base untouched.
    let body_rc = match Rc::try_unwrap(if_body_rc) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => rc.borrow().clone(),
    };
    let mut if_body_context = post_if_context.clone();
    for (var_id, var_type) in &body_rc.locals {
        if pre_seed_locals.get(var_id) != Some(var_type) {
            if_body_context.locals.insert(*var_id, var_type.clone());
        }
    }
    // Carry the clauses the &&/||/ternary operators reconciled during condition
    // analysis so the if-body reconcile can skip re-reporting them (Hakana). `&&`
    // records into the shared body context (body_rc); `||`/ternary record onto the
    // cond context (now post_if_context, already cloned into if_body_context).
    if_body_context
        .reconciled_expression_clauses
        .extend(body_rc.reconciled_expression_clauses);

    IfConditionalScope {
        if_body_context,
        outer_context: post_if_context.clone(),
        post_if_context,
        cond_referenced_var_ids,
    }
}

/// Mirrors Psalm `IfConditionalAnalyzer::getDefinitelyEvaluatedExpressionAfterIf`:
/// reduces a condition to the sub-expression that is definitely evaluated
/// regardless of which branch is taken — stripping `=== true`, taking the left
/// operand of `&&`/`and`/`xor`, and descending through `!` (which swaps to the
/// inside-if reduction). This keeps assignments such as `$matches` in
/// `if (!preg_match($re, $s, $matches))` defined after the `if`.
fn get_definitely_evaluated_expression_after_if<'a>(
    stmt: &'a Expression<'a>,
) -> &'a Expression<'a> {
    match stmt.unparenthesized() {
        Expression::Binary(binary)
            if matches!(
                binary.operator,
                BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
            ) =>
        {
            if is_true_literal(binary.lhs) {
                return get_definitely_evaluated_expression_after_if(binary.rhs);
            }
            if is_true_literal(binary.rhs) {
                return get_definitely_evaluated_expression_after_if(binary.lhs);
            }
            stmt
        }
        Expression::Binary(binary) => {
            if matches!(
                binary.operator,
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_) | BinaryOperator::LowXor(_)
            ) {
                return get_definitely_evaluated_expression_after_if(binary.lhs);
            }
            stmt
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = get_definitely_evaluated_expression_inside_if(unary.operand);
            if std::ptr::eq(inner, unary.operand) {
                stmt
            } else {
                inner
            }
        }
        _ => stmt,
    }
}

/// Mirrors Psalm `IfConditionalAnalyzer::getDefinitelyEvaluatedExpressionInsideIf`:
/// the expression definitely evaluated before any statements inside the `if`
/// body — like the above but taking the left operand of `||`/`or`/`xor`.
fn get_definitely_evaluated_expression_inside_if<'a>(
    stmt: &'a Expression<'a>,
) -> &'a Expression<'a> {
    match stmt.unparenthesized() {
        Expression::Binary(binary)
            if matches!(
                binary.operator,
                BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
            ) =>
        {
            if is_true_literal(binary.lhs) {
                return get_definitely_evaluated_expression_inside_if(binary.rhs);
            }
            if is_true_literal(binary.rhs) {
                return get_definitely_evaluated_expression_inside_if(binary.lhs);
            }
            stmt
        }
        Expression::Binary(binary) => {
            if matches!(
                binary.operator,
                BinaryOperator::Or(_) | BinaryOperator::LowOr(_) | BinaryOperator::LowXor(_)
            ) {
                return get_definitely_evaluated_expression_inside_if(binary.lhs);
            }
            stmt
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = get_definitely_evaluated_expression_after_if(unary.operand);
            if std::ptr::eq(inner, unary.operand) {
                stmt
            } else {
                inner
            }
        }
        _ => stmt,
    }
}

fn is_true_literal(expr: &Expression<'_>) -> bool {
    matches!(expr.unparenthesized(), Expression::Literal(Literal::True(_)))
}

/// Mirrors Psalm's `IfConditionalAnalyzer::handleParadoxicalCondition`.
pub fn handle_paradoxical_condition(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_redundant_with_assignment: bool,
    context: Option<&BlockContext>,
) {
    let Some(expr_type) = analysis_data
        .get_expr_type(expr_pos)
        .map(|union| (*union).clone())
    else {
        return;
    };

    if is_possibly_undefined_direct_var(expr, context, analyzer) {
        return;
    }

    // Inside a loop pzoom's iteration widening is incomplete, so a condition can be
    // transiently typed as a literal `true`/`false` that Psalm would have widened;
    // skip the always-truthy/falsy paradox check there to avoid false positives.
    if context.is_some_and(|context| context.inside_loop) {
        return;
    }

    if expr_type.is_always_falsy() {
        let issue_kind = if expr_type.from_docblock {
            IssueKind::DocblockTypeContradiction
        } else {
            IssueKind::TypeDoesNotContainType
        };
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            format!(
                "Operand of type {} is always falsy",
                expr_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            expr_pos.0,
            expr_pos.1,
            line,
            col,
        ));
        return;
    }

    if expr_type.is_always_truthy()
        && (!matches!(expr.unparenthesized(), Expression::Assignment(_))
            || emit_redundant_with_assignment)
    {
        let issue_kind = if expr_type.from_docblock {
            IssueKind::RedundantConditionGivenDocblockType
        } else {
            IssueKind::RedundantCondition
        };
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            format!(
                "Operand of type {} is always truthy",
                expr_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            expr_pos.0,
            expr_pos.1,
            line,
            col,
        ));
        return;
    }

    // Psalm: otherwise flag a risky truthy/falsy comparison
    // (`ExpressionAnalyzer::checkRiskyTruthyFalsyComparison`), skipped for the
    // `===` / `!==` / `!` forms that already compare explicitly.
    if !is_assignment_or_negated_assignment(expr)
        && should_check_risky_truthy_falsy(expr, analyzer)
        && get_truthy_falsy_target_union(expr, expr_type.clone(), analysis_data)
            .is_some_and(|target_union| is_risky_truthy_falsy_union(&target_union))
    {
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RiskyTruthyFalsyComparison,
            format!(
                "Operand of type {} may evaluate differently under truthy/falsy checks",
                expr_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            expr_pos.0,
            expr_pos.1,
            line,
            col,
        ));
    }
}

fn is_possibly_undefined_direct_var(
    expr: &Expression<'_>,
    context: Option<&BlockContext>,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    let Some(context) = context else {
        return false;
    };

    let var_name = match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name),
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            if let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized() {
                Some(direct.name)
            } else {
                None
            }
        }
        _ => None,
    };

    let Some(var_name) = var_name else {
        return false;
    };

    let var_id = analyzer.interner.intern(var_name);
    context.possibly_assigned_var_ids.contains(&var_id)
        && !context.assigned_var_ids.contains_key(&var_id)
}

fn is_risky_truthy_falsy_union(union: &TUnion) -> bool {
    if !union.is_nullable
        || union
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
    {
        return false;
    }

    union.types.iter().any(is_ambiguous_array_like_atomic)
}

fn get_truthy_falsy_target_union(
    expr: &Expression<'_>,
    expr_type: TUnion,
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return Some(expr_type);
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Not(_)) {
        return Some(expr_type);
    }

    analysis_data
        .get_expr_type((
            unary.operand.start_offset() as u32,
            unary.operand.end_offset() as u32,
        ))
        .map(|union| (*union).clone())
        .or(Some(expr_type))
}

fn is_array_like_atomic(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => as_type.types.iter().any(is_array_like_atomic),
        _ => false,
    }
}

fn is_ambiguous_array_like_atomic(atomic: &TAtomic) -> bool {
    if !is_array_like_atomic(atomic) {
        return false;
    }

    if atomic.is_truthy() || atomic.is_falsy() {
        return false;
    }

    match atomic {
        TAtomic::TTemplateParam { as_type, .. } => {
            as_type.types.iter().any(is_ambiguous_array_like_atomic)
        }
        _ => true,
    }
}

fn is_assignment_or_negated_assignment(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Assignment(_) => true,
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            matches!(unary.operand.unparenthesized(), Expression::Assignment(_))
        }
        _ => false,
    }
}

fn should_check_risky_truthy_falsy(
    expr: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            analyzer.interner.find(direct.name).is_some_and(|var_id| {
                analyzer.function_info.is_some_and(|function_info| {
                    function_info.params.iter().any(|p| p.name == var_id)
                })
            })
        }
        Expression::Call(Call::Function(_)) => true,
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            should_check_risky_truthy_falsy(unary.operand, analyzer)
        }
        _ => false,
    }
}
