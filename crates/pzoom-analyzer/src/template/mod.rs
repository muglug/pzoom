//! Template type utilities.
//!
//! pzoom uses Hakana's `TemplateResult` (pzoom-code-info `ttype::template`)
//! everywhere Hakana does: `template_types` carries the declared template
//! parameters (the old "defaults" map), `lower_bounds` the types inferred for
//! them (the old "replacements" map), keyed `[name][GenericParent]` with
//! `Vec<TemplateBound>` values resolved through
//! `get_most_specific_type_from_bounds`.
//!
//! The accessors here are pzoom's equivalents of the open-coded lookups at
//! Hakana call sites (`template_types_contains`, `lower_bounds.get(..)` +
//! `get_most_specific_type_from_bounds`, ...), shared because pzoom's
//! Psalm-shaped inference consults them from many more places.

pub mod inferred_type_replacer;
pub mod standin_type_replacer;

use std::sync::Arc;

use pzoom_code_info::code_location::CodeLocation;
use pzoom_code_info::ttype::template::get_most_specific_type_from_bounds;
use pzoom_code_info::{GenericParent, TUnion, TemplateBound, TemplateResult, TypeVariableBounds};
use pzoom_str::StrId;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// The location a type-variable bound was recorded at.
pub(crate) fn bound_location(
    analyzer: &StatementsAnalyzer<'_>,
    pos: crate::function_analysis_data::Pos,
) -> CodeLocation {
    let (line, col) = analyzer.get_line_column(pos.0);
    CodeLocation::new(analyzer.file_path, pos.0, pos.1, line, col)
}

/// Resolves any top-level type variables in `union` through their
/// accumulated lower bounds (Hakana's `instance_call_analyzer` receiver
/// pattern, applied wherever a concrete shape is required — mixin name
/// lookup, `@psalm-if-this-is` comparison, array reads). A variable with no
/// recorded lower bounds is kept as-is.
pub(crate) fn resolve_type_variables_in_union(
    union: &TUnion,
    type_variable_bounds: &rustc_hash::FxHashMap<String, TypeVariableBounds>,
) -> TUnion {
    if !union
        .types
        .iter()
        .any(|atomic| matches!(atomic, pzoom_code_info::TAtomic::TTypeVariable { .. }))
    {
        return union.clone();
    }

    let mut resolved_types = Vec::with_capacity(union.types.len());
    for atomic in &union.types {
        if let pzoom_code_info::TAtomic::TTypeVariable { name } = atomic
            && let Some(bounds) = type_variable_bounds.get(name)
            && !bounds.lower_bounds.is_empty()
        {
            for lower_bound_info in &bounds.lower_bounds {
                for bound_atomic in &lower_bound_info.bound_type.types {
                    if !resolved_types.contains(bound_atomic) {
                        resolved_types.push(bound_atomic.clone());
                    }
                }
            }
        } else if !resolved_types.contains(atomic) {
            resolved_types.push(atomic.clone());
        }
    }

    let mut resolved = TUnion::from_types(resolved_types);
    resolved.from_docblock = union.from_docblock;
    resolved.possibly_undefined = union.possibly_undefined;
    resolved.parent_nodes = union.parent_nodes.clone();
    resolved
}

/// [`resolve_type_variables_in_union`], recursing into generic type params,
/// array element types, and callable signatures — for template inference,
/// where a type-variable-laden argument (`ArrayIterator<`_0, `_1>`) must
/// contribute its accumulated bounds rather than `mixed`.
pub(crate) fn resolve_type_variables_in_union_deep(
    union: &TUnion,
    type_variable_bounds: &rustc_hash::FxHashMap<String, TypeVariableBounds>,
) -> TUnion {
    let resolved = resolve_type_variables_in_union(union, type_variable_bounds);

    let mut resolved_types = Vec::with_capacity(resolved.types.len());
    for atomic in &resolved.types {
        resolved_types.push(resolve_type_variables_in_atomic_deep(
            atomic,
            type_variable_bounds,
        ));
    }

    let mut deep_resolved = resolved.clone();
    deep_resolved.types = resolved_types;
    deep_resolved
}

