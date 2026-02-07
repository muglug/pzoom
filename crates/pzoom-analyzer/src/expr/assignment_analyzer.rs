//! Assignment expression analyzer.
//!
//! This module handles various forms of PHP assignments:
//! - Simple variable assignment: $x = value
//! - Property assignment: $obj->prop = value (handled by instance_property_assignment_analyzer)
//! - Static property assignment: Class::$prop = value (handled by static_property_assignment_analyzer)
//! - Array assignment: $arr[key] = value (handled by array_assignment_analyzer)
//! - Destructuring: list($a, $b) = $arr or [$a, $b] = $arr

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::assignment::{Assignment, AssignmentOperator};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use indexmap::IndexMap;
use pzoom_code_info::algebra::{Clause, ClauseKey, combine_ored_clauses};
use pzoom_code_info::t_atomic::{ArrayKey, NON_SPECIFIC_LITERAL_STRING_VALUE};
use pzoom_code_info::{
    Assertion, DataFlowNode, Issue, IssueKind, TAtomic, TUnion, VarId, combine_union_types,
};
use pzoom_str::StrId;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expr::assignment::{
    array_assignment_analyzer, instance_property_assignment_analyzer,
    static_property_assignment_analyzer,
};
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::issue_suppression;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator::is_class_subtype_of;
use crate::type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator};

