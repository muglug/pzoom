//! Foreach statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::foreach::{Foreach, ForeachTarget};
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{CodebaseInfo, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::loop_analyzer;
use crate::stmt::scope_analyzer::BreakContext;

/// Psalm's `ForeachAnalyzer::$always_non_empty_array`: true only when every
/// atomic of the iterated type is an array that is provably non-empty (a
/// template parameter is judged by its bound). Null/false, possibly-empty
/// arrays, objects, and mixed all clear it.
fn iterable_always_non_empty(iterable_type: &TUnion) -> bool {
    !iterable_type.types.is_empty()
        && iterable_type.types.iter().all(|atomic| {
            let atomic = if let TAtomic::TTemplateParam { as_type, .. } = atomic {
                match as_type.get_single() {
                    Some(single) => single,
                    None => return false,
                }
            } else {
                atomic
            };

            // TODO(unify-array): the old keyed-array check was
            // `properties.any(|p| !p.possibly_undefined)`; `is_nonempty` also
            // requires the entry not be `never` (`array_known_values_nonempty`),
            // so a required `never` entry no longer counts as non-empty.
            atomic.array_is_nonempty()
        })
}

/// Whether every member of the iterated union is an empty ARRAY (Psalm's
/// `TArray::isEmptyArray` check in ForeachAnalyzer): `[]` literals and
/// `array<never, never>` qualify; `list<never>` does not (Psalm keeps its
/// `never` values flowing into the body).
fn iterable_is_all_empty_arrays(iterable_type: &TUnion) -> bool {
    !iterable_type.types.is_empty()
        && iterable_type.types.iter().all(|atomic| match atomic {
            // `array<never, never>` (old `TArray{never,never}`): empty, with a
            // never-typed fallback. `list<never>` keeps its `never` values
            // flowing, so a list (`params.0` is `int`, not `never`) is excluded.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                ..
            } => known_values.is_empty() && params.0.is_nothing() && params.1.is_nothing(),
            // The empty sealed shape (`array{}` / `[]` / old empty sealed
            // `TKeyedArray`): no known entries and no fallback params.
            TAtomic::TArray {
                known_values,
                params: None,
                is_sealed,
                ..
            } => known_values.is_empty() && *is_sealed,
            _ => false,
        })
}

