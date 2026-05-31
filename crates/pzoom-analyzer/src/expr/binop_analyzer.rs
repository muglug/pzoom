//! Binary operation analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::Binary;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr::binop::{
    and_analyzer, coalesce_analyzer, non_comparison_op_analyzer, or_analyzer,
};
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator::is_class_subtype_of;
use crate::type_comparator::union_type_comparator;

/// Analyze a binary operation expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    binop: &Binary<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::binary::BinaryOperator;

    match &binop.operator {
        BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
            and_analyzer::analyze(analyzer, binop.lhs, binop.rhs, pos, analysis_data, context);
            return;
        }
        BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
            or_analyzer::analyze(analyzer, binop.lhs, binop.rhs, pos, analysis_data, context);
            return;
        }
        BinaryOperator::NullCoalesce(_) => {
            coalesce_analyzer::analyze(analyzer, binop.lhs, binop.rhs, pos, analysis_data, context);
            return;
        }
        _ => {}
    }

    let left_pos = analyze_binary_operand(analyzer, binop.lhs, analysis_data, context);
    let right_pos = analyze_binary_operand(analyzer, binop.rhs, analysis_data, context);

    let left_type = analysis_data.get_expr_type(left_pos);
    let right_type = analysis_data.get_expr_type(right_pos);

    if is_comparison_operator(&binop.operator) {
        let comparison_type = analyze_comparison_operation(
            analyzer,
            binop,
            left_type.as_deref(),
            right_type.as_deref(),
            pos,
            analysis_data,
            context,
        );
        analysis_data.set_expr_type(pos, comparison_type);
        return;
    }

    let result_type = non_comparison_op_analyzer::analyze(
        analyzer,
        &binop.operator,
        binop.lhs,
        binop.rhs,
        left_type.as_deref(),
        right_type.as_deref(),
        pos,
        analysis_data,
        context,
    );
    analysis_data.set_expr_type(pos, result_type);
}

fn analyze_binary_operand(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    if let Expression::Binary(inner_binop) = expr.unparenthesized() {
        let span = expr.span();
        let inner_pos = (span.start.offset, span.end.offset);
        analyze(analyzer, inner_binop, inner_pos, analysis_data, context);
        inner_pos
    } else {
        expression_analyzer::analyze(analyzer, expr, analysis_data, context)
    }
}

fn is_comparison_operator(operator: &mago_syntax::ast::ast::binary::BinaryOperator) -> bool {
    use mago_syntax::ast::ast::binary::BinaryOperator;

    matches!(
        operator,
        BinaryOperator::Equal(_)
            | BinaryOperator::NotEqual(_)
            | BinaryOperator::AngledNotEqual(_)
            | BinaryOperator::Identical(_)
            | BinaryOperator::NotIdentical(_)
            | BinaryOperator::LessThan(_)
            | BinaryOperator::LessThanOrEqual(_)
            | BinaryOperator::GreaterThan(_)
            | BinaryOperator::GreaterThanOrEqual(_)
            | BinaryOperator::Spaceship(_)
    )
}