/// Analyze an assignment expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    assignment: &Assignment<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Check if this is a property assignment - handle specially
    if let Expression::Access(access) = assignment.lhs {
        use mago_syntax::ast::ast::access::Access;

        match access {
            Access::Property(prop_access) => {
                instance_property_assignment_analyzer::analyze(
                    analyzer,
                    prop_access,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                );
                return;
            }
            Access::StaticProperty(static_prop) => {
                static_property_assignment_analyzer::analyze(
                    analyzer,
                    static_prop,
                    assignment.rhs,
                    pos,
                    analysis_data,
                    context,
                );
                return;
            }
            _ => {}
        }
    }

    // Check if this is an array element assignment
    if let Expression::ArrayAccess(array_access) = assignment.lhs {
        array_assignment_analyzer::analyze(
            analyzer,
            array_access,
            assignment.rhs,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    // Check if this is an array append assignment
    if let Expression::ArrayAppend(array_append) = assignment.lhs {
        array_assignment_analyzer::analyze_append(
            analyzer,
            array_append,
            assignment.rhs,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    if let Some(reference_operand) = get_reference_operand(assignment.rhs) {
        if analyze_reference_assignment(
            analyzer,
            assignment.lhs,
            reference_operand,
            pos,
            analysis_data,
            context,
        ) {
            return;
        }

        if !should_suppress_issue(analyzer, pos.0, "UnsupportedReferenceUsage") {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnsupportedReferenceUsage,
                "This reference assignment cannot be analyzed",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Analyze the right-hand side first
    let rhs_pos = expression_analyzer::analyze(analyzer, assignment.rhs, analysis_data, context);
    let rhs_type = analysis_data
        .get_expr_type(rhs_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);
    let rhs_type = if matches!(assignment.operator, AssignmentOperator::Concat(_)) {
        infer_concat_assignment_type(analyzer, assignment.lhs, &rhs_type, context)
    } else {
        rhs_type
    };

    emit_mixed_assignment_issue_if_needed(
        analyzer,
        assignment.lhs,
        assignment.rhs,
        &rhs_type,
        pos,
        analysis_data,
    );

    // Handle the left-hand side
    analyze_assignment_lhs(
        analyzer,
        assignment.lhs,
        assignment.rhs,
        &rhs_type,
        pos.0,
        analysis_data,
        context,
    );

    if let Expression::Variable(Variable::Direct(direct_var)) = assignment.lhs {
        handle_assignment_with_boolean_logic(
            analyzer,
            direct_var.name,
            assignment.lhs,
            assignment.rhs,
            &rhs_type,
            analysis_data,
            context,
        );
    }

    // The assignment expression itself has the type of the RHS
    analysis_data.set_expr_type(pos, rhs_type);
}

fn infer_concat_assignment_type(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_type: &TUnion,
    context: &BlockContext,
) -> TUnion {
    const MAX_LITERAL_CONCAT_COMBINATIONS: usize = 64;

    let lhs_type = match lhs {
        Expression::Variable(Variable::Direct(direct_var)) => {
            context.get_var_type(analyzer.interner.intern(direct_var.name))
        }
        _ => None,
    };

    if let (Some(lhs_type), Some(rhs_literals)) =
        (lhs_type, extract_concat_literal_fragments(rhs_type))
        && let Some(lhs_literals) = extract_concat_literal_fragments(lhs_type)
    {
        let combinations = lhs_literals.len() * rhs_literals.len();
        if combinations > 0 && combinations < MAX_LITERAL_CONCAT_COMBINATIONS {
            let mut concatenated_literals = Vec::with_capacity(combinations);

            for lhs_literal in lhs_literals {
                for rhs_literal in &rhs_literals {
                    let combined_literal = format!("{}{}", lhs_literal, rhs_literal);
                    if !concatenated_literals.contains(&combined_literal) {
                        concatenated_literals.push(combined_literal);
                    }
                }
            }

            if !concatenated_literals.is_empty() {
                return TUnion::from_types(
                    concatenated_literals
                        .into_iter()
                        .map(|value| TAtomic::TLiteralString { value })
                        .collect(),
                );
            }
        }
    }

    TUnion::new(TAtomic::TString)
}

fn extract_concat_literal_fragments(union: &TUnion) -> Option<Vec<String>> {
    let mut fragments = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let fragment = match atomic {
            TAtomic::TLiteralString { value } => {
                if value == NON_SPECIFIC_LITERAL_STRING_VALUE {
                    return None;
                }
                value.clone()
            }
            TAtomic::TLiteralInt { value } => value.to_string(),
            TAtomic::TLiteralFloat { value } => value.to_string(),
            TAtomic::TTrue => "1".to_string(),
            TAtomic::TFalse | TAtomic::TNull => String::new(),
            _ => return None,
        };

        if !fragments.contains(&fragment) {
            fragments.push(fragment);
        }
    }

    if fragments.is_empty() {
        None
    } else {
        Some(fragments)
    }
}

fn emit_mixed_assignment_issue_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !rhs_type.is_mixed() {
        return;
    }

    if matches!(
        rhs_expr.unparenthesized(),
        Expression::Access(Access::ClassConstant(_))
    ) {
        return;
    }

    if let Expression::Access(Access::Property(prop_access)) = rhs_expr.unparenthesized() {
        let object_span = prop_access.object.span();
        let object_pos = (object_span.start.offset, object_span.end.offset);
        if let Some(object_type) = analysis_data.get_expr_type(object_pos)
            && union_has_simplexml_object(analyzer, &object_type)
        {
            return;
        }
    }

    let Expression::Variable(Variable::Direct(direct_var)) = lhs else {
        return;
    };

    if direct_var.name.starts_with("$_") {
        return;
    }

    let var_id = analyzer.interner.intern(direct_var.name);
    let has_inline_annotation = get_inline_var_annotation_type(analyzer, pos.0, var_id)
        .or_else(|| {
            analysis_data
                .current_stmt_start
                .and_then(|stmt_start| get_inline_var_annotation_type(analyzer, stmt_start, var_id))
        })
        .is_some();

    if has_inline_annotation {
        return;
    }

    let issue_offset = analysis_data.current_stmt_start.unwrap_or(pos.0);
    if should_suppress_issue(analyzer, issue_offset, "MixedAssignment") {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::MixedAssignment,
        format!(
            "Unable to determine the type that {} is being assigned to",
            direct_var.name
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn union_has_simplexml_object(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            return false;
        };

        let class_name = analyzer.interner.lookup(*name);
        let normalized = class_name.trim_start_matches('\\');
        normalized.eq_ignore_ascii_case("SimpleXMLElement")
            || normalized.eq_ignore_ascii_case("SimpleXMLIterator")
    })
}

/// Analyze the left-hand side of an assignment and set variable types.
fn analyze_assignment_lhs(
    analyzer: &StatementsAnalyzer<'_>,
    lhs: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match lhs {
        Expression::Variable(var) => {
            if let Variable::Direct(direct) = var {
                let var_name = direct.name;

                // Intern the variable name
                let var_id = analyzer.interner.intern(var_name);

                if var_id == StrId::THIS_VAR && context.get_var_type(StrId::THIS_VAR).is_none() {
                    if !issue_suppression::is_issue_suppressed_at(
                        analyzer,
                        assignment_offset,
                        "InvalidScope",
                    ) {
                        let (line, col) = analyzer.get_line_column(assignment_offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidScope,
                            "Invalid assignment to $this in a non-class context",
                            analyzer.file_path,
                            assignment_offset,
                            assignment_offset.saturating_add(1),
                            line,
                            col,
                        ));
                    }
                    return;
                }

                if context.has_confusing_reference(var_id) {
                    let (line, col) = analyzer.get_line_column(assignment_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ReferenceReusedFromConfusingScope,
                        format!(
                            "${} may be a reference from a previous conditional/loop scope",
                            var_name.trim_start_matches('$')
                        ),
                        analyzer.file_path,
                        assignment_offset,
                        assignment_offset.saturating_add(1),
                        line,
                        col,
                    ));
                }

                let mut assigned_type =
                    get_inline_var_annotation_type(analyzer, assignment_offset, var_id)
                        .or_else(|| {
                            analysis_data.current_stmt_start.and_then(|stmt_start| {
                                get_inline_var_annotation_type(analyzer, stmt_start, var_id)
                            })
                        })
                        .unwrap_or_else(|| rhs_type.clone());

                emit_reference_constraint_issue_if_needed(
                    analyzer,
                    context,
                    var_id,
                    &assigned_type,
                    assignment_offset,
                    analysis_data,
                );

                let direct_span = direct.span();
                let assignment_node = DataFlowNode::get_for_lvar(
                    VarId(var_id),
                    make_data_flow_node_position(
                        analyzer,
                        (direct_span.start.offset, direct_span.end.offset),
                    ),
                );
                analysis_data
                    .data_flow_graph
                    .add_node(assignment_node.clone());
                if !assigned_type.parent_nodes.is_empty() {
                    add_default_dataflow_paths(
                        &mut analysis_data.data_flow_graph,
                        &assigned_type.parent_nodes,
                        &assignment_node,
                    );
                }
                assigned_type.parent_nodes = vec![assignment_node];

                // Set the variable's type in context (this also tracks assignment)
                context.set_var_type(var_id, assigned_type);
                clear_dependent_property_types(analyzer, context, var_name);
                clear_array_path_types_for_base_var(analyzer, context, var_name);
                clear_dependent_array_access_types(analyzer, context, var_name);
                clear_dependent_class_string_origins(context, var_id);
                remove_var_clauses_from_context(context, var_name);

                if let Some(source_var_id) = get_class_source_var_id(analyzer, rhs_expr) {
                    context.class_string_origins.insert(var_id, source_var_id);
                } else {
                    context.class_string_origins.remove(&var_id);
                }
            }
        }
        Expression::Access(access) => {
            use mago_syntax::ast::ast::access::Access;

            match access {
                Access::Property(prop_access) => {
                    // Top-level property assignments are handled before this function is called.
                    // We still need this branch for destructuring assignments, e.g.
                    // list($this->a, $this->b) = ["a", "b"];
                    let span = prop_access.span();
                    instance_property_assignment_analyzer::analyze_with_known_type(
                        analyzer,
                        prop_access,
                        rhs_type.clone(),
                        (span.start.offset, span.end.offset),
                        analysis_data,
                        context,
                    );
                }
                Access::NullSafeProperty(_) | Access::StaticProperty(_) => {
                    // Destructuring into nullsafe/static properties is uncommon and currently
                    // not modeled with per-element value expressions.
                }
                Access::ClassConstant(_) => {
                    // Cannot assign to class constants - this would be a parse error
                    // The PHP parser would reject this before we get here
                }
            }
        }
        Expression::List(list) => {
            // list() assignment - destructure RHS by offset/key, matching Psalm/Hakana behavior.
            for (offset, element) in list.elements.iter().enumerate() {
                analyze_destructuring_element(
                    analyzer,
                    element,
                    offset,
                    rhs_expr,
                    rhs_type,
                    assignment_offset,
                    analysis_data,
                    context,
                );
            }
        }
        Expression::Array(array) => {
            // Short destructuring syntax: [$a, $b] = $arr
            for (offset, element) in array.elements.iter().enumerate() {
                analyze_destructuring_element(
                    analyzer,
                    element,
                    offset,
                    rhs_expr,
                    rhs_type,
                    assignment_offset,
                    analysis_data,
                    context,
                );
            }
        }
        Expression::ArrayAccess(_) => {
            // Array element assignment - $arr[key] = value
            // Handled by array_assignment_analyzer when dispatched from expression_analyzer
        }
        Expression::ArrayAppend(_) => {
            // Array append - $arr[] = value
            // Handled by array_assignment_analyzer when dispatched from expression_analyzer
        }
        _ => {
            // Other expressions on LHS (invalid in most cases)
        }
    }
}

fn emit_reference_constraint_issue_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    var_id: StrId,
    assigned_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(constraints) = context.get_reference_constraints(var_id) else {
        return;
    };

    if constraints.is_empty() {
        return;
    }

    if reference_constraints_conflict(analyzer, constraints) {
        let (line, col) = analyzer.get_line_column(assignment_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::ConflictingReferenceConstraint,
            format!(
                "${} is constrained by incompatible reference types",
                analyzer.interner.lookup(var_id)
            ),
            analyzer.file_path,
            assignment_offset,
            assignment_offset.saturating_add(1),
            line,
            col,
        ));
        return;
    }

    let violates_constraint = constraints.iter().any(|constraint| {
        let mut comparison = TypeComparisonResult::new();
        !union_type_comparator::is_contained_by(
            analyzer.codebase,
            assigned_type,
            constraint,
            false,
            false,
            &mut comparison,
        )
    });

    if !violates_constraint {
        return;
    }

    let (line, col) = analyzer.get_line_column(assignment_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::ReferenceConstraintViolation,
        format!(
            "${} violates a by-reference type constraint",
            analyzer.interner.lookup(var_id)
        ),
        analyzer.file_path,
        assignment_offset,
        assignment_offset.saturating_add(1),
        line,
        col,
    ));
}