/// Analyze a foreach statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    foreach: &Foreach<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Psalm's ForeachAnalyzer assigns named statement-level `@var` comments
    // into the context before analyzing — except those naming the key/value
    // targets ($safe_var_ids), which the loop-value typing below consumes.
    if let Some(stmt_start) = analysis_data.current_stmt_start
        && let Some(annotations) = analyzer.get_inline_var_annotations(stmt_start)
    {
        let mut safe_var_ids: Vec<pzoom_str::StrId> = Vec::new();
        let (key_expr, value_expr) = match &foreach.target {
            ForeachTarget::Value(value_target) => (None, value_target.value),
            ForeachTarget::KeyValue(kv_target) => (Some(kv_target.key), kv_target.value),
        };
        for target_expr in key_expr.into_iter().chain(std::iter::once(value_expr)) {
            match unwrap_reference_target(target_expr).unparenthesized() {
                Expression::Variable(Variable::Direct(direct)) => {
                    safe_var_ids.push(analyzer.interner.intern(direct.name));
                }
                // Psalm's $safe_var_ids also covers list-destructuring
                // targets — each item's value variable and key variable.
                Expression::List(list) => collect_destructuring_safe_var_ids(
                    analyzer,
                    list.elements.as_slice(),
                    &mut safe_var_ids,
                ),
                Expression::Array(array) => collect_destructuring_safe_var_ids(
                    analyzer,
                    array.elements.as_slice(),
                    &mut safe_var_ids,
                ),
                _ => {}
            }
        }

        let annotations = annotations.clone();
        for annotation in &annotations {
            let Some(var_name) = annotation.var_name else {
                continue;
            };
            if safe_var_ids.contains(&var_name) {
                continue;
            }
            let var_id = VarName::new(analyzer.interner.lookup(var_name));
            let mut annotation_type = annotation.var_type.clone();
            if let Some(existing) = context.get_var_type(&var_id) {
                annotation_type.parent_nodes = existing.parent_nodes.clone();
            }
            context.set_var_type(var_id, annotation_type);
        }
    }

    // Analyze the iterable expression
    // Hakana's foreach_analyzer marks the iterated expression as general use.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let iterable_pos =
        expression_analyzer::analyze(analyzer, foreach.expression, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    // `get_expr_type` hands back an owned `Rc<TUnion>`, so this doesn't borrow
    // `analysis_data` — we can still emit issues against it below.
    let iterable_type = analysis_data.expr_types.get(&iterable_pos).cloned();

    // Create loop context
    let mut foreach_context = context.clone();
    foreach_context.inside_loop = true;
    foreach_context.inside_foreach = true;
    foreach_context.break_types.push(BreakContext::Loop);

    // Psalm's second var-comments pass: every named annotation also lands in
    // the loop context (unfiltered). A destructuring key variable that the
    // loop never assigns keeps the annotated type inside the body, while the
    // after-loop merge restores the zero-iteration alternative.
    if let Some(stmt_start) = analysis_data.current_stmt_start
        && let Some(annotations) = analyzer.get_inline_var_annotations(stmt_start)
    {
        let annotations = annotations.clone();
        for annotation in &annotations {
            let Some(var_name) = annotation.var_name else {
                continue;
            };
            let var_id = VarName::new(analyzer.interner.lookup(var_name));
            let mut annotation_type = annotation.var_type.clone();
            if let Some(existing) = foreach_context.get_var_type(&var_id) {
                annotation_type.parent_nodes = existing.parent_nodes.clone();
            }
            foreach_context.set_var_type(var_id, annotation_type);
        }
    }

    // Psalm's ForeachAnalyzer: entering the body proves the iterated
    // variable non-empty, so `reset($x)` etc. inside the loop don't see the
    // possibly-empty form.
    if let Some(iterable_var_key) =
        expression_identifier::get_expression_var_key(foreach.expression)
        && foreach_context
            .locals
            .contains_key(iterable_var_key.as_str())
    {
        let narrowed = crate::reconciler::assertion_reconciler::reconcile(
            &pzoom_code_info::Assertion::NonEmpty,
            foreach_context.locals.get(iterable_var_key.as_str()),
            false,
            None,
            analyzer,
            analysis_data,
            true,
            false,
        );
        foreach_context.locals.insert(iterable_var_key, narrowed);
    }

    // Determine the value type from the iterable
    let mut value_type = if let Some(ref iter_type) = iterable_type {
        // The element type inherits the iterable's docblock provenance
        // (Psalm keeps from_docblock through foreach value extraction).
        let mut extracted = extract_iterable_value_type(iter_type, analyzer);
        extracted.from_docblock = extracted.from_docblock || iter_type.from_docblock;
        extracted
    } else {
        TUnion::mixed()
    };

    // Determine the key type from the iterable
    let mut key_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_key_type(iter_type, analyzer)
    } else {
        TUnion::array_key()
    };
    // Psalm's foreach key loses docblock provenance (its combineUnionTypes
    // pass rebuilds the union): an impossible comparison on the key var
    // reports TypeDoesNotContainType, not DocblockTypeContradiction, even
    // when the iterable's type came from a docblock. The value type keeps it.
    key_type.from_docblock = false;
    key_type.sync_docblock_bits_from_union_flag();

    // An empty-array iterable contributes no key/value types: Psalm's
    // ForeachAnalyzer skips the empty TArray atomic and falls back to mixed
    // for both, still analyzing the body (`foreach ([] as $k => $v)` reports
    // MixedAssignment for $k and $v, not an unreachable body). A `list<never>`
    // (e.g. `array_keys([])`) is NOT an empty array to Psalm: its values stay
    // `never` through the body.
    if value_type.is_nothing()
        && iterable_type
            .as_ref()
            .is_some_and(|iter_type| iterable_is_all_empty_arrays(iter_type))
    {
        value_type = TUnion::mixed();
        key_type = TUnion::mixed();
    }

    // Validate that the expression is actually iterable, mirroring Psalm's
    // `ForeachAnalyzer::checkIteratorType` (InvalidIterator, PossiblyNullIterator,
    // RawObjectIteration, ...).
    if let Some(ref iter_type) = iterable_type {
        check_iterator_type(analyzer, analysis_data, iter_type, iterable_pos);
    }

    // foreach over an external iterator calls its Iterator methods (Psalm
    // records current/key/next/rewind/valid + getIterator as references).
    if analyzer.config.find_unused_code
        && let Some(ref iter_type) = iterable_type
    {
        for atomic in &iter_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };
            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                continue;
            };
            for iterator_method in ["current", "key", "next", "rewind", "valid", "getiterator"] {
                let method_lc = analyzer.interner.intern(iterator_method);
                let method_info = class_info.methods.get(&method_lc).or_else(|| {
                    class_info
                        .method_lc_names
                        .get(&method_lc)
                        .and_then(|cased| class_info.methods.get(cased))
                });
                if let Some(method_info) = method_info {
                    analysis_data
                        .referenced_class_members
                        .insert((*name, method_lc));
                    analysis_data.add_class_member_reference(
                        &context.function_context,
                        (*name, method_lc),
                        false,
                    );
                    if let Some(declaring) = method_info.declaring_class {
                        analysis_data
                            .referenced_class_members
                            .insert((declaring, method_lc));
                        analysis_data.add_class_member_reference(
                            &context.function_context,
                            (declaring, method_lc),
                            false,
                        );
                    }
                    // The loop consumes the produced values.
                    analysis_data.method_returns_used.insert((*name, method_lc));
                    if let Some(declaring) = method_info.declaring_class {
                        analysis_data
                            .method_returns_used
                            .insert((declaring, method_lc));
                    }
                }
            }
        }
    }

    // Hakana `foreach_analyzer::get_individual_iterator_types`: the iterated
    // value's dataflow continues into the key/value bindings through
    // array-fetch paths…
    crate::expr::fetch::array_fetch_analyzer::add_array_fetch_dataflow(
        analyzer,
        iterable_pos,
        analysis_data,
        None,
        &mut value_type,
        &mut key_type,
    );

    // …and (function-body graphs) the iterable itself is consumed by the
    // foreach statement via an unlabelled variable-use sink.
    if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
        && let Some(ref iter_type) = iterable_type
    {
        let foreach_span = foreach.span();
        let foreach_node = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
            crate::data_flow::make_data_flow_node_position(
                analyzer,
                (foreach_span.start.offset, foreach_span.end.offset),
            ),
        );

        for parent_node in &iter_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &foreach_node.id,
                pzoom_code_info::PathKind::Default,
                vec![],
                vec![],
            );
        }
        analysis_data.data_flow_graph.add_node(foreach_node);
    }

    // A foreach target reassigning an enclosing for/foreach counter is a
    // LoopInvalidation (Psalm's protected_var_ids).
    {
        let mut target_exprs: Vec<&Expression<'_>> = Vec::new();
        match &foreach.target {
            ForeachTarget::Value(value_target) => target_exprs.push(value_target.value),
            ForeachTarget::KeyValue(kv_target) => {
                target_exprs.push(kv_target.key);
                target_exprs.push(kv_target.value);
            }
        }
        for target_expr in target_exprs {
            let Expression::Variable(Variable::Direct(direct)) = target_expr.unparenthesized()
            else {
                continue;
            };
            let var_name = VarName::new(direct.name);
            if analysis_data
                .loop_scopes
                .iter()
                .any(|scope| scope.protected_var_ids.contains(&var_name))
            {
                let span = target_expr.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::LoopInvalidation,
                    format!(
                        "Variable {} has already been assigned in a for/foreach loop",
                        direct.name
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
        }
    }

    // Psalm applies inline @var annotations to foreach targets: the comment
    // overrides the inferred value type (refineForeachVarType). An explicit
    // annotation (even `@var mixed`) is intentional and silences
    // MixedAssignment for the target.
    let mut value_target_has_annotation = false;
    let mut value_type_before_annotation: Option<String> = None;
    {
        let value_target_expr = match &foreach.target {
            ForeachTarget::Value(value_target) => value_target.value,
            ForeachTarget::KeyValue(kv_target) => kv_target.value,
        };
        if let Expression::Variable(Variable::Direct(direct)) =
            unwrap_reference_target(value_target_expr).unparenthesized()
        {
            let var_id = VarName::new(direct.name);
            let annotation = analysis_data.current_stmt_start.and_then(|stmt_start| {
                crate::expr::variable_fetch_analyzer::get_inline_var_annotation_type(
                    analyzer, stmt_start, &var_id,
                )
            });
            if let Some(annotation_type) = annotation {
                value_type_before_annotation = Some(value_type.get_id(Some(analyzer.interner)));
                if annotation_type.get_id(Some(analyzer.interner))
                    != value_type.get_id(Some(analyzer.interner))
                {
                    let parent_nodes = std::mem::take(&mut value_type.parent_nodes);
                    value_type = annotation_type;
                    value_type.parent_nodes = parent_nodes;
                }
                value_target_has_annotation = true;
            }
        }

        // A statement-level `@var` can also annotate the foreach KEY var
        // (`/** @var string \$arg_name */ foreach (... as \$arg_name => ...)`)
        // — Psalm applies it through the same comment machinery.
        if let ForeachTarget::KeyValue(kv_target) = &foreach.target
            && let Expression::Variable(Variable::Direct(key_direct)) =
                kv_target.key.unparenthesized()
        {
            let key_var_id = VarName::new(key_direct.name);
            let key_annotation = analysis_data.current_stmt_start.and_then(|stmt_start| {
                crate::expr::variable_fetch_analyzer::get_inline_var_annotation_type(
                    analyzer,
                    stmt_start,
                    &key_var_id,
                )
            });
            if let Some(annotation_type) = key_annotation {
                let parent_nodes = std::mem::take(&mut key_type.parent_nodes);
                key_type = annotation_type;
                key_type.parent_nodes = parent_nodes;
            }
        }
    }

    // Psalm's ForeachAnalyzer assigns targets through AssignmentAnalyzer,
    // which reports MixedAssignment when the bound value is mixed.
    {
        let value_target_expr = match &foreach.target {
            ForeachTarget::Value(value_target) => value_target.value,
            ForeachTarget::KeyValue(kv_target) => kv_target.value,
        };
        if let ForeachTarget::KeyValue(kv_target) = &foreach.target
            && key_type.is_mixed()
            && let Expression::Variable(Variable::Direct(key_direct)) =
                kv_target.key.unparenthesized()
            && !key_direct.name.starts_with("$_")
        {
            let span = kv_target.key.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            let origin_secondary = crate::data_flow::mixed_origin_secondary(
                analyzer,
                analysis_data,
                &key_type,
                span.start.offset,
            );
            analysis_data.add_issue(
                Issue::new(
                    IssueKind::MixedAssignment,
                    format!(
                        "Unable to determine the type that {} is being assigned to",
                        key_direct.name
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                )
                .with_secondary_opt(origin_secondary),
            );
        }
        if value_type.is_mixed()
            && !value_target_has_annotation
            && let Expression::Variable(Variable::Direct(direct)) =
                value_target_expr.unparenthesized()
            && !direct.name.starts_with("$_")
        {
            let span = value_target_expr.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            let origin_secondary = crate::data_flow::mixed_origin_secondary(
                analyzer,
                analysis_data,
                &value_type,
                span.start.offset,
            );
            analysis_data.add_issue(
                Issue::new(
                    IssueKind::MixedAssignment,
                    format!(
                        "Unable to determine the type that {} is being assigned to",
                        direct.name
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                )
                .with_secondary_opt(origin_secondary),
            );
        }
    }

    // Psalm (find_unused_variables): a foreach-target @var annotation whose
    // type matches the inferred value type exactly is unnecessary.
    if analyzer.config.report_unused {
        let value_target_expr = match &foreach.target {
            ForeachTarget::Value(value_target) => value_target.value,
            ForeachTarget::KeyValue(kv_target) => kv_target.value,
        };
        if let Expression::Variable(Variable::Direct(direct)) = value_target_expr.unparenthesized()
        {
            let var_id = VarName::new(direct.name);
            let annotation = analysis_data.current_stmt_start.and_then(|stmt_start| {
                crate::expr::variable_fetch_analyzer::get_inline_var_annotation_type(
                    analyzer, stmt_start, &var_id,
                )
            });
            if let Some(annotation_type) = annotation
                && !annotation_type.is_mixed()
                && value_type_before_annotation.as_deref()
                    == Some(annotation_type.get_id(Some(analyzer.interner)).as_str())
            {
                let span = value_target_expr.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UnnecessaryVarAnnotation,
                    format!(
                        "The @var {} annotation for {} is unnecessary",
                        annotation_type.get_id(Some(analyzer.interner)),
                        direct.name
                    ),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
        }
    }

    // Foreach key/value targets are variable assignments for the data flow
    // graph: each direct-variable target gets a `VariableUseSource` node
    // feeding unused-variable analysis (Hakana assigns through the regular
    // assignment path; Psalm registers them via `registerVariable`). The
    // value target's span is also recorded so an unused one reports
    // UnusedForeachValue (Psalm's `$foreach_var_locations`).
    let add_foreach_target_source = |target: &Expression<'_>,
                                     target_type: &mut TUnion,
                                     analysis_data: &mut FunctionAnalysisData,
                                     is_value_target: bool| {
        if analysis_data.data_flow_graph.kind != pzoom_code_info::GraphKind::FunctionBody {
            return;
        }
        let Expression::Variable(Variable::Direct(direct)) =
            unwrap_reference_target(target).unparenthesized()
        else {
            return;
        };
        let span = target.span();
        if is_value_target {
            analysis_data
                .foreach_var_positions
                .push((span.start.offset, span.end.offset));
        }
        let source_node = pzoom_code_info::DataFlowNode::get_for_variable_source(
            pzoom_code_info::VariableSourceKind::Default,
            pzoom_code_info::VarId(analyzer.interner.intern(direct.name)),
            crate::data_flow::make_data_flow_node_position(
                analyzer,
                (span.start.offset, span.end.offset),
            ),
            false,
            !target_type.parent_nodes.is_empty(),
            false,
            false,
            false,
        );
        for parent_node in &target_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &source_node.id,
                pzoom_code_info::PathKind::Default,
                vec![],
                vec![],
            );
        }
        analysis_data.data_flow_graph.add_node(source_node.clone());
        target_type.parent_nodes = vec![source_node];
    };

    // Psalm routes the foreach binding through AssignmentAnalyzer, which
    // registers the target's first appearance; an always-exiting guard in the
    // body can later retract a MixedAssignment reported at that binding
    // (IfElseAnalyzer's `IssueBuffer::remove`).
    let register_target_appearance =
        |target: &Expression<'_>, analysis_data: &mut FunctionAnalysisData| {
            if let Expression::Variable(Variable::Direct(direct)) = target.unparenthesized() {
                analysis_data
                    .first_var_appearances
                    .entry(VarName::new(direct.name))
                    .or_insert(target.span().start.offset);
            }
        };

    // Set the iterator variable types in loop context
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            register_target_appearance(value_target.value, analysis_data);
            mark_foreach_reference_target(value_target.value, &mut foreach_context);
            add_foreach_target_source(value_target.value, &mut value_type, analysis_data, true);
            set_expression_var_type(
                value_target.value,
                &value_type,
                analyzer,
                &mut foreach_context,
            );
        }
        ForeachTarget::KeyValue(kv_target) => {
            register_target_appearance(kv_target.key, analysis_data);
            register_target_appearance(kv_target.value, analysis_data);
            add_foreach_target_source(kv_target.key, &mut key_type, analysis_data, false);
            set_expression_var_type(kv_target.key, &key_type, analyzer, &mut foreach_context);
            mark_foreach_reference_target(kv_target.value, &mut foreach_context);
            add_foreach_target_source(kv_target.value, &mut value_type, analysis_data, true);
            set_expression_var_type(kv_target.value, &value_type, analyzer, &mut foreach_context);

            // Psalm: iterating a list bakes `dependent_list_key = $list_var_id`
            // into the key's `TIntRange` (TKeyedArray::getGenericArrayType),
            // which later lets `$list[$key] = ...` keep the list a list.
            // pzoom's foreach key types don't carry int ranges, so the
            // dependency is tracked on the block context instead.
            if let Expression::Variable(Variable::Direct(key_direct)) =
                kv_target.key.unparenthesized()
            {
                let iterable_is_list = iterable_type.as_ref().is_some_and(|iter_type| {
                    iter_type
                        .types
                        .iter()
                        .any(|atomic| matches!(atomic, TAtomic::TArray { is_list: true, .. }))
                });
                if iterable_is_list {
                    if let Some(iterable_var_key) =
                        expression_identifier::get_expression_var_key(foreach.expression)
                    {
                        foreach_context
                            .list_key_dependencies
                            .insert(VarName::new(key_direct.name), iterable_var_key);
                    }
                }
            }
        }
    }

    // Psalm's `$always_non_empty_array`: iterating a provably non-empty
    // array guarantees the body runs, so its final variable state holds
    // after the loop (LoopAnalyzer::setLoopVars).
    let always_non_empty_array = iterable_type
        .as_ref()
        .is_some_and(|iter_type| iterable_always_non_empty(iter_type));

    let loop_scope = LoopScope::new(context.locals.clone());
    let body_stmts = foreach.body.statements();
    let (_loop_scope, _inner) = loop_analyzer::analyze(
        analyzer,
        body_stmts,
        vec![],
        vec![],
        loop_scope,
        &mut foreach_context,
        context,
        analysis_data,
        false,
        always_non_empty_array,
        false,
    )?;

    // Iterator variables are now visible in the parent scope (PHP quirk).
    // A guaranteed-non-empty iterable leaves the final element state
    // (Psalm's setLoopVars); otherwise a pre-existing variable keeps the
    // loop merge's combination with its pre-loop value (the loop may run
    // zero times), and only fresh variables get the element type.
    let always_non_empty_array = iterable_type
        .as_ref()
        .is_some_and(|iter_type| iterable_always_non_empty(iter_type));
    let set_target_after_loop =
        |target: &Expression<'_>, target_type: &TUnion, context: &mut BlockContext| {
            if !always_non_empty_array
                && let Expression::Variable(Variable::Direct(direct)) = target.unparenthesized()
                && context.locals.contains_key(&VarName::new(direct.name))
            {
                return;
            }
            set_expression_var_type(target, target_type, analyzer, context);
        };
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            set_target_after_loop(value_target.value, &value_type, context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_target_after_loop(kv_target.key, &key_type, context);
            set_target_after_loop(kv_target.value, &value_type, context);
        }
    }

    Ok(())
}