fn resolve_type_variables_in_atomic_deep(
    atomic: &pzoom_code_info::TAtomic,
    type_variable_bounds: &rustc_hash::FxHashMap<String, TypeVariableBounds>,
) -> pzoom_code_info::TAtomic {
    use pzoom_code_info::TAtomic;

    match atomic {
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            is_static,
            remapped_params,
        } => TAtomic::TNamedObject {
            name: *name,
            type_params: Some(
                type_params
                    .iter()
                    .map(|param| resolve_type_variables_in_union_deep(param, type_variable_bounds))
                    .collect(),
            ),
            is_static: *is_static,
            remapped_params: *remapped_params,
        },
        TAtomic::TIterable {
            key_type,
            value_type,
        } => TAtomic::TIterable {
            key_type: Box::new(resolve_type_variables_in_union_deep(
                key_type,
                type_variable_bounds,
            )),
            value_type: Box::new(resolve_type_variables_in_union_deep(
                value_type,
                type_variable_bounds,
            )),
        },
        // The unified array atomic: deep-resolve type variables in every
        // known-entry value and the typed fallback `params`, preserving the
        // flags and each entry's possibly-undefined bool. A direct struct
        // literal avoids re-normalising the flags (which would drop
        // `is_nonempty` from a generic `non-empty-array<K, V>`).
        TAtomic::TArray {
            known_values,
            params,
            is_list,
            is_nonempty,
            is_sealed,
        } => {
            let mut new_known_values = rustc_hash::FxHashMap::default();
            for (key, (possibly_undefined, value)) in known_values.iter() {
                new_known_values.insert(
                    key.clone(),
                    (
                        *possibly_undefined,
                        resolve_type_variables_in_union_deep(value, type_variable_bounds),
                    ),
                );
            }
            TAtomic::TArray {
                known_values: std::sync::Arc::new(new_known_values),
                params: params.as_ref().map(|params| {
                    Box::new((
                        resolve_type_variables_in_union_deep(&params.0, type_variable_bounds),
                        resolve_type_variables_in_union_deep(&params.1, type_variable_bounds),
                    ))
                }),
                is_list: *is_list,
                is_nonempty: *is_nonempty,
                is_sealed: *is_sealed,
            }
        }
        _ => atomic.clone(),
    }
}

/// Hakana's bound-transfer tail (`argument_analyzer::verify_type`,
/// `instance_property_assignment_analyzer`): moves the bounds a comparison
/// recorded into `analysis_data.type_variable_bounds`, stamping each with the
/// expression position. Bounds for unknown variables are silently dropped
/// (`get_mut` guard).
pub(crate) fn record_type_variable_bounds(
    analysis_data: &mut FunctionAnalysisData,
    lower: Vec<(String, TemplateBound)>,
    upper: Vec<(String, TemplateBound)>,
    pos: Option<CodeLocation>,
) {
    for (name, mut bound) in lower {
        if let Some(TypeVariableBounds { lower_bounds, .. }) =
            analysis_data.type_variable_bounds.get_mut(&name)
        {
            bound.pos = pos;
            lower_bounds.push(bound);
        }
    }

    for (name, mut bound) in upper {
        if let Some(TypeVariableBounds { upper_bounds, .. }) =
            analysis_data.type_variable_bounds.get_mut(&name)
        {
            bound.pos = pos;
            upper_bounds.push(bound);
        }
    }
}

/// Hakana's `template_types_contains` (standin_type_replacer), returning the
/// mapped type: the declared template type for `[name][defining_entity]`.
pub(crate) fn template_types_get(
    template_result: &TemplateResult,
    name: StrId,
    defining_entity: GenericParent,
) -> Option<&Arc<TUnion>> {
    template_result
        .template_types
        .get(&name)
        .and_then(|entries| {
            entries
                .iter()
                .find(|(entity, _)| *entity == defining_entity)
                .map(|(_, mapped_type)| mapped_type)
        })
}