fn reference_constraints_conflict(
    analyzer: &StatementsAnalyzer<'_>,
    constraints: &[TUnion],
) -> bool {
    for i in 0..constraints.len() {
        for j in (i + 1)..constraints.len() {
            let left = &constraints[i];
            let right = &constraints[j];
            let overlaps =
                union_type_comparator::can_be_contained_by(analyzer.codebase, left, right)
                    || union_type_comparator::can_be_contained_by(analyzer.codebase, right, left);
            if !overlaps {
                return true;
            }
        }
    }

    false
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

fn clear_array_path_types_for_base_var(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
) {
    let prefix = format!("{var_name}[");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|var_id| {
            analyzer
                .interner
                .lookup(*var_id)
                .as_ref()
                .starts_with(&prefix)
        })
        .collect();

    for key in keys_to_clear {
        context.locals.remove(&key);
        context.assigned_var_ids.remove(&key);
        context.possibly_assigned_var_ids.remove(&key);
        context.class_string_origins.remove(&key);
    }
}

fn clear_dependent_class_string_origins(context: &mut BlockContext, source_var_id: StrId) {
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

#[derive(Clone)]
enum DestructuringLookupKey {
    Int(i64),
    String(String),
    Unknown,
}

fn analyze_destructuring_element(
    analyzer: &StatementsAnalyzer<'_>,
    element: &ArrayElement<'_>,
    offset: usize,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    assignment_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let (target_expr, lookup_key) = match element {
        ArrayElement::Missing(_) => return,
        ArrayElement::Variadic(_) => return,
        ArrayElement::Value(value_element) => (
            value_element.value,
            DestructuringLookupKey::Int(offset as i64),
        ),
        ArrayElement::KeyValue(kv) => (
            kv.value,
            extract_destructuring_key(kv.key).unwrap_or(DestructuringLookupKey::Unknown),
        ),
    };

    if !rhs_can_be_destructured(analyzer, rhs_type) {
        let (line, col) = analyzer.get_line_column(assignment_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArrayOffset,
            "Cannot destructure non-array value",
            analyzer.file_path,
            assignment_offset,
            assignment_offset.saturating_add(1),
            line,
            col,
        ));
    }

    let target_type = infer_destructured_value_type(analyzer, rhs_type, &lookup_key);

    analyze_assignment_lhs(
        analyzer,
        target_expr,
        rhs_expr,
        &target_type,
        assignment_offset,
        analysis_data,
        context,
    );
}

