//! Type expansion, modeled on Psalm's `Internal\Type\TypeExpander` (and hakana-core's
//! `ttype::type_expander`).
//!
//! Expands a stored type for use as a concrete type at a use site:
//! - resolves `self` / `static` / `$this` to the analyzed class
//!   (`TypeExpansionOptions::self_class` / `static_class_type`),
//! - collapses a `TConditional` (not being resolved against concrete call arguments)
//!   to the union of its branches (Psalm's `expandConditional`),
//! - expands `properties-of<C>` to a shape,
//! - recurses into arrays, lists, iterables, named-object type params, closure
//!   param/return types, template-param `as` bounds, and object intersections.

use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::t_atomic::{ArrayKey, ConditionalReturnCondition, PropertiesOfVisibility};
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion, combine_union_types};
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

/// The late-static-bound (`static` / `$this`) class at the expansion site.
/// Mirrors Psalm's `$static_class_type` argument / hakana's `StaticClassType`.
#[derive(Debug, Clone, Default)]
pub enum StaticClassType {
    /// No late-static context — leave `static`/`$this` unresolved.
    #[default]
    None,
    /// Resolve to this concrete class name.
    Name(StrId),
    /// Resolve to this concrete object atomic (e.g. an intersection).
    Object(TAtomic),
}

/// Options controlling type expansion. Field names mirror Psalm's
/// `TypeExpander::expandUnion` parameters.
#[derive(Debug, Default)]
pub struct TypeExpansionOptions {
    /// The class `self`/`parent` resolve against (the declaring class).
    pub self_class: Option<StrId>,
    /// The late-static-bound class `static`/`$this` resolve against.
    pub static_class_type: StaticClassType,
    /// The parent class `parent` resolves against.
    pub parent_class: Option<StrId>,
    /// When the enclosing function is `final`, `static` collapses to a non-late
    /// `self` (it can no longer be a subclass).
    pub function_is_final: bool,
    /// Whether a `TConditional` should be collapsed to the union of its branches.
    /// Matches Hakana's `evaluate_conditional_types` (default `false`); only the
    /// concrete-return-type sites enable it. Localization leaves conditionals intact.
    pub evaluate_conditional_types: bool,
}

/// Expand a union in place, collapsing conditional types and recursing into nested
/// type parameters. Mirrors Psalm's `expandUnion` / hakana-core's `expand_union`.
pub fn expand_union(
    codebase: &CodebaseInfo,
    interner: &Interner,
    return_type: &mut TUnion,
    options: &TypeExpansionOptions,
) {
    let original_types = std::mem::take(&mut return_type.types);
    let mut new_atomic_types = Vec::with_capacity(original_types.len());

    for mut atomic in original_types {
        let mut skip_this_atomic = false;
        let mut replacements = Vec::new();

        expand_atomic(
            &mut atomic,
            codebase,
            interner,
            options,
            &mut skip_this_atomic,
            &mut replacements,
        );

        if skip_this_atomic {
            new_atomic_types.extend(replacements);
        } else {
            new_atomic_types.push(atomic);
        }
    }

    // The branch atomics introduced by collapsing a conditional (or resolving
    // `static` to an object) can change nullability/falsability, so refresh the
    // cached flags.
    return_type.is_nullable = new_atomic_types.iter().any(|atomic| atomic.is_nullable());
    return_type.is_falsable = new_atomic_types.iter().any(|atomic| atomic.is_falsable());
    return_type.types = new_atomic_types;
}

