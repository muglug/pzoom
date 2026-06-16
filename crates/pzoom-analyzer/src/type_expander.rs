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
use pzoom_code_info::t_atomic::{ArrayKey, PropertiesOfVisibility};
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
                // return type); expand its parts in place (Psalm expands the
                // conditional's type too).
                expand_union(codebase, interner, &mut conditional.as_type, options);
                expand_union(
                    codebase,
                    interner,
                    &mut conditional.conditional_type,
                    options,
                );
                expand_union(codebase, interner, &mut conditional.if_true_type, options);
                expand_union(codebase, interner, &mut conditional.if_false_type, options);
            }
        }
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            expand_union(codebase, interner, key_type, options);
            expand_union(codebase, interner, value_type, options);
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            expand_union(codebase, interner, value_type, options);
        }
        TAtomic::TClassStringMap {
            as_type,
            value_param,
            ..
        } => {
            if let Some(as_type) = as_type {
                expand_atomic_in_place(as_type, codebase, interner, options);
            }
            expand_union(codebase, interner, value_param, options);
        }
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static,
            ..
        } => {
            // A docblock class-constant reference kept as a `Class::CONST` /
            // `Class::PREFIX_*` token (Psalm's TClassConstant; Hakana's
            // TMemberReference) resolves to the constant types once the
            // populated codebase is available — the scan-time expansion only
            // covers same-file constants.
            if type_params.is_none()
                && let Some(constant_types) =
                    resolve_class_constant_token(*name, codebase, interner, options)
            {
                *skip = true;
                replacements.extend(constant_types);
                return;
            }

            // A deferred `key-of<Class::CONST>` / `value-of<Class::CONST>`
            // sentinel resolves to the keys/values of the constant's type.
            if type_params.is_none()
                && let Some((is_key_of, inner)) = split_key_value_of_sentinel(
                    &interner.lookup(*name),
                )
                // Psalm's TypeExpander replaces only `self` inside
                // key-of/value-of; `static::` stays unresolved (and is
                // reported as UnresolvableConstant in declarations).
                && !inner.trim_start().get(..8).is_some_and(|prefix| prefix.eq_ignore_ascii_case("static::"))
                && !inner.trim_start().starts_with("$this::")
                && let Some(constant_types) = resolve_class_constant_token(
                    interner.intern(inner),
                    codebase,
                    interner,
                    options,
                )
            {
                let constant_union = TUnion::from_types(constant_types);
                let resolved = if is_key_of {
                    pzoom_code_info::ttype::key_value_of::get_key_of_union(&constant_union)
                } else {
                    pzoom_code_info::ttype::key_value_of::get_value_of_union(&constant_union)
                };
                *skip = true;
                replacements.extend(resolved.types);
                return;
            }

            // Resolve `self`/`static`/`$this`/`parent` to the call site's class. Only
            // active when a `self_class` context is supplied. This is the single
            // TypeExpander mechanism Psalm/Hakana use; pzoom's old
            // `localize_special_class_type_*` is now a thin wrapper over `expand_union`.
            if let Some(replacement) = localize_class_name(name, is_static, options, codebase) {
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
                    if let Some(replacement) =
                        localize_class_name(name, is_static, options, codebase)
                    {
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
            // Recurse into shape field types and fallback params (Psalm expands
            // these). Avoid copy-on-write of the (possibly shared) properties
            // map — and per-entry union clones — when no entry can be changed
            // by expansion (Hakana's shapes-to-copy-on-write optimisation,
            // slackhq/hakana@8f9f1a4).
            let needs_expansion = properties
                .values()
                .any(|value_type| union_needs_expansion(value_type, options));
            if needs_expansion {
                for value_type in std::sync::Arc::make_mut(properties).values_mut() {
                    if union_needs_expansion(value_type, options) {
                        expand_union(codebase, interner, value_type, options);
                    }
                }
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
            let _ = localize_class_name(&mut resolved_name, &mut ignored_static, options, codebase);
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

/// Resolve a docblock class-constant reference that survived scan-time
/// expansion as a `Class::CONST` / `Class::PREFIX_*` token-named object.
///
/// Mirrors Psalm's `TypeExpander` `TClassConstant` arm: `self`/`static`/
/// `parent` resolve against the expansion context, the constants come from
/// the populated codebase, and a `*` in the constant name matches like
/// `StorageByPatternResolver::resolveConstants` (every `*` is `.*?`). Returns
/// `None` (atomic left untouched) when the token is not a constant reference,
/// the class is unknown, or nothing matches — analysis reports
/// UndefinedDocblockClass / UndefinedConstant for those as before.
fn resolve_class_constant_token(
    name: StrId,
    codebase: &CodebaseInfo,
    interner: &Interner,
    options: &TypeExpansionOptions,
) -> Option<Vec<TAtomic>> {
    let raw_name = interner.lookup(name);
    let (class_part, constant_part) = raw_name.split_once("::")?;
    let class_part = class_part.trim();
    let constant_part = constant_part.trim();
    if class_part.is_empty()
        || constant_part.is_empty()
        || constant_part.eq_ignore_ascii_case("class")
    {
        return None;
    }

    let class_id = match class_part.to_ascii_lowercase().as_str() {
        "self" | "static" | "$this" => match &options.static_class_type {
            StaticClassType::Name(static_id) if !class_part.eq_ignore_ascii_case("self") => {
                *static_id
            }
            _ => options.self_class?,
        },
        "parent" => options.parent_class?,
        // Already namespace/alias-resolved at scan time (names resolve once).
        _ => interner.intern(class_part),
    };
    let class_info = codebase.get_class(class_id)?;

    let mut resolved: Option<TUnion> = None;
    if constant_part.contains('*') {
        for (constant_name, constant_info) in &class_info.constants {
            if wildcard_matches(constant_part, &interner.lookup(*constant_name)) {
                resolved = Some(match resolved {
                    Some(existing) => {
                        combine_union_types(&existing, &constant_info.constant_type, false)
                    }
                    None => constant_info.constant_type.clone(),
                });
            }
        }
    } else {
        let constant_id = interner.intern(constant_part);
        resolved = class_info
            .constants
            .get(&constant_id)
            .map(|constant_info| constant_info.constant_type.clone());
    }

    let mut resolved = resolved?;
    // A constant's type can itself hold expandable parts (nested constant
    // references, self, conditionals) — Psalm re-expands the result.
    expand_union(codebase, interner, &mut resolved, options);
    Some(resolved.types)
}

/// Split a `key-of<...>` / `value-of<...>` sentinel token, returning
/// `(is_key_of, inner)`.
pub(crate) fn split_key_value_of_sentinel(raw: &str) -> Option<(bool, &str)> {
    let inner = raw
        .strip_prefix("key-of<")
        .map(|rest| (true, rest))
        .or_else(|| raw.strip_prefix("value-of<").map(|rest| (false, rest)));
    let (is_key_of, rest) = inner?;
    let inner = rest.strip_suffix('>')?;
    inner.contains("::").then_some((is_key_of, inner))
}

/// Anchored wildcard match with `*` matching any (possibly empty) substring —
/// the semantics of Psalm's `StorageByPatternResolver` regex translation
/// (`^seg0.*?seg1.*? … segN$`).
fn wildcard_matches(pattern: &str, candidate: &str) -> bool {
    let segments: Vec<&str> = pattern.split('*').collect();
    let (first, rest_segments) = match segments.split_first() {
        Some(parts) => parts,
        None => return pattern == candidate,
    };
    if rest_segments.is_empty() {
        return pattern == candidate;
    }
    let last = rest_segments[rest_segments.len() - 1];

    let Some(without_prefix) = candidate.strip_prefix(first) else {
        return false;
    };
    let Some(mut middle) = without_prefix.strip_suffix(last) else {
        return false;
    };
    for segment in &rest_segments[..rest_segments.len() - 1] {
        if segment.is_empty() {
            continue;
        }
        match middle.find(segment) {
            Some(idx) => middle = &middle[idx + segment.len()..],
            None => return false,
        }
    }
    true
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
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let Some(self_class) = options.self_class else {
        return None;
    };

    if *name == StrId::STATIC {
        // The literal `static` keyword: bind to the call site's class. It
        // stays flagged static (so it remains compatible with other `static`
        // types) unless the binding is final, where late-static binding
        // collapses to the concrete class (matches Psalm's TypeExpander).
        match &options.static_class_type {
            StaticClassType::Object(obj) => return Some(obj.clone()),
            StaticClassType::Name(static_id) => {
                *name = if *static_id == StrId::STATIC {
                    self_class
                } else {
                    *static_id
                };
                *is_static = !options.function_is_final;
            }
            StaticClassType::None => {}
        }
    } else if *is_static {
        // A concrete-named atomic still flagged static (e.g. a template bound
        // to the *caller's* `new static()`): Psalm only finalizes it when it
        // belongs to the expansion `self`'s own hierarchy — a foreign class's
        // late-static type passes through untouched.
        if let StaticClassType::Name(static_id) = &options.static_class_type
            && (*name == self_class
                || crate::type_comparator::object_type_comparator::is_class_subtype_of(
                    *name, self_class, codebase,
                )
                || crate::type_comparator::object_type_comparator::is_class_subtype_of(
                    self_class, *name, codebase,
                ))
        {
            if options.function_is_final {
                *name = *static_id;
                *is_static = false;
            } else if *static_id != *name
                && crate::type_comparator::object_type_comparator::is_class_subtype_of(
                    *static_id, *name, codebase,
                )
            {
                // A subclass receiver narrows the late-static target
                // (`(new B)->getThis()` with `getThis(): static` declared on
                // A is `B&static`, Psalm's TypeExpander).
                *name = *static_id;
            }
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
    expand_atomic(
        atomic,
        codebase,
        interner,
        options,
        &mut skip,
        &mut replacements,
    );
    if skip {
        if let Some(first) = replacements.into_iter().next() {
            *atomic = first;
        }
    }
}

/// Bind `properties-of<self|static|parent>` to the enclosing class before a
/// declared-return-type expansion. Psalm's `expandPropertiesOf` does this via
/// `replaceClassLike('self'/'static', $self_class)` — `static` collapses to
/// the self class for these checks.
pub(crate) fn bind_properties_of_self_names(
    union: &mut TUnion,
    self_class: Option<StrId>,
    parent_class: Option<StrId>,
) {
    for atomic in union.types.iter_mut() {
        if let TAtomic::TPropertiesOf { classlike_name, .. } = atomic {
            if *classlike_name == StrId::SELF || *classlike_name == StrId::STATIC {
                if let Some(self_class) = self_class {
                    *classlike_name = self_class;
                }
            } else if *classlike_name == StrId::PARENT
                && let Some(parent_class) = parent_class
            {
                *classlike_name = parent_class;
            }
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
        && class_info.all_parent_classes.iter().all(|ancestor| {
            codebase
                .get_class(*ancestor)
                .map_or(true, |info| info.is_final)
        });

    let (fallback_key_type, fallback_value_type) = if all_sealed {
        (None, None)
    } else {
        (
            Some(Box::new(TUnion::string())),
            Some(Box::new(TUnion::mixed())),
        )
    };

    Some(TAtomic::TKeyedArray {
        properties: std::sync::Arc::new(properties),
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
    localize_special_class_type_union_final(
        codebase,
        interner,
        union,
        self_class_id,
        static_class_id,
        parent_class_id,
        false,
    )
}

/// Like [`localize_special_class_type_union`], but with Psalm's `$final`
/// expander flag: when set, `static` binds to the concrete class instead of
/// staying late-static (Psalm finalizes on final receivers, and on static
/// calls naming a class other than the enclosing `self`).
pub(crate) fn localize_special_class_type_union_final(
    codebase: &CodebaseInfo,
    interner: &Interner,
    union: &TUnion,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
    function_is_final: bool,
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
            function_is_final,
            // Localization must not collapse conditionals (e.g. inside a callable's
            // return type) — they are evaluated against call arguments elsewhere.
            evaluate_conditional_types: false,
        },
    );
    localized
}

/// Like [`localize_special_class_type_union`], but binding `static` to a
/// concrete *atomic* rather than a class name — Psalm's
/// `StaticClassType::Object`. A `static` return through a `T`-typed or
/// `class-string<T>`-typed receiver resolves to the template param itself.
pub(crate) fn localize_special_class_type_union_with_static_object(
    codebase: &CodebaseInfo,
    interner: &Interner,
    union: &TUnion,
    self_class_id: StrId,
    static_object: TAtomic,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = union.clone();
    expand_union(
        codebase,
        interner,
        &mut localized,
        &TypeExpansionOptions {
            self_class: Some(self_class_id),
            static_class_type: StaticClassType::Object(static_object),
            parent_class: parent_class_id,
            function_is_final: false,
            evaluate_conditional_types: false,
        },
    );
    localized
}

/// Conservative, read-only check of whether `expand_atomic` could change this
/// union. Mirrors the match in `expand_atomic` above — if an arm is added
/// there, this must be updated. Unlike Hakana's version (whose expander
/// handles every variant explicitly), pzoom's `expand_atomic` has a no-op
/// default arm, so a `false` default here is exact rather than merely safe.
fn union_needs_expansion(union: &TUnion, options: &TypeExpansionOptions) -> bool {
    union
        .types
        .iter()
        .any(|atomic| atomic_needs_expansion(atomic, options))
}

fn atomic_needs_expansion(atomic: &TAtomic, options: &TypeExpansionOptions) -> bool {
    match atomic {
        TAtomic::TConditional(_) | TAtomic::TPropertiesOf { .. } => true,
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => union_needs_expansion(key_type, options) || union_needs_expansion(value_type, options),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_needs_expansion(value_type, options)
        }
        TAtomic::TClassStringMap {
            as_type,
            value_param,
            ..
        } => {
            as_type
                .as_ref()
                .is_some_and(|as_type| atomic_needs_expansion(as_type, options))
                || union_needs_expansion(value_param, options)
        }
        TAtomic::TNamedObject {
            name,
            type_params,
            is_static,
            ..
        } => {
            (options.self_class.is_some()
                && (*is_static
                    || *name == StrId::STATIC
                    || *name == StrId::SELF
                    || *name == StrId::PARENT))
                || type_params.as_ref().is_some_and(|type_params| {
                    type_params
                        .iter()
                        .any(|type_param| union_needs_expansion(type_param, options))
                })
        }
        TAtomic::TObjectIntersection { types } => types
            .iter()
            .any(|member| atomic_needs_expansion(member, options)),
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
            return_type
                .as_ref()
                .is_some_and(|return_type| union_needs_expansion(return_type, options))
                || params.as_ref().is_some_and(|params| {
                    params
                        .iter()
                        .any(|param| union_needs_expansion(&param.param_type, options))
                })
        }
        TAtomic::TTemplateParam { as_type, .. }
        | TAtomic::TTemplateKeyOf { as_type, .. }
        | TAtomic::TTemplateValueOf { as_type, .. } => union_needs_expansion(as_type, options),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            properties
                .values()
                .any(|value_type| union_needs_expansion(value_type, options))
                || fallback_key_type
                    .as_ref()
                    .is_some_and(|fallback| union_needs_expansion(fallback, options))
                || fallback_value_type
                    .as_ref()
                    .is_some_and(|fallback| union_needs_expansion(fallback, options))
        }
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_needs_expansion(as_type, options),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => atomic_needs_expansion(as_type, options),
        // expand_atomic's default arm is a no-op for everything else.
        _ => false,
    }
}
