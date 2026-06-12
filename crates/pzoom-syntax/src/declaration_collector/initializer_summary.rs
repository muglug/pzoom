//! Scan-time property-initialization summaries.
//!
//! Psalm checks `PropertyNotSetInConstructor` / `UninitializedProperty` /
//! `MissingConstructor` by re-analyzing the constructor (and the methods it
//! calls) with a `collect_initializations` context
//! (`ClassAnalyzer::checkPropertyInitialization`). pzoom can't re-run method
//! bodies from other files at class-check time, so the scanner summarizes each
//! method body once:
//!
//! - which `$this->prop` properties the body assigns on **every** control-flow
//!   path (the analog of the property still being in `vars_in_scope` when
//!   Psalm's simulated constructor finishes),
//! - which `$this`/`self::`/`static::`/`parent::` methods it calls on every
//!   path (Psalm follows those calls via `collectSpecialInformation`),
//! - reads of `$this->prop` that happen before any assignment or method call
//!   could have initialized the property (Psalm's `UninitializedProperty`).
//!
//! The class analyzer later expands constructor summaries across the class
//! hierarchy to decide which properties end up initialized.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::assignment::AssignmentOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::control_flow::r#if::IfBody;
use mago_syntax::ast::ast::control_flow::switch::SwitchCase;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::r#loop::foreach::{ForeachBody, ForeachTarget};
use mago_syntax::ast::ast::r#loop::r#for::ForBody;
use mago_syntax::ast::ast::r#loop::r#while::WhileBody;
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::string::{CompositeString, StringPart};
use mago_syntax::ast::ast::variable::Variable;

/// One step of a method body relevant to property initialization, in
/// execution order. `Branch` captures an exhaustive alternation (an
/// `if`/`else`, a `try` whose catches re-throw, ...): every path takes
/// exactly one of the alternatives, so an assignment establishes itself only
/// if every alternative (after expanding the methods it calls) establishes
/// it. Calls resolve at check time the way Psalm's
/// `collectSpecialInformation` resolves them.
#[derive(Debug, Clone, PartialEq)]
pub enum SummaryEvent<'arena> {
    /// `$this->prop = ...` (or a bare `$this->prop` passed to a followable
    /// call, where a by-ref parameter may assign through it).
    Assign(&'arena str),
    /// `$this->m()`, `self::m()`, `static::m()`.
    ThisCall(&'arena str),
    /// `parent::m()`.
    ParentCall(&'arena str),
    /// `SomeClass::m()` with the class name as written (resolved at check
    /// time against the analyzed class's ancestors).
    NamedCall(&'arena str, &'arena str),
    /// Exhaustive alternation over the non-diverged branches' event lists.
    Branch(Vec<Vec<SummaryEvent<'arena>>>),
}

/// Per-method initialization summary (see module docs).
#[derive(Debug, Default)]
pub struct InitializerSummary<'arena> {
    pub events: Vec<SummaryEvent<'arena>>,
    /// `(property, offset)` reads of `$this->prop` reached before any
    /// assignment to the property and before any method call that could have
    /// initialized it. Conditional reads count (Psalm reports a read of an
    /// uninitialized property regardless of the branch it sits in).
    pub uninit_reads: Vec<(&'arena str, u32)>,
}

#[derive(Debug, Clone, Default)]
struct WalkState<'arena> {
    /// Events on this path, in order (only definite ones are recorded).
    events: Vec<SummaryEvent<'arena>>,
    /// Assigned on *some* path so far (suppresses uninitialized-read reports).
    may_assigned: Vec<&'arena str>,
    /// Any method call has happened on this path (a called method may have
    /// initialized arbitrary properties, so later reads are not reported).
    seen_call: bool,
    /// The path has terminated (return/throw/exit).
    diverged: bool,
}

impl<'arena> WalkState<'arena> {
    /// A fresh state for one branch of an alternation: events accumulate
    /// separately, read-suppression facts carry over.
    fn branch(&self) -> WalkState<'arena> {
        WalkState {
            events: Vec::new(),
            may_assigned: self.may_assigned.clone(),
            seen_call: self.seen_call,
            diverged: false,
        }
    }

    fn note_assign(&mut self, name: &'arena str, definite: bool) {
        if definite {
            self.events.push(SummaryEvent::Assign(name));
        }
        if !self.may_assigned.contains(&name) {
            self.may_assigned.push(name);
        }
    }

    fn note_call(&mut self, call: SummaryEvent<'arena>, definite: bool) {
        if definite {
            self.events.push(call);
        }
        self.seen_call = true;
    }
}

pub fn summarize_method_body<'arena>(
    statements: &[Statement<'arena>],
) -> InitializerSummary<'arena> {
    let mut summary = InitializerSummary::default();
    let mut state = WalkState::default();
    walk_statements(statements, &mut state, &mut summary);
    summary.events = state.events;
    summary
}

