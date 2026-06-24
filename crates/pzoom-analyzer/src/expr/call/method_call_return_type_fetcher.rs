//! Method-call return-type fetcher: inherited/effective return & param types,
//! return-type-provider adjustments. Mirrors Psalm `MethodCallReturnTypeFetcher`.

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind};
use pzoom_code_info::{
    ArrayDataKind, DataFlowNode, FunctionLikeIdentifier, FunctionLikeInfo, GraphKind, PathKind,
    TAtomic, TUnion,
};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::data_flow::make_data_flow_node_position;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

use super::function_call_analyzer;
use super::function_call_return_type_fetcher::add_special_param_dataflow;

use super::method_call_analyzer::*;

use super::atomic_method_call_analyzer::*;
use super::missing_method_call_handler::*;

/// Port of Hakana `method_call_return_type_fetcher::add_dataflow`
/// (function-body branch; the whole-program branch builds taint-specific
/// `$this`-before/after and declaring-vs-appearing method nodes that pzoom
/// skips because it only builds function-body graphs).
///
/// - Builds the call's return node (`CallTo`, specialized to the call site —
///   Hakana's `specialize_call` is true for almost every method).
/// - Receiver flow: a pure method lets the receiver's dataflow continue into
///   the call's return node; any other method consumes the receiver with an
///   unlabelled sink (the receiver value is "used" by the call).
/// - `MessageFormatter::formatMessage` keeps Hakana's special argument edges.
/// - Hack-only `Shapes::keyExists` and per-param `propagate_taint` edges are
///   skipped (no PHP equivalent / no storage field).
/// - The returned union's `parent_nodes` become `[call node]`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_method_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    return_type_candidate: TUnion,
    lhs_expr_pos: Option<Pos>,
    classlike_name: StrId,
    method_name: StrId,
    functionlike_storage: Option<&FunctionLikeInfo>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    call_pos: Pos,
) -> TUnion {
    add_method_call_dataflow_with_receiver(
        analyzer,
        return_type_candidate,
        lhs_expr_pos,
        None,
        None,
        classlike_name,
        method_name,
        functionlike_storage,
        arg_positions,
        analysis_data,
        call_pos,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn add_method_call_dataflow_with_receiver(
    analyzer: &StatementsAnalyzer<'_>,
    mut return_type_candidate: TUnion,
    lhs_expr_pos: Option<Pos>,
    lhs_var_id: Option<&str>,
    context: Option<&mut crate::context::BlockContext>,
    classlike_name: StrId,
    method_name: StrId,
    functionlike_storage: Option<&FunctionLikeInfo>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    call_pos: Pos,
) -> TUnion {
    // Hakana gates whole-program dataflow on `context.allow_taints`; pzoom
    // only builds function-body graphs, so no gate is needed here.

    let call_node_pos = make_data_flow_node_position(analyzer, call_pos);
    let functionlike_id = FunctionLikeIdentifier::Method(classlike_name, method_name);

    // Psalm positions the return node at the declared return type when one
    // exists — that becomes the "Consider improving the type at …" origin.
    // Same-file only (line/column derivation needs the current source).
    let return_decl_pos = functionlike_storage
        .filter(|storage| storage.file_path == analyzer.file_path)
        .and_then(|storage| storage.return_type_location)
        .map(|(start, end)| make_data_flow_node_position(analyzer, (start, end)));

    let method_call_node = if let GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind {
        // Hakana `get_tainted_method_node`: the return node specializes per
        // call site only for `specialize_call` storages, and a method called
        // through a subclass gets a declaring→called edge so the declaring
        // body's return reaches this call (`A::getTaint → B::getTaint`).
        let specialize_call = functionlike_storage
            .map(|storage| storage.taints.specialize_call)
            .unwrap_or(true);
        let specialization = specialize_call.then_some(call_node_pos);

        let declaring_class = functionlike_storage
            .and_then(|storage| storage.declaring_class)
            .unwrap_or(classlike_name);

        if declaring_class != classlike_name {
            let method_call_node =
                DataFlowNode::get_for_method_return(&functionlike_id, None, specialization);

            let declaring_node = DataFlowNode::get_for_method_return(
                &FunctionLikeIdentifier::Method(declaring_class, method_name),
                return_decl_pos,
                specialization,
            );

            analysis_data
                .data_flow_graph
                .add_node(declaring_node.clone());
            analysis_data.data_flow_graph.add_path(
                &declaring_node.id,
                &method_call_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );

            method_call_node
        } else {
            DataFlowNode::get_for_method_return(&functionlike_id, return_decl_pos, specialization)
        }
    } else {
        DataFlowNode::get_for_method_return(
            &functionlike_id,
            Some(return_decl_pos.unwrap_or(call_node_pos)),
            Some(call_node_pos),
        )
    };

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
        && let Some(lhs_expr_pos) = lhs_expr_pos
        && let Some(lhs_parent_nodes) = analysis_data
            .expr_types
            .get(&lhs_expr_pos)
            .cloned()
            .map(|expr_type| expr_type.parent_nodes.clone())
    {
        if functionlike_storage.is_some_and(|storage| storage.is_pure) {
            for parent_node in &lhs_parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &method_call_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        } else {
            let sink_node = DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(
                analyzer,
                lhs_expr_pos,
            ));

            for parent_node in &lhs_parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &sink_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }

            analysis_data.data_flow_graph.add_node(sink_node);
        }
    }

    if classlike_name == StrId::MESSAGE_FORMATTER && method_name == StrId::FORMAT_MESSAGE {
        if let Some(arg_pos) = arg_positions.first() {
            add_special_param_dataflow(
                analyzer,
                &functionlike_id,
                true,
                0,
                *arg_pos,
                call_pos,
                &mut analysis_data.data_flow_graph,
                &method_call_node,
                PathKind::Aggregate,
                vec![],
                vec![],
            );
        }
        if let Some(arg_pos) = arg_positions.get(1) {
            add_special_param_dataflow(
                analyzer,
                &functionlike_id,
                true,
                1,
                *arg_pos,
                call_pos,
                &mut analysis_data.data_flow_graph,
                &method_call_node,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
        if let Some(arg_pos) = arg_positions.get(2) {
            add_special_param_dataflow(
                analyzer,
                &functionlike_id,
                true,
                2,
                *arg_pos,
                call_pos,
                &mut analysis_data.data_flow_graph,
                &method_call_node,
                PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue),
                vec![],
                vec![],
            );
        }
    }

    // Hakana `get_tainted_method_node` tail: a `specialize_call` method's
    // receiver flows through `ThisBeforeMethod`/`ThisAfterMethod` nodes
    // specialized to this call site — instance state enters the body's
    // `$this` and the (possibly mutated) state flows back onto the receiver
    // variable.
    if let GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind
        && functionlike_storage.is_some_and(|storage| storage.taints.specialize_call)
        && let (Some(lhs_expr_pos), Some(lhs_var_id), Some(context)) =
            (lhs_expr_pos, lhs_var_id, context)
        && let Some(receiver_type) = analysis_data.expr_types.get(&lhs_expr_pos).cloned()
    {
        let declaring_class = functionlike_storage
            .and_then(|storage| storage.declaring_class)
            .unwrap_or(classlike_name);
        let declaring_method_id =
            pzoom_code_info::method_identifier::MethodIdentifier(declaring_class, method_name);

        let var_node = DataFlowNode::get_for_lvar(
            pzoom_code_info::VarId(
                analyzer
                    .interner
                    .find(&pzoom_code_info::VarName::new(lhs_var_id))
                    .unwrap_or(pzoom_str::StrId::EMPTY),
            ),
            make_data_flow_node_position(analyzer, lhs_expr_pos),
        );

        let this_before_method_node = DataFlowNode::get_for_this_before_method(
            &declaring_method_id,
            None,
            Some(call_node_pos),
        );

        for parent_node in &receiver_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &parent_node.id,
                &this_before_method_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }

        let this_after_method_node = DataFlowNode::get_for_this_after_method(
            &declaring_method_id,
            None,
            Some(call_node_pos),
        );

        analysis_data.data_flow_graph.add_path(
            &this_after_method_node.id,
            &var_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );

        if let Some(receiver_in_scope) = context.locals.get_mut_owned(lhs_var_id) {
            receiver_in_scope.parent_nodes = vec![var_node.clone()];
        }

        analysis_data.data_flow_graph.add_node(var_node);
        analysis_data
            .data_flow_graph
            .add_node(this_before_method_node);
        analysis_data
            .data_flow_graph
            .add_node(this_after_method_node);
    }

    // Whole-program taint mode: storage-driven `@psalm-flow` edges and
    // `@psalm-taint-source` sources, shared with function calls.
    let (storage_added_taints, storage_removed_taints) = functionlike_storage
        .map(|info| {
            (
                info.taints.added_taints.clone(),
                info.taints.removed_taints.clone(),
            )
        })
        .unwrap_or_default();
    super::function_call_return_type_fetcher::add_storage_taint_dataflow(
        analyzer,
        &functionlike_id,
        functionlike_storage,
        arg_positions,
        call_pos,
        &method_call_node,
        analysis_data,
        &storage_added_taints,
        &storage_removed_taints,
    );

    analysis_data
        .data_flow_graph
        .add_node(method_call_node.clone());

    // `@psalm-taint-escape (<conditional>)` resolved against this call's
    // arguments (Psalm's StaticCallAnalyzer; see
    // `apply_conditionally_escaped_taints` for the dead-end-node semantics).
    if let Some(escaped_node) =
        super::function_call_return_type_fetcher::apply_conditionally_escaped_taints(
            analyzer,
            &functionlike_id,
            functionlike_storage,
            arg_positions,
            analysis_data,
            &method_call_node,
            call_pos,
        )
    {
        return_type_candidate.parent_nodes = vec![escaped_node];
    } else {
        return_type_candidate.parent_nodes = vec![method_call_node];
    }

    return_type_candidate
}

