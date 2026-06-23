//! Binary operation analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::Binary;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::{DataFlowNode, Issue, IssueKind, PathKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expr::binop::{
    and_analyzer, coalesce_analyzer, non_comparison_op_analyzer, or_analyzer,
};
use crate::expression_analyzer;
use crate::expression_analyzer::add_decision_dataflow;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator::is_class_subtype_of;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

/// Emit `ImpureMethodCall` for every named-object atomic in `union` whose
/// `__toString` is not mutation-free. Callers gate on a mutation-free context
/// (Psalm's checkForImpureEqualityComparison / ConcatAnalyzer).
pub(crate) fn emit_impure_to_string_for_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    for atomic in &union.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };
        let Some(to_string_info) = class_info.methods.get(&StrId::TO_STRING) else {
            continue;
        };

        if !to_string_info.is_mutation_free && !to_string_info.is_pure {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ImpureMethodCall,
                format!(
                    "Cannot call a possibly-mutating method {}::__toString from a pure context",
                    analyzer.interner.lookup(*name)
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
            // Hakana follows the `&&` analyzer with decision dataflow on the result.
            let cond_type = analysis_data
                .expr_types
                .get(&pos)
                .cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::bool);
            add_decision_dataflow(
                analyzer,
                analysis_data,
                binop.lhs,
                Some(binop.rhs),
                pos,
                cond_type,
            );
            return;
        }
        BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
            or_analyzer::analyze(analyzer, binop.lhs, binop.rhs, pos, analysis_data, context);
            // Hakana follows the `||` analyzer with decision dataflow on the result.
            let cond_type = analysis_data
                .expr_types
                .get(&pos)
                .cloned()
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::bool);
            add_decision_dataflow(
                analyzer,
                analysis_data,
                binop.lhs,
                Some(binop.rhs),
                pos,
                cond_type,
            );
            return;
        }
        BinaryOperator::NullCoalesce(_) => {
            coalesce_analyzer::analyze(analyzer, binop.lhs, binop.rhs, pos, analysis_data, context);
            return;
        }
        _ => {}
    }

    // Comparison/instanceof/spaceship operands are consumed as values
    // (Hakana's inside_general_use); arithmetic and concat operands keep
    // their own dataflow sinks so self-referential assignments still count
    // as unused.
    let operands_are_general_use = is_comparison_operator(&binop.operator)
        || matches!(binop.operator, BinaryOperator::Instanceof(_));
    let marks_use = |operand: &Expression<'_>| {
        operands_are_general_use
            || !matches!(
                operand.unparenthesized(),
                Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(_))
            )
    };
    let left_pos = analyze_binary_operand_marked(
        analyzer,
        binop.lhs,
        analysis_data,
        context,
        marks_use(binop.lhs),
    );
    let right_pos = analyze_binary_operand_marked(
        analyzer,
        binop.rhs,
        analysis_data,
        context,
        marks_use(binop.rhs),
    );

    let left_type = analysis_data.expr_types.get(&left_pos).cloned();
    let right_type = analysis_data.expr_types.get(&right_pos).cloned();

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
        // Hakana wires equality/relational (and `<=>`) results through decision
        // dataflow, which also records the expression type.
        add_decision_dataflow(
            analyzer,
            analysis_data,
            binop.lhs,
            Some(binop.rhs),
            pos,
            comparison_type,
        );
        return;
    }

    let result_type = non_comparison_op_analyzer::analyze(
        analyzer,
        &binop.operator,
        left_type.as_deref(),
        right_type.as_deref(),
        pos,
        analysis_data,
        context,
        context.inside_loop,
    );

    match &binop.operator {
        // Hakana routes arithmetic/bitwise ops and string concatenation through a
        // composition node taking parents from both operands.
        BinaryOperator::Addition(_)
        | BinaryOperator::Subtraction(_)
        | BinaryOperator::Multiplication(_)
        | BinaryOperator::Division(_)
        | BinaryOperator::Modulo(_)
        | BinaryOperator::Exponentiation(_)
        | BinaryOperator::BitwiseAnd(_)
        | BinaryOperator::BitwiseOr(_)
        | BinaryOperator::BitwiseXor(_)
        | BinaryOperator::LeftShift(_)
        | BinaryOperator::RightShift(_)
        | BinaryOperator::StringConcat(_) => {
            assign_arithmetic_type(
                analyzer,
                analysis_data,
                result_type,
                binop.lhs,
                binop.rhs,
                pos,
            );
        }
        // PHP `instanceof` maps to Hakana's `Is`, which uses decision dataflow on
        // the inspected operand only.
        BinaryOperator::Instanceof(_) => {
            check_instanceof_class_exists(analyzer, binop.rhs, analysis_data, context);
            // A dynamic instanceof class expression (`instanceof $class`)
            // consumes its variable.
            let rhs_span_pos = (binop.rhs.span().start.offset, binop.rhs.span().end.offset);
            if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
                && let Some(rhs_type) = analysis_data.expr_types.get(&rhs_span_pos).cloned()
                && !rhs_type.parent_nodes.is_empty()
            {
                let rhs_sink = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
                    make_data_flow_node_position(analyzer, rhs_span_pos),
                );
                for parent_node in &rhs_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &parent_node.id,
                        &rhs_sink.id,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
                analysis_data.data_flow_graph.add_node(rhs_sink);
            }
            add_decision_dataflow(analyzer, analysis_data, binop.lhs, None, pos, result_type);
        }
        _ => {
            analysis_data.expr_types.insert(pos, Rc::new(result_type));
        }
    }
}

