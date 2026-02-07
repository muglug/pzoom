//! Array assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::{ArrayAccess, ArrayAppend};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::algebra::ClauseKey;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{TAtomic, TUnion, combine_union_types};
use rustc_hash::FxHashMap;

use crate::context::BlockContext;
use crate::expr::assignment::instance_property_assignment_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

#[derive(Clone, Copy)]
enum AssignmentDimKind<'a> {
    Key(&'a Expression<'a>),
    Append,
}

#[derive(Clone, Copy)]
struct AssignmentDim<'a> {
    kind: AssignmentDimKind<'a>,
    result_pos: Pos,
}

#[derive(Clone)]
struct ResolvedAssignmentDim {
    key_type: Option<TUnion>,
    key_repr: Option<String>,
    result_pos: Pos,
}

/// Analyze an array assignment ($arr[key] = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let mut dims = Vec::new();
    let root_expr = collect_assignment_dims(access.array, &mut dims);
    dims.push(AssignmentDim {
        kind: AssignmentDimKind::Key(access.index),
        result_pos: span_to_pos(access.span()),
    });

    analyze_assignment_chain(
        analyzer,
        root_expr,
        dims,
        value_expr,
        pos,
        analysis_data,
        context,
    );
}

/// Analyze an array append assignment ($arr[] = value).
pub fn analyze_append(
    analyzer: &StatementsAnalyzer<'_>,
    append: &ArrayAppend<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let mut dims = Vec::new();
    let root_expr = collect_assignment_dims(append.array, &mut dims);
    dims.push(AssignmentDim {
        kind: AssignmentDimKind::Append,
        result_pos: span_to_pos(append.span()),
    });

    analyze_assignment_chain(
        analyzer,
        root_expr,
        dims,
        value_expr,
        pos,
        analysis_data,
        context,
    );
}