pub(crate) fn method_has_more_specific_return(
    analyzer: &StatementsAnalyzer<'_>,
    candidate_method: &pzoom_code_info::FunctionLikeInfo,
    current_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let Some(candidate_return) = candidate_method
        .signature_return_type
        .as_ref()
        .or(candidate_method.return_type.as_ref())
    else {
        return false;
    };

    let Some(current_return) = current_method
        .signature_return_type
        .as_ref()
        .or(current_method.return_type.as_ref())
    else {
        return true;
    };

    let mut candidate_in_current = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        candidate_return,
        current_return,
        false,
        false,
        &mut candidate_in_current,
    ) {
        return false;
    }

    let mut current_in_candidate = TypeComparisonResult::new();
    let current_is_contained_by_candidate = union_type_comparator::is_contained_by(
        analyzer.codebase,
        current_return,
        candidate_return,
        false,
        false,
        &mut current_in_candidate,
    );

    !current_is_contained_by_candidate
}

pub(crate) fn is_datetime_interface_add(class_name: pzoom_str::StrId, method_name: &str) -> bool {
    if !method_name.eq_ignore_ascii_case("add") {
        return false;
    }

    class_name == StrId::DATE_TIME_INTERFACE
}

pub(crate) fn should_strip_false_from_datetime_modify_return(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class_id: StrId,
    method_name: &str,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> bool {
    if !method_name.eq_ignore_ascii_case("modify") {
        return false;
    }

    if !is_datetime_like_class(analyzer, receiver_class_id) {
        return false;
    }

    let Some(first_arg_pos) = arg_positions.first().copied() else {
        return false;
    };

    let Some(first_arg_type) = analysis_data.expr_types.get(&first_arg_pos).cloned() else {
        return false;
    };

    !first_arg_type.types.is_empty()
        && first_arg_type.types.iter().all(|atomic| match atomic {
            TAtomic::TLiteralString { value } => !value.trim().is_empty(),
            _ => false,
        })
}

pub(crate) fn should_strip_false_from_pdo_prepare_return(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class_id: StrId,
    method_name: &str,
) -> bool {
    if !method_name.eq_ignore_ascii_case("prepare") {
        return false;
    }

    is_pdo_like_class(analyzer, receiver_class_id)
}

fn is_datetime_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    if class_id == StrId::DATE_TIME
        || class_id == StrId::DATE_TIME_IMMUTABLE
        || class_id == StrId::DATE_TIME_INTERFACE
    {
        return true;
    }

    // DateTime/DateTimeImmutable both implement DateTimeInterface, so a
    // single ancestor-interface check covers every datetime-like class.
    analyzer
        .codebase
        .get_class(class_id)
        .is_some_and(|class_info| {
            class_info
                .all_parent_interfaces
                .contains(&StrId::DATE_TIME_INTERFACE)
        })
}