/// Validate that `iter_type` can be iterated over, emitting the same family of
/// issues Psalm's `ForeachAnalyzer::checkIteratorType` does.
///
/// Each atomic member is classified as a valid iterable (array/iterable/object
/// implementing `Traversable`), `null`, a non-Traversable "raw" object (PHP
/// iterates its public properties), or an outright invalid value (a scalar). The
/// emitted issue then depends on whether the offending members are the whole
/// type (`InvalidIterator`/`NullIterator`/`RawObjectIteration`) or only part of
/// it (`PossiblyInvalidIterator`/`PossiblyNullIterator`).
fn check_iterator_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    iter_type: &TUnion,
    pos: Pos,
) {
    // `mixed` carries no information to check against.
    if iter_type.is_mixed() {
        return;
    }

    // Psalm: iterating an `iterable` (or any possibly-Traversable value) from
    // a pure context may invoke impure iterator methods — ImpureMethodCall.
    if analyzer.function_info.is_some_and(|info| info.is_pure) {
        let may_call_iterator_methods = iter_type.types.iter().any(|atomic| match atomic {
            TAtomic::TIterable { .. } => true,
            TAtomic::TNamedObject { name, .. } => analyzer
                .codebase
                .get_class(*name)
                .is_none_or(|class_info| !class_info.is_immutable),
            _ => false,
        });
        if may_call_iterator_methods {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ImpureMethodCall,
                "Cannot call a possibly-mutating iterator from a pure context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    let interner = Some(analyzer.interner);

    let (start_offset, end_offset) = pos;
    let (start_line, start_column) = analyzer.get_line_column(start_offset);
    let emit = |analysis_data: &mut FunctionAnalysisData, kind: IssueKind, message: String| {
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            start_offset,
            end_offset,
            start_line,
            start_column,
        ));
    };

    // Psalm's early whole-union checks: wholly null, then possibly-null /
    // possibly-false (each silenced by the union's ignore flag, and each
    // short-circuiting the member checks).
    if iter_type.is_null() {
        emit(
            analysis_data,
            IssueKind::NullIterator,
            "Cannot iterate over null".to_string(),
        );
        return;
    }

    if iter_type.is_nullable() && !iter_type.ignore_nullable_issues {
        emit(
            analysis_data,
            IssueKind::PossiblyNullIterator,
            format!(
                "Cannot iterate over nullable var {}",
                iter_type.get_id(interner)
            ),
        );
        return;
    }

    if iter_type.is_falsable() && !iter_type.ignore_falsable_issues {
        emit(
            analysis_data,
            IssueKind::PossiblyFalseIterator,
            format!(
                "Cannot iterate over falsable var {}",
                iter_type.get_id(interner)
            ),
        );
        return;
    }

    let mut has_valid_iterator = false;
    let mut invalid_types: Vec<String> = Vec::new();
    let mut raw_object_types: Vec<String> = Vec::new();
    let mut invalid_generator_types: Vec<String> = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            // Reachable only with the ignore flags set (otherwise the early
            // checks above returned); Psalm skips these members silently.
            TAtomic::TNull | TAtomic::TFalse => {}

            // Arrays, `iterable`, and anything whose runtime value could be a
            // Traversable (`object`, a template parameter, `mixed`) are accepted.
            TAtomic::TArray { .. }
            | TAtomic::TIterable { .. }
            | TAtomic::TObject
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TMixed
            | TAtomic::TNonEmptyMixed => {
                has_valid_iterator = true;
            }

            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                // Psalm: foreach sends null into a generator, so iterating a
                // Generator whose TSend is non-nullable (and not void/mixed)
                // is invalid.
                if analyzer
                    .interner
                    .lookup(*name)
                    .eq_ignore_ascii_case("Generator")
                    && let Some(send_type) = type_params.as_ref().and_then(|params| params.get(2))
                    && !send_type.is_nullable()
                    && !send_type.is_void()
                    && !send_type.is_mixed()
                {
                    invalid_generator_types.push(TUnion::new(atomic.clone()).get_id(interner));
                    continue;
                }
                if *name == StrId::STDCLASS
                    || !analyzer.codebase.class_exists(*name)
                    || class_is_traversable(analyzer.codebase, *name)
                {
                    // Implements Traversable, or an unknown class we can't
                    // disprove — assume it is iterable.
                    has_valid_iterator = true;
                } else {
                    // A concrete object that does not implement Traversable: PHP
                    // iterates its public properties (Psalm: RawObjectIteration).
                    raw_object_types.push(TUnion::new(atomic.clone()).get_id(interner));
                }
            }

            // Scalars and other non-iterable values cannot be iterated at all.
            TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TIntRange { .. }
            | TAtomic::TArrayKey
            | TAtomic::TScalar
            | TAtomic::TNumeric
            | TAtomic::TVoid
            | TAtomic::TResource
            | TAtomic::TClosedResource
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. } => {
                invalid_types.push(TUnion::new(atomic.clone()).get_id(interner));
            }

            // Anything else (enums, intersections, conditionals, …): be
            // conservative and don't flag it, to avoid false positives.
            _ => {}
        }
    }

    if !invalid_types.is_empty() {
        // If only *some* of the union can't be iterated, it's a possible error.
        let kind = if has_valid_iterator || !raw_object_types.is_empty() {
            IssueKind::PossiblyInvalidIterator
        } else {
            IssueKind::InvalidIterator
        };
        emit(
            analysis_data,
            kind,
            format!("Cannot iterate over {}", invalid_types.join("|")),
        );
    }

    if !invalid_generator_types.is_empty() {
        let kind = if has_valid_iterator || !raw_object_types.is_empty() {
            IssueKind::PossiblyInvalidIterator
        } else {
            IssueKind::InvalidIterator
        };
        emit(
            analysis_data,
            kind,
            format!(
                "Cannot iterate over generator with non-null send() type {}",
                invalid_generator_types.join("|")
            ),
        );
    }

    if !raw_object_types.is_empty() {
        // Psalm: when other union members iterate fine, the object iteration
        // is only possible, not definite.
        if has_valid_iterator {
            emit(
                analysis_data,
                IssueKind::PossibleRawObjectIteration,
                format!(
                    "Possibly undesired iteration over regular object {}",
                    raw_object_types.join("|"),
                ),
            );
        } else {
            emit(
                analysis_data,
                IssueKind::RawObjectIteration,
                format!(
                    "Trying to iterate over the non-Traversable object {}",
                    raw_object_types.join("|"),
                ),
            );
        }
    }
}