fn rhs_can_be_destructured(analyzer: &StatementsAnalyzer<'_>, rhs_type: &TUnion) -> bool {
    let array_access_id = analyzer.interner.intern("ArrayAccess");

    rhs_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
                | TAtomic::TMixed
                | TAtomic::TNonEmptyMixed
        ) || matches!(atomic, TAtomic::TNamedObject { name, .. } if is_class_subtype_of(*name, array_access_id, analyzer.codebase))
    })
}

fn extract_destructuring_key(expr: &Expression<'_>) -> Option<DestructuringLookupKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .map(|value| DestructuringLookupKey::Int(value as i64)),
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| DestructuringLookupKey::String(value.to_string())),
        _ => None,
    }
}

fn infer_destructured_value_type(
    analyzer: &StatementsAnalyzer<'_>,
    rhs_type: &TUnion,
    lookup_key: &DestructuringLookupKey,
) -> TUnion {
    let mut inferred_type: Option<TUnion> = None;
    let mut saw_destructurable_type = false;
    let array_access_id = analyzer.interner.intern("ArrayAccess");

    for atomic in &rhs_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                saw_destructurable_type = true;
                add_inferred_union(&mut inferred_type, value_type);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                saw_destructurable_type = true;
                if let Some(array_key) = lookup_key_to_array_key(lookup_key) {
                    if let Some(property_type) = properties.get(&array_key) {
                        add_inferred_union(&mut inferred_type, property_type);
                    } else if let Some(fallback_value_type) = fallback_value_type {
                        add_inferred_union(&mut inferred_type, fallback_value_type);
                    }
                } else if let Some(fallback_value_type) = fallback_value_type {
                    add_inferred_union(&mut inferred_type, fallback_value_type);
                } else if !properties.is_empty() {
                    for property_type in properties.values() {
                        add_inferred_union(&mut inferred_type, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                return TUnion::mixed();
            }
            TAtomic::TNamedObject { name, .. } => {
                if is_class_subtype_of(*name, array_access_id, analyzer.codebase) {
                    saw_destructurable_type = true;
                    add_inferred_union(&mut inferred_type, &TUnion::mixed());
                }
            }
            _ => {}
        }
    }

    if let Some(inferred_type) = inferred_type {
        inferred_type
    } else if saw_destructurable_type {
        TUnion::mixed()
    } else {
        TUnion::mixed()
    }
}