/// Expand a single atomic in place. If the atomic should be replaced, `skip` is set
/// and the replacement atomics are pushed into `replacements`; otherwise the atomic
/// is mutated in place. Mirrors Psalm's `expandAtomic` / hakana-core's `expand_atomic`.
fn expand_atomic(
    return_type_part: &mut TAtomic,
    codebase: &CodebaseInfo,
    interner: &Interner,
    options: &TypeExpansionOptions,
    skip: &mut bool,
    replacements: &mut Vec<TAtomic>,
) {
    match return_type_part {
        TAtomic::TConditional(conditional) => {
            if options.evaluate_conditional_types {
                let mut if_true = conditional.if_true_type.clone();
                expand_union(codebase, interner, &mut if_true, options);
                let mut if_false = conditional.if_false_type.clone();
                expand_union(codebase, interner, &mut if_false, options);

                *skip = true;
                replacements.extend(combine_union_types(&if_true, &if_false, false).types);
            } else {
                // Leave the conditional intact (e.g. when localizing a callable's
                // return type); expand its branches and the tested (asserted) type
                // in place (Psalm expands the conditional's type too).
                match &mut conditional.condition {
                    ConditionalReturnCondition::TemplateIs { asserted_type, .. }
                    | ConditionalReturnCondition::ParamIs { asserted_type, .. } => {
                        expand_union(codebase, interner, asserted_type, options);
                    }
                    ConditionalReturnCondition::FuncNumArgsIs { .. } => {}
                }
                expand_union(codebase, interner, &mut conditional.if_true_type, options);
                expand_union(codebase, interner, &mut conditional.if_false_type, options);
            }
        }
        TAtomic::TArray { key_type, value_type }
        | TAtomic::TNonEmptyArray { key_type, value_type }
        | TAtomic::TIterable { key_type, value_type } => {
            expand_union(codebase, interner, key_type, options);
            expand_union(codebase, interner, value_type, options);
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            expand_union(codebase, interner, value_type, options);
        }
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static,
            ..
        } => {
            // Resolve `self`/`static`/`$this`/`parent` to the call site's class. Only
            // active when a `self_class` context is supplied. This is the single
            // TypeExpander mechanism Psalm/Hakana use; pzoom's old
            // `localize_special_class_type_*` is now a thin wrapper over `expand_union`.
            if let Some(replacement) = localize_class_name(name, is_static, options) {
                *skip = true;
                replacements.push(replacement);
                return;
            }

            if let Some(type_params) = type_params {
                for type_param in type_params.iter_mut() {
                    expand_union(codebase, interner, type_param, options);
                }
            }
        }
        TAtomic::TObjectIntersection { types } => {
            let mut new_types: Vec<TAtomic> = Vec::with_capacity(types.len());
            for mut member in std::mem::take(types) {
                if let TAtomic::TNamedObject {
                    name,
                    type_params,
                    is_static,
                    ..
                } = &mut member
                {
                    // Resolve `self`/`static` to the call-site class; when `static`
                    // resolves to a concrete object/intersection, splice it in
                    // (flattening nested intersections). Matches Psalm.
                    if let Some(replacement) = localize_class_name(name, is_static, options) {
                        match replacement {
                            TAtomic::TObjectIntersection { types: inner } => {
                                new_types.extend(inner)
                            }
                            other => new_types.push(other),
                        }
                        continue;
                    }
                    if let Some(type_params) = type_params {
                        for type_param in type_params.iter_mut() {
                            expand_union(codebase, interner, type_param, options);
                        }
                    }
                }
                new_types.push(member);
            }
            *types = new_types;
        }
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => {
            if let Some(return_type) = return_type {
                expand_union(codebase, interner, return_type, options);
            }
            if let Some(params) = params {
                for param in params.iter_mut() {
                    expand_union(codebase, interner, &mut param.param_type, options);
                }
            }
        }
        TAtomic::TTemplateParam { as_type, .. }
        | TAtomic::TTemplateKeyOf { as_type, .. }
        | TAtomic::TTemplateValueOf { as_type, .. } => {
            expand_union(codebase, interner, as_type, options);
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            // Recurse into shape field types and fallback params (Psalm expands these).
            for value_type in properties.values_mut() {
                expand_union(codebase, interner, value_type, options);
            }
            if let Some(fallback_key) = fallback_key_type {
                expand_union(codebase, interner, fallback_key, options);
            }
            if let Some(fallback_value) = fallback_value_type {
                expand_union(codebase, interner, fallback_value, options);
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            expand_atomic_in_place(as_type, codebase, interner, options);
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            expand_atomic_in_place(as_type, codebase, interner, options);
        }
        TAtomic::TPropertiesOf {
            classlike_name,
            visibility_filter,
        } => {
            // Localize `properties-of<self>` / `<static>` / `<parent>` to the
            // call-site class before resolving its properties (matches Psalm).
            let mut resolved_name = *classlike_name;
            let mut ignored_static = false;
            let _ = localize_class_name(&mut resolved_name, &mut ignored_static, options);
            if let Some(keyed_array) = build_properties_of_keyed_array(
                codebase,
                interner,
                resolved_name,
                *visibility_filter,
            ) {
                *skip = true;
                replacements.push(keyed_array);
            }
        }
        _ => {}
    }
}

