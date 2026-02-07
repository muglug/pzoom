//! Unary operation analyzer.

use std::collections::BTreeMap;

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::{UnaryPostfix, UnaryPrefix, UnaryPrefixOperator};
use mago_syntax::ast::ast::variable::Variable;
use rustc_hash::FxHashSet;

use pzoom_code_info::algebra::ClauseKey;
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr::cast_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;

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

    let operand_pos = expression_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let operand_type = analysis_data.get_expr_type(operand_pos);

    let expr_type = match &unary.operator {
        // Boolean not
        UnaryPrefixOperator::Not(_) => {
            if let Some(op_type) = operand_type.as_deref() {
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
            }
        }

        // Arithmetic negation/plus
        UnaryPrefixOperator::Negation(_) => {
            if let Some(op_type) = operand_type {
                // If operand is a literal int, negate it
                if let Some(TAtomic::TLiteralInt { value }) = op_type.types.first() {
                    if op_type.types.len() == 1 {
                        return analysis_data.set_expr_type(
                            pos,
                            TUnion::new(TAtomic::TLiteralInt { value: -value }),
                        );
                    }
                }
                if op_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
                {
                    TUnion::float()
                } else {
                    TUnion::int()
                }
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }

        UnaryPrefixOperator::Plus(_) => {
            if let Some(op_type) = operand_type {
                if op_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
                {
                    TUnion::float()
                } else {
                    TUnion::int_from_calculation()
                }
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
            if operand_type
                .as_ref()
                .is_some_and(|t| union_is_string_like_for_bitwise(t))
            {
                TUnion::string()
            } else {
                TUnion::int()
            }
        }

        // Pre-increment/decrement - returns the modified value
        UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_) => {
            // Update the variable's type in context if this is a variable
            maybe_emit_increment_operand_issue(
                analyzer,
                operand_type.as_deref(),
                pos,
                analysis_data,
            );
            let result_type = get_increment_result_type(operand_type.as_deref());
            maybe_emit_undefined_increment_variable(
                analyzer,
                unary.operand,
                analysis_data,
                context,
            );
            update_var_type_for_increment(
                analyzer,
                unary.operand,
                &result_type,
                analysis_data,
                context,
            );
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
        UnaryPrefixOperator::ArrayCast(_, _) => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        UnaryPrefixOperator::ObjectCast(_, _) => TUnion::new(TAtomic::TObject),
        UnaryPrefixOperator::UnsetCast(_, _) => TUnion::null(),
        UnaryPrefixOperator::VoidCast(_, _) => TUnion::void(),
    };

    analysis_data.set_expr_type(pos, expr_type);
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
    let operand_type = analysis_data.get_expr_type(operand_pos);

    // Post increment/decrement returns the original value before modification
    let expr_type = operand_type
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_else(TUnion::mixed);

    maybe_emit_increment_operand_issue(analyzer, operand_type.as_deref(), pos, analysis_data);
    maybe_emit_undefined_increment_variable(analyzer, unary.operand, analysis_data, context);

    // Update the variable's type in context (the variable gets the incremented value)
    let new_var_type = get_increment_result_type(operand_type.as_deref());
    update_var_type_for_increment(
        analyzer,
        unary.operand,
        &new_var_type,
        analysis_data,
        context,
    );

    analysis_data.set_expr_type(pos, expr_type);
}

/// Get the result type after incrementing/decrementing a value.
fn get_increment_result_type(operand_type: Option<&TUnion>) -> TUnion {
    match operand_type {
        Some(t) => {
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

            // If it's a literal int, we can't preserve the literal value since we don't
            // know if it's increment or decrement. Just return int.
            if t.types.iter().all(|a| {
                matches!(
                    a,
                    TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TNumeric
                )
            }) {
                TUnion::int_from_calculation()
            } else if t
                .types
                .iter()
                .any(|a| matches!(a, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
            {
                TUnion::float()
            } else {
                // PHP's increment behavior on non-numeric types is complex,
                // fall back to numeric
                TUnion::new(TAtomic::TNumeric)
            }
        }
        None => TUnion::new(TAtomic::TNumeric),
    }
}

/// Update a variable's type in the context after increment/decrement.
fn update_var_type_for_increment(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    new_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if let Expression::Variable(Variable::Direct(direct)) = expr.unparenthesized() {
        let var_id = analyzer.interner.intern(direct.name);
        context.set_var_type(var_id, new_type.clone());

        clear_dependent_property_types(analyzer, context, direct.name);
        clear_dependent_array_access_types(analyzer, context, direct.name);
        clear_dependent_class_string_origins(context, var_id);
        remove_var_clauses_from_context(context, direct.name);
        return;
    }

    let Some(var_name) = expression_identifier::get_expression_var_key(expr) else {
        return;
    };

    let var_id = analyzer.interner.intern(&var_name);
    context.locals.insert(var_id, new_type.clone());

    if var_name.ends_with(']') {
        let mut assertions = BTreeMap::new();
        assertions.insert(var_name.clone(), vec![Assertion::IsIsset]);
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
            false,
            None,
        );
    }
    remove_var_clauses_from_context(context, &var_name);
}

fn clear_dependent_property_types(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
) {
    let property_prefix = format!("{var_name}->");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            analyzer
                .interner
                .lookup(*var_id)
                .as_ref()
                .starts_with(&property_prefix)
        })
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
        context.class_string_origins.remove(&key);
    }
}

fn clear_dependent_array_access_types(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
) {
    let key_fragment = format!("[{var_name}]");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            analyzer
                .interner
                .lookup(*var_id)
                .as_ref()
                .contains(&key_fragment)
        })
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
        context.class_string_origins.remove(&key);
    }
}

fn clear_dependent_class_string_origins(context: &mut BlockContext, source_var_id: pzoom_str::StrId) {
    let dependent_keys: Vec<_> = context
        .class_string_origins
        .iter()
        .filter_map(|(class_var_id, tracked_source_var_id)| {
            if *tracked_source_var_id == source_var_id {
                Some(*class_var_id)
            } else {
                None
            }
        })
        .collect();

    for class_var_id in dependent_keys {
        context.class_string_origins.remove(&class_var_id);
    }
}

fn remove_var_clauses_from_context(context: &mut BlockContext, assigned_var_name: &str) {
    context.clauses.retain(|clause| {
        !clause
            .possibilities
            .keys()
            .any(|key| matches_assignment_target_key(key, assigned_var_name))
    });
}

fn matches_assignment_target_key(key: &ClauseKey, assigned_var_name: &str) -> bool {
    match key {
        ClauseKey::Name(name) => {
            name == assigned_var_name
                || name.starts_with(&format!("{}[", assigned_var_name))
                || name.starts_with(&format!("{}->", assigned_var_name))
                || name.contains(&format!("[{}]", assigned_var_name))
        }
        ClauseKey::Range(..) => false,
    }
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

    let is_defined = analyzer
        .interner
        .find(direct.name)
        .and_then(|var_id| context.get_var_type(var_id))
        .is_some();

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
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(operand_type) = operand_type else {
        return;
    };

    let mut saw_true_or_bool = false;
    let mut saw_false = false;
    let mut saw_string = false;

    for atomic in &operand_type.types {
        match atomic {
            TAtomic::TTrue | TAtomic::TBool => saw_true_or_bool = true,
            TAtomic::TFalse => saw_false = true,
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString => saw_string = true,
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
    } else if saw_string {
        (
            IssueKind::StringIncrement,
            "Possibly unintended string increment".to_string(),
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
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
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
