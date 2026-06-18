//! Array assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::{ArrayAccess, ArrayAppend};
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_str::StrId;

use pzoom_code_info::VarName;
use pzoom_code_info::data_flow::node::DataFlowNodeKind;
use pzoom_code_info::data_flow::path::ArrayDataKind;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{
    DataFlowNode, GraphKind, PathKind, TAtomic, TUnion, VarId, VariableSourceKind,
    combine_union_types,
};
use rustc_hash::FxHashMap;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expr::assignment::{
    instance_property_assignment_analyzer, static_property_assignment_analyzer,
};
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

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
        None,
        pos,
        analysis_data,
        context,
    );
}

/// Array-offset assignment with an already-computed value type and no value
/// expression of its own — destructuring targets like
/// `list($a["foo"]) = $parts;` (Psalm routes these through
/// ArrayAssignmentAnalyzer with the element type).
pub fn analyze_with_known_type(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    value_expr: &Expression<'_>,
    value_type: TUnion,
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
        Some(value_type),
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
        None,
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
    known_value_type: Option<TUnion>,
    assignment_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Psalm: the root of an array-assignment target is being (re)defined, not
    // read — an undeclared `$out` in `$out[] = $x;` creates the array rather
    // than reporting UndefinedVariable. Suppress the undefined check only for
    // genuinely-undeclared direct-variable roots.
    let root_is_direct_variable = matches!(
        root_expr.unparenthesized(),
        Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(_))
    );
    let suppress_undefined_root = match root_expr.unparenthesized() {
        Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) => {
            let root_var_id = VarName::new(direct.name);
            // A superglobal (`$GLOBALS`, `$_GET`, …) is always defined with a
            // known type, even on first use — don't reseed it as a fresh empty
            // array, or `$GLOBALS['foo'][0] = …` would miss the mixed offset
            // assignment (the root is read here at line 180 and supplies its
            // superglobal type below).
            !context.locals.contains_key(&root_var_id)
                && !crate::expr::variable_fetch_analyzer::is_superglobal(
                    direct.name.strip_prefix('$').unwrap_or(direct.name),
                )
        }
        _ => false,
    };
    let was_inside_assignment_root = context.inside_assignment_root;
    if root_is_direct_variable {
        context.inside_assignment_root = true;
    }
    // An append (`$a->foo[] = …`) doesn't read the root property; an offset
    // write does (Psalm's find_unused_code reference semantics).
    let was_inside_array_append_root = context.inside_array_append_root;
    context.inside_array_append_root = dims
        .first()
        .is_some_and(|dim| matches!(dim.kind, AssignmentDimKind::Append));
    let root_pos = expression_analyzer::analyze(analyzer, root_expr, analysis_data, context);
    context.inside_array_append_root = was_inside_array_append_root;
    context.inside_assignment_root = was_inside_assignment_root;
    let root_type = if suppress_undefined_root {
        // Psalm seeds an undeclared assignment root as a fresh empty array.
        TUnion::new(TAtomic::empty_array())
    } else {
        analysis_data
            .expr_types
            .get(&root_pos)
            .cloned()
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed)
    };

    if matches!(
        root_expr.unparenthesized(),
        Expression::Call(
            Call::Function(_) | Call::Method(_) | Call::NullSafeMethod(_) | Call::StaticMethod(_)
        )
    ) && root_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TArray { .. }))
    {
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

    // Single-dim offset expression, kept for Psalm's dependent-list-key check
    // below (it only ever applies when the parent var id has no `[`, i.e. the
    // assignment has exactly one dim rooted at a simple variable).
    let single_dim_key_expr: Option<&'a Expression<'a>> = if dims.len() == 1 {
        match dims[0].kind {
            AssignmentDimKind::Key(index_expr) => Some(index_expr),
            AssignmentDimKind::Append => None,
        }
    } else {
        None
    };

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
                    .expr_types
                    .get(&key_pos)
                    .cloned()
                    .map(|t| (*t).clone())
                    .unwrap_or_else(TUnion::array_key);
                // A null (or possibly-null) key coerces to "" by PHP, but Psalm
                // flags it: NullArrayOffset for a definite null,
                // PossiblyNullArrayOffset when only part of the key type is null.
                maybe_emit_null_array_offset_for_assignment(
                    analyzer,
                    analysis_data,
                    index_expr,
                    &raw_key_type,
                );
                if !root_supports_offset_set {
                    maybe_emit_invalid_array_offset_for_assignment(
                        analyzer,
                        analysis_data,
                        index_expr,
                        &raw_key_type,
                    );
                }
                // ArrayAccess containers receive the raw key in offsetSet —
                // PHP's array-key coercions don't apply to object offsets.
                let key_type = if root_supports_offset_set {
                    raw_key_type
                } else {
                    normalize_assignment_key_union(&raw_key_type)
                };
                (Some(key_type), get_assignment_index_key(index_expr))
            }
            AssignmentDimKind::Append => (None, None),
        };

        resolved_dims.push(ResolvedAssignmentDim {
            key_type,
            key_repr,
            result_pos: dim.result_pos,
        });
    }

    // The assigned value is used by the assignment (Psalm sets
    // inside_assignment while analyzing it) — without this, a mutation-free
    // call in `$x->prop[$k] = $y->call()` reports UnusedMethodCall.
    // Psalm additionally flips inside_general_use when the assignment root is
    // not a plain variable ("if we don't know where this data is going, treat
    // as a dead-end usage"): `$obj->prop[$k] = $v` is a use of `$v`.
    let was_inside_assignment = context.inside_assignment;
    let was_inside_general_use = context.inside_general_use;
    context.inside_assignment = true;
    let root_is_plain_var = match root_expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            !crate::expr::variable_fetch_analyzer::is_superglobal(direct.name)
        }
        _ => false,
    };
    if !root_is_plain_var {
        context.inside_general_use = true;
    }
    // A destructuring caller already analyzed the source and computed the
    // element type; re-analyzing `value_expr` (the whole source array) would
    // both use the wrong type and double-count the read.
    let value_type = if let Some(known_value_type) = known_value_type {
        context.inside_assignment = was_inside_assignment;
        context.inside_general_use = was_inside_general_use;
        known_value_type
    } else {
        let value_pos = expression_analyzer::analyze(analyzer, value_expr, analysis_data, context);
        context.inside_assignment = was_inside_assignment;
        context.inside_general_use = was_inside_general_use;
        analysis_data
            .expr_types
            .get(&value_pos)
            .cloned()
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed)
    };

    if resolved_dims.is_empty() {
        analysis_data
            .expr_types
            .insert(assignment_pos, Rc::new(value_type));
        return;
    }

    // Forward pass: infer container/value types for each nested dim.
    let mut container_types = Vec::with_capacity(resolved_dims.len());
    let mut running_container = root_type.clone();

    for dim in &resolved_dims {
        container_types.push(running_container.clone());

        let child_type =
            infer_child_type_for_dim(analyzer, &running_container, dim.key_type.as_ref());
        analysis_data
            .expr_types
            .insert(dim.result_pos, Rc::new(child_type.clone()));
        running_container = child_type;
    }

    // Container expr identifiers per nesting level, used for dataflow node labels
    // (Hakana derives these via `expression_identifier::get_var_id` per level).
    let root_var_key = expression_identifier::get_expression_var_key(root_expr);
    let container_var_ids: Vec<Option<String>> = {
        let mut ids = Vec::with_capacity(resolved_dims.len());
        let mut current = root_var_key.clone().map(|key| key.to_string());
        ids.push(current.clone());
        for dim in resolved_dims.iter().take(resolved_dims.len() - 1) {
            current = match (current, dim.key_repr.as_ref()) {
                (Some(base), Some(repr)) => Some(format!("{}[{}]", base, repr)),
                _ => None,
            };
            ids.push(current.clone());
        }
        ids
    };

    // Psalm's `$offset_already_existed` (ArrayAssignmentAnalyzer): the offset
    // is known to exist when the full fetch path is already a defined in-scope
    // variable (e.g. narrowed by `isset($this->list[$offset])`), or when the
    // offset is the foreach key over this very list (Psalm carries that as
    // `TIntRange::$dependent_list_key` on the key type; pzoom tracks it in
    // `context.list_key_dependencies`). When the offset already existed and
    // the parent is a list-typed simple variable, the assignment keeps the
    // list a list: `combine($root_type, non-empty-list<$value_type>)`. Psalm
    // gates this on `!str_contains($parent_var_id, '[')`, so it only ever
    // applies to single-dim assignments rooted at a bracket-free var id.
    let preserve_root_list = resolved_dims.len() == 1
        && root_var_key
            .as_ref()
            .is_some_and(|key| !key.as_str().contains('['))
        && resolved_dims[0]
            .key_type
            .as_ref()
            .is_some_and(|key_type| get_literal_keys_if_all_literals(key_type).is_none())
        && root_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TArray { is_list: true, .. }))
        && {
            let offset_in_scope = root_var_key
                .as_ref()
                .zip(resolved_dims[0].key_repr.as_ref())
                .and_then(|(root_key, key_repr)| {
                    context
                        .locals
                        .get(&VarName::new(format!("{}[{}]", root_key, key_repr)))
                })
                .is_some_and(|existing| !existing.possibly_undefined_from_try);

            let key_is_dependent_list_key = single_dim_key_expr.is_some_and(|key_expr| {
                matches!(
                    key_expr.unparenthesized(),
                    Expression::Variable(Variable::Direct(key_direct))
                        if context
                            .list_key_dependencies
                            .get(&VarName::new(key_direct.name))
                            == root_var_key.as_ref()
                )
            });

            offset_in_scope || key_is_dependent_list_key
        };

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
            analysis_data
                .expr_types
                .insert(dim.result_pos, Rc::new(value_type.clone()));
        } else {
            analysis_data
                .expr_types
                .insert(dim.result_pos, Rc::new(updated_child_type.clone()));
        }

        let child_type_for_dataflow = updated_child_type.clone();

        updated_child_type = if is_root_assignment && preserve_root_list {
            // Psalm: `$array_atomic_type_list = $value_type` then
            // `combineUnionTypes($root_type, non-empty-list<$value_type>)` —
            // the write lands on a known-existing list offset, so the list
            // stays a list instead of degrading to `array<array-key, _>`.
            combine_union_types(
                container_type,
                &TUnion::new(TAtomic::non_empty_list(updated_child_type)),
                true,
            )
        } else {
            apply_assignment_to_container(
                analyzer,
                analysis_data,
                container_type,
                dim.key_type.as_ref(),
                &updated_child_type,
                // Psalm passes `$replacement_type` only on the last (leaf)
                // dimension; intermediate dimensions get null and so report a
                // PossiblyNullArrayAssignment on a null container.
                is_leaf,
                dim.result_pos,
                is_root_assignment,
                emit_mixed_issues,
                context.inside_loop,
            )
        };

        // Hakana attaches array-assignment dataflow per level: old container parents
        // and the assigned child's parents flow into a per-level assignment node.
        let mut new_container_type = updated_child_type;
        new_container_type.parent_nodes = container_type.parent_nodes.clone();

        let container_expr_pos = if i == 0 {
            root_pos
        } else {
            resolved_dims[i - 1].result_pos
        };

        let key_values = dim
            .key_type
            .as_ref()
            .map(|key_type| get_array_assignment_offset_types(key_type))
            .unwrap_or_default();

        let inside_general_use = context.inside_general_use
            || (is_leaf
                && root_var_key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("$_")));

        updated_child_type = add_array_assignment_dataflow(
            analyzer,
            analysis_data,
            container_expr_pos,
            new_container_type,
            &child_type_for_dataflow,
            container_var_ids[i].clone(),
            &key_values,
            inside_general_use,
        );
    }

    analysis_data
        .expr_types
        .insert(root_pos, Rc::new(updated_child_type.clone()));

    if let Expression::Variable(Variable::Direct(direct)) = root_expr.unparenthesized() {
        let var_id = VarName::new(direct.name);

        // Hakana marks the root variable as a variable-use source after a nested
        // array assignment (function-body graphs only). The updated container's
        // dataflow parents feed the new source, which becomes the stored type's
        // parent — the array write is an assignment like any other.
        if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
            let root_span = root_expr.span();
            let node_pos = make_data_flow_node_position(
                analyzer,
                (root_span.start.offset, root_span.end.offset),
            );
            let is_external_ref = context.references_to_external_scope.contains(&var_id)
                || context.static_var_ids.contains(&var_id);
            // An array write to an UNDECLARED root autovivifies the variable —
            // that IS its original assignment and is reportable
            // (noUseOfInstantArrayAssignment). A write to an existing variable
            // is a plain vertex: Psalm never reports `$arr[k] = …` lines as
            // unused variables, but the node still chains the container's
            // dataflow forward.
            let source_node = if suppress_undefined_root {
                DataFlowNode::get_for_variable_source(
                    if is_external_ref {
                        VariableSourceKind::InoutArg
                    } else {
                        VariableSourceKind::Default
                    },
                    VarId(analyzer.interner.intern(&var_id)),
                    node_pos,
                    false,
                    !updated_child_type.parent_nodes.is_empty(),
                    false,
                    false,
                    false,
                )
            } else {
                DataFlowNode::get_for_lvar(VarId(analyzer.interner.intern(&var_id)), node_pos)
            };
            for parent_node in &updated_child_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &source_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            analysis_data.data_flow_graph.add_node(source_node.clone());

            // Values written into a by-ref-captured container escape the
            // current scope: consume them with a use sink.
            if is_external_ref {
                let escape_sink = DataFlowNode::get_for_unlabelled_sink(node_pos);
                analysis_data.data_flow_graph.add_path(
                    &source_node.id,
                    &escape_sink.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
                analysis_data.data_flow_graph.add_node(escape_sink);
            }

            updated_child_type.parent_nodes = vec![source_node];
        }

        // Loop convergence widening only applies to appends/dynamic keys: a
        // literal-key write (`$entry['variadic'] = true` in a do-while) keeps
        // the keyed shape — Psalm preserves array{...} through such loops.
        let all_dims_literal = resolved_dims.iter().all(|dim| {
            dim.key_repr
                .as_ref()
                .is_some_and(|repr| assignment_key_repr_is_literal(repr))
        });
        let stored_type = if context.inside_loop && !context.inside_foreach && !all_dims_literal {
            let mut widened = widen_array_like_type_for_loop(&updated_child_type);
            widened.parent_nodes = updated_child_type.parent_nodes.clone();
            widened
        } else {
            updated_child_type.clone()
        };

        clear_dependent_property_types(context, direct.name);
        clear_array_path_types_for_base_var(context, direct.name);
        clear_dependent_array_access_types(context, direct.name);
        context.invalidate_dependent_types(&var_id);
        remove_var_clauses_from_context(context, direct.name);
        context.set_var_type(var_id.clone(), stored_type);
        // Assigning to an offset modifies the base variable, so mark it possibly
        // assigned (mirroring Psalm marking the root var in
        // `possibly_assigned_var_ids`). Without this, a base that was also narrowed
        // by the enclosing condition (e.g. `if (isset($p[$i])) { $p[$i] = ...; }`)
        // is treated as merely narrowed and dropped from the branch-merge's
        // possibly-redefined set, losing the assignment's widening.
        context.possibly_assigned_var_ids.insert(var_id.clone());

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

                if let Some(dim_type) = analysis_data.expr_types.get(&dim.result_pos).cloned() {
                    let dim_key_id = VarName::new(&running_key);
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
    } else if let Expression::Access(Access::StaticProperty(static_prop)) =
        root_expr.unparenthesized()
    {
        // Psalm re-checks the updated root type against the declared property
        // type when the assignment root is a static property
        // (`self::$map[$k] = ...` runs StaticPropertyAssignmentAnalyzer).
        static_property_assignment_analyzer::analyze_with_known_type(
            analyzer,
            static_prop,
            &updated_child_type,
            assignment_pos,
            analysis_data,
            context,
        );

        // The write invalidates previously narrowed offset types on the same
        // property path (mirrors the variable-root invalidation above), then
        // stores the assigned dim types.
        if let Some(base_key) = root_var_key.as_ref() {
            clear_array_path_types_for_base_var(context, base_key);

            let mut running_key = base_key.to_string();
            for dim in &resolved_dims {
                let Some(index_repr) = dim.key_repr.as_ref() else {
                    break;
                };

                running_key.push('[');
                running_key.push_str(index_repr);
                running_key.push(']');

                if let Some(dim_type) = analysis_data.expr_types.get(&dim.result_pos).cloned() {
                    let dim_key_id = VarName::new(&running_key);
                    context.locals.insert(dim_key_id, (*dim_type).clone());
                }
            }
        }
    }

    // Assignment expression evaluates to the assigned RHS value.
    analysis_data
        .expr_types
        .insert(assignment_pos, Rc::new(value_type));
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
            TAtomic::TArray { .. } | TAtomic::TClassStringMap { .. }
        ) || matches!(atomic, TAtomic::TObjectIntersection { types } if types.iter().any(|nested| matches!(
            nested,
            TAtomic::TArray { .. }
        )))
    })
}

