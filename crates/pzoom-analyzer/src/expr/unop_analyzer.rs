//! Unary operation analyzer.

use std::collections::BTreeMap;

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::{UnaryPostfix, UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;
use rustc_hash::FxHashSet;

use pzoom_code_info::VarName;
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr::cast_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze a unary prefix expression.
pub fn analyze_prefix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if cast_analyzer::is_cast_operator(&unary.operator) {
        cast_analyzer::analyze(analyzer, unary, pos, analysis_data, context);
        return;
    }

    // Psalm flips $context->inside_negation for `!` and gates the &&/|| if-body
    // merges on it: an operator inside a negation must not push its (un-negated)
    // narrowing into the enclosing if's shared body context.
    let saved_if_body_context = if matches!(unary.operator, UnaryPrefixOperator::Not(_)) {
        context.if_body_context.take()
    } else {
        None
    };
    // `@expr` suppresses runtime errors; Psalm records this on the context
    // (`Context::error_suppressing`) so list-destructuring underneath can widen
    // not-guaranteed targets with `null` (see AssignmentAnalyzer).
    let was_error_suppressing = context.error_suppressing;
    if matches!(unary.operator, UnaryPrefixOperator::ErrorControl(_)) {
        context.error_suppressing = true;
    }
    let operand_pos = expression_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    context.error_suppressing = was_error_suppressing;
    if let Some(if_body_context) = saved_if_body_context {
        context.if_body_context = Some(if_body_context);
    }
    let operand_type = analysis_data.expr_types.get(&operand_pos).cloned();

    let expr_type = match &unary.operator {
        // Boolean not
        UnaryPrefixOperator::Not(_) => {
            let result = if let Some(op_type) = operand_type.as_deref() {
                if op_type.is_always_falsy() {
                    let mut result = TUnion::new(TAtomic::TTrue);
                    result.from_docblock = op_type.from_docblock;
                    result
                } else if op_type.is_always_truthy() {
                    let mut result = TUnion::new(TAtomic::TFalse);
                    result.from_docblock = op_type.from_docblock;
                    result
                } else {
                    TUnion::bool()
                }
            } else {
                TUnion::bool()
            };

            // Hakana wires `!` through decision dataflow (which also records the
            // expression type).
            return expression_analyzer::add_decision_dataflow(
                analyzer,
                analysis_data,
                unary.operand,
                None,
                pos,
                result,
            );
        }

        // Arithmetic negation/plus — the result is derived from the operand:
        // keep its dataflow parents (Psalm treats `-$a`/`+$a` as uses of $a).
        UnaryPrefixOperator::Negation(_) => {
            if let Some(op_type) = operand_type {
                // If operand is a literal int, negate it
                if let Some(TAtomic::TLiteralInt { value }) = op_type.types.first() {
                    if op_type.types.len() == 1 {
                        let mut result = TUnion::new(TAtomic::TLiteralInt { value: -value });
                        result.parent_nodes = op_type.parent_nodes.clone();
                        analysis_data.expr_types.insert(pos, Rc::new(result));
                        return;
                    }
                }
                let mut result = if op_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
                {
                    TUnion::float()
                } else {
                    TUnion::int()
                };
                result.parent_nodes = op_type.parent_nodes.clone();
                result
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }

        UnaryPrefixOperator::Plus(_) => {
            if let Some(op_type) = operand_type {
                let mut result = if op_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
                {
                    TUnion::float()
                } else {
                    TUnion::int_from_calculation()
                };
                result.parent_nodes = op_type.parent_nodes.clone();
                result
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }

        // Bitwise not
        UnaryPrefixOperator::BitwiseNot(_) => {
            maybe_emit_bitwise_not_operand_issue(
                analyzer,
                operand_type.as_deref(),
                pos,
                analysis_data,
            );
            // The result is derived from the operand: keep its dataflow
            // parents (the operand is "used" by whatever consumes the result).
            let operand_parents = operand_type
                .as_ref()
                .map(|t| t.parent_nodes.clone())
                .unwrap_or_default();
            let mut result = if operand_type
                .as_ref()
                .is_some_and(|t| union_is_string_like_for_bitwise(t))
            {
                TUnion::string()
            } else {
                TUnion::int()
            };
            result.parent_nodes = operand_parents;
            result
        }

        // Pre-increment/decrement - returns the modified value
        UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_) => {
            let is_increment = matches!(unary.operator, UnaryPrefixOperator::PreIncrement(_));
            // Update the variable's type in context if this is a variable
            maybe_emit_increment_operand_issue(
                analyzer,
                operand_type.as_deref(),
                pos,
                is_increment,
                analysis_data,
            );
            let delta = if is_increment { 1 } else { -1 };
            let result_type = get_increment_result_type(
                operand_type.as_deref(),
                delta,
                context.inside_loop,
                context.inside_assignment,
            );
            maybe_emit_undefined_increment_variable(
                analyzer,
                unary.operand,
                analysis_data,
                context,
            );
            update_var_type_for_increment(
                analyzer,
                unary.operand,
                pos,
                &result_type,
                analysis_data,
                context,
            );
            // Pre-increment evaluates to the freshly-written value: surface
            // its dataflow node so `if (++$i > 10)` counts as a use.
            let mut result_type = result_type;
            if let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized()
                && let Some(stored) = context.get_var_type(direct.name)
            {
                result_type.parent_nodes = stored.parent_nodes.clone();
            }
            result_type
        }

        // Error control (@) - type is same as operand
        UnaryPrefixOperator::ErrorControl(_) => operand_type
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed),

        // Reference (&) - type is same as operand
        UnaryPrefixOperator::Reference(_) => operand_type
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed),

        // Type casts - delegate to cast_analyzer
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            TUnion::int()
        }
        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),
        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            TUnion::string()
        }
        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }
        UnaryPrefixOperator::ArrayCast(_, _) => {
            TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed()))
        }
        UnaryPrefixOperator::ObjectCast(_, _) => TUnion::new(TAtomic::TObject),
        UnaryPrefixOperator::UnsetCast(_, _) => TUnion::null(),
        UnaryPrefixOperator::VoidCast(_, _) => TUnion::void(),
    };

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

