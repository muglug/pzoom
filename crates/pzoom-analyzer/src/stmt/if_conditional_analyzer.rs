//! If-conditional helpers.

use mago_syntax::ast::ast::access::Access;
use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::{Binary, BinaryOperator};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::algebra::ClauseKey;
use pzoom_code_info::{Assertion, Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator};

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

    emit_docblock_type_check_contradiction(analyzer, expr, expr_pos, analysis_data, context);
    if context.is_some_and(|context| context.inside_loop) {
        return;
    }

    if emit_paradoxical_empty_issue(analyzer, expr, expr_pos, analysis_data) {
        return;
    }

    if emit_paradoxical_count_comparison_issue(analyzer, expr, expr_pos, analysis_data) {
        return;
    }

    emit_impossible_isset_check_contradiction(analyzer, expr, analysis_data);
    emit_impossible_builtin_type_check_contradiction(analyzer, expr, analysis_data);
    if let Some(condition_is_always_truthy) =
        get_clause_determined_truthiness_for_scalar_var(expr, &expr_type, context)
    {
        let issue_kind = if condition_is_always_truthy {
            if expr_type.from_docblock {
                IssueKind::RedundantConditionGivenDocblockType
            } else {
                IssueKind::RedundantCondition
            }
        } else if expr_type.from_docblock {
            IssueKind::DocblockTypeContradiction
        } else {
            IssueKind::TypeDoesNotContainType
        };
        let (line, col) = analyzer.get_line_column(expr_pos.0);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            format!(
                "Operand of type {} is always {}",
                expr_type.get_id(Some(analyzer.interner)),
                if condition_is_always_truthy {
                    "truthy"
                } else {
                    "falsy"
                }
            ),
            analyzer.file_path,
            expr_pos.0,
            expr_pos.1,
            line,
            col,
        ));
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

fn emit_paradoxical_count_comparison_issue(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> bool {
    let Expression::Binary(binary) = expr.unparenthesized() else {
        return false;
    };

    let Some((counted_expr, compared_count, operator)) = get_count_comparison(binary) else {
        return false;
    };

    let counted_pos = (
        counted_expr.start_offset() as u32,
        counted_expr.end_offset() as u32,
    );
    let Some(counted_type) = analysis_data.get_expr_type(counted_pos).map(|t| (*t).clone()) else {
        return false;
    };

    let Some((min_count, max_count)) = get_union_count_bounds(&counted_type) else {
        return false;
    };

    let (always_true, always_false) = match operator {
        CountComparisonOp::GreaterThan => (
            min_count > compared_count,
            max_count.is_some_and(|max_count| max_count <= compared_count),
        ),
        CountComparisonOp::GreaterThanOrEqual => (
            min_count >= compared_count,
            max_count.is_some_and(|max_count| max_count < compared_count),
        ),
        CountComparisonOp::LessThan => (
            max_count.is_some_and(|max_count| max_count < compared_count),
            min_count >= compared_count,
        ),
        CountComparisonOp::LessThanOrEqual => (
            max_count.is_some_and(|max_count| max_count <= compared_count),
            min_count > compared_count,
        ),
    };

    if !always_true && !always_false {
        return false;
    }

    let issue_kind = if always_true {
        if counted_type.from_docblock {
            IssueKind::RedundantConditionGivenDocblockType
        } else {
            IssueKind::RedundantCondition
        }
    } else if counted_type.from_docblock {
        IssueKind::DocblockTypeContradiction
    } else {
        IssueKind::TypeDoesNotContainType
    };

    let (line, col) = analyzer.get_line_column(expr_pos.0);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!(
            "Count comparison on {} is always {}",
            counted_type.get_id(Some(analyzer.interner)),
            if always_true { "true" } else { "false" }
        ),
        analyzer.file_path,
        expr_pos.0,
        expr_pos.1,
        line,
        col,
    ));

    true
}