/// Whether a class (by interned name) is — or implements/extends — `Traversable`,
/// and may therefore be used directly in `foreach`.
fn class_is_traversable(codebase: &CodebaseInfo, name: StrId) -> bool {
    if matches!(
        name,
        StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
    ) {
        return true;
    }

    let Some(class_info) = codebase.get_class(name) else {
        return false;
    };

    class_info.interfaces.contains(&StrId::TRAVERSABLE)
        || class_info
            .all_parent_interfaces
            .iter()
            .any(|interface| *interface == StrId::TRAVERSABLE)
}

/// Psalm's `ForeachAnalyzer::getKeyValueParamsForTraversableObject`: resolve a
/// named object's `Traversable<TKey, TValue>` binding by walking the class's
/// flattened `template_extended_params` to `Traversable`, substituting the
/// object's own type params — or, when iterated bare, its template
/// constraints ("assume that it's inside the calling class") — along the way.
pub(crate) fn traversable_extended_param(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: StrId,
    type_params: Option<&Vec<TUnion>>,
    template_name: &str,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_name)?;
    let template_id = analyzer.interner.intern(template_name);

    let passed_type_params: Option<Vec<TUnion>> = if let Some(params) = type_params {
        Some(params.clone())
    } else if !class_info.template_types.is_empty() {
        Some(
            class_info
                .template_types
                .iter()
                .map(|template| template.as_type.clone())
                .collect(),
        )
    } else {
        None
    };

    get_extended_type(
        template_id,
        StrId::TRAVERSABLE,
        class_name,
        class_info,
        passed_type_params.as_deref(),
    )
}