fn analyze_assignment_chain<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    root_expr: &'a Expression<'a>,
    dims: Vec<AssignmentDim<'a>>,
    value_expr: &'a Expression<'a>,
    assignment_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let root_pos = expression_analyzer::analyze(analyzer, root_expr, analysis_data, context);
    let root_type = analysis_data
        .get_expr_type(root_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    if matches!(
        root_expr.unparenthesized(),
        Expression::Call(
            Call::Function(_) | Call::Method(_) | Call::NullSafeMethod(_) | Call::StaticMethod(_)
        )
    ) && root_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
        )
    }) {
        let span = root_expr.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArrayAssignment,
            "Assigning to the output of a function has no effect",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    let mut resolved_dims = Vec::with_capacity(dims.len());
    let root_supports_offset_set = union_supports_offset_set(analyzer, &root_type);
    for dim in dims {
        let (key_type, key_repr) = match dim.kind {
            AssignmentDimKind::Key(index_expr) => {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;
                let key_pos =
                    expression_analyzer::analyze(analyzer, index_expr, analysis_data, context);
                context.inside_general_use = was_inside_general_use;
                let raw_key_type = analysis_data
                    .get_expr_type(key_pos)
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::array_key);
                if !root_supports_offset_set {
                    maybe_emit_invalid_array_offset_for_assignment(
                        analyzer,
                        analysis_data,
                        index_expr,
                        &raw_key_type,
                    );
                }
                (
                    Some(normalize_assignment_key_union(&raw_key_type)),
                    get_assignment_index_key(index_expr),
                )
            }
            AssignmentDimKind::Append => (None, None),
        };

        resolved_dims.push(ResolvedAssignmentDim {
            key_type,
            key_repr,
            result_pos: dim.result_pos,
        });
    }

    let value_pos = expression_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    if resolved_dims.is_empty() {
        analysis_data.set_expr_type(assignment_pos, value_type);
        return;
    }

    // Forward pass: infer container/value types for each nested dim.
    let mut container_types = Vec::with_capacity(resolved_dims.len());
    let mut running_container = root_type.clone();

    for dim in &resolved_dims {
        container_types.push(running_container.clone());

        let child_type = infer_child_type_for_dim(&running_container, dim.key_type.as_ref());
        analysis_data.set_expr_type(dim.result_pos, child_type.clone());
        running_container = child_type;
    }

    // Reverse pass: apply the assignment from leaf to root, autovivifying where needed.
    let mut updated_child_type = value_type.clone();

    for i in (0..resolved_dims.len()).rev() {
        let dim = &resolved_dims[i];
        let container_type = &container_types[i];
        let is_root_assignment = i == 0;
        let is_leaf = i == resolved_dims.len() - 1;
        let emit_mixed_issues = is_root_assignment
            || resolved_dims[..i].iter().all(|previous_dim| {
                previous_dim
                    .key_repr
                    .as_ref()
                    .is_some_and(|repr| assignment_key_repr_is_literal(repr))
            });

        if is_leaf {
            analysis_data.set_expr_type(dim.result_pos, value_type.clone());
        } else {
            analysis_data.set_expr_type(dim.result_pos, updated_child_type.clone());
        }

        updated_child_type = apply_assignment_to_container(
            analyzer,
            analysis_data,
            container_type,
            dim.key_type.as_ref(),
            dim.key_repr.as_ref(),
            &updated_child_type,
            dim.result_pos,
            is_root_assignment,
            emit_mixed_issues,
        );
    }

    analysis_data.set_expr_type(root_pos, updated_child_type.clone());

    if let Expression::Variable(Variable::Direct(direct)) = root_expr.unparenthesized() {
        let var_id = analyzer.interner.intern(direct.name);
        let stored_type = if context.inside_loop && !context.inside_foreach {
            widen_array_like_type_for_loop(&updated_child_type)
        } else {
            updated_child_type.clone()
        };

        clear_dependent_property_types(analyzer, context, direct.name);
        clear_array_path_types_for_base_var(analyzer, context, direct.name);
        clear_dependent_array_access_types(analyzer, context, direct.name);
        clear_dependent_class_string_origins(context, var_id);
        remove_var_clauses_from_context(context, direct.name);
        context.set_var_type(var_id, stored_type);

        let stores_array_path_types =
            union_has_array_like(&root_type) || union_has_array_like(&updated_child_type);
        let stores_object_offset_path_types =
            root_supports_offset_set && !union_has_array_like(&root_type);

        if stores_array_path_types && !stores_object_offset_path_types {
            let mut running_key = direct.name.to_string();

            for dim in &resolved_dims {
                let Some(index_repr) = dim.key_repr.as_ref() else {
                    break;
                };

                running_key.push('[');
                running_key.push_str(index_repr);
                running_key.push(']');

                if let Some(dim_type) = analysis_data.get_expr_type(dim.result_pos) {
                    let dim_key_id = analyzer.interner.intern(&running_key);
                    context.locals.insert(dim_key_id, (*dim_type).clone());
                }
            }
        }
    } else if let Expression::Access(Access::Property(prop_access)) = root_expr.unparenthesized() {
        instance_property_assignment_analyzer::analyze_with_known_type(
            analyzer,
            prop_access,
            updated_child_type.clone(),
            assignment_pos,
            analysis_data,
            context,
        );
    }

    // Assignment expression evaluates to the assigned RHS value.
    analysis_data.set_expr_type(assignment_pos, value_type);
}

fn collect_assignment_dims<'a>(
    expr: &'a Expression<'a>,
    dims: &mut Vec<AssignmentDim<'a>>,
) -> &'a Expression<'a> {
    let expr = expr.unparenthesized();

    match expr {
        Expression::ArrayAccess(access) => {
            let root = collect_assignment_dims(access.array, dims);
            dims.push(AssignmentDim {
                kind: AssignmentDimKind::Key(access.index),
                result_pos: span_to_pos(expr.span()),
            });
            root
        }
        Expression::ArrayAppend(append) => {
            let root = collect_assignment_dims(append.array, dims);
            dims.push(AssignmentDim {
                kind: AssignmentDimKind::Append,
                result_pos: span_to_pos(expr.span()),
            });
            root
        }
        _ => expr,
    }
}

fn span_to_pos(span: mago_span::Span) -> Pos {
    (span.start.offset, span.end.offset)
}