#[derive(Clone, Copy)]
enum CountComparisonOp {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

fn get_count_comparison<'a>(
    binary: &'a Binary<'a>,
) -> Option<(&'a Expression<'a>, usize, CountComparisonOp)> {
    let operator = match &binary.operator {
        BinaryOperator::GreaterThan(_) => CountComparisonOp::GreaterThan,
        BinaryOperator::GreaterThanOrEqual(_) => CountComparisonOp::GreaterThanOrEqual,
        BinaryOperator::LessThan(_) => CountComparisonOp::LessThan,
        BinaryOperator::LessThanOrEqual(_) => CountComparisonOp::LessThanOrEqual,
        _ => return None,
    };

    if let (Some(counted_expr), Some(count)) = (
        get_count_call_argument(binary.lhs),
        get_usize_literal(binary.rhs),
    ) {
        return Some((counted_expr, count, operator));
    }

    if let (Some(counted_expr), Some(count)) = (
        get_count_call_argument(binary.rhs),
        get_usize_literal(binary.lhs),
    ) {
        let reversed_operator = match operator {
            CountComparisonOp::GreaterThan => CountComparisonOp::LessThan,
            CountComparisonOp::GreaterThanOrEqual => CountComparisonOp::LessThanOrEqual,
            CountComparisonOp::LessThan => CountComparisonOp::GreaterThan,
            CountComparisonOp::LessThanOrEqual => CountComparisonOp::GreaterThanOrEqual,
        };

        return Some((counted_expr, count, reversed_operator));
    }

    None
}

fn get_count_call_argument<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    let normalized_name = function_name
        .value()
        .strip_prefix('\\')
        .unwrap_or(function_name.value())
        .to_ascii_lowercase();

    if normalized_name != "count" {
        return None;
    }

    function_call
        .argument_list
        .arguments
        .first()
        .filter(|arg| !arg.is_unpacked())
        .map(|arg| arg.value())
}

fn get_usize_literal(expr: &Expression<'_>) -> Option<usize> {
    let Expression::Literal(Literal::Integer(int_lit)) = expr.unparenthesized() else {
        return None;
    };

    int_lit.raw.parse::<usize>().ok()
}

fn get_union_count_bounds(union: &TUnion) -> Option<(usize, Option<usize>)> {
    if union.types.is_empty() {
        return None;
    }

    let mut min_count = usize::MAX;
    let mut max_count = Some(0usize);

    for atomic in &union.types {
        let (atomic_min, atomic_max) = match atomic {
            TAtomic::TArray { .. } | TAtomic::TList { .. } => (0, None),
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => (1, None),
            TAtomic::TKeyedArray {
                properties,
                sealed,
                fallback_value_type,
                ..
            } => {
                let required_count = properties
                    .values()
                    .filter(|value_type| !value_type.possibly_undefined)
                    .count();
                let max_count = if *sealed && fallback_value_type.is_none() {
                    Some(properties.len())
                } else {
                    None
                };
                (required_count, max_count)
            }
            _ => return None,
        };

        min_count = min_count.min(atomic_min);
        max_count = match (max_count, atomic_max) {
            (Some(existing_max), Some(next_max)) => Some(existing_max.max(next_max)),
            _ => None,
        };
    }

    if min_count == usize::MAX {
        None
    } else {
        Some((min_count, max_count))
    }
}