/// Psalm's `ForeachAnalyzer::getExtendedType`: follow a template name through
/// the calling class's flattened extended params until it bottoms out in a
/// concrete type (or in the calling class's own params, substituted from
/// `calling_type_params`).
fn get_extended_type(
    template_name: StrId,
    template_class: StrId,
    calling_class: StrId,
    class_info: &pzoom_code_info::class_like_info::ClassLikeInfo,
    calling_type_params: Option<&[TUnion]>,
) -> Option<TUnion> {
    if calling_class == template_class {
        if let Some(calling_type_params) = calling_type_params
            && let Some(offset) = class_info
                .template_types
                .iter()
                .position(|template| template.name == template_name)
            && let Some(param) = calling_type_params.get(offset)
        {
            return Some(param.clone());
        }
        return None;
    }

    let extended_type = class_info
        .template_extended_params
        .get(&template_class)?
        .get(&template_name)?;

    let mut return_type: Option<TUnion> = None;
    for extended_atomic in &extended_type.types {
        let candidate = if let TAtomic::TTemplateParam {
            name,
            defining_entity: pzoom_code_info::GenericParent::ClassLike(defining_class),
            ..
        } = extended_atomic
        {
            get_extended_type(
                *name,
                *defining_class,
                calling_class,
                class_info,
                calling_type_params,
            )
        } else {
            // Psalm combines the whole extended union for a non-template part.
            Some(extended_type.clone())
        };

        if let Some(candidate) = candidate {
            return_type = Some(match return_type {
                Some(existing) => combine_union_types(&existing, &candidate, false),
                None => candidate,
            });
        }
    }

    return_type
}