fn get_assignment_index_key(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => {
            int_lit.value.map(|value| value.to_string())
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                int_value.to_string()
            } else {
                let escaped = value.replace('\'', "\\'");
                format!("'{}'", escaped)
            }
        }),
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name.to_string()),
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            let class_name = match class_const_access.class.unparenthesized() {
                Expression::Identifier(identifier) => identifier.value().to_string(),
                Expression::Self_(_) => "self".to_string(),
                Expression::Static(_) => "static".to_string(),
                Expression::Parent(_) => "parent".to_string(),
                _ => return None,
            };

            let constant_name = match &class_const_access.constant {
                ClassLikeConstantSelector::Identifier(identifier) => identifier.value,
                _ => return None,
            };

            Some(format!("{}::{}", class_name, constant_name))
        }
        _ => None,
    }
}

fn union_has_array_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
        ) || matches!(atomic, TAtomic::TObjectIntersection { types } if types.iter().any(|nested| matches!(
            nested,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { .. }
        )))
    })
}

fn assignment_key_repr_is_literal(key_repr: &str) -> bool {
    key_repr.starts_with('\'') || key_repr.parse::<i64>().is_ok()
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

fn clear_dependent_class_string_origins(
    context: &mut BlockContext,
    source_var_id: pzoom_str::StrId,
) {
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

fn apply_assignment_to_container(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    container_type: &TUnion,
    key_type: Option<&TUnion>,
    key_repr: Option<&String>,
    assigned_type: &TUnion,
    issue_pos: Pos,
    is_root_assignment: bool,
    emit_mixed_issues: bool,
) -> TUnion {
    let mut updated = Vec::new();
    let mut has_writable = false;
    let mut invalid_atomic_name: Option<String> = None;
    let mut undefined_offset_set_class: Option<pzoom_str::StrId> = None;
    let mut saw_mixed_assignment = false;

    let offset_set_name = analyzer.interner.intern("offsetSet");

    for atomic in &container_type.types {
        match atomic {
            TAtomic::TArray {
                key_type: existing_key,
                value_type: existing_value,
            }
            | TAtomic::TNonEmptyArray {
                key_type: existing_key,
                value_type: existing_value,
            } => {
                has_writable = true;
                updated.push(update_generic_array_atomic(
                    existing_key,
                    existing_value,
                    key_type,
                    assigned_type,
                ));
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                has_writable = true;
                updated.push(update_list_atomic(
                    value_type,
                    key_type,
                    key_repr,
                    assigned_type,
                ));
            }
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => {
                has_writable = true;
                updated.push(update_keyed_array_atomic(
                    properties,
                    *is_list,
                    *sealed,
                    fallback_key_type.as_deref(),
                    fallback_value_type.as_deref(),
                    key_type,
                    assigned_type,
                ));
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                has_writable = true;
                saw_mixed_assignment = true;
                if is_root_assignment {
                    updated.push(atomic.clone());
                } else {
                    updated.push(create_mixed_container_assignment_atomic(
                        key_type,
                        assigned_type,
                    ));
                }
            }
            TAtomic::TNull | TAtomic::TFalse => {
                has_writable = true;
                updated.push(create_autovivified_array_atomic(key_type, assigned_type));
            }
            TAtomic::TNothing => {
                has_writable = true;
                updated.push(create_autovivified_array_atomic(key_type, assigned_type));
            }
            TAtomic::TNamedObject { name, .. } => {
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    if let Some(offset_set_info) = class_info.methods.get(&offset_set_name) {
                        has_writable = true;
                        maybe_emit_offset_set_argument_issue(
                            analyzer,
                            analysis_data,
                            issue_pos,
                            *name,
                            offset_set_info,
                            key_type,
                        );
                        updated.push(atomic.clone());
                    } else {
                        if undefined_offset_set_class.is_none() {
                            undefined_offset_set_class = Some(*name);
                        }
                        updated.push(atomic.clone());
                    }
                } else {
                    if undefined_offset_set_class.is_none() {
                        undefined_offset_set_class = Some(*name);
                    }
                    updated.push(atomic.clone());
                }
            }
            TAtomic::TObjectIntersection { types } => {
                if types.iter().any(|intersection_atomic| {
                    matches!(
                        intersection_atomic,
                        TAtomic::TArray { .. }
                            | TAtomic::TNonEmptyArray { .. }
                            | TAtomic::TList { .. }
                            | TAtomic::TNonEmptyList { .. }
                            | TAtomic::TKeyedArray { .. }
                            | TAtomic::TString
                            | TAtomic::TNonEmptyString
                            | TAtomic::TLiteralString { .. }
                            | TAtomic::TNumericString
                            | TAtomic::TNonEmptyNumericString
                            | TAtomic::TLowercaseString
                            | TAtomic::TNonEmptyLowercaseString
                            | TAtomic::TTruthyString
                            | TAtomic::TClassString { .. }
                            | TAtomic::TLiteralClassString { .. }
                            | TAtomic::TNull
                            | TAtomic::TFalse
                            | TAtomic::TNothing
                    )
                }) || types.iter().any(|intersection_atomic| {
                    if let TAtomic::TNamedObject { name, .. } = intersection_atomic {
                        return analyzer
                            .codebase
                            .get_class(*name)
                            .is_some_and(|class_info| {
                                class_info.methods.contains_key(&offset_set_name)
                            });
                    }

                    false
                }) {
                    has_writable = true;
                    updated.push(atomic.clone());
                } else {
                    if invalid_atomic_name.is_none() {
                        invalid_atomic_name = Some(atomic.get_id(Some(analyzer.interner)));
                    }
                    updated.push(atomic.clone());
                }
            }
            // String offsets are writable in PHP; preserve string type information.
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. } => {
                has_writable = true;
                if emit_mixed_issues && assigned_type.is_mixed() {
                    emit_mixed_string_offset_assignment_issue(analyzer, analysis_data, issue_pos);
                }
                updated.push(atomic.clone());
            }
            _ => {
                if invalid_atomic_name.is_none() {
                    invalid_atomic_name = Some(atomic.get_id(Some(analyzer.interner)));
                }
                updated.push(atomic.clone());
            }
        }
    }

    if emit_mixed_issues && saw_mixed_assignment {
        emit_mixed_array_assignment_issue(analyzer, analysis_data, issue_pos);
    }

    if !has_writable {
        if let Some(class_id) = undefined_offset_set_class {
            emit_undefined_offset_set_issue(analyzer, analysis_data, issue_pos, class_id);
        } else {
            emit_invalid_array_assignment_issue(
                analyzer,
                analysis_data,
                issue_pos,
                invalid_atomic_name.unwrap_or_else(|| "unknown".to_string()),
            );
        }
    }

    if updated.is_empty() {
        return TUnion::new(create_autovivified_array_atomic(key_type, assigned_type));
    }

    TUnion::from_types(type_combiner::combine(updated, false))
}