fn emit_impossible_builtin_type_check_contradiction(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let (is_negated, function_call) = match expr.unparenthesized() {
        Expression::Call(Call::Function(function_call)) => (false, function_call),
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let Expression::Call(Call::Function(function_call)) = unary.operand.unparenthesized()
            else {
                return;
            };

            (true, function_call)
        }
        _ => return,
    };

    let Expression::Identifier(function_name) = &function_call.function else {
        return;
    };

    let normalized = function_name
        .value()
        .strip_prefix('\\')
        .unwrap_or(function_name.value())
        .to_ascii_lowercase();

    let Some(asserted_atomic) = get_builtin_type_check_atomic(&normalized) else {
        return;
    };

    let Some(first_arg) = function_call.argument_list.arguments.first() else {
        return;
    };

    let arg_pos = (
        first_arg.value().start_offset() as u32,
        first_arg.value().end_offset() as u32,
    );

    let Some(arg_type) = analysis_data.get_expr_type(arg_pos).map(|t| (*t).clone()) else {
        return;
    };

    if arg_type.is_mixed() {
        return;
    }

    if arg_type.is_nothing() {
        return;
    }

    let var_name = expression_identifier::get_expression_var_key(first_arg.value());

    // Mirror Psalm's processIrreconcilableFunctionCall: when we cannot build a tracked
    // assertion key, still report always-true/always-false built-in type checks.
    if var_name.is_none() {
        let expected_union = TUnion::new(asserted_atomic.clone());
        let mut comparison_result = TypeComparisonResult::new();
        let is_contained = union_type_comparator::is_contained_by(
            analyzer.codebase,
            &arg_type,
            &expected_union,
            false,
            false,
            &mut comparison_result,
        );

        if !is_contained {
            return;
        }

        let issue_kind = if !is_negated {
            if arg_type.from_docblock {
                IssueKind::RedundantConditionGivenDocblockType
            } else {
                IssueKind::RedundantCondition
            }
        } else if arg_type.from_docblock {
            IssueKind::DocblockTypeContradiction
        } else {
            IssueKind::TypeDoesNotContainType
        };

        let cond_pos = (
            expr.start_offset() as u32,
            expr.end_offset() as u32,
        );
        let (line, col) = analyzer.get_line_column(cond_pos.0);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            format!(
                "{} {} {}",
                if arg_type.from_docblock {
                    "Docblock type"
                } else {
                    "Type"
                },
                arg_type.get_id(Some(analyzer.interner)),
                if is_negated {
                    "makes this condition always false"
                } else {
                    "makes this condition always true"
                }
            ),
            analyzer.file_path,
            cond_pos.0,
            cond_pos.1,
            line,
            col,
        ));
        return;
    }

    let Some(var_name) = var_name else {
        return;
    };

    if normalized == "is_numeric" {
        let expected_union = TUnion::new(TAtomic::TNumeric);
        let mut comparison_result = TypeComparisonResult::new();
        let is_contained = union_type_comparator::is_contained_by(
            analyzer.codebase,
            &arg_type,
            &expected_union,
            false,
            false,
            &mut comparison_result,
        );

        if !is_contained {
            return;
        }

        let assertion = Assertion::IsType(TAtomic::TNumeric);
        reconciler::trigger_issue_for_impossible(
            analysis_data,
            analyzer,
            &arg_type,
            &var_name,
            &assertion,
            true,
            is_negated,
        );
        return;
    }

    if normalized != "is_bool" && normalized != "is_scalar" {
        return;
    }

    if normalized == "is_bool" {
        if is_negated {
            return;
        }

        if assertion_reconciler::intersect_union_with_atomic(&arg_type, &TAtomic::TBool, analyzer)
            .is_some()
        {
            return;
        }

        let assertion = Assertion::IsType(TAtomic::TBool);
        reconciler::trigger_issue_for_impossible(
            analysis_data,
            analyzer,
            &arg_type,
            &var_name,
            &assertion,
            false,
            false,
        );
        return;
    }

    match get_scalar_check_status(&arg_type) {
        ScalarCheckStatus::Never => {
            let assertion = Assertion::IsType(TAtomic::TScalar);
            reconciler::trigger_issue_for_impossible(
                analysis_data,
                analyzer,
                &arg_type,
                &var_name,
                &assertion,
                false,
                is_negated,
            );
        }
        ScalarCheckStatus::Always => {
            let assertion = Assertion::IsType(TAtomic::TScalar);
            reconciler::trigger_issue_for_impossible(
                analysis_data,
                analyzer,
                &arg_type,
                &var_name,
                &assertion,
                true,
                is_negated,
            );
        }
        ScalarCheckStatus::Maybe => {}
    }
}

fn get_builtin_type_check_atomic(function_name: &str) -> Option<TAtomic> {
    Some(match function_name {
        "is_string" => TAtomic::TString,
        "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
        "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
        "is_bool" => TAtomic::TBool,
        "is_array" => TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        },
        "is_object" => TAtomic::TObject,
        "is_null" => TAtomic::TNull,
        "is_numeric" => TAtomic::TNumeric,
        "is_callable" => TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        },
        "is_resource" => TAtomic::TResource,
        "is_scalar" => TAtomic::TScalar,
        "is_iterable" => TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        },
        _ => return None,
    })
}