fn lookup_key_to_array_key(key: &DestructuringLookupKey) -> Option<ArrayKey> {
    match key {
        DestructuringLookupKey::Int(value) => Some(ArrayKey::Int(*value)),
        DestructuringLookupKey::String(value) => Some(ArrayKey::String(value.clone())),
        DestructuringLookupKey::Unknown => None,
    }
}

fn add_inferred_union(target: &mut Option<TUnion>, next: &TUnion) {
    if let Some(existing) = target {
        *existing = combine_union_types(existing, next, false);
    } else {
        *target = Some(next.clone());
    }
}

fn get_reference_operand<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return None;
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Reference(_)) {
        return None;
    }

    Some(unary.operand)
}

fn analyze_reference_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    lhs_expr: &Expression<'_>,
    rhs_operand: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> bool {
    let Some(lhs_key) = expression_identifier::get_expression_var_key(lhs_expr) else {
        return false;
    };
    let Some(rhs_key) = expression_identifier::get_expression_var_key(rhs_operand) else {
        return false;
    };

    let lhs_var_id = analyzer.interner.intern(&lhs_key);
    let rhs_var_id = analyzer.interner.intern(&rhs_key);

    if lhs_var_id == StrId::THIS_VAR && context.get_var_type(StrId::THIS_VAR).is_none() {
        if !issue_suppression::is_issue_suppressed_at(analyzer, pos.0, "InvalidScope") {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidScope,
                "Invalid assignment to $this in a non-class context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return true;
    }

    if let Expression::Variable(Variable::Direct(_)) = rhs_operand.unparenthesized()
        && context.get_var_type(rhs_var_id).is_none()
    {
        // Psalm initializes newly created references to null.
        context.set_var_type_direct(rhs_var_id, TUnion::null());
    }

    let rhs_pos = expression_analyzer::analyze(analyzer, rhs_operand, analysis_data, context);
    let rhs_type = analysis_data
        .get_expr_type(rhs_pos)
        .map(|t| (*t).clone())
        .or_else(|| context.get_var_type(rhs_var_id).cloned())
        .unwrap_or_else(TUnion::mixed);

    if has_unnamed_inline_var_annotation(analyzer, pos.0)
        || analysis_data
            .current_stmt_start
            .is_some_and(|start| has_unnamed_inline_var_annotation(analyzer, start))
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidDocblock,
            "Docblock type cannot be used for reference assignment",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let rhs_is_external = rhs_key.contains('[') || rhs_key.contains("->") || rhs_key.contains("::");
    context.set_reference(lhs_var_id, rhs_var_id, rhs_type.clone(), rhs_is_external);

    if let Expression::Variable(Variable::Direct(direct)) = lhs_expr.unparenthesized() {
        clear_dependent_property_types(analyzer, context, direct.name);
        clear_array_path_types_for_base_var(analyzer, context, direct.name);
        clear_dependent_array_access_types(analyzer, context, direct.name);
        clear_dependent_class_string_origins(context, lhs_var_id);
        remove_var_clauses_from_context(context, direct.name);

        if let Some(source_var_id) = get_class_source_var_id(analyzer, rhs_operand) {
            context
                .class_string_origins
                .insert(lhs_var_id, source_var_id);
        } else {
            context.class_string_origins.remove(&lhs_var_id);
        }
    }

    analysis_data.set_expr_type(pos, rhs_type);
    true
}

fn has_unnamed_inline_var_annotation(analyzer: &StatementsAnalyzer<'_>, offset: u32) -> bool {
    analyzer
        .get_inline_var_annotations(offset)
        .is_some_and(|annotations| {
            annotations
                .iter()
                .any(|annotation| annotation.var_name.is_none())
        })
}

fn should_suppress_issue(
    analyzer: &StatementsAnalyzer<'_>,
    issue_offset: u32,
    issue_name: &str,
) -> bool {
    if analyzer.config.is_issue_suppressed(issue_name) {
        return true;
    }

    let source = analyzer.source;
    let offset = issue_offset as usize;
    if offset == 0 || offset > source.len() {
        return false;
    }

    let bytes = source.as_bytes();
    let mut cursor = offset;
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }

    if cursor < 2 || &source[cursor - 2..cursor] != "*/" {
        return false;
    }

    let doc_end = cursor;
    let Some(doc_start) = source[..doc_end - 2].rfind("/**") else {
        return false;
    };

    let docblock = &source[doc_start..doc_end];
    docblock
        .split('\n')
        .filter(|line| line.contains("@psalm-suppress"))
        .any(|line| {
            line.split_whitespace()
                .skip_while(|part| *part != "@psalm-suppress")
                .nth(1)
                .is_some_and(|suppressed| suppressed == issue_name)
        })
}