fn union_supports_offset_set(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let offset_set_name = analyzer.interner.intern("offsetSet");

    union.types.iter().any(|atomic| {
        if let TAtomic::TNamedObject { name, .. } = atomic {
            return analyzer
                .codebase
                .get_class(*name)
                .is_some_and(|class_info| class_info.methods.contains_key(&offset_set_name));
        }

        false
    })
}

fn maybe_emit_offset_set_argument_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
    class_id: pzoom_str::StrId,
    offset_set_info: &pzoom_code_info::FunctionLikeInfo,
    key_type: Option<&TUnion>,
) {
    let Some(key_type) = key_type else {
        return;
    };

    let Some(first_param) = offset_set_info.params.first() else {
        return;
    };

    let expected_type = first_param
        .get_type()
        .or(first_param.signature_type.as_ref());
    let Some(expected_type) = expected_type else {
        return;
    };

    if expected_type.is_mixed() || expected_type.is_nothing() {
        return;
    }

    let mut comparison_result = TypeComparisonResult::new();
    if union_type_comparator::is_contained_by(
        analyzer.codebase,
        key_type,
        expected_type,
        false,
        false,
        &mut comparison_result,
    ) {
        return;
    }

    let maybe_valid =
        union_type_comparator::can_be_contained_by(analyzer.codebase, key_type, expected_type);
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    let class_name = analyzer.interner.lookup(class_id);

    analysis_data.add_issue(Issue::new(
        if maybe_valid {
            IssueKind::PossiblyInvalidArgument
        } else {
            IssueKind::InvalidArgument
        },
        format!(
            "Argument 1 of {}::offsetSet expects {}, {} provided",
            class_name,
            expected_type.get_id(Some(analyzer.interner)),
            key_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}

fn emit_mixed_array_assignment_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
) {
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::MixedArrayAssignment,
        "Cannot assign array offset on mixed type".to_string(),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}

fn emit_mixed_string_offset_assignment_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
) {
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::MixedStringOffsetAssignment,
        "Cannot assign string offset from mixed value".to_string(),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}

fn widen_array_like_type_for_loop(union: &TUnion) -> TUnion {
    let mut widened_key_type: Option<TUnion> = None;
    let mut widened_value_type: Option<TUnion> = None;
    let mut other_types = Vec::new();
    let mut saw_array_like = false;

    for atomic in &union.types {
        match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => {
                saw_array_like = true;
                widened_key_type = Some(match widened_key_type {
                    Some(ref existing) => combine_union_types(existing, key_type, false),
                    None => (**key_type).clone(),
                });
                widened_value_type = Some(match widened_value_type {
                    Some(ref existing) => combine_union_types(existing, value_type, false),
                    None => (**value_type).clone(),
                });
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                saw_array_like = true;
                widened_key_type = Some(match widened_key_type {
                    Some(ref existing) => combine_union_types(existing, &TUnion::int(), false),
                    None => TUnion::int(),
                });
                widened_value_type = Some(match widened_value_type {
                    Some(ref existing) => combine_union_types(existing, value_type, false),
                    None => (**value_type).clone(),
                });
            }
            TAtomic::TKeyedArray {
                properties,
                is_list,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                saw_array_like = true;

                let mut keyed_key_type: Option<TUnion> = None;
                let mut keyed_value_type: Option<TUnion> = None;

                for (key, value_type) in properties {
                    let key_union = match key {
                        ArrayKey::Int(_) => TUnion::int(),
                        ArrayKey::String(_) => TUnion::string(),
                    };
                    keyed_key_type = Some(match keyed_key_type {
                        Some(ref existing) => combine_union_types(existing, &key_union, false),
                        None => key_union,
                    });

                    keyed_value_type = Some(match keyed_value_type {
                        Some(ref existing) => combine_union_types(existing, value_type, false),
                        None => value_type.clone(),
                    });
                }

                if let Some(fallback_key_type) = fallback_key_type {
                    keyed_key_type = Some(match keyed_key_type {
                        Some(ref existing) => {
                            combine_union_types(existing, fallback_key_type, false)
                        }
                        None => (**fallback_key_type).clone(),
                    });
                } else if *is_list {
                    keyed_key_type = Some(match keyed_key_type {
                        Some(ref existing) => combine_union_types(existing, &TUnion::int(), false),
                        None => TUnion::int(),
                    });
                }

                if let Some(fallback_value_type) = fallback_value_type {
                    keyed_value_type = Some(match keyed_value_type {
                        Some(ref existing) => {
                            combine_union_types(existing, fallback_value_type, false)
                        }
                        None => (**fallback_value_type).clone(),
                    });
                }

                let keyed_key_type = keyed_key_type.unwrap_or_else(TUnion::array_key);
                let keyed_value_type = keyed_value_type.unwrap_or_else(TUnion::mixed);

                widened_key_type = Some(match widened_key_type {
                    Some(ref existing) => combine_union_types(existing, &keyed_key_type, false),
                    None => keyed_key_type,
                });
                widened_value_type = Some(match widened_value_type {
                    Some(ref existing) => combine_union_types(existing, &keyed_value_type, false),
                    None => keyed_value_type,
                });
            }
            _ => other_types.push(atomic.clone()),
        }
    }

    if saw_array_like {
        other_types.push(TAtomic::TArray {
            key_type: Box::new(widened_key_type.unwrap_or_else(TUnion::array_key)),
            value_type: Box::new(widened_value_type.unwrap_or_else(TUnion::mixed)),
        });
    }

    if other_types.is_empty() {
        TUnion::mixed()
    } else {
        TUnion::from_types(other_types)
    }
}