fn emit_impossible_isset_check_contradiction(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let (is_negated, isset_construct) = match expr.unparenthesized() {
        Expression::Construct(Construct::Isset(isset_construct)) => (false, isset_construct),
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let Expression::Construct(Construct::Isset(isset_construct)) = unary.operand.unparenthesized()
            else {
                return;
            };
            (true, isset_construct)
        }
        _ => return,
    };

    if isset_construct.values.len() != 1 {
        return;
    }

    let Some(value_expr) = isset_construct.values.first() else {
        return;
    };

    let value_pos = (
        value_expr.start_offset() as u32,
        value_expr.end_offset() as u32,
    );

    if let Some(var_name) = expression_identifier::get_expression_var_key(value_expr)
        && var_name.contains("::$")
        && !var_name.contains('[')
        && let Some(value_type) = analysis_data.get_expr_type(value_pos).map(|t| (*t).clone())
        && !value_type.is_mixed()
        && !value_type.is_nullable
        && !value_type.possibly_undefined
    {
        let (line, col) = analyzer.get_line_column(value_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RedundantPropertyInitializationCheck,
            format!("Static property {} is always initialized", var_name),
            analyzer.file_path,
            value_pos.0,
            value_pos.1,
            line,
            col,
        ));
        return;
    }

    if !matches!(
        value_expr.unparenthesized(),
        Expression::Variable(Variable::Direct(_))
    ) {
        return;
    }

    let Some(var_name) = expression_identifier::get_expression_var_key(value_expr) else {
        return;
    };

    let Some(value_type) = analysis_data.get_expr_type(value_pos).map(|t| (*t).clone()) else {
        return;
    };

    if value_type.is_mixed() {
        return;
    }

    if is_negated {
        if value_type.is_nullable || value_type.possibly_undefined {
            return;
        }

        let assertion = Assertion::IsType(TAtomic::TNull);
        reconciler::trigger_issue_for_impossible(
            analysis_data,
            analyzer,
            &value_type,
            &var_name,
            &assertion,
            false,
            false,
        );
    } else {
        if !value_type.is_null() {
            return;
        }

        let assertion = Assertion::IsNotType(TAtomic::TNull);
        reconciler::trigger_issue_for_impossible(
            analysis_data,
            analyzer,
            &value_type,
            &var_name,
            &assertion,
            false,
            false,
        );
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ScalarCheckStatus {
    Always,
    Never,
    Maybe,
}

fn get_scalar_check_status(union: &TUnion) -> ScalarCheckStatus {
    let mut saw_always = false;
    let mut saw_never = false;

    for atomic in &union.types {
        match get_atomic_scalar_status(atomic) {
            ScalarCheckStatus::Always => saw_always = true,
            ScalarCheckStatus::Never => saw_never = true,
            ScalarCheckStatus::Maybe => return ScalarCheckStatus::Maybe,
        }
    }

    match (saw_always, saw_never) {
        (true, true) => ScalarCheckStatus::Maybe,
        (true, false) => ScalarCheckStatus::Always,
        (false, true) => ScalarCheckStatus::Never,
        (false, false) => ScalarCheckStatus::Maybe,
    }
}

fn get_atomic_scalar_status(atomic: &TAtomic) -> ScalarCheckStatus {
    match atomic {
        TAtomic::TInt
        | TAtomic::TFloat
        | TAtomic::TString
        | TAtomic::TBool
        | TAtomic::TTrue
        | TAtomic::TFalse
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TClassString { .. }
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TTemplateParamClass { .. }
        | TAtomic::TArrayKey
        | TAtomic::TScalar
        | TAtomic::TNumeric => ScalarCheckStatus::Always,
        TAtomic::TNull
        | TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. }
        | TAtomic::TNamedObject { .. }
        | TAtomic::TObjectIntersection { .. }
        | TAtomic::TObject
        | TAtomic::TClosedResource
        | TAtomic::TResource
        | TAtomic::TCallable { .. }
        | TAtomic::TClosure { .. }
        | TAtomic::TNothing
        | TAtomic::TVoid
        | TAtomic::TIterable { .. }
        | TAtomic::TEnum { .. }
        | TAtomic::TEnumCase { .. } => ScalarCheckStatus::Never,
        TAtomic::TTemplateParam { as_type, .. } => get_scalar_check_status(as_type),
        TAtomic::TMixed | TAtomic::TNonEmptyMixed => ScalarCheckStatus::Maybe,
    }
}

fn emit_docblock_type_check_contradiction(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: Option<&BlockContext>,
) {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return;
    };

    let Expression::Identifier(function_name) = &function_call.function else {
        return;
    };

    let normalized = function_name
        .value()
        .strip_prefix('\\')
        .unwrap_or(function_name.value())
        .to_ascii_lowercase();

    if normalized != "is_bool" {
        return;
    }

    let Some(first_arg) = function_call.argument_list.arguments.first() else {
        return;
    };

    let Some(target_var_name) = expression_identifier::get_expression_var_key(first_arg.value())
    else {
        return;
    };

    // This parity fix targets array-access boolean checks like `is_bool($a[0])`.
    if !target_var_name.contains('[') {
        return;
    }

    let arg_pos = (
        first_arg.value().start_offset() as u32,
        first_arg.value().end_offset() as u32,
    );

    let Some(arg_type) = analysis_data.get_expr_type(arg_pos).map(|t| (*t).clone()) else {
        return;
    };

    let mut from_docblock = arg_type.from_docblock;

    if !from_docblock {
        let Some(context) = context else {
            return;
        };
        let Some(var_name) = expression_identifier::get_expression_var_key(first_arg.value()) else {
            return;
        };

        let mut root_name = var_name.as_str();
        for separator in ["[", "->", "::"] {
            if let Some(idx) = root_name.find(separator) {
                root_name = &root_name[..idx];
                break;
            }
        }

        let root_type = analyzer
            .interner
            .find(root_name)
            .and_then(|id| context.locals.get(&id))
            .cloned()
            .or_else(|| {
                if let Some(stripped) = root_name.strip_prefix('$') {
                    analyzer
                        .interner
                        .find(stripped)
                        .and_then(|id| context.locals.get(&id))
                        .cloned()
                } else {
                    let with_dollar = format!("${root_name}");
                    analyzer
                        .interner
                        .find(&with_dollar)
                        .and_then(|id| context.locals.get(&id))
                        .cloned()
                }
            });

        from_docblock = root_type.is_some_and(|t| t.from_docblock);
    }

    if !from_docblock {
        return;
    }

    if assertion_reconciler::intersect_union_with_atomic(&arg_type, &TAtomic::TBool, analyzer)
        .is_some()
    {
        return;
    }

    let (line, col) = analyzer.get_line_column(expr_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::DocblockTypeContradiction,
        format!(
            "Docblock-defined type {} for {} is never bool",
            arg_type.get_id(Some(analyzer.interner)),
            target_var_name
        ),
        analyzer.file_path,
        expr_pos.0,
        expr_pos.1,
        line,
        col,
    ));
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