/// Inserts (or overwrites) the declared template type for
/// `[name][defining_entity]`.
pub(crate) fn template_types_insert(
    template_result: &mut TemplateResult,
    name: StrId,
    defining_entity: GenericParent,
    union: TUnion,
) {
    let entries = template_result.template_types.entry(name).or_default();
    if let Some(entry) = entries
        .iter_mut()
        .find(|(entity, _)| *entity == defining_entity)
    {
        entry.1 = Arc::new(union);
    } else {
        entries.push((defining_entity, Arc::new(union)));
    }
}

/// Whether any entity declares `name` in `template_types`.
pub(crate) fn template_types_contains_name(template_result: &TemplateResult, name: StrId) -> bool {
    template_result.template_types.contains_key(&name)
}

/// Name-only `template_types` lookup for call sites with no defining entity in
/// scope: the sole entry for `name`, or `None` when absent or ambiguous.
pub(crate) fn template_types_get_by_name(
    template_result: &TemplateResult,
    name: StrId,
) -> Option<&Arc<TUnion>> {
    let entries = template_result.template_types.get(&name)?;
    if let [(_, mapped_type)] = entries.as_slice() {
        Some(mapped_type)
    } else {
        None
    }
}

/// The sole entity declaring `name` in `template_types`, or `None` when absent
/// or ambiguous.
pub(crate) fn template_types_entity_for_name(
    template_result: &TemplateResult,
    name: StrId,
) -> Option<GenericParent> {
    let entries = template_result.template_types.get(&name)?;
    if let [(entity, _)] = entries.as_slice() {
        Some(*entity)
    } else {
        None
    }
}

/// The inferred type for `[name][defining_entity]`: the lower bounds resolved
/// through Hakana's `get_most_specific_type_from_bounds`.
pub(crate) fn lower_bounds_get(
    template_result: &TemplateResult,
    name: StrId,
    defining_entity: GenericParent,
) -> Option<TUnion> {
    template_result
        .lower_bounds
        .get(&name)
        .and_then(|entities| entities.get(&defining_entity))
        .map(|bounds| get_most_specific_type_from_bounds(bounds))
}

/// Name-only inferred-type lookup: resolves the sole entity's bounds for
/// `name`, or `None` when the name is absent or ambiguous.
pub(crate) fn lower_bounds_get_by_name(
    template_result: &TemplateResult,
    name: StrId,
) -> Option<TUnion> {
    let entities = template_result.lower_bounds.get(&name)?;
    if entities.len() == 1 {
        entities
            .values()
            .next()
            .map(|bounds| get_most_specific_type_from_bounds(bounds))
    } else {
        None
    }
}

/// Whether any entity has inferred bounds for `name`.
pub(crate) fn lower_bounds_contains_name(template_result: &TemplateResult, name: StrId) -> bool {
    template_result.lower_bounds.contains_key(&name)
}

/// Replaces any existing bounds for `[name][defining_entity]` with the single
/// depth-0 bound `union` (the old overwrite-`insert`).
pub(crate) fn lower_bounds_insert(
    template_result: &mut TemplateResult,
    name: StrId,
    defining_entity: GenericParent,
    union: TUnion,
) {
    template_result
        .lower_bounds
        .entry(name)
        .or_default()
        .insert(
            defining_entity,
            vec![TemplateBound::new(union, 0, None, None)],
        );
}

/// Pushes a depth-0 bound for `[name][defining_entity]`; multiple bounds union
/// together at resolution (the old combining `insert_combined`).
pub(crate) fn lower_bounds_insert_combined(
    template_result: &mut TemplateResult,
    name: StrId,
    defining_entity: GenericParent,
    union: TUnion,
) {
    let depth = template_result.bound_insertion_depth;
    template_result
        .lower_bounds
        .entry(name)
        .or_default()
        .entry(defining_entity)
        .or_default()
        .push(TemplateBound::new(union, depth, None, None));
}

/// Overlays `incoming`'s lower bounds onto `template_result`, overwriting per
/// `(name, entity)` key (the old `extend_overlay`).
pub(crate) fn lower_bounds_extend_overlay(
    template_result: &mut TemplateResult,
    incoming: TemplateResult,
) {
    for (name, entities) in incoming.lower_bounds {
        for (entity, bounds) in entities {
            template_result
                .lower_bounds
                .entry(name)
                .or_default()
                .insert(entity, bounds);
        }
    }
}