/// Analyze a unary postfix expression.
pub fn analyze_postfix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPostfix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let operand_pos = expression_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let operand_type = analysis_data.expr_types.get(&operand_pos).cloned();

    // Post increment/decrement returns the original value before modification
    let expr_type = operand_type
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_else(TUnion::mixed);

    let is_increment = matches!(
        unary.operator,
        mago_syntax::ast::ast::unary::UnaryPostfixOperator::PostIncrement(_)
    );
    maybe_emit_increment_operand_issue(
        analyzer,
        operand_type.as_deref(),
        pos,
        is_increment,
        analysis_data,
    );
    maybe_emit_undefined_increment_variable(analyzer, unary.operand, analysis_data, context);

    // Update the variable's type in context (the variable gets the incremented value)
    let delta = if is_increment { 1 } else { -1 };
    let new_var_type = get_increment_result_type(
        operand_type.as_deref(),
        delta,
        context.inside_loop,
        context.inside_assignment,
    );
    update_var_type_for_increment(
        analyzer,
        unary.operand,
        pos,
        &new_var_type,
        analysis_data,
        context,
    );

    // Psalm's IncDecExpressionAnalyzer: when the operand has a string member
    // and this is an increment, the EXPRESSION takes the arithmetic result
    // (numeric strings give int|float) rather than the pre-increment value.
    let expr_type = if delta > 0
        && operand_type.as_ref().is_some_and(|t| {
            t.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TString
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TNonEmptyString
                        | TAtomic::TNumericString
                        | TAtomic::TNonEmptyNumericString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString
                        | TAtomic::TTruthyString
                )
            })
        }) {
        new_var_type
    } else {
        expr_type
    };

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
}