/// Extract the value type from an iterable type.
fn extract_iterable_value_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut value_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            // Iterating mixed yields mixed values (Psalm) — without this, a
            // union like `array<never, never>|mixed` (from `$m ?? []`) would
            // collapse to `never` and wrongly skip the loop body.
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => value_types.push(TUnion::mixed()),
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                // Union of all known entry types plus the typed fallback value.
                for (_, prop_type) in known_values.values() {
                    value_types.push(prop_type.clone());
                }
                if let Some(params) = params {
                    value_types.push(params.1.clone());
                }
            }
            TAtomic::TIterable { value_type, .. } => value_types.push((**value_type).clone()),
            // A constrained template iterates through its as-type (Psalm
            // dissolves `T as iterable<int>` for iteration).
            TAtomic::TTemplateParam { as_type, .. } => {
                value_types.push(extract_iterable_value_type(as_type, _analyzer));
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } if type_params.is_none() => {
                // IteratorAggregate-style objects iterate via getIterator()'s
                // declared return type (Psalm's ForeachAnalyzer). When that
                // yields no information (e.g. an unparameterized Traversable
                // return), fall back to the class's declared `Traversable<_,
                // TValue>` binding (Psalm's
                // getKeyValueParamsForTraversableObject — e.g.
                // `@template-implements IteratorAggregate<int, int>`), then to
                // an Iterator implementor's `current()` return type.
                let mut resolved: Option<TUnion> = None;
                if let Some(iterator_return) = classlike_get_iterator_return(_analyzer, *name) {
                    let extracted = extract_iterable_value_type(&iterator_return, _analyzer);
                    if !extracted.is_mixed() {
                        resolved = Some(extracted);
                    }
                }
                if resolved.is_none() {
                    resolved = traversable_extended_param(_analyzer, *name, None, "TValue");
                }
                if resolved.is_none() {
                    resolved = classlike_iterator_method_return(_analyzer, *name, "current");
                }
                value_types.push(resolved.unwrap_or_else(TUnion::mixed));
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(type_params) = type_params {
                    if let Some(value) =
                        traversable_extended_param(_analyzer, *name, Some(type_params), "TValue")
                    {
                        value_types.push(value);
                    } else if type_params.len() >= 2 {
                        value_types.push(type_params[1].clone());
                    } else if let Some(first) = type_params.first() {
                        value_types.push(first.clone());
                    } else {
                        value_types.push(TUnion::mixed());
                    }
                } else {
                    value_types.push(TUnion::mixed());
                }
            }
            // `iterable<A>&iterable<B>`: each intersection part constrains the
            // element, so the value type is the intersection of the parts'
            // value types (Psalm walks intersection types the same way).
            TAtomic::TObjectIntersection { types } => {
                let mut intersected: Option<TUnion> = None;
                for part in types {
                    let extracted =
                        extract_iterable_value_type(&TUnion::new(part.clone()), _analyzer);
                    intersected = Some(match intersected {
                        None => extracted,
                        Some(existing) if existing.is_mixed() => extracted,
                        Some(existing) if extracted.is_mixed() => existing,
                        Some(existing) => {
                            crate::reconciler::assertion_reconciler::intersect_union_with_union(
                                &existing, &extracted,
                            )
                            .unwrap_or(existing)
                        }
                    });
                }
                if let Some(intersected) = intersected {
                    value_types.push(intersected);
                }
            }
            _ => {}
        }
    }

    if value_types.is_empty() {
        TUnion::mixed()
    } else {
        // Combine all value types using the type combiner
        let mut result = value_types.remove(0);
        for t in value_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Extract the key type from an iterable type.
fn extract_iterable_key_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            // Iterating mixed yields mixed keys (Psalm).
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => key_types.push(TUnion::mixed()),
            // Iterating a list shape yields integer keys in `0..len`. Psalm
            // widens the known indices to a range (TKeyedArray::getGenericArrayType)
            // — `list{int, int}` iterates with key `int<0, 1>`, not `0|1`.
            // Keeping them as literal offsets would let a write at that key to a
            // *different* list preserve list-ness, masking the PropertyTypeCoercion
            // Psalm reports (the literal-offset write path treats `0|1` as a
            // sequential list build).
            TAtomic::TArray {
                known_values,
                is_list: true,
                params,
                ..
            } if !known_values.is_empty() => {
                let max = if params.is_some() {
                    None
                } else {
                    Some(known_values.len() as i64 - 1)
                };
                key_types.push(TUnion::new(TAtomic::TIntRange { min: Some(0), max }));
            }
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                // Union of all known key types (a list's fallback key is `int`).
                for key in known_values.keys() {
                    match key {
                        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                            key_types.push(TUnion::new(TAtomic::TLiteralInt { value: *value }));
                        }
                        // A `Foo::class` key iterates as a class-string, not a
                        // plain literal string (Psalm's TKeyedArray::$class_strings).
                        pzoom_code_info::t_atomic::ArrayKey::ClassString(value) => {
                            key_types.push(TUnion::new(TAtomic::TLiteralClassString {
                                name: value.clone(),
                            }));
                        }
                        pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                            // Canonical int strings were already normalized to
                            // ArrayKey::Int at array creation; what remains
                            // ("01", "1e2", ...) keeps its string identity.
                            key_types.push(TUnion::new(TAtomic::TLiteralString {
                                value: value.clone(),
                            }));
                        }
                    }
                }
                if let Some(params) = params {
                    key_types.push(params.0.clone());
                }
            }
            TAtomic::TIterable { key_type, .. } => key_types.push((**key_type).clone()),
            // A constrained template iterates through its as-type.
            TAtomic::TTemplateParam { as_type, .. } => {
                key_types.push(extract_iterable_key_type(as_type, _analyzer));
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } if type_params.is_none() => {
                let mut resolved: Option<TUnion> = None;
                if let Some(iterator_return) = classlike_get_iterator_return(_analyzer, *name) {
                    let extracted = extract_iterable_key_type(&iterator_return, _analyzer);
                    if !extracted.is_mixed()
                        && extracted.get_id(None) != TUnion::array_key().get_id(None)
                    {
                        resolved = Some(extracted);
                    }
                }
                if resolved.is_none() {
                    resolved = traversable_extended_param(_analyzer, *name, None, "TKey");
                }
                if resolved.is_none() {
                    resolved = classlike_iterator_method_return(_analyzer, *name, "key");
                }
                key_types.push(resolved.unwrap_or_else(TUnion::array_key));
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(type_params) = type_params {
                    if let Some(key) =
                        traversable_extended_param(_analyzer, *name, Some(type_params), "TKey")
                    {
                        key_types.push(key);
                    } else if type_params.len() >= 2 {
                        key_types.push(type_params[0].clone());
                    } else {
                        key_types.push(TUnion::array_key());
                    }
                } else {
                    key_types.push(TUnion::array_key());
                }
            }
            // `iterable<A>&iterable<B>`: the key type is the intersection of
            // the parts' key types.
            TAtomic::TObjectIntersection { types } => {
                let mut intersected: Option<TUnion> = None;
                for part in types {
                    let extracted =
                        extract_iterable_key_type(&TUnion::new(part.clone()), _analyzer);
                    intersected = Some(match intersected {
                        None => extracted,
                        Some(existing) if existing.is_mixed() => extracted,
                        Some(existing) if extracted.is_mixed() => existing,
                        Some(existing) => {
                            crate::reconciler::assertion_reconciler::intersect_union_with_union(
                                &existing, &extracted,
                            )
                            .unwrap_or(existing)
                        }
                    });
                }
                if let Some(intersected) = intersected {
                    key_types.push(intersected);
                }
            }
            _ => {}
        }
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        // Combine all key types using the type combiner
        let mut result = key_types.remove(0);
        for t in key_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Set a variable's type in the context from an expression.