/// Merge sibling branch outcomes back into `state`. `exhaustive` means the
/// branches cover every path (an `if` with an `else`, a `try` with catches,
/// ...); only then do the branches' events survive, as an alternation.
/// May-facts accumulate from all branches either way.
fn merge_branches<'arena>(
    state: &mut WalkState<'arena>,
    branches: Vec<WalkState<'arena>>,
    exhaustive: bool,
) {
    let mut live: Vec<Vec<SummaryEvent<'arena>>> = branches
        .iter()
        .filter(|branch| !branch.diverged)
        .map(|branch| branch.events.clone())
        .collect();

    if exhaustive {
        if live.is_empty() {
            state.diverged = true;
        } else if live.len() == 1 {
            state.events.append(&mut live[0]);
        } else {
            state.events.push(SummaryEvent::Branch(live));
        }
    }

    for branch in &branches {
        for name in &branch.may_assigned {
            if !state.may_assigned.contains(name) {
                state.may_assigned.push(name);
            }
        }
        state.seen_call = state.seen_call || branch.seen_call;
    }
}

fn walk_statements<'arena>(
    statements: &[Statement<'arena>],
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    for statement in statements {
        if state.diverged {
            return;
        }
        walk_statement(statement, state, summary);
    }
}

fn walk_statement<'arena>(
    statement: &Statement<'arena>,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    match statement {
        Statement::Expression(expression_statement) => {
            walk_expression(expression_statement.expression, true, state, summary);
        }
        Statement::Block(block) => {
            walk_statements(block.statements.as_slice(), state, summary);
        }
        Statement::Echo(echo) => {
            for value in echo.values.iter() {
                walk_expression(value, true, state, summary);
            }
        }
        Statement::Return(return_statement) => {
            if let Some(value) = &return_statement.value {
                walk_expression(value, true, state, summary);
            }
            state.diverged = true;
        }
        Statement::If(if_statement) => {
            walk_expression(if_statement.condition, true, state, summary);
            let mut branches = Vec::new();
            let mut has_else = false;
            match &if_statement.body {
                IfBody::Statement(body) => {
                    let mut branch = state.branch();
                    walk_statement(body.statement, &mut branch, summary);
                    branches.push(branch);
                    for else_if in body.else_if_clauses.iter() {
                        // The elseif condition is evaluated only on some paths.
                        walk_expression(else_if.condition, false, state, summary);
                        let mut branch = state.branch();
                        walk_statement(else_if.statement, &mut branch, summary);
                        branches.push(branch);
                    }
                    if let Some(else_clause) = &body.else_clause {
                        has_else = true;
                        let mut branch = state.branch();
                        walk_statement(else_clause.statement, &mut branch, summary);
                        branches.push(branch);
                    }
                }
                IfBody::ColonDelimited(body) => {
                    let mut branch = state.branch();
                    walk_statements(body.statements.as_slice(), &mut branch, summary);
                    branches.push(branch);
                    for else_if in body.else_if_clauses.iter() {
                        walk_expression(else_if.condition, false, state, summary);
                        let mut branch = state.branch();
                        walk_statements(else_if.statements.as_slice(), &mut branch, summary);
                        branches.push(branch);
                    }
                    if let Some(else_clause) = &body.else_clause {
                        has_else = true;
                        let mut branch = state.branch();
                        walk_statements(else_clause.statements.as_slice(), &mut branch, summary);
                        branches.push(branch);
                    }
                }
            }
            merge_branches(state, branches, has_else);
        }
        Statement::While(while_statement) => {
            walk_expression(while_statement.condition, true, state, summary);
            let mut body_state = state.branch();
            match &while_statement.body {
                WhileBody::Statement(body) => walk_statement(body, &mut body_state, summary),
                WhileBody::ColonDelimited(body) => {
                    walk_statements(body.statements.as_slice(), &mut body_state, summary)
                }
            }
            // The body may never run: keep only may-facts.
            merge_branches(state, vec![body_state], false);
        }
        Statement::DoWhile(do_while) => {
            // A do-while body runs at least once.
            walk_statement(do_while.statement, state, summary);
            if !state.diverged {
                walk_expression(do_while.condition, true, state, summary);
            }
        }
        Statement::For(for_statement) => {
            for init in for_statement.initializations.iter() {
                walk_expression(init, true, state, summary);
            }
            for condition in for_statement.conditions.iter() {
                walk_expression(condition, true, state, summary);
            }
            let mut body_state = state.branch();
            match &for_statement.body {
                ForBody::Statement(body) => walk_statement(body, &mut body_state, summary),
                ForBody::ColonDelimited(body) => {
                    walk_statements(body.statements.as_slice(), &mut body_state, summary)
                }
            }
            for increment in for_statement.increments.iter() {
                walk_expression(increment, false, &mut body_state, summary);
            }
            merge_branches(state, vec![body_state], false);
        }
        Statement::Foreach(foreach) => {
            walk_expression(foreach.expression, true, state, summary);
            let mut body_state = state.branch();
            match &foreach.target {
                ForeachTarget::Value(target) => {
                    walk_assignment_target(target.value, false, &mut body_state, summary);
                }
                ForeachTarget::KeyValue(target) => {
                    walk_assignment_target(target.key, false, &mut body_state, summary);
                    walk_assignment_target(target.value, false, &mut body_state, summary);
                }
            }
            match &foreach.body {
                ForeachBody::Statement(body) => walk_statement(body, &mut body_state, summary),
                ForeachBody::ColonDelimited(body) => {
                    walk_statements(body.statements.as_slice(), &mut body_state, summary)
                }
            }
            merge_branches(state, vec![body_state], false);
        }
        Statement::Switch(switch) => {
            walk_expression(switch.expression, true, state, summary);
            // Conservative: case bodies contribute may-facts only (fallthrough
            // and break placement make must-analysis unreliable).
            let mut branches = Vec::new();
            for case in switch.body.cases() {
                let mut branch = state.branch();
                match case {
                    SwitchCase::Expression(case) => {
                        walk_expression(case.expression, false, &mut branch, summary);
                        walk_statements(case.statements.as_slice(), &mut branch, summary);
                    }
                    SwitchCase::Default(case) => {
                        walk_statements(case.statements.as_slice(), &mut branch, summary);
                    }
                }
                branches.push(branch);
            }
            merge_branches(state, branches, false);
        }
        Statement::Try(try_statement) => {
            let mut try_state = state.branch();
            walk_statements(try_statement.block.statements.as_slice(), &mut try_state, summary);
            if try_statement.catch_clauses.is_empty() {
                state.diverged = try_state.diverged;
                merge_branches(state, vec![try_state], true);
            } else {
                let try_seen_call = try_state.seen_call;
                let try_may_assigned = try_state.may_assigned.clone();
                let mut branches = vec![try_state];
                for catch in try_statement.catch_clauses.iter() {
                    let mut catch_state = state.branch();
                    // The try block may have partially run before the catch.
                    catch_state.seen_call = catch_state.seen_call || try_seen_call;
                    for name in &try_may_assigned {
                        if !catch_state.may_assigned.contains(name) {
                            catch_state.may_assigned.push(name);
                        }
                    }
                    walk_statements(catch.block.statements.as_slice(), &mut catch_state, summary);
                    branches.push(catch_state);
                }
                // A catch whose body always rethrows/returns drops out of the
                // merge, so straight-line try assignments survive.
                merge_branches(state, branches, true);
            }
            if let Some(finally) = &try_statement.finally_clause {
                let was_diverged = state.diverged;
                state.diverged = false;
                walk_statements(finally.block.statements.as_slice(), state, summary);
                state.diverged = state.diverged || was_diverged;
            }
        }
        Statement::Continue(_) | Statement::Break(_) => {
            state.diverged = true;
        }
        Statement::Unset(unset) => {
            // unset($this->prop) is not a read; deinitialization in a
            // constructor is rare enough to ignore for the must-set.
            for value in unset.values.iter() {
                if this_property_name(value).is_none() {
                    walk_expression(value, true, state, summary);
                }
            }
        }
        Statement::Global(_) | Statement::Static(_) | Statement::Goto(_) | Statement::Label(_) => {}
        // Nested declarations don't run as part of this body.
        Statement::Class(_)
        | Statement::Interface(_)
        | Statement::Trait(_)
        | Statement::Enum(_)
        | Statement::Function(_)
        | Statement::Constant(_) => {}
        _ => {}
    }
}