fn analyze_comparison_operation(
    analyzer: &StatementsAnalyzer<'_>,
    binop: &Binary<'_>,
    left_type: Option<&TUnion>,
    right_type: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> TUnion {
    use mago_syntax::ast::ast::binary::BinaryOperator;

    if matches!(
        &binop.operator,
        BinaryOperator::Identical(_) | BinaryOperator::NotIdentical(_)
    ) {
        if let (Some(left_union), Some(right_union)) = (left_type, right_type) {
            let is_positive_comparison = matches!(&binop.operator, BinaryOperator::Identical(_));
            let is_negative_comparison = matches!(&binop.operator, BinaryOperator::NotIdentical(_));

            if has_substr_literal_length_mismatch(binop) && is_positive_comparison {
                if !context.inside_loop || context.inside_foreach {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::TypeDoesNotContainType,
                        format!(
                            "{} cannot be compared to {}",
                            left_union.get_id(Some(analyzer.interner)),
                            right_union.get_id(Some(analyzer.interner))
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            } else if !matches!(binop.lhs.unparenthesized(), Expression::ArrayAccess(_))
                && !matches!(binop.rhs.unparenthesized(), Expression::ArrayAccess(_))
                && !union_type_comparator::can_expression_types_be_identical(
                    analyzer.codebase,
                    left_union,
                    right_union,
                )
            {
                if is_positive_comparison
                    && (is_null_singleton(left_union) || is_null_singleton(right_union))
                {
                    let non_null_union = if is_null_singleton(left_union) {
                        right_union
                    } else {
                        left_union
                    };

                    if !non_null_union.is_nullable
                        && (!context.inside_loop || context.inside_foreach)
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        let issue_kind = if non_null_union.from_docblock {
                            IssueKind::DocblockTypeContradiction
                        } else {
                            IssueKind::TypeDoesNotContainNull
                        };

                        analysis_data.add_issue(Issue::new(
                            issue_kind,
                            format!(
                                "{} does not contain null",
                                non_null_union.get_id(Some(analyzer.interner))
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                } else if !context.inside_loop || context.inside_foreach {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    let from_docblock = left_union.from_docblock || right_union.from_docblock;
                    let issue_kind = if is_negative_comparison {
                        if from_docblock {
                            IssueKind::RedundantConditionGivenDocblockType
                        } else {
                            IssueKind::RedundantCondition
                        }
                    } else if from_docblock {
                        IssueKind::DocblockTypeContradiction
                    } else {
                        IssueKind::TypeDoesNotContainType
                    };

                    analysis_data.add_issue(Issue::new(
                        issue_kind,
                        if is_negative_comparison {
                            format!(
                                "{} can never contain {}",
                                left_union.get_id(Some(analyzer.interner)),
                                right_union.get_id(Some(analyzer.interner))
                            )
                        } else {
                            format!(
                                "{} cannot be compared to {}",
                                left_union.get_id(Some(analyzer.interner)),
                                right_union.get_id(Some(analyzer.interner))
                            )
                        },
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            } else if matches!(&binop.operator, BinaryOperator::Identical(_))
                && let (Some(left_named), Some(right_named)) = (
                    get_single_named_object_id(left_union),
                    get_single_named_object_id(right_union),
                )
            {
                if left_named != right_named
                    && !is_class_subtype_of(left_named, right_named, analyzer.codebase)
                    && !is_class_subtype_of(right_named, left_named, analyzer.codebase)
                    && named_objects_are_definitely_disjoint(analyzer, left_named, right_named)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::TypeDoesNotContainType,
                        format!(
                            "{} cannot be identical to {}",
                            analyzer.interner.lookup(left_named),
                            analyzer.interner.lookup(right_named)
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    if matches!(
        &binop.operator,
        BinaryOperator::Equal(_) | BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_)
    ) {
        if let (Some(left_union), Some(right_union)) = (left_type, right_type) {
            let should_check = is_get_class_call(binop.lhs)
                || is_get_class_call(binop.rhs)
                || (matches!(&binop.operator, BinaryOperator::Equal(_))
                    && (weak_equality_compares_object_to_non_object(left_union, right_union)
                        || (union_is_int_like(left_union) && union_is_int_like(right_union))));

            // A weak `==` comparison between a Stringable object (one with a
            // `__toString` method) and a string-like value is valid in PHP: the
            // object is coerced to its string form. Psalm does not flag it.
            let should_check = should_check
                && !weak_equality_compares_stringable_to_string(
                    analyzer,
                    left_union,
                    right_union,
                );

            if !should_check {
                // no-op
            } else {
                let is_negative_comparison = matches!(
                    &binop.operator,
                    BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_)
                );

                if !union_type_comparator::can_expression_types_be_identical(
                    analyzer.codebase,
                    left_union,
                    right_union,
                ) && (!context.inside_loop || context.inside_foreach)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    let from_docblock = left_union.from_docblock || right_union.from_docblock;
                    let issue_kind = if is_negative_comparison {
                        if from_docblock {
                            IssueKind::RedundantConditionGivenDocblockType
                        } else {
                            IssueKind::RedundantCondition
                        }
                    } else if from_docblock {
                        IssueKind::DocblockTypeContradiction
                    } else {
                        IssueKind::TypeDoesNotContainType
                    };

                    analysis_data.add_issue(Issue::new(
                        issue_kind,
                        format!(
                            "{} cannot be compared to {}",
                            left_union.get_id(Some(analyzer.interner)),
                            right_union.get_id(Some(analyzer.interner))
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    match &binop.operator {
        BinaryOperator::Identical(_) => infer_strict_equality_result_type(
            analyzer, binop.lhs, binop.rhs, left_type, right_type, context, true,
        ),
        BinaryOperator::NotIdentical(_) => infer_strict_equality_result_type(
            analyzer, binop.lhs, binop.rhs, left_type, right_type, context, false,
        ),
        BinaryOperator::Spaceship(_) => TUnion::int(),
        _ => TUnion::bool(),
    }
}

pub(crate) fn resolve_instanceof_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    rhs: &Expression<'_>,
) -> Option<StrId> {
    match rhs.unparenthesized() {
        Expression::Identifier(identifier) => analyzer
            .get_resolved_name(identifier.span().start.offset)
            .or_else(|| {
                Some(
                    analyzer
                        .interner
                        .intern(identifier.value().trim_start_matches('\\')),
                )
            }),
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }
}

pub(crate) fn evaluate_instanceof_possibility(
    analyzer: &StatementsAnalyzer<'_>,
    left_union: &TUnion,
    asserted_class_id: StrId,
) -> (bool, bool) {
    if left_union.types.is_empty() {
        return (false, false);
    }

    let mut can_be_instance = false;
    let mut always_instance = true;

    for atomic in &left_union.types {
        let (atomic_can_be_instance, atomic_always_instance) =
            atomic_instanceof_relation(analyzer, atomic, asserted_class_id);

        if atomic_can_be_instance {
            can_be_instance = true;
        } else {
            always_instance = false;
            continue;
        }

        if !atomic_always_instance {
            always_instance = false;
        }
    }

    (can_be_instance, always_instance)
}

fn atomic_instanceof_relation(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    asserted_class_id: StrId,
) -> (bool, bool) {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            if is_class_subtype_of(*name, asserted_class_id, analyzer.codebase) {
                return (true, true);
            }

            if named_objects_are_definitely_disjoint(analyzer, *name, asserted_class_id) {
                return (false, false);
            }

            (true, false)
        }
        TAtomic::TObject | TAtomic::TMixed | TAtomic::TNonEmptyMixed => (true, false),
        TAtomic::TTemplateParam { as_type, .. } => {
            evaluate_instanceof_possibility(analyzer, as_type, asserted_class_id)
        }
        TAtomic::TObjectIntersection { .. } => (true, false),
        _ => (false, false),
    }
}

fn named_objects_are_definitely_disjoint(
    analyzer: &StatementsAnalyzer<'_>,
    left_named: pzoom_str::StrId,
    right_named: pzoom_str::StrId,
) -> bool {
    let Some(left_info) = analyzer.codebase.get_class(left_named) else {
        return false;
    };
    let Some(right_info) = analyzer.codebase.get_class(right_named) else {
        return false;
    };

    if left_info.kind != ClassLikeKind::Class || right_info.kind != ClassLikeKind::Class {
        return false;
    }

    true
}

fn infer_strict_equality_result_type(
    analyzer: &StatementsAnalyzer<'_>,
    left_expr: &Expression<'_>,
    right_expr: &Expression<'_>,
    left: Option<&TUnion>,
    right: Option<&TUnion>,
    context: &BlockContext,
    is_identical: bool,
) -> TUnion {
    if context.inside_loop {
        return TUnion::bool();
    }

    if expression_has_external_reference(analyzer, left_expr, context)
        || expression_has_external_reference(analyzer, right_expr, context)
    {
        return TUnion::bool();
    }

    let (Some(left), Some(right)) = (left, right) else {
        return TUnion::bool();
    };

    if let (Some(left_atomic), Some(right_atomic)) = (left.get_single(), right.get_single())
        && has_deterministic_strict_identity(left_atomic)
        && has_deterministic_strict_identity(right_atomic)
    {
        return if left_atomic == right_atomic {
            if is_identical {
                TUnion::new(TAtomic::TTrue)
            } else {
                TUnion::new(TAtomic::TFalse)
            }
        } else {
            if is_identical {
                TUnion::new(TAtomic::TFalse)
            } else {
                TUnion::new(TAtomic::TTrue)
            }
        };
    }

    TUnion::bool()
}

fn expression_has_external_reference(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> bool {
    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => {
            expression_has_external_reference(analyzer, parenthesized.expression, context)
        }
        Expression::Variable(Variable::Direct(direct)) => {
            let var_id = analyzer.interner.intern(direct.name);
            context.references_to_external_scope.contains(&var_id)
                || context.references_in_scope.contains_key(&var_id)
        }
        _ => false,
    }
}

fn is_get_class_call(expr: &Expression<'_>) -> bool {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return false;
    };

    let Expression::Identifier(function_name) = &function_call.function else {
        return false;
    };

    function_name.value().eq_ignore_ascii_case("get_class")
        || function_name.value().eq_ignore_ascii_case("\\get_class")
}

fn has_deterministic_strict_identity(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TNull
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
    )
}

fn weak_equality_compares_object_to_non_object(left_union: &TUnion, right_union: &TUnion) -> bool {
    if left_union.is_mixed() || right_union.is_mixed() {
        return false;
    }

    let left_has_object = union_has_object_like(left_union);
    let right_has_object = union_has_object_like(right_union);

    if left_has_object == right_has_object {
        return false;
    }

    if left_has_object {
        union_is_definitely_non_object(right_union)
    } else {
        union_is_definitely_non_object(left_union)
    }
}

/// Returns true when one operand is a string-like value and the other is an
/// object that has a `__toString` method (Stringable). Such a `==` comparison
/// is legal in PHP because the object is implicitly cast to a string.
fn weak_equality_compares_stringable_to_string(
    analyzer: &StatementsAnalyzer<'_>,
    left_union: &TUnion,
    right_union: &TUnion,
) -> bool {
    let stringable_vs_string = |object_union: &TUnion, other_union: &TUnion| {
        union_is_stringable_object(analyzer, object_union) && union_is_string_like(other_union)
    };

    stringable_vs_string(left_union, right_union) || stringable_vs_string(right_union, left_union)
}

fn union_is_stringable_object(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| atomic_is_stringable_object(analyzer, atomic))
}

fn atomic_is_stringable_object(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            *name == pzoom_str::StrId::STRINGABLE
                || analyzer.codebase.get_class(*name).is_some_and(|class_info| {
                    class_info.methods.contains_key(&pzoom_str::StrId::TO_STRING)
                        || class_info
                            .all_parent_interfaces
                            .contains(&pzoom_str::StrId::STRINGABLE)
                })
        }
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(|t| atomic_is_stringable_object(analyzer, t))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_is_stringable_object(analyzer, as_type),
        _ => false,
    }
}

fn union_is_string_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TTruthyString
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
            )
        })
}

fn union_is_int_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }                    | TAtomic::TIntRange { .. }
            )
        })
}