fn is_pdo_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    if class_id == StrId::PDO {
        return true;
    }

    analyzer
        .codebase
        .get_class(class_id)
        .is_some_and(|class_info| class_info.all_parent_classes.contains(&StrId::PDO))
}

pub(crate) fn localize_class_union_type(
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    union: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return union.clone();
    }

    let mut template_result = function_call_analyzer::get_class_template_defaults(class_info);
    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut template_result,
        class_info,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_result,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    if crate::template::template_result_is_empty(&template_result) {
        return union.clone();
    }

    function_call_analyzer::replace_templates_in_union(union, &template_result)
}

pub(crate) fn merge_receiver_intersection_into_return_type(
    localized_return_type: &TUnion,
    receiver_type: &TUnion,
) -> TUnion {
    let receiver_named_types = collect_receiver_named_types(receiver_type);
    if receiver_named_types.is_empty() {
        return localized_return_type.clone();
    }

    let mut changed = false;
    let mut merged = Vec::with_capacity(localized_return_type.types.len());

    for atomic in &localized_return_type.types {
        match atomic {
            TAtomic::TObjectIntersection { types } => {
                let mut merged_types = types.clone();
                for receiver_named in &receiver_named_types {
                    if !merged_types.contains(receiver_named) {
                        merged_types.push(receiver_named.clone());
                        changed = true;
                    }
                }

                merged.push(TAtomic::TObjectIntersection {
                    types: merged_types,
                });
            }
            _ => merged.push(atomic.clone()),
        }
    }

    if changed {
        TUnion::from_types(merged)
    } else {
        localized_return_type.clone()
    }
}

