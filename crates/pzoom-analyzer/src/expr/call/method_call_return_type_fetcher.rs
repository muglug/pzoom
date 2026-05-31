//! Method-call return-type fetcher: inherited/effective return & param types,
//! return-type-provider adjustments. Mirrors Psalm `MethodCallReturnTypeFetcher`.


use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind};
use pzoom_code_info::{
    TAtomic, TUnion,
};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

use super::function_call_analyzer;

use super::method_call_analyzer::*;

use super::atomic_method_call_analyzer::*;
use super::missing_method_call_handler::*;
use crate::template::TemplateMap;

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

pub(crate) fn is_datetime_interface_add(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: pzoom_str::StrId,
    method_name: &str,
) -> bool {
    if !method_name.eq_ignore_ascii_case("add") {
        return false;
    }

    class_name == analyzer.interner.intern("DateTimeInterface")
        || class_name == analyzer.interner.intern("\\DateTimeInterface")
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

    let Some(first_arg_type) = analysis_data.get_expr_type(first_arg_pos) else {
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

pub(crate) fn is_datetime_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let candidates = [
        analyzer.interner.intern("DateTime"),
        analyzer.interner.intern("\\DateTime"),
        analyzer.interner.intern("DateTimeImmutable"),
        analyzer.interner.intern("\\DateTimeImmutable"),
        analyzer.interner.intern("DateTimeInterface"),
        analyzer.interner.intern("\\DateTimeInterface"),
    ];

    for candidate in candidates {
        if class_id == candidate {
            return true;
        }

        if analyzer
            .codebase
            .all_classlike_descendants
            .get(&candidate)
            .is_some_and(|descendants| descendants.contains(&class_id))
        {
            return true;
        }
    }

    false
}

pub(crate) fn is_pdo_like_class(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let candidates = [
        analyzer.interner.intern("PDO"),
        analyzer.interner.intern("\\PDO"),
    ];

    for candidate in candidates {
        if class_id == candidate {
            return true;
        }

        if analyzer
            .codebase
            .all_classlike_descendants
            .get(&candidate)
            .is_some_and(|descendants| descendants.contains(&class_id))
        {
            return true;
        }
    }

    false
}

pub(crate) fn localize_class_union_type(
    class_info: &ClassLikeInfo,
    object_type_params: Option<&[TUnion]>,
    union: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return union.clone();
    }

    let template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );

    if template_defaults.is_empty() && template_replacements.is_empty() {
        return union.clone();
    }

    function_call_analyzer::replace_templates_in_union(
        union,
        &template_replacements,
        &template_defaults,
    )
}

pub(crate) fn resolve_effective_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    arg_count: usize,
) -> TUnion {
    let own_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        method_info,
        template_defaults,
        template_replacements,
        &FxHashMap::default(),
        arg_count,
    );

    let inherited_return_type = get_inherited_method_return_type(
        analyzer,
        class_id,
        method_name,
        template_defaults,
        template_replacements,
        arg_count,
    );

    match (own_return_type, inherited_return_type) {
        (Some(own_return_type), Some(inherited_return_type))
            if should_prefer_inherited_return(
                analyzer,
                method_info,
                &own_return_type,
                &inherited_return_type,
            ) =>
        {
            inherited_return_type
        }
        (Some(own_return_type), _) => own_return_type,
        (None, Some(inherited_return_type)) => inherited_return_type,
        (None, None) => TUnion::mixed(),
    }
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