fn union_has_object_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TObject
                | TAtomic::TNamedObject { .. }
                | TAtomic::TObjectIntersection { .. }
                | TAtomic::TClosure { .. }
        )
    })
}

fn union_is_definitely_non_object(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            !matches!(
                atomic,
                TAtomic::TObject
                    | TAtomic::TNamedObject { .. }
                    | TAtomic::TObjectIntersection { .. }
                    | TAtomic::TClosure { .. }
                    | TAtomic::TMixed
                    | TAtomic::TNonEmptyMixed
            )
        })
}

pub(crate) fn infer_bitwise_type(
    operator: &mago_syntax::ast::ast::binary::BinaryOperator,
    left: Option<&TUnion>,
    right: Option<&TUnion>,
) -> TUnion {
    let (Some(left), Some(right)) = (left, right) else {
        return TUnion::int();
    };

    let Some(TAtomic::TLiteralInt { value: left_value }) = left.get_single() else {
        return TUnion::int();
    };
    let Some(TAtomic::TLiteralInt { value: right_value }) = right.get_single() else {
        return TUnion::int();
    };

    let result = match operator {
        mago_syntax::ast::ast::binary::BinaryOperator::BitwiseAnd(_) => left_value & right_value,
        mago_syntax::ast::ast::binary::BinaryOperator::BitwiseOr(_) => left_value | right_value,
        mago_syntax::ast::ast::binary::BinaryOperator::BitwiseXor(_) => left_value ^ right_value,
        mago_syntax::ast::ast::binary::BinaryOperator::LeftShift(_) => {
            if *right_value < 0 {
                return TUnion::int();
            }
            left_value.wrapping_shl(*right_value as u32)
        }
        mago_syntax::ast::ast::binary::BinaryOperator::RightShift(_) => {
            if *right_value < 0 {
                return TUnion::int();
            }
            left_value.wrapping_shr(*right_value as u32)
        }
        _ => return TUnion::int(),
    };

    TUnion::new(TAtomic::TLiteralInt { value: result })
}