fn get_inline_var_annotation_type(
    analyzer: &StatementsAnalyzer<'_>,
    assignment_offset: u32,
    var_id: StrId,
) -> Option<TUnion> {
    let annotations = analyzer.get_inline_var_annotations(assignment_offset)?;

    let mut unnamed_match = None;
    for annotation in annotations {
        match annotation.var_name {
            Some(name) if name == var_id => return Some(annotation.var_type.clone()),
            None if unnamed_match.is_none() => unnamed_match = Some(annotation.var_type.clone()),
            _ => {}
        }
    }

    unnamed_match
}

fn get_class_source_var_id(
    analyzer: &StatementsAnalyzer<'_>,
    rhs_expr: &Expression<'_>,
) -> Option<pzoom_str::StrId> {
    let Expression::Call(Call::Function(function_call)) = rhs_expr.unparenthesized() else {
        return None;
    };

    let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
        return None;
    };

    if !function_name.value().eq_ignore_ascii_case("get_class") {
        return None;
    }

    let first_arg = function_call.argument_list.arguments.first()?;
    let Expression::Variable(Variable::Direct(direct)) = first_arg.value().unparenthesized() else {
        return None;
    };

    Some(analyzer.interner.intern(direct.name))
}

fn handle_assignment_with_boolean_logic(
    analyzer: &StatementsAnalyzer<'_>,
    assigned_var_name: &str,
    lhs_expr: &Expression<'_>,
    rhs_expr: &Expression<'_>,
    rhs_type: &TUnion,
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if !is_bool_like(rhs_type) {
        return;
    }

    if !matches!(rhs_expr.unparenthesized(), Expression::Binary(_)) {
        return;
    }

    let var_object_id = (lhs_expr.start_offset() as u32, lhs_expr.end_offset() as u32);
    let cond_object_id = (rhs_expr.start_offset() as u32, rhs_expr.end_offset() as u32);

    let right_clauses =
        assertion_finder::get_assertions(analyzer, rhs_expr, analysis_data).if_true_clauses;
    if right_clauses.is_empty() {
        return;
    }

    let right_clauses = filter_clauses_for_assignment_target(assigned_var_name, right_clauses);
    if right_clauses.is_empty() {
        return;
    }

    let mut possibilities = BTreeMap::new();
    possibilities.insert(
        ClauseKey::Name(assigned_var_name.to_string()),
        IndexMap::from([(Assertion::Falsy.to_hash(), Assertion::Falsy)]),
    );

    let assignment_clauses = combine_ored_clauses(
        vec![Clause::new(
            possibilities,
            var_object_id,
            var_object_id,
            None,
            None,
            None,
        )],
        right_clauses,
        cond_object_id,
    );

    if let Ok(assignment_clauses) = assignment_clauses {
        context
            .clauses
            .extend(assignment_clauses.into_iter().map(Rc::new));
    }
}

fn is_bool_like(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .all(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
}

fn filter_clauses_for_assignment_target(
    assigned_var_name: &str,
    clauses: Vec<Clause>,
) -> Vec<Clause> {
    clauses
        .into_iter()
        .filter_map(|clause| {
            let mut possibilities = clause.possibilities.clone();
            possibilities.retain(|key, _| match key {
                ClauseKey::Name(name) => {
                    name != assigned_var_name
                        && !name.starts_with(&format!("{}[", assigned_var_name))
                        && !name.starts_with(&format!("{}->", assigned_var_name))
                        && !name.contains(&format!("[{}]", assigned_var_name))
                }
                ClauseKey::Range(..) => true,
            });

            if possibilities.is_empty() {
                return None;
            }

            Some(Clause::new(
                possibilities,
                clause.creating_conditional_id,
                clause.creating_object_id,
                Some(clause.wedge),
                Some(clause.reconcilable),
                Some(clause.generated),
            ))
        })
        .collect()
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