pub(crate) fn get_inherited_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
    arg_count: usize,
) -> Option<TUnion> {
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
    candidate_class_ids.extend(class_info.interfaces.iter().copied());
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    // Prefer the ancestor that *documents* the method with a docblock return
    // type (mirrors Psalm's `documenting_method_ids`): a nearer ancestor that
    // merely restates the native signature (e.g. an abstract `NodeAbstract`
    // re-declaring `getAttributes(): array`) must not shadow an interface/parent
    // whose docblock gives the precise type (`@return array<string, mixed>`).
    // Fall back to the nearest ancestor carrying any return type when none
    // document it.
    {
        let mut documenting: Option<StrId> = None;
        let mut any_return: Option<StrId> = None;
        let mut seen_pick = FxHashSet::default();
        for candidate_class_id in &candidate_class_ids {
            if !seen_pick.insert(*candidate_class_id) {
                continue;
            }
            let Some(candidate_class_info) = analyzer.codebase.get_class(*candidate_class_id)
            else {
                continue;
            };
            let Some(candidate_method_info) =
                get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
            else {
                continue;
            };
            if candidate_method_info.return_type.is_some() {
                documenting = Some(*candidate_class_id);
                break;
            }
            if any_return.is_none() && candidate_method_info.get_return_type().is_some() {
                any_return = Some(*candidate_class_id);
            }
        }
        candidate_class_ids = vec![documenting.or(any_return)?];
    }

    let mut seen = FxHashSet::default();

    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        if candidate_method_info.get_return_type().is_none() {
            continue;
        }

        let mut candidate_defaults = template_defaults.clone();
        for (template_name, defining_entity, template_default) in
            function_call_analyzer::get_class_template_defaults(candidate_class_info).iter()
        {
            if !candidate_defaults.contains_name(template_name) {
                candidate_defaults.insert(template_name, defining_entity, template_default.clone());
            }
        }
        for (template_name, defining_entity, template_default) in
            function_call_analyzer::get_template_defaults(candidate_method_info).iter()
        {
            if !candidate_defaults.contains_name(template_name) {
                candidate_defaults.insert(template_name, defining_entity, template_default.clone());
            }
        }

        let mut candidate_replacements = template_replacements.clone();
        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            for (template_name, mapped_type) in candidate_template_map {
                let resolved_mapped_type = function_call_analyzer::replace_templates_in_union(
                    mapped_type,
                    &candidate_replacements,
                    &candidate_defaults,
                );
                candidate_replacements.insert(
                    *template_name,
                    candidate_class_id,
                    resolved_mapped_type,
                );
            }
        }

        let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            candidate_method_info,
            &candidate_defaults,
            &candidate_replacements,
            &FxHashMap::default(),
            arg_count,
        )
        .unwrap_or_else(TUnion::mixed);

        return Some(resolved_return_type);
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
            get_method_info_case_insensitive(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        let Some(candidate_param) = candidate_method_info.params.get(param_index) else {
            continue;
        };
        let Some(candidate_param_type) = candidate_param.get_type().cloned() else {
            continue;
        };

        let mut resolved_param_type = candidate_param_type;
        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            let mut candidate_defaults =
                function_call_analyzer::get_class_template_defaults(candidate_class_info);
            candidate_defaults.extend_overlay(function_call_analyzer::get_template_defaults(
                candidate_method_info,
            ));
            let candidate_replacements: TemplateMap = candidate_template_map
                .iter()
                .map(|(template_name, replacement)| {
                    (*template_name, candidate_class_id, replacement.clone())
                })
                .collect();
            resolved_param_type = function_call_analyzer::replace_templates_in_union(
                &resolved_param_type,
                &candidate_replacements,
                &candidate_defaults,
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

pub(crate) fn method_has_docblock_return_type(method_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    method_info.return_type.is_some()
}

pub(crate) fn method_has_docblock_param_types(method_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    method_info
        .params
        .iter()
        .any(|param| param.has_docblock_type)
}

pub(crate) fn should_prefer_inherited_return(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    own_return_type: &TUnion,
    inherited_return_type: &TUnion,
) -> bool {
    // A method with its own docblock return type should not defer to an inherited one.
    if method_info.return_type.is_some() {
        return false;
    }

    if own_return_type.is_mixed() && !inherited_return_type.is_mixed() {
        return true;
    }

    if let (
        Some(TAtomic::TNamedObject {
            name: own_name,
            type_params: own_params,
        .. }),
        Some(TAtomic::TNamedObject {
            name: inherited_name,
            type_params: inherited_params,
        .. }),
    ) = (
        own_return_type.get_single(),
        inherited_return_type.get_single(),
    ) && own_name == inherited_name
        && own_params.is_none()
        && inherited_params.is_some()
    {
        return true;
    }

    let mut inherited_to_own = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        inherited_return_type,
        own_return_type,
        false,
        false,
        &mut inherited_to_own,
    ) {
        return false;
    }

    let mut own_to_inherited = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        own_return_type,
        inherited_return_type,
        false,
        false,
        &mut own_to_inherited,
    )
}

pub(crate) fn should_prefer_inherited_param(
    analyzer: &StatementsAnalyzer<'_>,
    param: &pzoom_code_info::functionlike_info::ParamInfo,
    inherited_param_type: &TUnion,
) -> bool {
    if param.has_docblock_type {
        return false;
    }

    let Some(own_param_type) = param.get_type() else {
        return true;
    };

    if own_param_type.is_mixed() && !inherited_param_type.is_mixed() {
        return true;
    }

    let mut inherited_to_own = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        inherited_param_type,
        own_param_type,
        false,
        false,
        &mut inherited_to_own,
    ) {
        return false;
    }

    let mut own_to_inherited = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        own_param_type,
        inherited_param_type,
        false,
        false,
        &mut own_to_inherited,
    )
}

pub(crate) fn expand_template_object_union(obj_type: &TUnion) -> TUnion {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
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

    TUnion::from_types(expanded_types)
}