fn get_clause_determined_truthiness_for_scalar_var(
    expr: &Expression<'_>,
    expr_type: &TUnion,
    context: Option<&BlockContext>,
) -> Option<bool> {
    let context = context?;
    if !matches!(expr_type.get_single(), Some(TAtomic::TScalar)) {
        return None;
    }

    let var_name = expression_identifier::get_expression_var_key(expr)?;
    let clause_key = ClauseKey::Name(var_name);

    let mut known_truthy = false;
    let mut known_falsy = false;

    for clause in &context.clauses {
        let Some(possibilities) = clause.possibilities.get(&clause_key) else {
            continue;
        };

        if possibilities.len() != 1 {
            continue;
        }

        let Some(assertion) = possibilities.values().next() else {
            continue;
        };

        match assertion {
            Assertion::Truthy => known_truthy = true,
            Assertion::Falsy => known_falsy = true,
            _ => {}
        }
    }

    match (known_truthy, known_falsy) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

fn emit_paradoxical_empty_issue(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    expr_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> bool {
    let (is_negated, empty_value) = match expr.unparenthesized() {
        Expression::Construct(Construct::Empty(empty_construct)) => (false, empty_construct.value),
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let Expression::Construct(Construct::Empty(empty_construct)) =
                unary.operand.unparenthesized()
            else {
                return false;
            };

            (true, empty_construct.value)
        }
        _ => return false,
    };

    let empty_value_pos = (
        empty_value.start_offset() as u32,
        empty_value.end_offset() as u32,
    );

    let empty_value_is_fetch = matches!(
        empty_value.unparenthesized(),
        Expression::ArrayAccess(_)
            | Expression::Access(Access::Property(_))
            | Expression::Access(Access::NullSafeProperty(_))
    );

    let Some(empty_value_type) = analysis_data
        .get_expr_type(empty_value_pos)
        .map(|union| (*union).clone())
    else {
        return false;
    };

    if empty_value_is_fetch && !is_stable_fetched_empty_value_type(&empty_value_type) {
        return false;
    }

    let value_is_always_falsy = empty_value_type.is_always_falsy();
    let value_is_always_truthy = empty_value_type.is_always_truthy();
    if !value_is_always_falsy && !value_is_always_truthy {
        return false;
    }

    let condition_is_always_truthy = if value_is_always_falsy {
        !is_negated
    } else {
        is_negated
    };

    let issue_kind = if condition_is_always_truthy {
        if empty_value_type.from_docblock {
            IssueKind::RedundantConditionGivenDocblockType
        } else {
            IssueKind::RedundantCondition
        }
    } else if empty_value_type.from_docblock {
        IssueKind::DocblockTypeContradiction
    } else {
        IssueKind::TypeDoesNotContainType
    };

    let (line, col) = analyzer.get_line_column(expr_pos.0);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!(
            "Operand of type {} is always {}",
            empty_value_type.get_id(Some(analyzer.interner)),
            if value_is_always_falsy {
                "falsy"
            } else {
                "truthy"
            }
        ),
        analyzer.file_path,
        expr_pos.0,
        expr_pos.1,
        line,
        col,
    ));

    true
}

fn is_stable_fetched_empty_value_type(union: &TUnion) -> bool {
    matches!(
        union.get_single(),
        Some(
            TAtomic::TTrue
                | TAtomic::TFalse
                | TAtomic::TNull
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralFloat { .. }
                | TAtomic::TLiteralString { .. }
        )
    )
}