fn update_generic_array_atomic(
    existing_key: &TUnion,
    existing_value: &TUnion,
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    let merged_value = combine_union_types(existing_value, assigned_type, false);

    match key_type {
        None => {
            if existing_key.is_nothing() || key_union_is_int_only(existing_key) {
                TAtomic::TNonEmptyList {
                    value_type: Box::new(merged_value),
                }
            } else {
                let merged_key = combine_union_types(existing_key, &TUnion::int(), false);
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(merged_key),
                    value_type: Box::new(merged_value),
                }
            }
        }
        Some(key_type) => {
            let normalized_key = normalize_key_union(key_type);
            let merged_key = if existing_key.is_nothing() {
                // Mirror Psalm: assigning to a generic array variable does not freeze
                // the key-space to a sealed keyed-array shape.
                TUnion::array_key()
            } else {
                combine_union_types(existing_key, &normalized_key, false)
            };

            TAtomic::TNonEmptyArray {
                key_type: Box::new(merged_key),
                value_type: Box::new(merged_value),
            }
        }
    }
}

fn update_list_atomic(
    existing_value: &TUnion,
    key_type: Option<&TUnion>,
    key_repr: Option<&String>,
    assigned_type: &TUnion,
) -> TAtomic {
    let merged_value = combine_union_types(existing_value, assigned_type, false);

    match key_type {
        None => TAtomic::TNonEmptyList {
            value_type: Box::new(merged_value),
        },
        Some(key_type) if key_union_has_only_literal_ints(key_type) => {
            TAtomic::TNonEmptyList {
                value_type: Box::new(merged_value),
            }
        }
        Some(key_type) => TAtomic::TNonEmptyArray {
            key_type: Box::new(combine_union_types(&TUnion::array_key(), key_type, false)),
            value_type: Box::new(merged_value),
        },
    }
}

