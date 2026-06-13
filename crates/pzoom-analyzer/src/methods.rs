//! Port of the return-type resolution portion of Psalm's
//! `Internal/Codebase/Methods.php` — primarily `Methods::getMethodReturnType`.
//!
//! Psalm resolves a method's return type lazily (never baked into storage) by
//! consulting `ClassLikeStorage::$documenting_method_ids` (the ancestor whose
//! docblock documents the type) and, failing that, walking
//! `overridden_method_ids`. The pieces live here so both call sites (the
//! method-call / static-call return-type fetchers) and class analysis (the
//! method body's effective declared type) share one faithful implementation.
//!
//! pzoom keeps the method's own *docblock* return type (`return_type`) separate
//! from its native *signature* type (`signature_return_type`), whereas Psalm
//! unifies them into `$storage->return_type`. The helpers below bridge that
//! difference explicitly where Psalm reads `$storage->return_type`.

use pzoom_code_info::{ClassLikeInfo, FunctionLikeInfo, GenericParent, TUnion, TemplateResult};
use pzoom_str::StrId;
use rustc_hash::FxHashMap;

use crate::expr::call::function_call_analyzer;
use crate::expr::call::missing_method_call_handler::get_method_info;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::class_analyzer::replace_extended_templates_in_union;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// `Methods::getMethodReturnType` for a call site: the method's own return type
/// (`candidate`, resolved against any call arguments) reconciled with the type
/// documented by an ancestor (`documenting_method_ids`). When both are present
/// the more specific wins; Psalm intersects on a mutual containment, but pzoom
/// has no union intersection and falls back to the candidate (as Psalm
/// tolerates a failed intersection).
pub(crate) fn get_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    method_info: &FunctionLikeInfo,
    template_result: &TemplateResult,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> TUnion {
    let own_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        method_info,
        template_result,
        param_arg_types,
        arg_count,
    );

    let Some(documented_type) = get_inherited_method_return_type(
        analyzer,
        class_id,
        method_name,
        template_result,
        param_arg_types,
        arg_count,
    ) else {
        return own_return_type.unwrap_or_else(TUnion::mixed);
    };

    // Psalm's `Methods::getMethodReturnType` uses `$storage->return_type` (the
    // docblock return type) as the candidate: a method that only carries a
    // native signature hint has no candidate and defers to the documented type.
    let candidate_type = method_info
        .return_type
        .is_some()
        .then_some(own_return_type)
        .flatten();

    // Psalm special case: a method documented by `Iterator` keeps its own
    // native return type (when it has one matching its signature) so Iterator
    // types stay inferable.
    if documenting_method_id_class(analyzer, class_id, method_name) == Some(StrId::ITERATOR)
        && let (Some(return_type), Some(signature_return_type)) = (
            method_info.return_type.as_ref(),
            method_info.signature_return_type.as_ref(),
        )
        && return_type.get_id(None) == signature_return_type.get_id(None)
        && let Some(candidate_type) = candidate_type
    {
        return candidate_type;
    }

    let Some(candidate_type) = candidate_type else {
        return documented_type;
    };

    if candidate_type.get_id(Some(analyzer.interner))
        == documented_type.get_id(Some(analyzer.interner))
    {
        return candidate_type;
    }

    let mut candidate_in_documented = TypeComparisonResult::new();
    let old_contained_by_new = union_type_comparator::is_contained_by(
        analyzer.codebase,
        &candidate_type,
        &documented_type,
        false,
        false,
        &mut candidate_in_documented,
    );

    let mut documented_in_candidate = TypeComparisonResult::new();
    let new_contained_by_old = union_type_comparator::is_contained_by(
        analyzer.codebase,
        &documented_type,
        &candidate_type,
        false,
        false,
        &mut documented_in_candidate,
    );

    if (!old_contained_by_new && !new_contained_by_old)
        || (old_contained_by_new && new_contained_by_old)
    {
        return candidate_type;
    }

    if old_contained_by_new {
        candidate_type
    } else {
        documented_type
    }
}

/// The class of the `documenting_method_ids` entry for `method_name` on
/// `class_id`, if any (mirrors looking up
/// `ClassLikeStorage::$documenting_method_ids[$method_name]->fq_class_name`).
fn documenting_method_id_class(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> Option<StrId> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let method_name_id = analyzer.interner.intern(method_name);
    class_info
        .documenting_method_ids
        .get(&method_name_id)
        .map(|documenting_method_id| documenting_method_id.0)
}