/// Get the result type after incrementing/decrementing a value (`delta` is
/// +1 for `++`, -1 for `--`).
/// A string atomic whose increment is numeric (Psalm: TNumericString or a
/// numeric literal string increments to int|float, with no StringIncrement).
fn is_numericish_string_atomic(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => true,
        TAtomic::TLiteralString { value } => value.parse::<f64>().is_ok(),
        _ => false,
    }
}

fn get_increment_result_type(
    operand_type: Option<&TUnion>,
    delta: i64,
    inside_loop: bool,
    inside_assignment: bool,
) -> TUnion {
    match operand_type {
        Some(t) => {
            // Psalm's ArithmeticOpAnalyzer: incrementing a numeric string
            // yields int|float (from calculation); other strings stay strings.
            if t.types.iter().all(is_numericish_string_atomic) {
                let mut result = TUnion::from_types(vec![TAtomic::TFloat, TAtomic::TInt]);
                result.from_calculation = true;
                return result;
            }

            if t.types.iter().all(|a| {
                matches!(
                    a,
                    TAtomic::TString
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TNonEmptyString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString
                        | TAtomic::TTruthyString
                        | TAtomic::TClassString { .. }
                        | TAtomic::TLiteralClassString { .. }
                )
            }) {
                return TUnion::string();
            }

            // Psalm types `$i++` through ArithmeticOpAnalyzer as `$i + 1`
            // (VirtualPlus): inside a loop a literal widens to a half-open
            // range (`0` then `$i++` converges to int<1, max>); outside one
            // it stays literal arithmetic unless the increment is buried in
            // an assignment; ranges shift their bounds.
            if t.types.iter().all(|a| {
                matches!(
                    a,
                    TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
                )
            }) {
                let shifted: Vec<TAtomic> = t
                    .types
                    .iter()
                    .map(|a| match a {
                        TAtomic::TLiteralInt { value } => {
                            if inside_loop {
                                if delta > 0 {
                                    TAtomic::TIntRange {
                                        min: value.checked_add(delta),
                                        max: None,
                                    }
                                } else {
                                    TAtomic::TIntRange {
                                        min: None,
                                        max: value.checked_add(delta),
                                    }
                                }
                            } else if inside_assignment {
                                TAtomic::TInt
                            } else {
                                match value.checked_add(delta) {
                                    Some(new_value) => TAtomic::TLiteralInt { value: new_value },
                                    None => TAtomic::TInt,
                                }
                            }
                        }
                        TAtomic::TIntRange { min, max } => TAtomic::TIntRange {
                            min: min.map(|m| m.saturating_add(delta)),
                            max: max.map(|m| m.saturating_add(delta)),
                        },
                        other => other.clone(),
                    })
                    .collect();
                // Like Psalm's ArithmeticOpAnalyzer result, the int stays
                // flagged from_calculation: ++ can overflow to float, so a
                // later is_int/is_float check is not redundant.
                let mut shifted_union = TUnion::from_types(shifted);
                shifted_union.from_calculation = true;
                return shifted_union;
            }

            if t.types
                .iter()
                .any(|a| matches!(a, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
            {
                TUnion::float()
            } else {
                // Mixed unions increment per atomic (Psalm's virtual `$i + 1`
                // over each member): ints stay int, floats stay float, strings
                // become non-empty-string (`++"a"` is "b"); anything else
                // falls back to numeric.
                let mut incremented: Vec<TAtomic> = Vec::with_capacity(t.types.len() + 1);
                for atomic in &t.types {
                    // Numeric strings and `numeric` increment to int|float
                    // (Psalm's ArithmeticOpAnalyzer).
                    if is_numericish_string_atomic(atomic) || matches!(atomic, TAtomic::TNumeric) {
                        for incremented_atomic in [TAtomic::TInt, TAtomic::TFloat] {
                            if !incremented.contains(&incremented_atomic) {
                                incremented.push(incremented_atomic);
                            }
                        }
                        continue;
                    }
                    incremented.push(match atomic {
                        TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. } => {
                            TAtomic::TInt
                        }
                        TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => TAtomic::TFloat,
                        TAtomic::TString
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TNonEmptyString
                        | TAtomic::TLowercaseString
                        | TAtomic::TNonEmptyLowercaseString
                        | TAtomic::TTruthyString
                        | TAtomic::TClassString { .. }
                        | TAtomic::TLiteralClassString { .. } => TAtomic::TNonEmptyString,
                        _ => TAtomic::TNumeric,
                    });
                }
                let mut result = TUnion::from_types(incremented);
                result.from_calculation = true;
                result
            }
        }
        None => TUnion::new(TAtomic::TNumeric),
    }
}

/// Update a variable's type in the context after increment/decrement.
/// `full_pos` is the whole crement expression's span (`$i++` / `++$i`).
fn update_var_type_for_increment(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    full_pos: Pos,
    new_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if let Expression::Variable(Variable::Direct(direct)) = expr.unparenthesized() {
        let var_id = VarName::new(direct.name);
        let mut stored_type = new_type.clone();
        // `++$i` reads and rewrites the variable (Hakana's `$i = $i + 1`
        // rewrite): the old value's dataflow parents feed a fresh assignment
        // source so loop-carried uses see the increment.
        if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody {
            let span = expr.span();
            // The node id is keyed on the whole crement span: the operand
            // span already identifies the READ sink (`$result[$i++]` sinks
            // the old value as an array key), and a write node with the same
            // id would self-collide and never count as used by later reads.
            // The reported position stays on the operand, matching Psalm.
            let assignment_node = pzoom_code_info::DataFlowNode {
                id: pzoom_code_info::data_flow::node::DataFlowNodeId::Var(
                    pzoom_code_info::VarId(analyzer.interner.intern(&var_id)),
                    analyzer.file_path,
                    full_pos.0,
                    full_pos.1,
                ),
                kind: pzoom_code_info::data_flow::node::DataFlowNodeKind::VariableUseSource {
                    pos: crate::data_flow::make_data_flow_node_position(
                        analyzer,
                        (span.start.offset, span.end.offset),
                    ),
                    kind: if context.references_to_external_scope.contains(&var_id)
                        || context.static_var_ids.contains(&var_id)
                    {
                        pzoom_code_info::VariableSourceKind::InoutArg
                    } else {
                        pzoom_code_info::VariableSourceKind::Default
                    },
                    pure: false,
                    has_awaitable: false,
                    has_await_call: false,
                    has_parent_nodes: true,
                    from_loop_init: false,
                },
            };
            if let Some(old_type) = context.get_var_type(&var_id) {
                for parent_node in &old_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &parent_node.id,
                        &assignment_node.id,
                        pzoom_code_info::PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
            }
            analysis_data
                .data_flow_graph
                .add_node(assignment_node.clone());
            stored_type.parent_nodes = vec![assignment_node];
        }
        context.set_var_type(var_id, stored_type);

        clear_dependent_property_types(context, direct.name);
        clear_dependent_array_access_types(context, direct.name);
        context.invalidate_dependent_types(direct.name);
        remove_var_clauses_from_context(context, direct.name);
        return;
    }

    let Some(var_name) = expression_identifier::get_expression_var_key(expr) else {
        return;
    };

    let var_id = VarName::new(&var_name);
    context.locals.insert(var_id, new_type.clone());

    if var_name.ends_with(']') {
        let mut assertions = BTreeMap::new();
        assertions.insert(var_name.clone(), vec![vec![Assertion::IsIsset]]);
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = context.inside_loop;

        reconciler::reconcile_keyed_types(
            &assertions,
            context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::Silent,
            None,
        );
    }
    remove_var_clauses_from_context(context, &var_name);
}

fn clear_dependent_property_types(context: &mut BlockContext, var_name: &str) {
    let property_prefix = format!("{var_name}->");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.starts_with(&property_prefix))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

fn clear_dependent_array_access_types(context: &mut BlockContext, var_name: &str) {
    let key_fragment = format!("[{var_name}]");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.contains(&key_fragment))
        .cloned()
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
    }
}