fn update_keyed_array_atomic(
    properties: &FxHashMap<ArrayKey, TUnion>,
    is_list: bool,
    sealed: bool,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    match key_type {
        None => {
            if is_list {
                let mut new_properties = properties.clone();
                new_properties.insert(
                    ArrayKey::Int(next_list_index(&new_properties)),
                    assigned_type.clone(),
                );

                TAtomic::TKeyedArray {
                    is_list: true,
                    sealed,
                    properties: new_properties,
                    fallback_key_type: fallback_key_type.map(|t| Box::new(t.clone())),
                    fallback_value_type: fallback_value_type.map(|t| Box::new(t.clone())),
                }
            } else {
                keyed_array_to_non_empty_array(
                    properties,
                    fallback_key_type,
                    fallback_value_type,
                    &TUnion::int(),
                    assigned_type,
                )
            }
        }
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut new_properties = properties.clone();
                let multiple_keys = literal_keys.len() > 1;

                for key in literal_keys {
                    if let Some(existing) = new_properties.get(&key).cloned() {
                        let mut next_value = if multiple_keys {
                            combine_union_types(&existing, assigned_type, false)
                        } else {
                            assigned_type.clone()
                        };
                        next_value.possibly_undefined = false;
                        new_properties.insert(key, next_value);
                    } else {
                        let mut inserted_value = assigned_type.clone();
                        inserted_value.possibly_undefined = false;
                        new_properties.insert(key, inserted_value);
                    }
                }

                TAtomic::TKeyedArray {
                    is_list: keyed_array_properties_form_list(&new_properties),
                    sealed,
                    properties: new_properties,
                    fallback_key_type: fallback_key_type.map(|t| Box::new(t.clone())),
                    fallback_value_type: fallback_value_type.map(|t| Box::new(t.clone())),
                }
            } else {
                keyed_array_to_non_empty_array(
                    properties,
                    fallback_key_type,
                    fallback_value_type,
                    key_type,
                    assigned_type,
                )
            }
        }
    }
}

fn keyed_array_to_non_empty_array(
    properties: &FxHashMap<ArrayKey, TUnion>,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,
    assigned_key_type: &TUnion,
    assigned_type: &TUnion,
) -> TAtomic {
    let mut key_union = normalize_key_union(assigned_key_type);
    let mut value_union = assigned_type.clone();

    for (key, value) in properties {
        key_union = combine_union_types(&key_union, &union_for_array_key(key), false);
        value_union = combine_union_types(&value_union, value, false);
    }

    if let Some(fallback_key_type) = fallback_key_type {
        key_union = combine_union_types(&key_union, fallback_key_type, false);
    }

    if let Some(fallback_value_type) = fallback_value_type {
        value_union = combine_union_types(&value_union, fallback_value_type, false);
    }

    TAtomic::TNonEmptyArray {
        key_type: Box::new(key_union),
        value_type: Box::new(value_union),
    }
}