/// Resolve a `self`/`static`/`$this`/`parent` class name to the call site's class.
///
/// Only active when `options.self_class` is set (i.e. we are localizing a call's
/// return/parameter type); with no `self_class` the name is left untouched, which is
/// how the plain `expand_union` callers behave. Returns `Some(atomic)` when a late-
/// static `StaticClassType::Object` should replace the whole atomic. This is the exact
/// logic the old `localize_special_class_type_atomic` used.
fn localize_class_name(
    name: &mut StrId,
    is_static: &mut bool,
    options: &TypeExpansionOptions,
) -> Option<TAtomic> {
    let Some(self_class) = options.self_class else {
        return None;
    };

    if *is_static || *name == StrId::STATIC {
        match &options.static_class_type {
            StaticClassType::Object(obj) => return Some(obj.clone()),
            StaticClassType::Name(static_id) => {
                // Re-resolve the late-static type to the call site's class. It stays
                // flagged static (so it remains compatible with other `static` types)
                // unless the enclosing function is `final`, where late-static binding
                // collapses to the concrete class (matches Psalm; also how callable
                // return types capture `self`/`static` at definition).
                *name = if *static_id == StrId::STATIC {
                    if *name == StrId::STATIC {
                        self_class
                    } else {
                        *name
                    }
                } else {
                    *static_id
                };
                *is_static = !options.function_is_final;
            }
            StaticClassType::None => {}
        }
    } else if *name == StrId::SELF {
        *name = self_class;
        *is_static = false;
    } else if *name == StrId::PARENT {
        *name = options.parent_class.unwrap_or(StrId::PARENT);
        *is_static = false;
    }

    None
}

/// Expand a single boxed atomic in place (used for `class<T>` / `class-string` `as`
/// bounds, which hold one atomic rather than a union). If expansion produces a
/// replacement, the first replacement atomic is substituted.
fn expand_atomic_in_place(
    atomic: &mut TAtomic,
    codebase: &CodebaseInfo,
    interner: &Interner,
    options: &TypeExpansionOptions,
) {
    let mut skip = false;
    let mut replacements = Vec::new();
    expand_atomic(atomic, codebase, interner, options, &mut skip, &mut replacements);
    if skip {
        if let Some(first) = replacements.into_iter().next() {
            *atomic = first;
        }
    }
}

/// Build the keyed array `properties-of<C>` expands to: each (visible) property of `C`
/// becomes a shape field `name => type`. Mirrors Psalm's `TPropertiesOf` expansion.
fn build_properties_of_keyed_array(
    codebase: &CodebaseInfo,
    interner: &Interner,
    classlike_name: pzoom_str::StrId,
    visibility_filter: PropertiesOfVisibility,
) -> Option<TAtomic> {
    let class_info = codebase.get_class(classlike_name)?;

    let mut properties: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
    for (property_name, property_info) in &class_info.properties {
        if property_info.is_static {
            continue;
        }
        if !visibility_matches(visibility_filter, property_info.visibility) {
            continue;
        }
        let mut property_type = property_info
            .get_type()
            .cloned()
            .unwrap_or_else(TUnion::mixed);
        expand_union(
            codebase,
            interner,
            &mut property_type,
            &TypeExpansionOptions::default(),
        );
        let key = ArrayKey::String(interner.lookup(*property_name).to_string());
        properties.insert(key, property_type);
    }

    if properties.is_empty() {
        return None;
    }

    // Psalm: the shape is sealed only when the class and every existing
    // ancestor is `final` (no subclass can add properties); otherwise it
    // carries a `string => mixed` fallback for properties a subclass may add
    // (TypeExpander::expandPropertiesOf). Missing ancestors are skipped, not
    // treated as non-final.
    let all_sealed = class_info.is_final
        && class_info
            .all_parent_classes
            .iter()
            .all(|ancestor| codebase.get_class(*ancestor).map_or(true, |info| info.is_final));

    let (fallback_key_type, fallback_value_type) = if all_sealed {
        (None, None)
    } else {
        (
            Some(Box::new(TUnion::string())),
            Some(Box::new(TUnion::mixed())),
        )
    };

    Some(TAtomic::TKeyedArray {
        properties,
        is_list: false,
        sealed: all_sealed,
        fallback_key_type,
        fallback_value_type,
    })
}

fn visibility_matches(filter: PropertiesOfVisibility, visibility: Visibility) -> bool {
    match filter {
        PropertiesOfVisibility::All => true,
        PropertiesOfVisibility::Public => visibility == Visibility::Public,
        PropertiesOfVisibility::Protected => visibility == Visibility::Protected,
        PropertiesOfVisibility::Private => visibility == Visibility::Private,
    }
}

/// Re-resolve `self`/`static`/`$this`/`parent` (and recurse through generics,
/// callables, closures, template params, class-strings, and object intersections)
/// to a call site's class. Thin adapter over [`expand_union`] — the single
/// TypeExpander mechanism — kept for the method/static-call analyzers' call sites.
pub(crate) fn localize_special_class_type_union(
    codebase: &CodebaseInfo,
    interner: &Interner,
    union: &TUnion,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = union.clone();
    expand_union(
        codebase,
        interner,
        &mut localized,
        &TypeExpansionOptions {
            self_class: Some(self_class_id),
            static_class_type: StaticClassType::Name(static_class_id),
            parent_class: parent_class_id,
            function_is_final: false,
            // Localization must not collapse conditionals (e.g. inside a callable's
            // return type) — they are evaluated against call arguments elsewhere.
            evaluate_conditional_types: false,
        },
    );
    localized
}