pub(crate) fn emit_bitwise_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    union: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(union) = union else {
        return;
    };
    if union.is_mixed() {
        return;
    }

    let mut has_valid = false;
    let mut has_invalid = false;

    for atomic in &union.types {
        if is_valid_bitwise_atomic(atomic) {
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
            "Cannot use bitwise operation on type {}",
            union.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn is_valid_bitwise_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TLiteralInt { .. }            | TAtomic::TIntRange { .. }
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

pub(crate) fn union_is_string_like_for_bitwise(union: &TUnion) -> bool {
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

pub(crate) fn union_is_numeric_like_for_bitwise(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }                    | TAtomic::TIntRange { .. }
                    | TAtomic::TFloat
                    | TAtomic::TLiteralFloat { .. }
            )
        })
}

fn get_single_named_object_id(union: &TUnion) -> Option<pzoom_str::StrId> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TNamedObject { name, .. }) => Some(*name),
        _ => None,
    }
}

fn is_null_singleton(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TNull))
}

fn has_substr_literal_length_mismatch(binop: &Binary<'_>) -> bool {
    substr_length_mismatch_one_way(binop.lhs, binop.rhs)
        || substr_length_mismatch_one_way(binop.rhs, binop.lhs)
}

fn substr_length_mismatch_one_way(
    substr_expr: &Expression<'_>,
    literal_expr: &Expression<'_>,
) -> bool {
    let Some(expected_length) = get_positive_literal_substr_length(substr_expr) else {
        return false;
    };

    let Expression::Literal(Literal::String(string_lit)) = literal_expr.unparenthesized() else {
        return false;
    };
    let Some(value) = string_lit.value else {
        return false;
    };

    value.len() as i64 != expected_length
}

fn get_positive_literal_substr_length(expr: &Expression<'_>) -> Option<i64> {
    let Expression::Call(Call::Function(function_call)) = expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    if !function_name
        .value()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("substr")
    {
        return None;
    }

    let length_arg = function_call.argument_list.arguments.get(2)?;
    let Expression::Literal(Literal::Integer(int_lit)) = length_arg.value().unparenthesized()
    else {
        return None;
    };

    let value = int_lit.value? as i64;
    (value > 0).then_some(value)
}