fn create_autovivified_array_atomic(key_type: Option<&TUnion>, assigned_type: &TUnion) -> TAtomic {
    match key_type {
        None => TAtomic::TNonEmptyList {
            value_type: Box::new(assigned_type.clone()),
        },
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut properties = FxHashMap::default();
                for literal_key in literal_keys {
                    properties.insert(literal_key, assigned_type.clone());
                }

                TAtomic::TKeyedArray {
                    is_list: keyed_array_properties_form_list(&properties),
                    sealed: true,
                    properties,
                    fallback_key_type: None,
                    fallback_value_type: None,
                }
            } else {
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(normalize_key_union(key_type)),
                    value_type: Box::new(assigned_type.clone()),
                }
            }
        }
    }
}

fn create_mixed_container_assignment_atomic(
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    match key_type {
        None => TAtomic::TNonEmptyList {
            value_type: Box::new(assigned_type.clone()),
        },
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut properties = FxHashMap::default();
                for literal_key in literal_keys {
                    properties.insert(literal_key, assigned_type.clone());
                }

                TAtomic::TKeyedArray {
                    is_list: keyed_array_properties_form_list(&properties),
                    sealed: false,
                    properties,
                    fallback_key_type: Some(Box::new(TUnion::array_key())),
                    fallback_value_type: Some(Box::new(TUnion::mixed())),
                }
            } else {
                TAtomic::TNonEmptyArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(assigned_type.clone()),
                }
            }
        }
    }
}

fn infer_child_type_for_dim(container_type: &TUnion, key_type: Option<&TUnion>) -> TUnion {
    let literal_keys = key_type.and_then(get_literal_keys_if_all_literals);
    let mut result = Vec::new();

    for atomic in &container_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                append_union_types_unique(&mut result, value_type);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                if let Some(literal_keys) = &literal_keys {
                    let mut found = false;
                    for literal_key in literal_keys {
                        if let Some(property_type) = properties.get(literal_key) {
                            found = true;
                            append_union_types_unique(&mut result, property_type);
                        }
                    }

                    if !found {
                        if let Some(fallback) = fallback_value_type {
                            append_union_types_unique(&mut result, fallback);
                        }
                    }
                } else if let Some(fallback) = fallback_value_type {
                    append_union_types_unique(&mut result, fallback);
                } else {
                    for property_type in properties.values() {
                        append_union_types_unique(&mut result, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return TUnion::mixed(),
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing => {}
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. } => {
                push_atomic_unique(&mut result, TAtomic::TString);
            }
            _ => return TUnion::mixed(),
        }
    }

    if result.is_empty() {
        TUnion::nothing()
    } else {
        TUnion::from_types(type_combiner::combine(result, false))
    }
}

fn append_union_types_unique(target: &mut Vec<TAtomic>, union: &TUnion) {
    for atomic in &union.types {
        push_atomic_unique(target, atomic.clone());
    }
}

fn push_atomic_unique(target: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !target.contains(&atomic) {
        target.push(atomic);
    }
}

fn get_literal_keys_if_all_literals(key_type: &TUnion) -> Option<Vec<ArrayKey>> {
    if key_type.types.is_empty() {
        return None;
    }

    let mut literal_keys = Vec::new();

    for atomic in &key_type.types {
        match atomic {
            TAtomic::TLiteralInt { value } => literal_keys.push(ArrayKey::Int(*value)),
            TAtomic::TLiteralString { value } => {
                if let Ok(int_value) = value.parse::<i64>() {
                    literal_keys.push(ArrayKey::Int(int_value));
                } else {
                    literal_keys.push(ArrayKey::String(value.clone()));
                }
            }
            _ => return None,
        }
    }

    Some(literal_keys)
}

fn key_union_is_int_only(key_type: &TUnion) -> bool {
    if key_type.types.is_empty() {
        return false;
    }

    key_type.types.iter().all(is_int_like_atomic)
}

fn key_union_has_only_literal_ints(key_type: &TUnion) -> bool {
    if key_type.types.is_empty() {
        return false;
    }

    key_type
        .types
        .iter()
        .all(|atomic| matches!(atomic, TAtomic::TLiteralInt { .. }))
}