/// If `expr` is `$this->name` with a literal name (possibly behind
/// array-access/append subscripts), return the property name.
fn this_property_name<'arena>(expr: &Expression<'arena>) -> Option<&'arena str> {
    if let Expression::Access(Access::Property(property_access)) = expr.unparenthesized()
        && is_this(property_access.object)
        && let ClassLikeMemberSelector::Identifier(identifier) = &property_access.property
    {
        return Some(identifier.value);
    }
    None
}

fn is_this(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Variable(Variable::Direct(variable)) if variable.name == "$this"
    )
}

/// Handle an assignment target: mark `$this->prop` (possibly subscripted, or
/// inside list()/[] destructuring) as assigned without recording a read.
fn walk_assignment_target<'arena>(
    target: &Expression<'arena>,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    let mut target = target.unparenthesized();
    loop {
        match target {
            Expression::ArrayAccess(array_access) => {
                walk_expression(array_access.index, definite, state, summary);
                target = array_access.array.unparenthesized();
            }
            Expression::ArrayAppend(array_append) => {
                target = array_append.array.unparenthesized();
            }
            _ => break,
        }
    }

    if let Some(name) = this_property_name(target) {
        state.note_assign(name, definite);
        return;
    }

    match target {
        Expression::Array(array) => {
            for element in array.elements.iter() {
                walk_destructure_element(element, definite, state, summary);
            }
        }
        Expression::LegacyArray(array) => {
            for element in array.elements.iter() {
                walk_destructure_element(element, definite, state, summary);
            }
        }
        Expression::List(list) => {
            for element in list.elements.iter() {
                walk_destructure_element(element, definite, state, summary);
            }
        }
        other => walk_expression(other, definite, state, summary),
    }
}