/// The documenting ancestor's method-level `@template` declarations: a method
/// redeclared without docblock types of its own keeps binding the ancestor's
/// templates at call sites (Psalm resolves the call against the
/// declaring/documenting method's storage, templates included — so the
/// inherited `@param T $value` binds `T` from the argument).
pub(crate) fn inherited_method_template_types(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> Option<Vec<pzoom_code_info::functionlike_info::FunctionTemplateType>> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }
    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();
    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }
        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };
        let Some(candidate_method_info) =
            get_method_info(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };
        if !candidate_method_info.template_types.is_empty() {
            return Some(candidate_method_info.template_types.clone());
        }
    }

    None
}

pub(crate) fn get_inherited_method_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    param_index: usize,
) -> Option<InheritedParamType> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }

    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();
    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        let Some(candidate_param) = candidate_method_info.params.get(param_index) else {
            continue;
        };
        let Some(candidate_param_type) = candidate_param.get_type().cloned() else {
            continue;
        };

        // Psalm's Methods::getMethodParams localizes the inherited type onto
        // the calling class (TypeLocalizer): ancestor class templates resolve
        // through the extends chain to the calling class's own templates (or
        // concrete args), while method-level templates pass through untouched
        // and bind from the args during standin replacement.
        let mut resolved_param_type = candidate_param_type;
        if !class_info.template_extended_params.is_empty() {
            resolved_param_type = crate::stmt::class_analyzer::replace_extended_templates_in_union(
                &resolved_param_type,
                &class_info.template_extended_params,
            );
        }

        return Some(InheritedParamType {
            param_type: resolved_param_type,
            from_docblock: candidate_param.has_docblock_type,
            source_is_interface: candidate_class_info.kind == ClassLikeKind::Interface,
        });
    }

    None
}