fn is_int_like_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
    )
}

fn normalize_key_union(key_type: &TUnion) -> TUnion {
    if key_type.is_mixed() {
        TUnion::array_key()
    } else {
        key_type.clone()
    }
}

fn normalize_assignment_key_union(key_type: &TUnion) -> TUnion {
    let mut normalized = Vec::with_capacity(key_type.types.len());

    for atomic in &key_type.types {
        match atomic {
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                push_atomic_unique(&mut normalized, TAtomic::TArrayKey);
            }
            TAtomic::TNull => {
                push_atomic_unique(
                    &mut normalized,
                    TAtomic::TLiteralString {
                        value: String::new(),
                    },
                );
            }
            TAtomic::TLiteralString { value } => {
                if let Ok(int_value) = value.parse::<i64>() {
                    push_atomic_unique(&mut normalized, TAtomic::TLiteralInt { value: int_value });
                } else {
                    push_atomic_unique(&mut normalized, atomic.clone());
                }
            }
            TAtomic::TFalse => {
                push_atomic_unique(&mut normalized, TAtomic::TLiteralInt { value: 0 });
            }
            TAtomic::TTrue => {
                push_atomic_unique(&mut normalized, TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TBool => {
                push_atomic_unique(&mut normalized, TAtomic::TLiteralInt { value: 0 });
                push_atomic_unique(&mut normalized, TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TLiteralFloat { value } => {
                push_atomic_unique(
                    &mut normalized,
                    TAtomic::TLiteralInt {
                        value: *value as i64,
                    },
                );
            }
            TAtomic::TFloat => {
                push_atomic_unique(&mut normalized, TAtomic::TInt);
            }
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TResource
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TVoid => {
                push_atomic_unique(&mut normalized, TAtomic::TArrayKey);
            }
            _ => {
                push_atomic_unique(&mut normalized, atomic.clone());
            }
        }
    }

    if normalized.is_empty() {
        key_type.clone()
    } else {
        TUnion::from_types(type_combiner::combine(normalized, false))
    }
}

fn union_for_array_key(key: &ArrayKey) -> TUnion {
    match key {
        ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
        ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
            value: value.clone(),
        }),
    }
}

fn maybe_emit_invalid_array_offset_for_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    index_expr: &Expression<'_>,
    key_type: &TUnion,
) {
    let mut saw_invalid = false;
    let mut invalid_type = None;

    for atomic in &key_type.types {
        match atomic {
            TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TResource
            | TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TVoid => {
                saw_invalid = true;
                invalid_type = Some(atomic.get_id(Some(analyzer.interner)));
            }
            _ => {}
        }
    }

    if !saw_invalid {
        return;
    }

    let span = index_expr.span();
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidArrayOffset,
        format!(
            "Invalid array offset type: {}",
            invalid_type.unwrap_or_else(|| key_type.get_id(Some(analyzer.interner)))
        ),
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
}

fn keyed_array_properties_form_list(properties: &FxHashMap<ArrayKey, TUnion>) -> bool {
    if properties.is_empty() {
        return true;
    }

    let mut int_keys: Vec<i64> = Vec::with_capacity(properties.len());

    for key in properties.keys() {
        let ArrayKey::Int(value) = key else {
            return false;
        };

        if *value < 0 {
            return false;
        }

        int_keys.push(*value);
    }

    int_keys.sort_unstable();

    for (i, value) in int_keys.iter().enumerate() {
        if *value != i as i64 {
            return false;
        }
    }

    true
}

fn next_list_index(properties: &FxHashMap<ArrayKey, TUnion>) -> i64 {
    let mut max_index = -1_i64;

    for key in properties.keys() {
        if let ArrayKey::Int(value) = key {
            max_index = max_index.max(*value);
        }
    }

    max_index + 1
}

fn emit_invalid_array_assignment_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
    invalid_type_name: String,
) {
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidArrayAssignment,
        format!("Cannot assign array offset on {}", invalid_type_name),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}

fn emit_undefined_offset_set_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
    class_id: pzoom_str::StrId,
) {
    let class_name = analyzer.interner.lookup(class_id);
    let (line, col) = analyzer.get_line_column(issue_pos.0);

    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedMethod,
        format!("Method {}::offsetSet does not exist", class_name),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}