fn walk_destructure_element<'arena>(
    element: &ArrayElement<'arena>,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    match element {
        ArrayElement::KeyValue(key_value) => {
            walk_expression(key_value.key, definite, state, summary);
            walk_assignment_target(key_value.value, definite, state, summary);
        }
        ArrayElement::Value(value) => {
            walk_assignment_target(value.value, definite, state, summary);
        }
        ArrayElement::Variadic(variadic) => {
            walk_assignment_target(variadic.value, definite, state, summary);
        }
        ArrayElement::Missing(_) => {}
    }
}

/// Walk an expression in evaluation order. `definite` is true when this
/// expression is evaluated on every path through the enclosing statement;
/// only definite assignments/calls feed the must-sets, while reads and
/// may-facts are recorded regardless.
fn walk_expression<'arena>(
    expr: &Expression<'arena>,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    match expr {
        Expression::Parenthesized(parenthesized) => {
            walk_expression(parenthesized.expression, definite, state, summary);
        }
        Expression::Assignment(assignment) => {
            // `??=` doesn't read its target (isset semantics); other compound
            // operators read it, but a read inside the property's own
            // (re)assignment is what Psalm reports — keep it simple and treat
            // every assignment target as a pure write.
            let _ = matches!(assignment.operator, AssignmentOperator::Coalesce(_));
            walk_expression(assignment.rhs, definite, state, summary);
            walk_assignment_target(assignment.lhs, definite, state, summary);
        }
        Expression::Binary(binary) => {
            walk_expression(binary.lhs, definite, state, summary);
            // Short-circuit / coalesce right sides run conditionally.
            let rhs_definite = definite && !binary.operator.is_logical()
                && !matches!(
                    binary.operator,
                    mago_syntax::ast::ast::binary::BinaryOperator::NullCoalesce(_)
                );
            walk_expression(binary.rhs, rhs_definite, state, summary);
        }
        Expression::UnaryPrefix(unary) => {
            walk_expression(unary.operand, definite, state, summary);
        }
        Expression::UnaryPostfix(unary) => {
            walk_expression(unary.operand, definite, state, summary);
        }
        Expression::Conditional(conditional) => {
            walk_expression(conditional.condition, definite, state, summary);
            if let Some(then) = &conditional.then {
                walk_expression(then, false, state, summary);
            }
            walk_expression(conditional.r#else, false, state, summary);
        }
        Expression::Match(match_expression) => {
            walk_expression(match_expression.expression, definite, state, summary);
            for arm in match_expression.arms.iter() {
                match arm {
                    mago_syntax::ast::ast::control_flow::r#match::MatchArm::Expression(arm) => {
                        for condition in arm.conditions.iter() {
                            walk_expression(condition, false, state, summary);
                        }
                        walk_expression(arm.expression, false, state, summary);
                    }
                    mago_syntax::ast::ast::control_flow::r#match::MatchArm::Default(arm) => {
                        walk_expression(arm.expression, false, state, summary);
                    }
                }
            }
        }
        Expression::Throw(throw) => {
            walk_expression(throw.exception, definite, state, summary);
            if definite {
                state.diverged = true;
            }
        }
        Expression::Call(call) => walk_call(call, definite, state, summary),
        Expression::Access(access) => match access {
            Access::Property(property_access) => {
                if let Some(name) = this_property_name(expr) {
                    record_read(name, property_access.property.span().start.offset, state, summary);
                } else {
                    walk_expression(property_access.object, definite, state, summary);
                    if let ClassLikeMemberSelector::Expression(selector) = &property_access.property
                    {
                        walk_expression(selector.expression, definite, state, summary);
                    }
                }
            }
            Access::NullSafeProperty(property_access) => {
                walk_expression(property_access.object, definite, state, summary);
            }
            Access::StaticProperty(property_access) => {
                walk_expression(property_access.class, definite, state, summary);
            }
            Access::ClassConstant(constant_access) => {
                walk_expression(constant_access.class, definite, state, summary);
            }
        },
        Expression::ArrayAccess(array_access) => {
            walk_expression(array_access.array, definite, state, summary);
            walk_expression(array_access.index, definite, state, summary);
        }
        Expression::ArrayAppend(array_append) => {
            walk_expression(array_append.array, definite, state, summary);
        }
        Expression::Array(array) => {
            for element in array.elements.iter() {
                walk_value_element(element, definite, state, summary);
            }
        }
        Expression::LegacyArray(array) => {
            for element in array.elements.iter() {
                walk_value_element(element, definite, state, summary);
            }
        }
        Expression::List(list) => {
            for element in list.elements.iter() {
                walk_value_element(element, definite, state, summary);
            }
        }
        Expression::CompositeString(composite) => {
            let parts = match composite {
                CompositeString::ShellExecute(string) => &string.parts,
                CompositeString::Interpolated(string) => &string.parts,
                CompositeString::Document(string) => &string.parts,
            };
            for part in parts.iter() {
                match part {
                    StringPart::Expression(expression) => {
                        walk_expression(expression, definite, state, summary);
                    }
                    StringPart::BracedExpression(braced) => {
                        walk_expression(braced.expression, definite, state, summary);
                    }
                    StringPart::Literal(_) => {}
                }
            }
        }
        Expression::Construct(construct) => match construct {
            // isset()/empty() don't read for initialization purposes.
            Construct::Isset(_) | Construct::Empty(_) => {}
            Construct::Eval(eval) => walk_expression(eval.value, definite, state, summary),
            Construct::Print(print) => walk_expression(print.value, definite, state, summary),
            Construct::Exit(exit) => {
                if let Some(arguments) = &exit.arguments {
                    for argument in arguments.arguments.iter() {
                        walk_expression(argument.value(), definite, state, summary);
                    }
                }
                if definite {
                    state.diverged = true;
                }
            }
            Construct::Die(die) => {
                if let Some(arguments) = &die.arguments {
                    for argument in arguments.arguments.iter() {
                        walk_expression(argument.value(), definite, state, summary);
                    }
                }
                if definite {
                    state.diverged = true;
                }
            }
            Construct::Include(_)
            | Construct::IncludeOnce(_)
            | Construct::Require(_)
            | Construct::RequireOnce(_) => {}
        },
        Expression::Clone(clone) => {
            walk_expression(clone.object, definite, state, summary);
        }
        Expression::Instantiation(instantiation) => {
            walk_expression(instantiation.class, definite, state, summary);
            if let Some(arguments) = &instantiation.argument_list {
                for argument in arguments.arguments.iter() {
                    walk_expression(argument.value(), definite, state, summary);
                }
            }
        }
        Expression::Yield(yield_expression) => {
            use mago_syntax::ast::ast::r#yield::Yield;
            match yield_expression {
                Yield::Value(value) => {
                    if let Some(value) = &value.value {
                        walk_expression(value, definite, state, summary);
                    }
                }
                Yield::Pair(pair) => {
                    walk_expression(pair.key, definite, state, summary);
                    walk_expression(pair.value, definite, state, summary);
                }
                Yield::From(from) => {
                    walk_expression(from.iterator, definite, state, summary);
                }
            }
        }
        Expression::Pipe(pipe) => {
            walk_expression(pipe.input, definite, state, summary);
            walk_expression(pipe.callable, definite, state, summary);
        }
        // Closures capture `$this` but don't run here; anonymous classes and
        // arrow functions likewise.
        Expression::Closure(_) | Expression::ArrowFunction(_) | Expression::AnonymousClass(_) => {}
        Expression::Variable(Variable::Indirect(indirect)) => {
            walk_expression(indirect.expression, definite, state, summary);
        }
        _ => {}
    }
}