/// Applies ancestor docblock param types onto a method's params up front —
/// Psalm's `Methods::getMethodParams` documenting-method branch runs *before*
/// argument analysis, so templates declared by the documenting method (e.g.
/// an interface method's own `@template T`) bind from the args during standin
/// replacement. Returns `None` when no param inherits a type.
///
/// The per-param rules mirror `verify_method_arguments`' inline overlay (which
/// stays in place and re-derives the same types idempotently).
pub(crate) fn apply_inherited_method_param_types(
    analyzer: &StatementsAnalyzer<'_>,
    self_class_id: StrId,
    method_name: &str,
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> Option<Vec<pzoom_code_info::functionlike_info::ParamInfo>> {
    let has_docblock_params = method_has_docblock_param_types(method_info);
    let has_docblock_return = method_has_docblock_return_type(method_info);

    let mut params = method_info.params.clone();
    let mut changed = false;

    for (idx, param) in params.iter_mut().enumerate() {
        let Some(inherited_param_type) =
            get_inherited_method_param_type(analyzer, self_class_id, method_name, idx)
        else {
            continue;
        };

        let can_auto_inherit_docblock =
            inherited_param_type.from_docblock && !has_docblock_params && !has_docblock_return;
        let can_inherit_interface_contract =
            inherited_param_type.source_is_interface && !param.has_docblock_type;

        let should_use_inherited = param.get_type().is_none()
            || (method_info.inherits_docblock && !param.has_docblock_type)
            || (can_auto_inherit_docblock && !param.has_docblock_type)
            || can_inherit_interface_contract;

        if should_use_inherited {
            param.param_type = Some(inherited_param_type.param_type);
            changed = true;
        }
    }

    changed.then_some(params)
}

pub(crate) fn method_has_docblock_return_type(
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    method_info.return_type.is_some()
}

pub(crate) fn method_has_docblock_param_types(
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    method_info
        .params
        .iter()
        .any(|param| param.has_docblock_type)
}

/// Like [`expand_template_object_union`], but a receiver that is a type
/// variable resolves through its accumulated lower bounds (Hakana's
/// `instance_call_analyzer` `TTypeVariable` arm).
pub(crate) fn expand_template_object_union_with_type_variables(
    obj_type: &TUnion,
    type_variable_bounds: Option<
        &rustc_hash::FxHashMap<String, pzoom_code_info::TypeVariableBounds>,
    >,
) -> TUnion {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TTypeVariable { name } => {
                if let Some(bounds) = type_variable_bounds.and_then(|bounds| bounds.get(name)) {
                    for lower_bound_info in &bounds.lower_bounds {
                        for bound_atomic in &lower_bound_info.bound_type.types {
                            if !expanded_types.contains(bound_atomic) {
                                expanded_types.push(bound_atomic.clone());
                            }
                        }
                    }
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                for as_atomic in &as_type.types {
                    if !expanded_types.contains(as_atomic) {
                        expanded_types.push(as_atomic.clone());
                    }
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !expanded_types.contains(as_type) {
                    expanded_types.push((**as_type).clone());
                }
            }
            TAtomic::TObjectIntersection { types } => {
                let mut expanded_intersection = Vec::new();

                for intersection_atomic in types {
                    match intersection_atomic {
                        TAtomic::TTemplateParam { as_type, .. } => {
                            for as_atomic in &as_type.types {
                                if !expanded_intersection.contains(as_atomic) {
                                    expanded_intersection.push(as_atomic.clone());
                                }
                            }
                        }
                        TAtomic::TTemplateParamClass { as_type, .. } => {
                            if !expanded_intersection.contains(as_type) {
                                expanded_intersection.push((**as_type).clone());
                            }
                        }
                        _ => {
                            if !expanded_intersection.contains(intersection_atomic) {
                                expanded_intersection.push(intersection_atomic.clone());
                            }
                        }
                    }
                }

                if !expanded_intersection.is_empty() {
                    let expanded_atomic = TAtomic::TObjectIntersection {
                        types: expanded_intersection,
                    };

                    if !expanded_types.contains(&expanded_atomic) {
                        expanded_types.push(expanded_atomic);
                    }
                }
            }
            _ => {
                if !expanded_types.contains(atomic) {
                    expanded_types.push(atomic.clone());
                }
            }
        }
    }

    let mut expanded = TUnion::from_types(expanded_types);
    expanded.from_docblock = obj_type.from_docblock;
    // The leniency flags gate receiver issues (PossiblyFalseReference /
    // PossiblyNullReference honour @psalm-ignore-*-return) — carry them
    // through the rebuild like Psalm's clone-based expansion does.
    expanded.ignore_falsable_issues = obj_type.ignore_falsable_issues;
    expanded.ignore_nullable_issues = obj_type.ignore_nullable_issues;
    expanded
}