/// Hakana `arithmetic_analyzer::assign_arithmetic_type`: a composition node taking
/// parents from both operands becomes the result's parent. (Hakana additionally adds
/// URI-related taints on string results; pzoom's `SinkType` has no equivalents.)
fn assign_arithmetic_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    mut cond_type: TUnion,
    lhs_expr: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    expr_pos: Pos,
) {
    let decision_node =
        DataFlowNode::get_for_composition(make_data_flow_node_position(analyzer, expr_pos));

    analysis_data
        .data_flow_graph
        .add_node(decision_node.clone());

    let lhs_span = lhs_expr.span();
    if let Some(lhs_type) = analysis_data
        .expr_types
        .get(&(lhs_span.start.offset, lhs_span.end.offset))
        .cloned()
    {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &old_parent_node.id,
                &decision_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    let rhs_span = rhs_expr.span();
    if let Some(rhs_type) = analysis_data
        .expr_types
        .get(&(rhs_span.start.offset, rhs_span.end.offset))
        .cloned()
    {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &rhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &old_parent_node.id,
                &decision_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    analysis_data
        .expr_types
        .insert(expr_pos, Rc::new(cond_type));
}

fn analyze_binary_operand_marked(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    mark_use: bool,
) -> Pos {
    // A binary expression consumes both operands (Hakana marks them
    // inside_general_use), so e.g. `$x->getSource() instanceof Foo` counts as
    // a use of the call's return value. Arithmetic/concat operands keep their
    // own dataflow sinks instead — flagging them would make self-referential
    // assignments (`$i = $i + 1`) count as uses for find-unused-variables.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = context.inside_general_use || mark_use;
    let operand_pos = if let Expression::Binary(inner_binop) = expr.unparenthesized() {
        let span = expr.span();
        let inner_pos = (span.start.offset, span.end.offset);
        analyze(analyzer, inner_binop, inner_pos, analysis_data, context);
        inner_pos
    } else {
        expression_analyzer::analyze(analyzer, expr, analysis_data, context)
    };
    context.inside_general_use = was_inside_general_use;
    operand_pos
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

            // Calculation-derived literals (`$a ^ $b` over constants) carry
            // no assertion key in Psalm, so their comparisons never report.
            if left_union.from_calculation || right_union.from_calculation {
                return TUnion::bool();
            }

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
                // Psalm's NotIdentical paradox check stays silent when the
                // right operand is nullable (`$obj !== $maybeString`): its
                // null-inequality scrape claims that shape before the
                // disjointness check runs. Identical (===) always reports.
                && !(is_negative_comparison
                    && right_union.is_nullable()
                    && !right_union.is_null())
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

                    if !non_null_union.is_nullable()
                        && (!context.inside_loop || context.inside_foreach)
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        let issue_kind = if non_null_union.from_docblock {
                            IssueKind::DocblockTypeContradiction
                        } else {
                            IssueKind::TypeDoesNotContainNull
                        };

                        let non_null_id = non_null_union.get_id(Some(analyzer.interner));
                        analysis_data.add_issue(
                            Issue::new(
                                issue_kind,
                                format!("{} does not contain null", non_null_id),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            )
                            // Psalm's AssertionFinder passes "{type} null" as
                            // the dupe key for null-comparison contradictions.
                            .with_dupe_key(format!("{} null", non_null_id)),
                        );
                    }
                } else if (!context.inside_loop || context.inside_foreach)
                    // Two non-null literal operands carry no assertion var
                    // key, so Psalm's paradox checks never see them
                    // (`null !== "name"` from an operator-precedence accident
                    // stays silent); literal-vs-null reports above.
                    && !(comparison_operand_is_literal(binop.lhs.unparenthesized())
                        && comparison_operand_is_literal(binop.rhs.unparenthesized()))
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

    // Psalm's checkForImpureEqualityComparison: a loose `==` between a string
    // and an object implicitly calls the object's __toString; in a
    // mutation-free context a non-mutation-free __toString is an
    // ImpureMethodCall.
    if matches!(&binop.operator, BinaryOperator::Equal(_))
        && crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer)
        && let (Some(left_union), Some(right_union)) = (left_type, right_type)
    {
        let pairs = [(left_union, right_union), (right_union, left_union)];
        for (string_side, object_side) in pairs {
            if !string_side.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TString
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TNonEmptyString
                        | TAtomic::TNumericString
                )
            }) {
                continue;
            }

            emit_impure_to_string_for_union(analyzer, object_side, pos, analysis_data);
            break;
        }
    }

    if matches!(
        &binop.operator,
        BinaryOperator::Equal(_) | BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_)
    ) {
        if let (Some(left_union), Some(right_union)) = (left_type, right_type) {
            if left_union.from_calculation || right_union.from_calculation {
                return TUnion::bool();
            }
            let should_check = is_get_class_call(binop.lhs)
                || is_get_class_call(binop.rhs)
                || (matches!(&binop.operator, BinaryOperator::Equal(_))
                    && (weak_equality_compares_object_to_non_object(left_union, right_union)
                        || (union_is_int_like(left_union) && union_is_int_like(right_union))));

            // A weak `==` comparison between a Stringable object (one with a
            // `__toString` method) and a string-like value is valid in PHP: the
            // object is coerced to its string form. Psalm does not flag it.
            let should_check = should_check
                && !weak_equality_compares_stringable_to_string(analyzer, left_union, right_union);

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
        // Psalm types `<=>` as the literal union -1|0|1.
        BinaryOperator::Spaceship(_) => TUnion::from_types(vec![
            TAtomic::TLiteralInt { value: -1 },
            TAtomic::TLiteralInt { value: 0 },
            TAtomic::TLiteralInt { value: 1 },
        ]),
        _ => TUnion::bool(),
    }
}