/// Iterates `(name, defining_entity, resolved_type)` over the lower bounds in
/// insertion order.
pub(crate) fn lower_bounds_iter(
    template_result: &TemplateResult,
) -> impl Iterator<Item = (StrId, GenericParent, TUnion)> + '_ {
    template_result
        .lower_bounds
        .iter()
        .flat_map(|(name, entities)| {
            entities.iter().map(move |(entity, bounds)| {
                (*name, *entity, get_most_specific_type_from_bounds(bounds))
            })
        })
}

/// Whether the result carries neither declared template types nor inferred
/// bounds.
pub(crate) fn template_result_is_empty(template_result: &TemplateResult) -> bool {
    template_result.template_types.is_empty() && template_result.lower_bounds.is_empty()
}

/// Psalm ArgumentAnalyzer's bindable-template fallback: every template
/// referenced by a provided argument's parameter type that finished argument
/// processing without a lower bound binds to its upper bound — or, failing
/// that, its declared constraint. A coerced argument (a plain `string`
/// passed to `class-string<T as C>`) thus still resolves `T` to `C` instead
/// of leaving it unbound, which the return-type fetcher would collapse to
/// `never`.
pub(crate) fn bind_unbound_param_templates_to_constraints(
    param_type: &TUnion,
    template_result: &mut TemplateResult,
) {
    let mut referenced: Vec<(StrId, GenericParent, TUnion)> = Vec::new();
    for atomic in &param_type.types {
        collect_template_params_in_atomic(atomic, &mut referenced, 0);
    }

    for (name, defining_entity, as_type) in referenced {
        // Only templates this call's result declares (Psalm's param types only
        // mention fn-level templates here; class templates were substituted
        // away by the readonly class-generic replacement).
        if template_types_get(template_result, name, defining_entity).is_none() {
            continue;
        }
        if lower_bounds_get(template_result, name, defining_entity).is_some() {
            continue;
        }

        let bound = template_result
            .upper_bounds
            .get(&name)
            .and_then(|entities| entities.get(&defining_entity))
            .map(|upper_bound| upper_bound.bound_type.clone())
            .unwrap_or(as_type);
        lower_bounds_insert(template_result, name, defining_entity, bound);
    }
}

/// Collects `(name, defining_entity, constraint)` for every template param
/// nested anywhere in `atomic` (Psalm's `Union::getTemplateTypes`, which
/// visits the whole type tree).
fn collect_template_params_in_atomic(
    atomic: &pzoom_code_info::TAtomic,
    referenced: &mut Vec<(StrId, GenericParent, TUnion)>,
    depth: usize,
) {
    use pzoom_code_info::TAtomic;

    if depth > 8 {
        return;
    }

    let recurse_union = |union: &TUnion, referenced: &mut Vec<(StrId, GenericParent, TUnion)>| {
        for nested in &union.types {
            collect_template_params_in_atomic(nested, referenced, depth + 1);
        }
    };

    match atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            referenced.push((*name, *defining_entity, (**as_type).clone()));
        }
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            referenced.push((*name, *defining_entity, TUnion::new((**as_type).clone())));
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            collect_template_params_in_atomic(as_type, referenced, depth + 1);
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => {
            for type_param in type_params {
                recurse_union(type_param, referenced);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                collect_template_params_in_atomic(nested, referenced, depth + 1);
            }
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            recurse_union(key_type, referenced);
            recurse_union(value_type, referenced);
        }
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            for (_possibly_undefined, value_type) in known_values.values() {
                recurse_union(value_type, referenced);
            }
            if let Some(params) = params {
                recurse_union(&params.0, referenced);
                recurse_union(&params.1, referenced);
            }
        }
        TAtomic::TClosure {
            params,
            return_type,
            ..
        }
        | TAtomic::TCallable {
            params,
            return_type,
            ..
        } => {
            if let Some(params) = params {
                for param in params {
                    recurse_union(&param.param_type, referenced);
                }
            }
            if let Some(return_type) = return_type {
                recurse_union(return_type, referenced);
            }
        }
        _ => {}
    }
}