fn walk_value_element<'arena>(
    element: &ArrayElement<'arena>,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    match element {
        ArrayElement::KeyValue(key_value) => {
            walk_expression(key_value.key, definite, state, summary);
            walk_expression(key_value.value, definite, state, summary);
        }
        ArrayElement::Value(value) => walk_expression(value.value, definite, state, summary),
        ArrayElement::Variadic(variadic) => {
            walk_expression(variadic.value, definite, state, summary);
        }
        ArrayElement::Missing(_) => {}
    }
}

fn record_read<'arena>(
    name: &'arena str,
    offset: u32,
    state: &WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    if state.seen_call || state.may_assigned.contains(&name) {
        return;
    }
    if !summary.uninit_reads.iter().any(|(read, _)| *read == name) {
        summary.uninit_reads.push((name, offset));
    }
}

fn walk_call<'arena>(
    call: &Call<'arena>,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    match call {
        Call::Function(function_call) => {
            walk_expression(function_call.function, definite, state, summary);
            for argument in function_call.argument_list.arguments.iter() {
                walk_expression(argument.value(), definite, state, summary);
            }
        }
        Call::Method(method_call) => {
            let on_this = is_this(method_call.object);
            if !on_this {
                walk_expression(method_call.object, definite, state, summary);
            }
            walk_followable_call_arguments(
                &method_call.argument_list,
                on_this,
                definite,
                state,
                summary,
            );
            if on_this {
                if let ClassLikeMemberSelector::Identifier(identifier) = &method_call.method {
                    state.note_call(SummaryEvent::ThisCall(identifier.value), definite);
                } else {
                    state.seen_call = true;
                }
            }
        }
        Call::NullSafeMethod(method_call) => {
            let on_this = is_this(method_call.object);
            if !on_this {
                walk_expression(method_call.object, definite, state, summary);
            }
            walk_followable_call_arguments(
                &method_call.argument_list,
                on_this,
                definite,
                state,
                summary,
            );
            if on_this {
                if let ClassLikeMemberSelector::Identifier(identifier) = &method_call.method {
                    state.note_call(SummaryEvent::ThisCall(identifier.value), definite);
                } else {
                    state.seen_call = true;
                }
            }
        }
        Call::StaticMethod(static_call) => {
            walk_followable_call_arguments(&static_call.argument_list, true, definite, state, summary);
            if let ClassLikeMemberSelector::Identifier(identifier) = &static_call.method {
                match static_call.class.unparenthesized() {
                    Expression::Self_(_) | Expression::Static(_) => {
                        state.note_call(SummaryEvent::ThisCall(identifier.value), definite);
                    }
                    Expression::Parent(_) => {
                        state.note_call(SummaryEvent::ParentCall(identifier.value), definite);
                    }
                    Expression::Identifier(class_identifier) => {
                        let value = class_identifier.value();
                        if value.eq_ignore_ascii_case("self")
                            || value.eq_ignore_ascii_case("static")
                        {
                            state.note_call(SummaryEvent::ThisCall(identifier.value), definite);
                        } else if value.eq_ignore_ascii_case("parent") {
                            state.note_call(SummaryEvent::ParentCall(identifier.value), definite);
                        } else {
                            // `AncestorClass::m()`: resolved at check time
                            // (Psalm follows static-dispatch ancestor calls).
                            state.note_call(
                                SummaryEvent::NamedCall(value, identifier.value),
                                definite,
                            );
                        }
                    }
                    other => {
                        walk_expression(other, definite, state, summary);
                        state.seen_call = true;
                    }
                }
            } else {
                state.seen_call = true;
            }
        }
    }
}

/// Walk a followable (`$this`-bound) call's arguments. A bare `$this->prop`
/// argument may bind to a by-ref parameter that assigns through it (Psalm
/// resolves this from the callee signature and `@param-out`); treat it as an
/// assignment rather than a read.
fn walk_followable_call_arguments<'arena>(
    argument_list: &mago_syntax::ast::ast::argument::ArgumentList<'arena>,
    followable: bool,
    definite: bool,
    state: &mut WalkState<'arena>,
    summary: &mut InitializerSummary<'arena>,
) {
    for argument in argument_list.arguments.iter() {
        if followable && let Some(name) = this_property_name(argument.value()) {
            state.note_assign(name, definite);
            continue;
        }
        walk_expression(argument.value(), definite, state, summary);
    }
}