fn assignment_key_repr_is_literal(key_repr: &str) -> bool {
    key_repr.starts_with('\'') || key_repr.parse::<i64>().is_ok()
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

fn clear_array_path_types_for_base_var(context: &mut BlockContext, var_name: &str) {
    let prefix = format!("{var_name}[");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| var_id.starts_with(&prefix))
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

fn apply_assignment_to_container(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    container_type: &TUnion,
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
    // Psalm's `$replacement_type` to `getArrayAccessTypeGivenOffset` is present
    // only for the leaf dimension of an assignment (`!$is_last ? null :
    // $assignment_type`). When it is set, a null container autovivifies
    // silently; when it is absent (an intermediate dimension), Psalm reports a
    // PossiblyNullArrayAssignment instead. We always carry `assigned_type` for
    // the container build, so this flag separately tracks that presence.
    has_replacement_type: bool,
    issue_pos: Pos,
    is_root_assignment: bool,
    emit_mixed_issues: bool,
    inside_loop: bool,
) -> TUnion {
    let mut updated = Vec::new();
    let mut has_writable = false;
    let mut invalid_atomic_name: Option<String> = None;
    let mut undefined_offset_set_class: Option<pzoom_str::StrId> = None;
    let mut saw_mixed_assignment = false;
    let mut saw_null = false;

    let offset_set_name = StrId::OFFSET_SET;

    for atomic in &container_type.types {
        match atomic {
            // Hakana's array-assignment analyzer: writing into a type variable
            // constrains it from above to a keyed container of the written
            // key/value (PHP's `array<K, V>`); the variable itself flows on.
            TAtomic::TTypeVariable { name } => {
                if let Some(pzoom_code_info::TypeVariableBounds { upper_bounds, .. }) =
                    analysis_data.type_variable_bounds.get_mut(name)
                {
                    let mut bound = pzoom_code_info::TemplateBound::new(
                        TUnion::new(TAtomic::array(
                            key_type.cloned().unwrap_or_else(TUnion::array_key),
                            assigned_type.clone(),
                        )),
                        0,
                        None,
                        None,
                    );
                    bound.pos = Some(crate::template::bound_location(analyzer, issue_pos));
                    upper_bounds.push(bound);
                }

                has_writable = true;
                updated.push(atomic.clone());
            }
            // A shape (known entries) — keyed-array update.
            TAtomic::TArray {
                known_values,
                params,
                is_list,
                is_sealed,
                ..
            } if !known_values.is_empty() => {
                has_writable = true;
                updated.push(update_keyed_array_atomic(
                    known_values,
                    *is_list,
                    *is_sealed,
                    params.as_deref().map(|(k, _)| k),
                    params.as_deref().map(|(_, v)| v),
                    key_type,
                    assigned_type,
                    inside_loop,
                ));
            }
            // A generic list (`list<V>`).
            TAtomic::TArray {
                params: Some(params),
                is_list: true,
                ..
            } => {
                has_writable = true;
                updated.push(update_list_atomic(&params.1, key_type, assigned_type));
            }
            // A generic array (`array<K,V>`).
            TAtomic::TArray {
                params: Some(params),
                ..
            } => {
                has_writable = true;
                updated.push(update_generic_array_atomic(
                    &params.0,
                    &params.1,
                    key_type,
                    assigned_type,
                ));
            }
            // A fallback-less empty array (`[]`/`empty_array()`): the old empty
            // generic `array<nothing, nothing>` autovivification path.
            TAtomic::TArray { .. } => {
                has_writable = true;
                updated.push(update_generic_array_atomic(
                    &TUnion::nothing(),
                    &TUnion::nothing(),
                    key_type,
                    assigned_type,
                ));
            }
            TAtomic::TClassStringMap {
                param_name,
                as_type,
                value_param,
            } => {
                has_writable = true;
                updated.push(update_class_string_map_atomic(
                    *param_name,
                    as_type.as_deref(),
                    value_param,
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
            TAtomic::TNull => {
                // PHP autovivifies null into an array on offset write. Psalm only
                // flags this when there is no replacement value at this level:
                // in getArrayAccessTypeGivenOffset the TNull/in-assignment branch
                // combines `$replacement_type` in silently when it is set (the
                // leaf write, `$a[k] = v`) and reports PossiblyNullArrayAssignment
                // only when it is null (an intermediate dimension, e.g. the `$a`
                // step of `$a[0][] = 1`). Mirror that: autovivify always, but
                // defer the issue (after the loop) to the no-replacement case.
                has_writable = true;
                if !has_replacement_type {
                    saw_null = true;
                }
                updated.push(create_autovivified_array_atomic(key_type, assigned_type));
            }
            TAtomic::TFalse => {
                has_writable = true;
                updated.push(create_autovivified_array_atomic(key_type, assigned_type));
            }
            TAtomic::TNever => {
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
                            atomic,
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
                            | TAtomic::TNever
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
            // A template parameter is offset-assignable when its `as` bound is
            // (e.g. `@template T as array` permits `$t[$k] = ...`). Defer to the
            // bound rather than rejecting the abstract template outright.
            TAtomic::TTemplateParam { as_type, .. } => {
                let bound_writable = as_type.types.iter().any(|bound| {
                    matches!(
                        bound,
                        TAtomic::TArray { .. }
                            | TAtomic::TString
                            | TAtomic::TNonEmptyString
                            | TAtomic::TLiteralString { .. }
                            | TAtomic::TNumericString
                            | TAtomic::TNonEmptyNumericString
                            | TAtomic::TLowercaseString
                            | TAtomic::TNonEmptyLowercaseString
                            | TAtomic::TTruthyString
                    )
                }) || union_supports_offset_set(analyzer, as_type);

                if bound_writable {
                    has_writable = true;
                    // Psalm: writing through a template-typed ARRAY degrades it
                    // to the concrete written shape — `$s["a"] = 123` on
                    // `T as array{a: int}` yields `array{a: 123}`, which no
                    // longer satisfies `T` (modifyTemplatedShape).
                    // Only SHAPE bounds degrade: a generic-array bound
                    // (`TData as array`) written with key-of/indexed-access
                    // values stays within the template (Psalm's
                    // keyOfClassTemplate tests). A shape is an array with known
                    // entries (old `TKeyedArray`); a generic `array<…>`/`list<…>`
                    // has none.
                    let bound_is_shape = as_type.types.iter().all(|bound| {
                        matches!(
                            bound,
                            TAtomic::TArray { known_values, .. } if !known_values.is_empty()
                        )
                    });
                    if bound_is_shape {
                        let updated_bound = apply_assignment_to_container(
                            analyzer,
                            analysis_data,
                            as_type,
                            key_type,
                            assigned_type,
                            // the written value is the replacement itself
                            true,
                            issue_pos,
                            false,
                            false,
                            inside_loop,
                        );
                        updated.extend(updated_bound.types);
                        continue;
                    }
                } else if invalid_atomic_name.is_none() {
                    invalid_atomic_name = Some(atomic.get_id(Some(analyzer.interner)));
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
    } else if emit_mixed_issues {
        // Some union members accept the offset write while others do not —
        // Psalm's PossiblyNull/PossiblyInvalidArrayAssignment (the array part
        // keeps the write valid, the null/non-array part makes it possibly
        // wrong). A null member is reported separately from a non-array one,
        // matching Psalm's per-atomic loop in getArrayAccessTypeGivenOffset.
        if saw_null {
            emit_possibly_null_array_assignment_issue(analyzer, analysis_data, issue_pos);
        }
        if let Some(invalid_name) = invalid_atomic_name {
            emit_possibly_invalid_array_assignment_issue(
                analyzer,
                analysis_data,
                issue_pos,
                invalid_name,
            );
        }
    }

    if updated.is_empty() {
        return TUnion::new(create_autovivified_array_atomic(key_type, assigned_type));
    }

    TUnion::from_types(type_combiner::combine(updated, false))
}

fn union_supports_offset_set(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let offset_set_name = StrId::OFFSET_SET;

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
    receiver_atomic: &TAtomic,
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

    // The declared param may name the class's templates
    // (`ArrayAccess<TKey, …>::offsetSet(TKey $offset …)`). Psalm replaces them
    // with the receiver's type params (or the templates' defaults) before
    // comparing — the ClassTemplateParamCollector + standin pass every method
    // call gets.
    let declaring_class_id = offset_set_info.declaring_class.unwrap_or(class_id);
    let expected_type = match (
        analyzer.codebase.get_class(declaring_class_id),
        analyzer.codebase.get_class(class_id),
    ) {
        (Some(declaring_class_info), Some(receiver_class_info))
            if !declaring_class_info.template_types.is_empty() =>
        {
            let mut template_result =
                crate::expr::call::function_call_analyzer::get_class_template_defaults(
                    declaring_class_info,
                );
            if let Some(collected) = crate::expr::call::class_template_param_collector::collect(
                analyzer.codebase,
                declaring_class_info,
                receiver_class_info,
                Some(receiver_atomic),
                false,
            ) {
                template_result.lower_bounds = collected;
            }
            std::borrow::Cow::Owned(
                crate::expr::call::function_call_analyzer::replace_templates_in_union(
                    expected_type,
                    &template_result,
                ),
            )
        }
        _ => std::borrow::Cow::Borrowed(expected_type),
    };
    let expected_type = expected_type.as_ref();

    // NB: a `never` param (Psalm's bare `new SplObjectStorage()` quirk) must
    // reject every argument, so `is_nothing` does not bail out here.
    if expected_type.is_mixed() {
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

fn emit_possibly_null_array_assignment_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
) {
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::PossiblyNullArrayAssignment,
        "Cannot access array value on possibly null variable".to_string(),
        analyzer.file_path,
        issue_pos.0,
        issue_pos.1,
        line,
        col,
    ));
}

fn emit_possibly_invalid_array_assignment_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    issue_pos: Pos,
    non_array_type: String,
) {
    let (line, col) = analyzer.get_line_column(issue_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::PossiblyInvalidArrayAssignment,
        format!("Cannot access array value on non-array variable of type {non_array_type}"),
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
            // A list shape (known entries, `is_list`): collapse to a list,
            // keeping it non-empty when some entry is always-defined. Psalm keeps
            // lists lists across loop iterations rather than flattening to
            // `array<int, _>`.
            TAtomic::TArray {
                known_values,
                params,
                is_list: true,
                ..
            } if !known_values.is_empty() => {
                let fallback_value_type = params.as_deref().map(|(_, v)| v);
                let mut value_union: Option<TUnion> = None;
                let mut has_defined_property = false;
                for (possibly_undefined, value_type) in known_values.values() {
                    if !*possibly_undefined {
                        has_defined_property = true;
                    }
                    value_union = Some(match value_union {
                        Some(ref existing) => combine_union_types(existing, value_type, false),
                        None => value_type.clone(),
                    });
                }
                if let Some(fallback_value_type) = fallback_value_type {
                    value_union = Some(match value_union {
                        Some(ref existing) => {
                            combine_union_types(existing, fallback_value_type, false)
                        }
                        None => fallback_value_type.clone(),
                    });
                }
                let value_union = value_union.unwrap_or_else(TUnion::mixed);
                other_types.push(if has_defined_property {
                    TAtomic::non_empty_list(value_union)
                } else {
                    TAtomic::list(value_union)
                });
            }
            // A non-list shape (known entries): widen its keys/values into the
            // generic array accumulator.
            TAtomic::TArray {
                known_values,
                params,
                is_list,
                ..
            } if !known_values.is_empty() => {
                saw_array_like = true;

                let fallback_key_type = params.as_deref().map(|(k, _)| k);
                let fallback_value_type = params.as_deref().map(|(_, v)| v);

                let mut keyed_key_type: Option<TUnion> = None;
                let mut keyed_value_type: Option<TUnion> = None;

                for (key, (_possibly_undefined, value_type)) in known_values.iter() {
                    let key_union = match key {
                        ArrayKey::Int(_) => TUnion::int(),
                        ArrayKey::String(_) | ArrayKey::ClassString(_) => TUnion::string(),
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
                        None => fallback_key_type.clone(),
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
                        None => fallback_value_type.clone(),
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
            // A generic list (`list<V>`): keep it as-is — Psalm keeps lists lists
            // across loop iterations.
            TAtomic::TArray {
                is_list: true,
                params: Some(_),
                ..
            } => {
                other_types.push(atomic.clone());
            }
            // A generic array (`array<K,V>`): widen its key/value.
            TAtomic::TArray {
                params: Some(params),
                ..
            } => {
                saw_array_like = true;
                let key_type = &params.0;
                let value_type = &params.1;
                widened_key_type = Some(match widened_key_type {
                    Some(ref existing) => combine_union_types(existing, key_type, false),
                    None => key_type.clone(),
                });
                widened_value_type = Some(match widened_value_type {
                    Some(ref existing) => combine_union_types(existing, value_type, false),
                    None => value_type.clone(),
                });
            }
            // A fallback-less empty array (`[]`): the old empty generic
            // `array<nothing, nothing>` widen path (key/value = nothing).
            TAtomic::TArray { .. } => {
                saw_array_like = true;
                widened_key_type = Some(match widened_key_type {
                    Some(ref existing) => combine_union_types(existing, &TUnion::nothing(), false),
                    None => TUnion::nothing(),
                });
                widened_value_type = Some(match widened_value_type {
                    Some(ref existing) => combine_union_types(existing, &TUnion::nothing(), false),
                    None => TUnion::nothing(),
                });
            }
            _ => other_types.push(atomic.clone()),
        }
    }

    if saw_array_like {
        // The widening runs on the type *after* an array assignment, which
        // always leaves at least one element — Psalm keeps the post-write
        // type non-empty across loop iterations.
        other_types.push(TAtomic::non_empty_array(
            widened_key_type.unwrap_or_else(TUnion::array_key),
            widened_value_type.unwrap_or_else(TUnion::mixed),
        ));
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
                TAtomic::non_empty_list(merged_value)
            } else {
                let merged_key = combine_union_types(existing_key, &TUnion::int(), false);
                TAtomic::non_empty_array(merged_key, merged_value)
            }
        }
        Some(key_type) => {
            // Psalm's updateTypeWithKeyValues: a single-literal-key write
            // produces a shape — sealed over an empty array (`[] + ['x'=>1]`
            // is array{x: 1}), carrying the generic as fallback otherwise
            // (array{b: 5, ...<array-key, mixed>}).
            if let Some(literal_key) = single_literal_array_key(key_type) {
                let is_first_list_entry = matches!(literal_key, ArrayKey::Int(0));
                let mut known_values: FxHashMap<ArrayKey, (bool, TUnion)> = FxHashMap::default();
                known_values.insert(
                    literal_key,
                    (false, assigned_type.clone()),
                );

                if existing_key.is_nothing() {
                    return TAtomic::keyed_array(
                        known_values,
                        is_first_list_entry,
                        true,
                        None,
                        None,
                    );
                }

                return TAtomic::keyed_array(
                    known_values,
                    false,
                    false,
                    Some(existing_key.clone()),
                    Some(existing_value.clone()),
                );
            }

            let normalized_key = normalize_key_union(key_type);
            let merged_key = if existing_key.is_nothing() {
                // First write into an empty array: the key space is exactly the
                // assigned key's type (Psalm tracks the literal; it does not
                // widen to array-key).
                normalized_key
            } else {
                combine_union_types(existing_key, &normalized_key, false)
            };

            TAtomic::non_empty_array(merged_key, merged_value)
        }
    }
}

/// The single literal array key of an offset type, if any.
fn single_literal_array_key(key_type: &TUnion) -> Option<ArrayKey> {
    match key_type.get_single()? {
        TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => Some(ArrayKey::String(value.clone())),
        _ => None,
    }
}

fn update_list_atomic(
    existing_value: &TUnion,
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    let merged_value = combine_union_types(existing_value, assigned_type, false);

    match key_type {
        None => TAtomic::non_empty_list(merged_value),
        Some(key_type) if key_union_has_only_literal_ints(key_type) => {
            TAtomic::non_empty_list(merged_value)
        }
        Some(key_type) => TAtomic::non_empty_array(
            // The list's own keys are int<0, max>; the union with the written
            // key keeps Psalm's precision (array<int<0, max>, T> for an
            // int-typed offset) instead of degrading to array-key.
            combine_union_types(
                &TUnion::new(TAtomic::TIntRange {
                    min: Some(0),
                    max: None,
                }),
                key_type,
                false,
            ),
            merged_value,
        ),
    }
}

fn update_keyed_array_atomic(
    known_values: &FxHashMap<ArrayKey, (bool, TUnion)>,
    is_list: bool,
    sealed: bool,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,

    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
    inside_loop: bool,
) -> TAtomic {
    match key_type {
        None => {
            if is_list {
                if inside_loop {
                    // Psalm gates shape extension on `!$context->inside_loop`
                    // (updateArrayAssignmentChildType): inside a loop the
                    // append instead combines the shape with
                    // `non-empty-list<value>`, so the type converges across
                    // iterations while staying a list.
                    let shape = TAtomic::keyed_array(
                        known_values.clone(),
                        is_list,
                        sealed,
                        fallback_key_type.cloned(),
                        fallback_value_type.cloned(),
                    );
                    let combined = combine_union_types(
                        &TUnion::new(shape),
                        &TUnion::new(TAtomic::non_empty_list(assigned_type.clone())),
                        true,
                    );
                    if let Some(single) = combined.get_single() {
                        return single.clone();
                    }
                }

                let mut new_known_values = known_values.clone();
                new_known_values.insert(
                    ArrayKey::Int(next_list_index(&new_known_values)),
                    (false, assigned_type.clone()),
                );

                TAtomic::keyed_array(
                    new_known_values,
                    true,
                    sealed,
                    fallback_key_type.cloned(),
                    fallback_value_type.cloned(),
                )
            } else {
                keyed_array_to_non_empty_array(
                    known_values,
                    fallback_key_type,
                    fallback_value_type,
                    &TUnion::int(),
                    assigned_type,
                )
            }
        }
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut new_known_values = known_values.clone();
                let multiple_keys = literal_keys.len() > 1;

                for key in literal_keys {
                    let next_value = if let Some((_, existing)) = new_known_values.get(&key) {
                        if multiple_keys {
                            combine_union_types(existing, assigned_type, false)
                        } else {
                            assigned_type.clone()
                        }
                    } else {
                        assigned_type.clone()
                    };
                    // A definite write makes the entry always-defined.
                    new_known_values.insert(key, (false, next_value));
                }

                // `keyed_array` re-derives `is_list` (and `is_nonempty`) from the
                // entries — equivalent to the old `keyed_array_properties_form_list`.
                TAtomic::keyed_array(
                    new_known_values,
                    true,
                    sealed,
                    fallback_key_type.cloned(),
                    fallback_value_type.cloned(),
                )
            } else if is_list && key_union_is_int_only(key_type) {
                // Non-literal int offset into a list: broaden every element with the
                // new value, keeping the list (Psalm widens the element type rather
                // than collapsing a list to a generic `array<int, …>`).
                let mut value_union = assigned_type.clone();
                for (_, value) in known_values.values() {
                    value_union = combine_union_types(&value_union, value, false);
                }
                if let Some(fallback_value_type) = fallback_value_type {
                    value_union = combine_union_types(&value_union, fallback_value_type, false);
                }
                TAtomic::non_empty_list(value_union)
            } else {
                keyed_array_to_non_empty_array(
                    known_values,
                    fallback_key_type,
                    fallback_value_type,
                    key_type,
                    assigned_type,
                )
            }
        }
    }
}

/// Port of the `class-string-map` branch of Psalm's
/// `ArrayAssignmentAnalyzer::updateArrayAssignmentChildType`: assigning at a
/// templated class-string offset (`$map[$class] = $obj` with
/// `$class: class-string<T2>`) keeps the map type — the offset's template `T2`
/// is substituted with the map's placeholder in the assigned type, which is
/// then combined into the map's value param (Psalm combines the new
/// `TClassStringMap` with the root type). A non-templated offset degrades to
/// the array equivalent, as Psalm's generic-array branch does.
fn update_class_string_map_atomic(
    param_name: pzoom_str::StrId,
    as_type: Option<&TAtomic>,
    value_param: &TUnion,
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    // Psalm's `$key_type->isTemplatedClassString()`: a single templated
    // class-string offset, in either pzoom representation.
    let key_template = key_type.and_then(|key_type| {
        let single = key_type.get_single()?;
        match single {
            TAtomic::TTemplateParamClass {
                name,
                defining_entity,
                as_type,
            } => Some((*name, *defining_entity, (**as_type).clone())),
            TAtomic::TClassString {
                as_type: Some(class_string_target),
            } => match class_string_target.as_ref() {
                TAtomic::TTemplateParam {
                    name,
                    defining_entity,
                    as_type,
                } => Some((
                    *name,
                    *defining_entity,
                    as_type.get_single().cloned().unwrap_or(TAtomic::TObject),
                )),
                _ => None,
            },
            _ => None,
        }
    });

    if let Some((key_name, key_entity, key_bound)) = key_template {
        let mut template_result = pzoom_code_info::TemplateResult::default();
        crate::template::lower_bounds_insert(
            &mut template_result,
            key_name,
            key_entity,
            TUnion::new(TAtomic::TTemplateParam {
                name: param_name,
                defining_entity: pzoom_code_info::GenericParent::TypeDefinition(
                    pzoom_str::StrId::CLASS_STRING_MAP,
                ),
                as_type: Box::new(TUnion::new(key_bound)),
            }),
        );
        let replaced_value =
            crate::template::inferred_type_replacer::replace(assigned_type, &template_result);

        return TAtomic::TClassStringMap {
            param_name,
            as_type: as_type.cloned().map(Box::new),
            value_param: Box::new(combine_union_types(value_param, &replaced_value, false)),
        };
    }

    // Non-templated offset: fall back to the map's array equivalent merged
    // with the new key/value.
    let standin_key = TUnion::new(TAtomic::TTemplateParamClass {
        name: param_name,
        defining_entity: pzoom_code_info::GenericParent::TypeDefinition(
            pzoom_str::StrId::CLASS_STRING_MAP,
        ),
        as_type: Box::new(as_type.cloned().unwrap_or(TAtomic::TObject)),
    });
    update_generic_array_atomic(&standin_key, value_param, key_type, assigned_type)
}

fn keyed_array_to_non_empty_array(
    known_values: &FxHashMap<ArrayKey, (bool, TUnion)>,
    fallback_key_type: Option<&TUnion>,
    fallback_value_type: Option<&TUnion>,

    assigned_key_type: &TUnion,
    assigned_type: &TUnion,
) -> TAtomic {
    let mut key_union = normalize_key_union(assigned_key_type);
    let mut value_union = assigned_type.clone();

    for (key, (_possibly_undefined, value)) in known_values.iter() {
        key_union = combine_union_types(&key_union, &union_for_array_key(key), false);
        value_union = combine_union_types(&value_union, value, false);
    }

    if let Some(fallback_key_type) = fallback_key_type {
        key_union = combine_union_types(&key_union, fallback_key_type, false);
    }

    if let Some(fallback_value_type) = fallback_value_type {
        value_union = combine_union_types(&value_union, fallback_value_type, false);
    }

    TAtomic::non_empty_array(key_union, value_union)
}

fn create_autovivified_array_atomic(key_type: Option<&TUnion>, assigned_type: &TUnion) -> TAtomic {
    match key_type {
        None => TAtomic::non_empty_list(assigned_type.clone()),
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut known_values: FxHashMap<ArrayKey, (bool, TUnion)> = FxHashMap::default();
                for literal_key in literal_keys {
                    known_values.insert(
                        literal_key,
                        (false, assigned_type.clone()),
                    );
                }

                // `keyed_array` derives `is_list` from the entries (old
                // `keyed_array_properties_form_list`).
                TAtomic::keyed_array(known_values, true, true, None, None)
            } else {
                TAtomic::non_empty_array(normalize_key_union(key_type), assigned_type.clone())
            }
        }
    }
}

fn create_mixed_container_assignment_atomic(
    key_type: Option<&TUnion>,
    assigned_type: &TUnion,
) -> TAtomic {
    match key_type {
        None => TAtomic::non_empty_list(assigned_type.clone()),
        Some(key_type) => {
            if let Some(literal_keys) = get_literal_keys_if_all_literals(key_type) {
                let mut known_values: FxHashMap<ArrayKey, (bool, TUnion)> = FxHashMap::default();
                for literal_key in literal_keys {
                    known_values.insert(
                        literal_key,
                        (false, assigned_type.clone()),
                    );
                }

                // `keyed_array` derives `is_list` from the entries (old
                // `keyed_array_properties_form_list`); the `...<array-key, mixed>`
                // fallback keeps it unsealed.
                TAtomic::keyed_array(
                    known_values,
                    true,
                    false,
                    Some(TUnion::array_key()),
                    Some(TUnion::mixed()),
                )
            } else {
                TAtomic::non_empty_array(TUnion::array_key(), assigned_type.clone())
            }
        }
    }
}

fn infer_child_type_for_dim(
    analyzer: &StatementsAnalyzer<'_>,
    container_type: &TUnion,
    key_type: Option<&TUnion>,
) -> TUnion {
    let literal_keys = key_type.and_then(get_literal_keys_if_all_literals);
    let mut result = Vec::new();

    for atomic in &container_type.types {
        match atomic {
            // Generic array/list (no known entries): the element type is the
            // fallback `params` value (a fallback-less empty array has none).
            TAtomic::TArray {
                known_values,
                params,
                ..
            } if known_values.is_empty() => {
                if let Some((_, value_type)) = params.as_deref() {
                    append_union_types_unique(&mut result, value_type);
                }
            }
            // Shape (known entries): read the named entry, falling back to the
            // typed fallback `params` value, then to every known value.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                let fallback_value_type = params.as_deref().map(|(_, v)| v);
                if let Some(literal_keys) = &literal_keys {
                    let mut found = false;
                    for literal_key in literal_keys {
                        if let Some((_, property_type)) = known_values.get(literal_key) {
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
                    for (_, property_type) in known_values.values() {
                        append_union_types_unique(&mut result, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return TUnion::mixed(),
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNever => {}
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
            // Writing through an ArrayAccess object reads offsetGet's value
            // type for the next dim (Psalm's UpdateAnalyzer), not mixed.
            TAtomic::TNamedObject { name, .. } => {
                if let Some((_, value_type)) =
                    crate::expr::fetch::array_fetch_analyzer::resolve_array_access_method_types(
                        analyzer, atomic, *name,
                    )
                {
                    append_union_types_unique(&mut result, &value_type);
                } else {
                    return TUnion::mixed();
                }
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
        TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. }
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
        ArrayKey::String(value) | ArrayKey::ClassString(value) => {
            TUnion::new(TAtomic::TLiteralString {
                value: value.clone(),
            })
        }
    }
}

/// A `null` array key on a plain array coerces to `""` at runtime; Psalm still
/// reports it — `NullArrayOffset` when the key is definitely null,
/// `PossiblyNullArrayOffset` when null is only one member of the key type.
/// Mirrors the read-side check in `array_fetch_analyzer`.
fn maybe_emit_null_array_offset_for_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    index_expr: &Expression<'_>,
    key_type: &TUnion,
) {
    let (kind, message) = if key_type.is_null() {
        (
            IssueKind::NullArrayOffset,
            "Cannot access value using null offset".to_string(),
        )
    } else if key_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNull))
    {
        (
            IssueKind::PossiblyNullArrayOffset,
            format!(
                "Cannot access value using possibly null offset {}",
                key_type.get_id(Some(analyzer.interner))
            ),
        )
    } else {
        return;
    };

    let span = index_expr.span();
    let (line, col) = analyzer.get_line_column(span.start.offset);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        line,
        col,
    ));
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

/// The next sequential integer key for a list append (max int key + 1), over a
/// unified array's `known_values`. The entries' possibly-undefined `bool` is
/// irrelevant — only the keys matter.
fn next_list_index(known_values: &FxHashMap<ArrayKey, (bool, TUnion)>) -> i64 {
    let mut max_index = -1_i64;

    for key in known_values.keys() {
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

/// Hakana `array_assignment_analyzer::add_array_assignment_dataflow`: rewires the
/// container's old parents and the assigned child's parents into a per-assignment
/// node, returning the container type with that node as its sole parent.
#[allow(clippy::too_many_arguments)]
fn add_array_assignment_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    expr_var_pos: Pos,
    mut parent_expr_type: TUnion,
    child_expr_type: &TUnion,
    var_var_id: Option<String>,
    key_values: &[TAtomic],
    inside_general_use: bool,
) -> TUnion {
    // Hakana also skips this work in whole-program taint mode when the child type is
    // not taintable; pzoom does not track `has_taintable_value` yet.

    let parent_node = if let Some(var_var_id) = &var_var_id {
        DataFlowNode::get_for_lvar(
            VarId(analyzer.interner.intern(var_var_id)),
            make_data_flow_node_position(analyzer, expr_var_pos),
        )
    } else {
        DataFlowNode::get_for_array_assignment(make_data_flow_node_position(analyzer, expr_var_pos))
    };

    if inside_general_use && analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        let sink_pos = make_data_flow_node_position(analyzer, expr_var_pos);

        let assignment_node = DataFlowNode {
            id: parent_node.id.clone(),
            kind: DataFlowNodeKind::VariableUseSink { pos: sink_pos },
        };

        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &assignment_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );

        analysis_data.data_flow_graph.add_node(assignment_node);
    }

    analysis_data.data_flow_graph.add_node(parent_node.clone());

    let old_parent_nodes = parent_expr_type.parent_nodes.clone();

    parent_expr_type.parent_nodes = vec![parent_node.clone()];

    for old_parent_node in old_parent_nodes {
        analysis_data.data_flow_graph.add_path(
            &old_parent_node.id,
            &parent_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    for child_parent_node in &child_expr_type.parent_nodes {
        if !key_values.is_empty() {
            for key_value in key_values {
                let key_value = match key_value {
                    TAtomic::TLiteralString { value } => value.clone(),
                    TAtomic::TLiteralInt { value } => value.to_string(),
                    _ => continue,
                };

                analysis_data.data_flow_graph.add_path(
                    &child_parent_node.id,
                    &parent_node.id,
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value),
                    vec![],
                    vec![],
                );
            }
        } else {
            analysis_data.data_flow_graph.add_path(
                &child_parent_node.id,
                &parent_node.id,
                PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                vec![],
                vec![],
            );
        }
    }

    parent_expr_type
}

/// Hakana `get_array_assignment_offset_types`: literal key atomics from a dim type.
fn get_array_assignment_offset_types(child_stmt_dim_type: &TUnion) -> Vec<TAtomic> {
    let mut valid_offset_types = vec![];
    for single_atomic in &child_stmt_dim_type.types {
        if matches!(
            single_atomic,
            TAtomic::TLiteralString { .. } | TAtomic::TLiteralInt { .. }
        ) {
            valid_offset_types.push(single_atomic.clone());
        }
    }

    valid_offset_types
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