fn set_expression_var_type(
    expr: &Expression<'_>,
    var_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let target = unwrap_reference_target(expr);

    match target.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            let var_id = VarName::new(direct.name);
            context.set_var_type(var_id, var_type.clone());
        }
        Expression::List(list) => {
            for (offset, element) in list.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        Expression::Array(array) => {
            for (offset, element) in array.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
enum DestructuringLookupKey {
    Int(i64),
    String(String),
    Unknown,
}

fn set_destructuring_element_var_type(
    element: &ArrayElement<'_>,
    offset: usize,
    source_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let (target_expr, lookup_key) = match element {
        ArrayElement::Missing(_) | ArrayElement::Variadic(_) => return,
        ArrayElement::Value(value_element) => (
            value_element.value,
            DestructuringLookupKey::Int(offset as i64),
        ),
        ArrayElement::KeyValue(key_value) => (
            key_value.value,
            extract_destructuring_key(key_value.key).unwrap_or(DestructuringLookupKey::Unknown),
        ),
    };

    let target_type = infer_destructured_value_type(source_type, &lookup_key);
    set_expression_var_type(target_expr, &target_type, analyzer, context);
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
    source_type: &TUnion,
    lookup_key: &DestructuringLookupKey,
) -> TUnion {
    let mut inferred_type: Option<TUnion> = None;
    let mut saw_destructurable_type = false;

    for atomic in &source_type.types {
        match atomic {
            // Generic array/list (no known entries): the value type applies to
            // every offset, so it is added regardless of the lookup key.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } if known_values.is_empty() => {
                saw_destructurable_type = true;
                if let Some(params) = params {
                    add_inferred_union(&mut inferred_type, &params.1);
                }
            }
            // Shape (known entries): pick the looked-up entry, else the typed
            // fallback, else the union of every entry.
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                saw_destructurable_type = true;
                let fallback_value_type = params.as_ref().map(|params| &params.1);
                if let Some(array_key) = lookup_key_to_array_key(lookup_key) {
                    if let Some((_, property_type)) = known_values.get(&array_key) {
                        add_inferred_union(&mut inferred_type, property_type);
                    } else if let Some(fallback_value_type) = fallback_value_type {
                        add_inferred_union(&mut inferred_type, fallback_value_type);
                    }
                } else if let Some(fallback_value_type) = fallback_value_type {
                    add_inferred_union(&mut inferred_type, fallback_value_type);
                } else {
                    for (_, property_type) in known_values.values() {
                        add_inferred_union(&mut inferred_type, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return TUnion::mixed(),
            _ => {}
        }
    }

    if let Some(inferred_type) = inferred_type {
        inferred_type
    } else if saw_destructurable_type {
        TUnion::mixed()
    } else {
        source_type.clone()
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

/// Collect the variables named by list/array destructuring items — each
/// item's value variable and key variable (Psalm's `$safe_var_ids` walk over
/// `List_::items`).
fn collect_destructuring_safe_var_ids(
    analyzer: &StatementsAnalyzer<'_>,
    elements: &[ArrayElement<'_>],
    safe_var_ids: &mut Vec<StrId>,
) {
    for element in elements {
        let (key_expr, value_expr) = match element {
            ArrayElement::Value(value_element) => (None, value_element.value),
            ArrayElement::KeyValue(kv) => (Some(kv.key), kv.value),
            ArrayElement::Variadic(_) | ArrayElement::Missing(_) => continue,
        };
        for expr in key_expr.into_iter().chain(std::iter::once(value_expr)) {
            if let Expression::Variable(Variable::Direct(direct)) =
                unwrap_reference_target(expr).unparenthesized()
            {
                safe_var_ids.push(analyzer.interner.intern(direct.name));
            }
        }
    }
}

fn unwrap_reference_target<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    if let Expression::UnaryPrefix(unary) = expr.unparenthesized()
        && matches!(unary.operator, UnaryPrefixOperator::Reference(_))
    {
        return unary.operand;
    }

    expr
}

fn mark_foreach_reference_target(expr: &Expression<'_>, context: &mut BlockContext) {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return;
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Reference(_)) {
        return;
    }

    let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized() else {
        return;
    };

    let var_id = VarName::new(direct.name);
    context.clear_confusing_reference(&var_id);
    context.mark_external_reference(var_id);
}

/// The declared return type of an Iterator implementor's `current()`/`key()`
/// — Psalm's ForeachAnalyzer resolves Iterator iteration through them when
/// no Traversable template binding exists.
fn classlike_iterator_method_return(
    analyzer: &StatementsAnalyzer<'_>,
    name: pzoom_str::StrId,
    method_name: &str,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(name)?;
    let implements_iterator = class_info
        .interfaces
        .iter()
        .chain(class_info.all_parent_interfaces.iter())
        .any(|interface| {
            *interface == pzoom_str::StrId::ITERATOR || *interface == pzoom_str::StrId::TRAVERSABLE
        });
    if !implements_iterator {
        return None;
    }
    let method_id = analyzer.interner.intern(method_name);
    let method = class_info.methods.get(&method_id)?;
    method.get_return_type().cloned()
}

/// The declared return type of `name`'s getIterator() method, if any —
/// Psalm resolves IteratorAggregate iteration through it. The
/// `originating_class` guard breaks `@return self`-style cycles.
fn classlike_get_iterator_return(
    analyzer: &StatementsAnalyzer<'_>,
    name: pzoom_str::StrId,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(name)?;
    let get_iterator_id = analyzer.interner.intern("getIterator");
    let method = class_info.methods.get(&get_iterator_id)?;
    let return_type = method.get_return_type()?.clone();
    let is_self_cycle = return_type.types.iter().any(
        |atomic| matches!(atomic, TAtomic::TNamedObject { name: inner, .. } if *inner == name),
    );
    (!is_self_cycle).then_some(return_type)
}