/// Resolves the return type documented by the ancestor recorded in
/// `ClassLikeStorage::$documenting_method_ids` (mirrors the documenting branch
/// of Psalm's `Methods::getMethodReturnType`). Returns `None` when the method
/// has no documenting ancestor, so the caller falls back to the method's own
/// return type.
pub(crate) fn get_inherited_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    template_result: &TemplateResult,
    param_arg_types: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;

    let method_name_id = analyzer.interner.intern(method_name);
    let documenting_method_id = class_info.documenting_method_ids.get(&method_name_id)?;
    let candidate_class_id = documenting_method_id.0;

    let candidate_class_info = analyzer.codebase.get_class(candidate_class_id)?;
    let candidate_method_info = get_method_info(analyzer, candidate_class_info, method_name)?;

    candidate_method_info.get_return_type()?;

    // Psalm: a documenting `@return null` denotes a void method.
    if candidate_method_info
        .return_type
        .as_ref()
        .is_some_and(|return_type| return_type.is_null())
    {
        return Some(TUnion::void());
    }

    let mut candidate_result = template_result.clone();
    for (template_name, entries) in
        function_call_analyzer::get_class_template_defaults(candidate_class_info).template_types
    {
        candidate_result.template_types.entry(template_name).or_insert(entries);
    }
    for (template_name, entries) in
        function_call_analyzer::get_template_defaults(candidate_method_info).template_types
    {
        candidate_result.template_types.entry(template_name).or_insert(entries);
    }

    if let Some(candidate_template_map) =
        class_info.template_extended_params.get(&candidate_class_id)
    {
        for (template_name, mapped_type) in candidate_template_map {
            let resolved_mapped_type =
                function_call_analyzer::replace_templates_in_union(mapped_type, &candidate_result);
            crate::template::lower_bounds_insert(
                &mut candidate_result,
                *template_name,
                GenericParent::ClassLike(candidate_class_id),
                resolved_mapped_type,
            );
        }
    }

    let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        candidate_method_info,
        &candidate_result,
        param_arg_types,
        arg_count,
    )
    .unwrap_or_else(TUnion::mixed);

    Some(resolved_return_type)
}

/// Reconcile a documenting ancestor's return type against the overriding
/// method's own native signature, mirroring the documenting branch of Psalm's
/// `Methods::getMethodReturnType`: the method's own type is the `candidate`, the
/// ancestor's documented type the `overridden`. When the method declares no
/// native signature the documented type is used as-is; otherwise the more
/// specific of the two wins, and a conflict (neither or mutually contained)
/// keeps the method's own type (Psalm intersects, but falls back to the
/// candidate when that is not possible).
pub(crate) fn reconcile_documented_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    documented: TUnion,
    signature_candidate: Option<&TUnion>,
) -> TUnion {
    let Some(candidate) = signature_candidate else {
        return documented;
    };

    let expand = |union: &TUnion| {
        let mut expanded = union.clone();
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            &mut expanded,
            &crate::type_expander::TypeExpansionOptions {
                self_class: Some(class_info.name),
                static_class_type: crate::type_expander::StaticClassType::Name(class_info.name),
                evaluate_conditional_types: true,
                ..Default::default()
            },
        );
        expanded
    };

    let candidate_expanded = expand(candidate);
    let documented_expanded = expand(&documented);

    if candidate_expanded.get_id(Some(analyzer.interner))
        == documented_expanded.get_id(Some(analyzer.interner))
    {
        return candidate.clone();
    }

    let mut candidate_in_documented = TypeComparisonResult::new();
    let old_contained_by_new = union_type_comparator::is_contained_by(
        analyzer.codebase,
        &candidate_expanded,
        &documented_expanded,
        false,
        false,
        &mut candidate_in_documented,
    );

    let mut documented_in_candidate = TypeComparisonResult::new();
    let new_contained_by_old = union_type_comparator::is_contained_by(
        analyzer.codebase,
        &documented_expanded,
        &candidate_expanded,
        false,
        false,
        &mut documented_in_candidate,
    );

    if old_contained_by_new && !new_contained_by_old {
        // The method's own type is the more specific — keep it.
        candidate.clone()
    } else if new_contained_by_old && !old_contained_by_new {
        // The documented type is the more specific — adopt it.
        documented
    } else {
        // Conflict (neither, or mutually contained) — keep the method's own.
        candidate.clone()
    }
}

/// Psalm's `Methods::getMethodReturnType` `overridden_method_ids` fallback: when
/// a method declares no return type of its own, walk its ancestors (used
/// traits, parent class, then interfaces) for the inherited return type,
/// localizing ancestor templates through this class's `@template-extends` /
/// `@template-implements` bindings.
pub(crate) fn get_specialized_inherited_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    method_name: StrId,
) -> Option<TUnion> {
    // Trait methods are flattened into the class, so a used trait is the
    // closest documenting source for an override (resolved via `@use T<...>`).
    for trait_name in &class_info.used_traits {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *trait_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    if let Some(parent_class) = class_info.parent_class
        && let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, parent_class, method_name)
    {
        return Some(replace_extended_templates_in_union(
            &inherited_type,
            &class_info.template_extended_params,
        ));
    }

    for interface_name in &class_info.interfaces {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *interface_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    for interface_name in &class_info.all_parent_interfaces {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *interface_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    None
}

/// The return type declared by `method_name` on `classlike_name` — the docblock
/// type if present, else the native signature type (Psalm's
/// `$storage->return_type`). A `static::CONST` return defers to the signature
/// type, since the inheritor's own constant may differ.
fn get_return_type_from_classlike(
    analyzer: &StatementsAnalyzer<'_>,
    classlike_name: StrId,
    method_name: StrId,
) -> Option<TUnion> {
    let class_storage = analyzer.codebase.get_class(classlike_name)?;
    let method_storage = class_storage.methods.get(&method_name)?;

    if method_storage.return_type_mentions_static_const {
        return method_storage.signature_return_type.clone();
    }

    method_storage
        .return_type
        .clone()
        .or_else(|| method_storage.signature_return_type.clone())
}