fn remove_var_clauses_from_context(context: &mut BlockContext, assigned_var_name: &str) {
    context.remove_var_name_from_conflicting_clauses(assigned_var_name);
}

fn maybe_emit_undefined_increment_variable(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Expression::Variable(Variable::Direct(direct)) = expr else {
        return;
    };

    if !should_emit_undefined_variable(direct.name) {
        return;
    }

    let is_defined = context.locals.contains_key(direct.name);

    if is_defined {
        return;
    }

    let span = expr.span();
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedVariable,
        format!("Undefined variable ${}", normalize_var_name(direct.name)),
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
}

fn maybe_emit_increment_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    operand_type: Option<&TUnion>,
    pos: Pos,
    is_increment: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(operand_type) = operand_type else {
        return;
    };

    let mut saw_true_or_bool = false;
    let mut saw_false = false;
    let mut saw_string = false;
    // A numeric operand (int/float/numeric string) is a valid arithmetic
    // operand: its presence alongside a string makes a decrement only
    // *possibly* invalid (Psalm's `has_valid_left_operand`).
    let mut saw_numeric = false;

    for atomic in &operand_type.types {
        match atomic {
            TAtomic::TTrue | TAtomic::TBool => saw_true_or_bool = true,
            TAtomic::TFalse => saw_false = true,
            // Numeric strings increment to int|float — intended, no report
            // (Psalm's ArithmeticOpAnalyzer).
            atomic if is_numericish_string_atomic(atomic) => saw_numeric = true,
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString => saw_string = true,
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TNumeric => saw_numeric = true,
            _ => {}
        }
    }

    let (kind, message) = if saw_true_or_bool {
        (
            IssueKind::InvalidOperand,
            format!(
                "Cannot increment value of type {}",
                operand_type.get_id(Some(analyzer.interner))
            ),
        )
    } else if saw_false {
        (
            IssueKind::FalseOperand,
            "Cannot increment false".to_string(),
        )
    } else if saw_string && is_increment {
        // Psalm's IncDecExpressionAnalyzer routes `++` through the
        // TString+TInt branch of ArithmeticOpAnalyzer: a non-numeric string
        // increments to a non-empty-string and reports StringIncrement.
        (
            IssueKind::StringIncrement,
            "Possibly unintended string increment".to_string(),
        )
    } else if saw_string {
        // `--` (and `+`/`-` generally) instead analyzes `$var - 1`, where a
        // non-numeric string is an invalid numeric operand. It is only
        // *possibly* invalid when a valid numeric member is also present.
        (
            if saw_numeric {
                IssueKind::PossiblyInvalidOperand
            } else {
                IssueKind::InvalidOperand
            },
            format!(
                "Cannot perform a numeric operation with a non-numeric type {}",
                operand_type.get_id(Some(analyzer.interner))
            ),
        )
    } else {
        return;
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn maybe_emit_bitwise_not_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    operand_type: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(operand_type) = operand_type else {
        return;
    };

    let mut has_valid = false;
    let mut has_invalid = false;

    for atomic in &operand_type.types {
        if is_valid_bitwise_not_atomic(atomic) {
            has_valid = true;
        } else {
            has_invalid = true;
        }
    }

    if !has_invalid {
        return;
    }

    let kind = if has_valid {
        IssueKind::PossiblyInvalidOperand
    } else {
        IssueKind::InvalidOperand
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        format!(
            "Cannot use bitwise not on type {}",
            operand_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn is_valid_bitwise_not_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
    )
}

fn union_is_string_like_for_bitwise(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
            )
        })
}

fn normalize_var_name(name: &str) -> &str {
    name.strip_prefix('$').unwrap_or(name)
}

fn should_emit_undefined_variable(var_name: &str) -> bool {
    let normalized = normalize_var_name(var_name);
    !normalized.eq_ignore_ascii_case("this") && !is_superglobal(normalized)
}

fn is_superglobal(var_name: &str) -> bool {
    matches!(
        var_name,
        "GLOBALS"
            | "_SERVER"
            | "_GET"
            | "_POST"
            | "_FILES"
            | "_COOKIE"
            | "_SESSION"
            | "_REQUEST"
            | "_ENV"
            | "argc"
            | "argv"
    )
}