/// `$x instanceof SomeClass` against an unknown class is UndefinedClass
/// (names resolve case-sensitively; a wrong-cased reference gets the declared
/// casing named in the message). `class_exists()` guards suppress the report.
fn check_instanceof_class_exists(
    analyzer: &StatementsAnalyzer<'_>,
    rhs: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Expression::Identifier(identifier) = rhs.unparenthesized() else {
        return;
    };
    let raw = identifier.value();
    if raw.eq_ignore_ascii_case("self")
        || raw.eq_ignore_ascii_case("static")
        || raw.eq_ignore_ascii_case("parent")
    {
        return;
    }

    let requested = analyzer
        .get_resolved_name(identifier.span().start.offset)
        .unwrap_or_else(|| analyzer.interner.intern(raw.trim_start_matches('\\')));
    let requested = context
        .class_aliases
        .get(&requested)
        .copied()
        .unwrap_or(requested);

    if analyzer.codebase.get_class(requested).is_some() {
        return;
    }

    // class_exists()-guarded names are phantom classes; don't report.
    let guard_key = format!(
        "@class_exists({})",
        analyzer
            .interner
            .lookup(requested)
            .trim_start_matches('\\')
            .to_ascii_lowercase()
    );
    let guard_id = VarName::new(&guard_key);
    if context
        .locals
        .get(&guard_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
    {
        return;
    }

    let span = rhs.span();
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedClass,
        crate::class_casing::undefined_class_message(analyzer, analyzer.interner.lookup(requested)),
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
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

    // Psalm derives comparison verdicts from assertion keys and clauses, not
    // from folding call results — `count($a) !== count($b)` over two
    // two-element shapes stays bool even though both sides are literal 2.
    // Property/constant accesses (`Suit::Hearts->value === "h"`) likewise
    // stay bool; the reconciler owns any redundancy verdicts.
    if matches!(
        left_expr.unparenthesized(),
        Expression::Call(_) | Expression::Access(_)
    ) || matches!(
        right_expr.unparenthesized(),
        Expression::Call(_) | Expression::Access(_)
    ) {
        return TUnion::bool();
    }

    let (Some(left), Some(right)) = (left, right) else {
        return TUnion::bool();
    };

    if left.from_calculation || right.from_calculation {
        return TUnion::bool();
    }

    if let (Some(left_atomic), Some(right_atomic)) = (left.get_single(), right.get_single())
        && has_deterministic_strict_identity(left_atomic)
        && has_deterministic_strict_identity(right_atomic)
        // Psalm never folds comparisons with `null`: the reconciler owns the
        // verdict there (TypeDoesNotContainNull for an impossible `=== null`,
        // silence for null-vs-null), so the expression stays `bool` and the
        // if-condition's always-falsy check cannot double-report.
        && !matches!(left_atomic, TAtomic::TNull)
        && !matches!(right_atomic, TAtomic::TNull)
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
            let var_id = VarName::new(direct.name);
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
    // The non-specific `literal-string` sentinel is a `TLiteralString` but stands
    // for *any* literal string (including `""`), so it carries no determined
    // value — a `literal-string === ""` must stay `bool`, not fold to a verdict.
    if let TAtomic::TLiteralString { value } = atomic {
        return value != pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE;
    }
    matches!(
        atomic,
        TAtomic::TNull
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
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
        && union
            .types
            .iter()
            .all(|atomic| atomic_is_stringable_object(analyzer, atomic))
}

fn atomic_is_stringable_object(analyzer: &StatementsAnalyzer<'_>, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            *name == pzoom_str::StrId::STRINGABLE
                || analyzer
                    .codebase
                    .get_class(*name)
                    .is_some_and(|class_info| {
                        class_info
                            .methods
                            .contains_key(&pzoom_str::StrId::TO_STRING)
                            || class_info
                                .all_parent_interfaces
                                .contains(&pzoom_str::StrId::STRINGABLE)
                    })
        }
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|t| atomic_is_stringable_object(analyzer, t)),
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
                TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
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

    // Psalm's ArithmeticOpAnalyzer folds the operation over every atomic
    // pair, so literal-int unions (`32 | ($promoted ? 8 : 0)`) produce a
    // union of literal results rather than plain int.
    let literal_values = |union: &TUnion| -> Option<Vec<i64>> {
        union
            .types
            .iter()
            .map(|atomic| match atomic {
                TAtomic::TLiteralInt { value } => Some(*value),
                _ => None,
            })
            .collect()
    };
    let (Some(left_values), Some(right_values)) = (literal_values(left), literal_values(right))
    else {
        return TUnion::int();
    };

    let mut results: Vec<i64> = Vec::new();
    for left_value in &left_values {
        for right_value in &right_values {
            let result = match operator {
                mago_syntax::ast::ast::binary::BinaryOperator::BitwiseAnd(_) => {
                    left_value & right_value
                }
                mago_syntax::ast::ast::binary::BinaryOperator::BitwiseOr(_) => {
                    left_value | right_value
                }
                mago_syntax::ast::ast::binary::BinaryOperator::BitwiseXor(_) => {
                    left_value ^ right_value
                }
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
            if !results.contains(&result) {
                results.push(result);
            }
        }
    }

    // Folded results are calculation-derived (Psalm's `Type::getInt(true, N)`):
    // comparisons against them stay silent.
    let mut folded = TUnion::from_types(
        results
            .into_iter()
            .map(|value| TAtomic::TLiteralInt { value })
            .collect(),
    );
    folded.from_calculation = true;
    folded
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
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TIntRange { .. }
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

/// A bare literal operand (scalar literal or `null`/`true`/`false` constant):
/// no assertion var key exists for it.
fn comparison_operand_is_literal(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Literal(_) | Expression::ConstantAccess(_))
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
