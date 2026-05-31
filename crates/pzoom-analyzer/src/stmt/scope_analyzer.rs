//! Control flow analyzer - determines what control actions a set of statements takes.
//!
//! Modeled after Psalm's ScopeAnalyzer and Hakana's scope_analyzer.
//! This analyzes statements to determine if they return, exit, break, continue, etc.

use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::statement::Statement;
use rustc_hash::FxHashSet;

use crate::function_analysis_data::FunctionAnalysisData;

/// Control flow actions that statements can take.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ControlAction {
    /// Normal flow (no special control action)
    None,
    /// Control flow ends (return, throw, exit with never type)
    End,
    /// Return statement specifically (distinguishes from throw/exit)
    Return,
    /// Break out of current loop/switch
    Break,
    /// Break immediately out of the innermost loop
    BreakImmediateLoop,
    /// Continue to next loop iteration
    Continue,
    /// Leave switch statement (break within switch)
    LeaveSwitch,
}

/// Context for break/continue statements to know what construct they're in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BreakContext {
    Switch,
    Loop,
}

/// Get control actions for a sequence of statements.
///
/// This determines what control flow actions the statements take (return, exit, break, etc.)
/// which is essential for determining if code after the statements is reachable.
///
/// # Arguments
/// * `stmts` - The statements to analyze
/// * `analysis_data` - Analysis data containing expression types
/// * `break_context` - Stack of loop/switch contexts for break/continue resolution
/// * `return_is_exit` - If true, return is treated same as throw/exit (ACTION_END).
///                      If false, return is distinguished (ACTION_RETURN).
pub fn get_control_actions(
    stmts: &[Statement<'_>],
    analysis_data: &FunctionAnalysisData,
    break_context: &[BreakContext],
    return_is_exit: bool,
) -> FxHashSet<ControlAction> {
    let mut control_actions = FxHashSet::default();

    if stmts.is_empty() {
        control_actions.insert(ControlAction::None);
        return control_actions;
    }

    'outer: for stmt in stmts {
        match stmt {
            // Return statement always ends control flow
            Statement::Return(_) => {
                if !return_is_exit {
                    control_actions.insert(ControlAction::Return);
                } else {
                    control_actions.insert(ControlAction::End);
                }
                return control_actions;
            }

            // Expression statements - check for throw/exit/never-returning calls
            Statement::Expression(expr_stmt) => {
                use mago_span::HasSpan;

                // Check if this is a throw expression
                if matches!(expr_stmt.expression, Expression::Throw(_)) {
                    control_actions.insert(ControlAction::End);
                    return control_actions;
                }

                // Check if this is an exit/die expression
                if let Expression::Construct(construct) = expr_stmt.expression {
                    if matches!(construct, Construct::Exit(_) | Construct::Die(_)) {
                        control_actions.insert(ControlAction::End);
                        return control_actions;
                    }
                }

                // Check if the expression type is `never` (e.g., a function that always throws)
                let span = expr_stmt.expression.span();
                let pos = (span.start.offset, span.end.offset);
                if let Some(t) = analysis_data.get_expr_type(pos) {
                    if t.is_nothing() {
                        control_actions.insert(ControlAction::End);
                        return control_actions;
                    }
                }
            }

            // Break statement
            Statement::Break(break_stmt) => {
                // Get the break depth (default 1)
                let depth = break_stmt
                    .level
                    .as_ref()
                    .and_then(|level| {
                        if let Expression::Literal(lit) = level {
                            if let mago_syntax::ast::ast::literal::Literal::Integer(int_lit) = lit {
                                int_lit.value.map(|v| v as usize)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(1);

                if !break_context.is_empty() && break_context.len() >= depth {
                    let target_idx = break_context.len() - depth;
                    match break_context.get(target_idx) {
                        Some(BreakContext::Switch) => {
                            control_actions.insert(ControlAction::LeaveSwitch);
                        }
                        Some(BreakContext::Loop) => {
                            control_actions.insert(ControlAction::BreakImmediateLoop);
                        }
                        None => {
                            control_actions.insert(ControlAction::Break);
                        }
                    }
                } else {
                    control_actions.insert(ControlAction::Break);
                }
                return control_actions;
            }

            // Continue statement
            Statement::Continue(continue_stmt) => {
                // Get the continue depth (default 1)
                let depth = continue_stmt
                    .level
                    .as_ref()
                    .and_then(|level| {
                        if let Expression::Literal(lit) = level {
                            if let mago_syntax::ast::ast::literal::Literal::Integer(int_lit) = lit {
                                int_lit.value.map(|v| v as usize)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(1);

                if !break_context.is_empty() && break_context.len() >= depth {
                    let target_idx = break_context.len() - depth;
                    match break_context.get(target_idx) {
                        // `continue` targeting a switch behaves as leaving switch
                        Some(BreakContext::Switch) => {
                            control_actions.insert(ControlAction::LeaveSwitch);
                        }
                        Some(BreakContext::Loop) | None => {
                            control_actions.insert(ControlAction::Continue);
                        }
                    }
                } else {
                    control_actions.insert(ControlAction::Continue);
                }

                return control_actions;
            }

            // If statement - check all branches
            Statement::If(if_stmt) => {
                let if_actions = get_control_actions(
                    if_stmt.body.statements(),
                    analysis_data,
                    break_context,
                    return_is_exit,
                );

                let mut all_leave = !if_actions.contains(&ControlAction::None);

                // Check all elseif branches
                let mut all_elseif_actions = FxHashSet::default();
                for (elseif_cond, elseif_stmts) in if_stmt.body.else_if_clauses() {
                    let _ = elseif_cond; // Unused but part of the iterator
                    let elseif_actions = get_control_actions(
                        elseif_stmts,
                        analysis_data,
                        break_context,
                        return_is_exit,
                    );
                    all_leave = all_leave && !elseif_actions.contains(&ControlAction::None);
                    all_elseif_actions.extend(elseif_actions);
                }

                // Check else branch
                let else_actions = if let Some(else_stmts) = if_stmt.body.else_statements() {
                    let actions = get_control_actions(
                        else_stmts,
                        analysis_data,
                        break_context,
                        return_is_exit,
                    );
                    all_leave = all_leave && !actions.contains(&ControlAction::None);
                    actions
                } else {
                    // No else means the else branch has implicit None
                    all_leave = false;
                    FxHashSet::default()
                };

                control_actions.extend(if_actions);
                control_actions.extend(all_elseif_actions);
                control_actions.extend(else_actions);

                if all_leave {
                    // All branches exit, so the if statement as a whole exits
                    return control_actions;
                }

                // Some branches don't exit, filter out None for later
                control_actions.retain(|a| *a != ControlAction::None);
            }

            // While loop
            Statement::While(while_stmt) => {
                let mut loop_context = break_context.to_vec();
                loop_context.push(BreakContext::Loop);

                let loop_actions = get_control_actions(
                    while_stmt.body.statements(),
                    analysis_data,
                    &loop_context,
                    return_is_exit,
                );

                control_actions.extend(loop_actions);
                control_actions.retain(|a| *a != ControlAction::None);

                // Check for infinite loop (while(true) with no break)
                // TODO: Check if condition is always truthy
                // For now, just remove BreakImmediateLoop since we exited the loop
                control_actions.retain(|a| *a != ControlAction::BreakImmediateLoop);
            }

            // Do-While loop - body is a single statement
            Statement::DoWhile(do_while_stmt) => {
                let mut loop_context = break_context.to_vec();
                loop_context.push(BreakContext::Loop);

                // DoWhile has a single statement as body
                let loop_actions = get_control_actions(
                    std::slice::from_ref(do_while_stmt.statement),
                    analysis_data,
                    &loop_context,
                    return_is_exit,
                );

                control_actions.extend(loop_actions);
                control_actions.retain(|a| *a != ControlAction::None);
                control_actions.retain(|a| *a != ControlAction::BreakImmediateLoop);
            }

            // For loop
            Statement::For(for_stmt) => {
                let mut loop_context = break_context.to_vec();
                loop_context.push(BreakContext::Loop);

                let loop_actions = get_control_actions(
                    for_stmt.body.statements(),
                    analysis_data,
                    &loop_context,
                    return_is_exit,
                );

                control_actions.extend(loop_actions);
                control_actions.retain(|a| *a != ControlAction::None);
                control_actions.retain(|a| *a != ControlAction::BreakImmediateLoop);
            }

            // Foreach loop
            Statement::Foreach(foreach_stmt) => {
                let mut loop_context = break_context.to_vec();
                loop_context.push(BreakContext::Loop);

                let loop_actions = get_control_actions(
                    foreach_stmt.body.statements(),
                    analysis_data,
                    &loop_context,
                    return_is_exit,
                );

                control_actions.extend(loop_actions);
                control_actions.retain(|a| *a != ControlAction::None);
                control_actions.retain(|a| *a != ControlAction::BreakImmediateLoop);
            }

            // Switch statement
            Statement::Switch(switch_stmt) => {
                let mut switch_context = break_context.to_vec();
                switch_context.push(BreakContext::Switch);

                let mut has_ended = false;
                let mut has_default_terminator = false;
                let mut all_case_actions = FxHashSet::default();

                // Get all cases
                let cases: Vec<_> = switch_stmt.body.cases().iter().collect();

                // Iterate cases in reverse order (like Psalm does)
                for case in cases.iter().rev() {
                    let case_stmts = case.statements();
                    let case_actions = get_control_actions(
                        case_stmts,
                        analysis_data,
                        &switch_context,
                        return_is_exit,
                    );

                    // If case breaks/continues/leaves, skip further processing
                    if case_actions.contains(&ControlAction::LeaveSwitch)
                        || case_actions.contains(&ControlAction::Break)
                        || case_actions.contains(&ControlAction::Continue)
                    {
                        continue 'outer;
                    }

                    let case_does_end = !case_actions.is_empty()
                        && case_actions
                            .iter()
                            .all(|a| *a == ControlAction::End || *a == ControlAction::Return);

                    if case_does_end {
                        has_ended = true;
                    }

                    if has_ended {
                        all_case_actions.extend(
                            case_actions
                                .into_iter()
                                .filter(|a| *a != ControlAction::None),
                        );
                    } else {
                        all_case_actions.extend(case_actions);
                    }

                    if !case_does_end && !has_ended {
                        continue 'outer;
                    }

                    // Check if this is the default case and it terminates
                    if case.is_default() && case_does_end {
                        has_default_terminator = true;
                    }
                }

                control_actions.extend(all_case_actions);

                if has_default_terminator {
                    return control_actions;
                }
            }

            // Try-catch-finally
            Statement::Try(try_stmt) => {
                let try_actions = get_control_actions(
                    try_stmt.block.statements.as_slice(),
                    analysis_data,
                    break_context,
                    return_is_exit,
                );

                let try_leaves = !try_actions.contains(&ControlAction::None);

                let mut all_catch_actions = FxHashSet::default();
                let mut all_catches_leave = try_leaves;

                for catch in try_stmt.catch_clauses.iter() {
                    let catch_actions = get_control_actions(
                        catch.block.statements.as_slice(),
                        analysis_data,
                        break_context,
                        return_is_exit,
                    );

                    if all_catches_leave {
                        all_catches_leave = !catch_actions.contains(&ControlAction::None);
                    }

                    if !all_catches_leave {
                        control_actions.extend(catch_actions);
                    } else {
                        all_catch_actions.extend(catch_actions);
                    }
                }

                // If try and all catches leave, the whole try-catch exits
                if all_catches_leave && !try_stmt.catch_clauses.is_empty() {
                    let mut only_none = FxHashSet::default();
                    only_none.insert(ControlAction::None);
                    if try_actions != only_none {
                        control_actions.extend(try_actions);
                        control_actions.extend(all_catch_actions);
                        return control_actions;
                    }
                } else if try_leaves && try_stmt.catch_clauses.is_empty() {
                    control_actions.extend(try_actions);
                    return control_actions;
                }

                // Check finally block
                if let Some(finally) = &try_stmt.finally_clause {
                    let finally_actions = get_control_actions(
                        finally.block.statements.as_slice(),
                        analysis_data,
                        break_context,
                        return_is_exit,
                    );

                    if !finally_actions.contains(&ControlAction::None) {
                        control_actions.retain(|a| *a != ControlAction::None);
                        control_actions.extend(finally_actions);
                        return control_actions;
                    }
                }

                control_actions.extend(try_actions);
                control_actions.retain(|a| *a != ControlAction::None);
            }

            // Block statement (e.g., { ... })
            Statement::Block(block) => {
                let block_actions = get_control_actions(
                    block.statements.as_slice(),
                    analysis_data,
                    break_context,
                    return_is_exit,
                );

                if !block_actions.contains(&ControlAction::None) {
                    control_actions.extend(block_actions);
                    control_actions.retain(|a| *a != ControlAction::None);
                    return control_actions;
                }

                control_actions.extend(
                    block_actions
                        .into_iter()
                        .filter(|a| *a != ControlAction::None),
                );
            }

            // These don't affect control flow
            Statement::Echo(_)
            | Statement::Use(_)
            | Statement::Class(_)
            | Statement::Interface(_)
            | Statement::Trait(_)
            | Statement::Enum(_)
            | Statement::Function(_)
            | Statement::Constant(_)
            | Statement::OpeningTag(_)
            | Statement::ClosingTag(_)
            | Statement::Inline(_)
            | Statement::Declare(_)
            | Statement::Goto(_)
            | Statement::Label(_)
            | Statement::Global(_)
            | Statement::Static(_)
            | Statement::Unset(_)
            | Statement::HaltCompiler(_)
            | Statement::EchoTag(_)
            | Statement::Noop(_)
            | Statement::Namespace(_) => {}

            // Catch-all for any future statement types
            _ => {}
        }
    }

    control_actions.insert(ControlAction::None);
    control_actions
}

/// Check if statements only throw or exit (useful for various analyses).
pub fn only_throws_or_exits(stmts: &[Statement<'_>], analysis_data: &FunctionAnalysisData) -> bool {
    if stmts.is_empty() {
        return false;
    }

    for stmt in stmts.iter().rev() {
        if let Statement::Expression(expr_stmt) = stmt {
            use mago_span::HasSpan;

            if matches!(expr_stmt.expression, Expression::Throw(_)) {
                return true;
            }

            if let Expression::Construct(construct) = expr_stmt.expression {
                if matches!(construct, Construct::Exit(_) | Construct::Die(_)) {
                    return true;
                }
            }

            let span = expr_stmt.expression.span();
            let pos = (span.start.offset, span.end.offset);
            if let Some(t) = analysis_data.get_expr_type(pos) {
                if t.is_nothing() {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if statements only throw (no exit, no return).
pub fn only_throws(stmts: &[Statement<'_>]) -> bool {
    if stmts.len() != 1 {
        return false;
    }

    if let Some(Statement::Expression(expr_stmt)) = stmts.first() {
        return matches!(expr_stmt.expression, Expression::Throw(_));
    }

    false
}
