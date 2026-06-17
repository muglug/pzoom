//! Phase 2: Populating - Resolve inheritance and build complete type info.
//!
//! The populator takes the scanned symbols and:
//! - Resolves class inheritance chains
//! - Inherits methods and properties from parent classes
//! - Processes trait usage
//! - Builds up all_parent_classes, all_parent_interfaces, etc.
//! - Populates types (resolves type references)
//!
//! This follows the pattern from hakana where `populate_codebase` is the main
//! entry point and classes are recursively populated to ensure ancestors
//! are processed before descendants.

use indexmap::IndexMap;
use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::codebase_info::ConstantInfo;
use pzoom_code_info::{CodebaseInfo, GlobalDefineValue, MethodIdentifier, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Register every scanned `define()` as a global constant — Psalm's
/// `addGlobalConstantType` under `allConstantsGlobal`. Runs after populate so
/// a deferred call value can borrow the callee's declared return type (the
/// stand-in for the runtime value Psalm-on-itself reads via
/// `get_defined_constants()`).
pub fn register_global_defined_constants(codebase: &mut CodebaseInfo) {
    let defines = std::mem::take(&mut codebase.global_defines);
    for define in &defines {
        if codebase.constants.contains_key(&define.name) {
            continue;
        }
        let constant_type = match &define.value {
            GlobalDefineValue::Resolved(value_type) => value_type.clone(),
            GlobalDefineValue::FunctionReturn(func_id) => codebase
                .functionlike_infos
                .get(func_id)
                .and_then(|func_info| {
                    func_info
                        .return_type
                        .clone()
                        .or_else(|| func_info.signature_return_type.clone())
                })
                .unwrap_or_else(TUnion::mixed),
            GlobalDefineValue::MethodReturn(class_id, method_id) => codebase
                .get_class(*class_id)
                .and_then(|class_info| class_info.methods.get(method_id))
                .and_then(|method_info| {
                    method_info
                        .return_type
                        .clone()
                        .or_else(|| method_info.signature_return_type.clone())
                })
                .unwrap_or_else(TUnion::mixed),
        };
        codebase.constants.insert(
            define.name,
            ConstantInfo {
                name: define.name,
                constant_type,
                file_path: define.file_path,
                start_offset: define.start_offset,
                unresolved_initializer: None,
            },
        );
    }
    codebase.global_defines = defines;
}

/// Main entry point for the population phase.
/// Follows hakana's `populate_codebase` function.
pub fn populate_codebase(codebase: &mut CodebaseInfo, interner: &Interner) {
    // First, reset population state for classlikes that need repopulation
    let classlike_names: Vec<_> = codebase
        .classlike_infos
        .iter()
        .filter(|(_, storage)| !storage.is_populated)
        .map(|(k, _)| *k)
        .collect();

    // Case-insensitive classlike lookup used by type comparators: full build
    // on the first populate, incremental extension afterwards (a re-populate
    // only adds the newly scanned symbols — symbols are append-only in-run).
    if codebase.classlike_name_lookup.is_empty() {
        codebase.classlike_name_lookup = codebase
            .classlike_infos
            .keys()
            .map(|classlike_id| {
                (
                    interner
                        .lookup(*classlike_id)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase(),
                    *classlike_id,
                )
            })
            .collect();
    } else {
        for classlike_id in &classlike_names {
            codebase.classlike_name_lookup.insert(
                interner
                    .lookup(*classlike_id)
                    .trim_start_matches('\\')
                    .to_ascii_lowercase(),
                *classlike_id,
            );
        }
    }

    // Same for top-level functions (rebuilt cheaply each populate: the map is
    // small and functions are append-only in-run).
    for function_id in codebase.functionlike_infos.keys() {
        codebase.functionlike_name_lookup.insert(
            interner
                .lookup(*function_id)
                .trim_start_matches('\\')
                .to_ascii_lowercase(),
            *function_id,
        );
    }

    for name in &classlike_names {
        if let Some(info) = codebase.classlike_infos.get_mut(name) {
            info.is_populated = false;
            info.declaring_property_ids = FxHashMap::default();
            info.appearing_property_ids = FxHashMap::default();
            info.declaring_method_ids = FxHashMap::default();
            info.appearing_method_ids = FxHashMap::default();
        }
    }

    // Populate declared property types BEFORE inheritance flattening: the
    // flattening Arc-shares PropertyInfo into descendants, so the shared
    // storage must already be fully resolved (populate_union_type is
    // codebase-independent, so this is order-safe). At this point the Arcs
    // are freshly scanned (unshared) and make_mut mutates in place.
    for name in &classlike_names {
        if let Some(storage) = codebase.classlike_infos.get_mut(name) {
            for prop_info in storage.properties.values_mut() {
                let prop_info = std::sync::Arc::make_mut(prop_info);
                if let Some(ref mut prop_type) = prop_info.property_type {
                    populate_union_type(prop_type);
                }
                if let Some(ref mut sig_type) = prop_info.signature_type {
                    populate_union_type(sig_type);
                }
            }
        }
    }

    // Populate all classlikes (recursive to handle inheritance order)
    for name in &classlike_names {
        populate_classlike_storage(name, codebase);
    }

    // String property-name lookup for interner-less contexts (object-shape
    // containment checks resolve `object{foo: ...}` keys against class
    // properties). Built after inheritance flattening so inherited
    // properties are included.
    for classlike_id in &classlike_names {
        if let Some(storage) = codebase.classlike_infos.get_mut(classlike_id) {
            let lookup: rustc_hash::FxHashMap<String, pzoom_str::StrId> = storage
                .properties
                .keys()
                .map(|property_id| (interner.lookup(*property_id).to_string(), *property_id))
                .collect();
            storage.property_name_lookup = lookup;
        }
    }

    // Lowercase -> correctly-cased name maps used for casing hints in
    // Undefined* diagnostics. Incremental like the lookup above: only newly
    // scanned classlikes/functions contribute on re-populates.
    for classlike_id in &classlike_names {
        let name = interner.lookup(*classlike_id);
        let lc = name.to_ascii_lowercase();
        if lc != *name {
            codebase
                .classlike_lc_names
                .insert(interner.intern(&lc), *classlike_id);
        }
    }
    let unpopulated_functions: Vec<_> = codebase
        .functionlike_infos
        .iter()
        .filter(|(_, info)| !info.is_populated)
        .map(|(k, _)| *k)
        .collect();
    for function_id in &unpopulated_functions {
        let name = interner.lookup(*function_id);
        let lc = name.to_ascii_lowercase();
        if lc != *name {
            codebase
                .functionlike_lc_names
                .insert(interner.intern(&lc), *function_id);
        }
    }
    for classlike_id in &classlike_names {
        let Some(storage) = codebase.classlike_infos.get_mut(classlike_id) else {
            continue;
        };
        storage.method_lc_names = storage
            .methods
            .keys()
            .filter_map(|method_id| {
                let name = interner.lookup(*method_id);
                let lc = name.to_ascii_lowercase();
                (lc != *name).then(|| (interner.intern(&lc), *method_id))
            })
            .collect();
    }

    // (Declared property types were populated before inheritance, above.)
    for name in &classlike_names {
        let Some(storage) = codebase.classlike_infos.get_mut(name) else {
            continue;
        };
        for prop_type in storage.pseudo_property_get_types.values_mut() {
            populate_union_type(prop_type);
        }

        for prop_type in storage.pseudo_property_set_types.values_mut() {
            populate_union_type(prop_type);
        }

        // Populate constant types
        for (_, const_info) in storage.constants.iter_mut() {
            populate_union_type(&mut const_info.constant_type);
        }

        // Populate template type bounds
        for template_type in storage.template_types.iter_mut() {
            populate_union_type(&mut template_type.as_type);
        }

        for param_types in storage.template_extended_offsets.values_mut() {
            for param_type in param_types.iter_mut() {
                populate_union_type(param_type);
            }
        }

        for template_map in storage.template_extended_params.values_mut() {
            for param_type in template_map.values_mut() {
                populate_union_type(param_type);
            }
        }
    }

    // Populate function/method types (skip already-populated symbols)
    for (_, func_info) in codebase
        .functionlike_infos
        .iter_mut()
        .filter(|(_, info)| !info.is_populated)
    {
        func_info.is_populated = true;
        if let Some(ref mut return_type) = func_info.return_type {
            populate_union_type(return_type);
        }
        if let Some(ref mut signature_return_type) = func_info.signature_return_type {
            populate_union_type(signature_return_type);
        }
        for param in func_info.params.iter_mut() {
            if let Some(ref mut param_type) = param.param_type {
                populate_union_type(param_type);
            }
            if let Some(ref mut param_out_type) = param.param_out_type {
                populate_union_type(param_out_type);
            }
            if let Some(ref mut signature_type) = param.signature_type {
                populate_union_type(signature_type);
            }
        }
    }

    // Populate pseudo method signatures
    for name in &classlike_names {
        let Some(storage) = codebase.classlike_infos.get_mut(name) else {
            continue;
        };
        for (_, method_info) in storage.pseudo_methods.iter_mut() {
            if let Some(ref mut return_type) = method_info.return_type {
                populate_union_type(return_type);
            }
            if let Some(ref mut signature_return_type) = method_info.signature_return_type {
                populate_union_type(signature_return_type);
            }
            for param in method_info.params.iter_mut() {
                if let Some(ref mut param_type) = param.param_type {
                    populate_union_type(param_type);
                }
                if let Some(ref mut param_out_type) = param.param_out_type {
                    populate_union_type(param_out_type);
                }
                if let Some(ref mut signature_type) = param.signature_type {
                    populate_union_type(signature_type);
                }
            }
        }

        for (_, method_info) in storage.pseudo_static_methods.iter_mut() {
            if let Some(ref mut return_type) = method_info.return_type {
                populate_union_type(return_type);
            }
            if let Some(ref mut signature_return_type) = method_info.signature_return_type {
                populate_union_type(signature_return_type);
            }
            for param in method_info.params.iter_mut() {
                if let Some(ref mut param_type) = param.param_type {
                    populate_union_type(param_type);
                }
                if let Some(ref mut param_out_type) = param.param_out_type {
                    populate_union_type(param_out_type);
                }
                if let Some(ref mut signature_type) = param.signature_type {
                    populate_union_type(signature_type);
                }
            }
        }
    }

    // Build descendant maps
    let mut all_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>> = FxHashMap::default();
    let mut direct_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>> = FxHashMap::default();

    for (classlike_name, storage) in &codebase.classlike_infos {
        // Track descendants through parent interfaces
        for parent_interface in &storage.all_parent_interfaces {
            all_classlike_descendants
                .entry(*parent_interface)
                .or_default()
                .insert(*classlike_name);
        }

        // Track descendants through direct parent interfaces
        for parent_interface in &storage.interfaces {
            direct_classlike_descendants
                .entry(*parent_interface)
                .or_default()
                .insert(*classlike_name);
        }

        // Track descendants through parent classes
        for parent_class in &storage.all_parent_classes {
            all_classlike_descendants
                .entry(*parent_class)
                .or_default()
                .insert(*classlike_name);
        }

        // Track descendants through used traits
        for used_trait in &storage.used_traits {
            all_classlike_descendants
                .entry(*used_trait)
                .or_default()
                .insert(*classlike_name);
        }

        // Track direct descendants through parent class
        if let Some(parent_class) = storage.parent_class {
            direct_classlike_descendants
                .entry(parent_class)
                .or_default()
                .insert(*classlike_name);
        }
    }

    // Store descendant maps in codebase
    codebase.all_classlike_descendants = all_classlike_descendants;
    codebase.direct_classlike_descendants = direct_classlike_descendants;

    // Psalm's ConstantTypeResolver: now that every class is known, evaluate
    // constant initializers whose cross-class references were deferred at
    // scan time (UnresolvedConstantComponent).
    resolve_unresolved_class_constants(codebase, interner);
}

/// Resolve every class constant carrying an `unresolved_initializer`.
fn resolve_unresolved_class_constants(codebase: &mut CodebaseInfo, interner: &Interner) {
    use pzoom_code_info::class_constant_info::{ConstResolutionFailure, UnresolvedConstExpr};

    let pending: Vec<(StrId, StrId)> = codebase
        .classlike_infos
        .iter()
        .flat_map(|(class_id, info)| {
            info.constants
                .iter()
                .filter(|(_, const_info)| const_info.unresolved_initializer.is_some())
                .map(move |(const_id, _)| (*class_id, *const_id))
        })
        .collect();

    let mut resolved: Vec<(StrId, StrId, TUnion, bool, Vec<ConstResolutionFailure>)> =
        Vec::with_capacity(pending.len());
    for (class_id, const_id) in &pending {
        let mut visiting = FxHashSet::default();
        visiting.insert((*class_id, *const_id));
        let mut hit_cycle = false;
        let mut failures = Vec::new();
        let Some(initializer) = codebase
            .get_class(*class_id)
            .and_then(|info| info.constants.get(const_id))
            .and_then(|const_info| const_info.unresolved_initializer.as_ref())
        else {
            continue;
        };
        let constant_type = resolve_const_expr(
            initializer,
            codebase,
            interner,
            &mut visiting,
            &mut hit_cycle,
            &mut failures,
        );
        resolved.push((*class_id, *const_id, constant_type, hit_cycle, failures));
    }

    let mut enums_needing_value_rebuild: FxHashSet<StrId> = FxHashSet::default();
    for (class_id, const_id, constant_type, hit_cycle, failures) in resolved {
        if let Some(const_info) = codebase
            .get_class_mut(class_id)
            .and_then(|info| info.constants.get_mut(&const_id))
        {
            if const_info.enum_case_value.is_some() {
                // An enum case's deferred initializer is its backed VALUE;
                // the constant type stays the case itself.
                const_info.enum_case_value = Some(constant_type);
                const_info.circular = hit_cycle;
                const_info.resolution_failures = failures;
                const_info.unresolved_initializer = None;
                enums_needing_value_rebuild.insert(class_id);
                continue;
            }
            const_info.constant_type = constant_type.clone();
            if const_info.declared_type.is_none() && !hit_cycle {
                const_info.declared_type = Some(constant_type);
            }
            const_info.circular = hit_cycle;
            const_info.resolution_failures = failures;
            const_info.unresolved_initializer = None;
        }
    }

    // The `$value` property union was built at scan time with `mixed`
    // placeholders for deferred case values; rebuild it from the resolved
    // values.
    for class_id in enums_needing_value_rebuild {
        let Some(class_info) = codebase.get_class(class_id) else {
            continue;
        };
        let value_atomics: Vec<TAtomic> = class_info
            .constants
            .values()
            .filter_map(|const_info| const_info.enum_case_value.as_ref())
            .map(|case_value| case_value.get_single().cloned().unwrap_or(TAtomic::TMixed))
            .collect();
        if value_atomics.is_empty() {
            continue;
        }
        if let Some(class_info) = codebase.get_class_mut(class_id)
            && let Some(value_property) = class_info.properties.get_mut(&StrId::VALUE)
        {
            let mut updated = (**value_property).clone();
            updated.property_type = Some(TUnion::from_types(value_atomics));
            *value_property = std::sync::Arc::new(updated);
        }
    }

    // GLOBAL constants with deferred initializers
    // (`const classId = Module::id;`) resolve through the same machinery.
    let pending_globals: Vec<StrId> = codebase
        .constants
        .iter()
        .filter(|(_, const_info)| const_info.unresolved_initializer.is_some())
        .map(|(const_id, _)| *const_id)
        .collect();
    let mut resolved_globals: Vec<(StrId, TUnion)> = Vec::with_capacity(pending_globals.len());
    for const_id in &pending_globals {
        let Some(initializer) = codebase
            .constants
            .get(const_id)
            .and_then(|const_info| const_info.unresolved_initializer.as_ref())
        else {
            continue;
        };
        let mut visiting = FxHashSet::default();
        let mut hit_cycle = false;
        let mut failures = Vec::new();
        let constant_type = resolve_const_expr(
            initializer,
            codebase,
            interner,
            &mut visiting,
            &mut hit_cycle,
            &mut failures,
        );
        resolved_globals.push((*const_id, constant_type));
    }
    for (const_id, constant_type) in resolved_globals {
        if let Some(const_info) = codebase.constants.get_mut(&const_id) {
            const_info.constant_type = constant_type;
            const_info.unresolved_initializer = None;
        }
    }

    fn resolve_const_expr(
        expr: &UnresolvedConstExpr,
        codebase: &CodebaseInfo,
        interner: &Interner,
        visiting: &mut FxHashSet<(StrId, StrId)>,
        hit_cycle: &mut bool,
        failures: &mut Vec<ConstResolutionFailure>,
    ) -> TUnion {
        use pzoom_code_info::t_atomic::ArrayKey;
        match expr {
            UnresolvedConstExpr::Resolved(resolved) => resolved.clone(),
            UnresolvedConstExpr::ClassConstant { class, constant } => lookup_constant_type(
                codebase, interner, *class, *constant, visiting, hit_cycle, failures,
            ),
            UnresolvedConstExpr::EnumCasePropertyFetch {
                class,
                case,
                fetch_name,
            } => {
                if *fetch_name {
                    TUnion::new(TAtomic::string_from_literal(
                        interner.lookup(*case).to_string(),
                        pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
                    ))
                } else {
                    lookup_enum_case_value(
                        codebase, interner, *class, *case, visiting, hit_cycle, failures,
                    )
                }
            }
            UnresolvedConstExpr::ArrayLiteral(entries) => {
                // Mirrors the scan-time inferer's shape assembly.
                let mut properties = FxHashMap::default();
                let mut next_int_key = 0i64;
                let mut is_list = true;
                for entry in entries {
                    let value_type = resolve_const_expr(
                        &entry.value,
                        codebase,
                        interner,
                        visiting,
                        hit_cycle,
                        failures,
                    );
                    if entry.is_spread {
                        // Spreading an empty array contributes nothing.
                        if matches!(
                            value_type.get_single(),
                            Some(TAtomic::TArray { value_type, .. }) if value_type.is_nothing()
                        ) {
                            continue;
                        }
                        // Inline the spread array's entries (string keys kept,
                        // int keys renumbered), like PHP's constant evaluation.
                        let Some(TAtomic::TKeyedArray {
                            properties: spread_properties,
                            ..
                        }) = value_type.get_single()
                        else {
                            return TUnion::mixed();
                        };
                        let mut spread_entries: Vec<_> = spread_properties
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect();
                        spread_entries.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in spread_entries {
                            match key {
                                ArrayKey::Int(_) => {
                                    properties.insert(ArrayKey::Int(next_int_key), value);
                                    next_int_key += 1;
                                }
                                ArrayKey::String(_) | ArrayKey::ClassString(_) => {
                                    is_list = false;
                                    properties.insert(key, value);
                                }
                            }
                        }
                        continue;
                    }
                    match &entry.key {
                        Some(key_expr) => {
                            let key_type = resolve_const_expr(
                                key_expr, codebase, interner, visiting, hit_cycle, failures,
                            );
                            let Some(key) = const_union_to_array_key(&key_type) else {
                                return TUnion::mixed();
                            };
                            if !matches!(key, ArrayKey::Int(value) if value == next_int_key) {
                                is_list = false;
                            }
                            if let ArrayKey::Int(value) = key {
                                next_int_key = value + 1;
                                properties.insert(ArrayKey::Int(value), value_type);
                            } else {
                                properties.insert(key, value_type);
                            }
                        }
                        None => {
                            properties.insert(ArrayKey::Int(next_int_key), value_type);
                            next_int_key += 1;
                        }
                    }
                }
                if properties.is_empty() {
                    return TUnion::new(TAtomic::TArray {
                        key_type: Box::new(TUnion::nothing()),
                        value_type: Box::new(TUnion::nothing()),
                    });
                }
                TUnion::new(TAtomic::TKeyedArray {
                    properties: std::sync::Arc::new(properties),
                    is_list,
                    sealed: true,
                    fallback_key_type: None,
                    fallback_value_type: None,
                })
            }
            UnresolvedConstExpr::Concat(lhs, rhs) => {
                let lhs_type =
                    resolve_const_expr(lhs, codebase, interner, visiting, hit_cycle, failures);
                let rhs_type =
                    resolve_const_expr(rhs, codebase, interner, visiting, hit_cycle, failures);
                let literal_piece = |union: &TUnion| -> Option<String> {
                    match union.get_single()? {
                        TAtomic::TLiteralString { value } => Some(value.clone()),
                        TAtomic::TLiteralClassString { name } => Some(name.clone()),
                        _ => None,
                    }
                };
                if let (Some(lhs_value), Some(rhs_value)) =
                    (literal_piece(&lhs_type), literal_piece(&rhs_type))
                {
                    TUnion::new(TAtomic::string_from_literal(
                        format!("{lhs_value}{rhs_value}"),
                        pzoom_code_info::t_atomic::DEFAULT_MAX_STRING_LENGTH,
                    ))
                } else {
                    TUnion::string()
                }
            }
            UnresolvedConstExpr::ArrayAccess { array, key } => {
                let array_type =
                    resolve_const_expr(array, codebase, interner, visiting, hit_cycle, failures);
                let key_type =
                    resolve_const_expr(key, codebase, interner, visiting, hit_cycle, failures);
                let (Some(TAtomic::TKeyedArray { properties, .. }), Some(key)) =
                    (array_type.get_single(), const_union_to_array_key(&key_type))
                else {
                    return TUnion::mixed();
                };
                properties.get(&key).cloned().unwrap_or_else(TUnion::mixed)
            }
            UnresolvedConstExpr::Plus(lhs, rhs) => {
                let lhs_type =
                    resolve_const_expr(lhs, codebase, interner, visiting, hit_cycle, failures);
                let rhs_type =
                    resolve_const_expr(rhs, codebase, interner, visiting, hit_cycle, failures);
                match (lhs_type.get_single(), rhs_type.get_single()) {
                    // PHP's `+` on arrays keeps the left operand's keys.
                    (
                        Some(TAtomic::TKeyedArray {
                            properties: lhs_properties,
                            is_list: lhs_is_list,
                            sealed,
                            fallback_key_type,
                            fallback_value_type,
                        }),
                        Some(TAtomic::TKeyedArray {
                            properties: rhs_properties,
                            is_list: rhs_is_list,
                            ..
                        }),
                    ) => {
                        let mut properties = (**lhs_properties).clone();
                        for (key, value) in rhs_properties.iter() {
                            properties
                                .entry(key.clone())
                                .or_insert_with(|| value.clone());
                        }
                        TUnion::new(TAtomic::TKeyedArray {
                            properties: std::sync::Arc::new(properties),
                            is_list: *lhs_is_list && *rhs_is_list,
                            sealed: *sealed,
                            fallback_key_type: fallback_key_type.clone(),
                            fallback_value_type: fallback_value_type.clone(),
                        })
                    }
                    (
                        Some(TAtomic::TLiteralInt { value: lhs_value }),
                        Some(TAtomic::TLiteralInt { value: rhs_value }),
                    ) => match lhs_value.checked_add(*rhs_value) {
                        Some(sum) => TUnion::new(TAtomic::TLiteralInt { value: sum }),
                        None => TUnion::mixed(),
                    },
                    _ => TUnion::mixed(),
                }
            }
            UnresolvedConstExpr::GlobalConstant(constant_id) => {
                match codebase.constants.get(constant_id) {
                    Some(constant) => constant.constant_type.clone(),
                    None => {
                        failures.push(ConstResolutionFailure::MissingGlobalConstant(*constant_id));
                        TUnion::mixed()
                    }
                }
            }
            UnresolvedConstExpr::Ternary {
                cond,
                if_branch,
                else_branch,
            } => {
                // Psalm's ConstantTypeResolver UnresolvedTernary handling: a
                // single-literal condition picks the branch; anything less
                // determinable resolves to mixed.
                let cond_type =
                    resolve_const_expr(cond, codebase, interner, visiting, hit_cycle, failures);
                let if_type = if_branch.as_ref().map(|if_expr| {
                    resolve_const_expr(if_expr, codebase, interner, visiting, hit_cycle, failures)
                });
                let else_type = resolve_const_expr(
                    else_branch,
                    codebase,
                    interner,
                    visiting,
                    hit_cycle,
                    failures,
                );
                match cond_type.get_single() {
                    Some(TAtomic::TLiteralInt { value }) => {
                        if *value != 0 {
                            return if_type.unwrap_or(cond_type);
                        }
                        TUnion::mixed()
                    }
                    Some(TAtomic::TLiteralFloat { value }) => {
                        if *value != 0.0 {
                            return if_type.unwrap_or(cond_type);
                        }
                        TUnion::mixed()
                    }
                    Some(TAtomic::TLiteralString { value }) => {
                        if !value.is_empty() && value != "0" {
                            return if_type.unwrap_or(cond_type);
                        }
                        TUnion::mixed()
                    }
                    Some(TAtomic::TTrue) => if_type.unwrap_or(cond_type),
                    Some(TAtomic::TFalse) | Some(TAtomic::TNull) => else_type,
                    _ => TUnion::mixed(),
                }
            }
            UnresolvedConstExpr::IntOp { op, lhs, rhs } => {
                use pzoom_code_info::class_constant_info::UnresolvedIntOp;
                let lhs_type =
                    resolve_const_expr(lhs, codebase, interner, visiting, hit_cycle, failures);
                let rhs_type =
                    resolve_const_expr(rhs, codebase, interner, visiting, hit_cycle, failures);
                if let (
                    Some(TAtomic::TLiteralInt { value: lhs_value }),
                    Some(TAtomic::TLiteralInt { value: rhs_value }),
                ) = (lhs_type.get_single(), rhs_type.get_single())
                {
                    let computed = match op {
                        UnresolvedIntOp::Sub => lhs_value.checked_sub(*rhs_value),
                        UnresolvedIntOp::Mul => lhs_value.checked_mul(*rhs_value),
                        UnresolvedIntOp::Mod => {
                            if *rhs_value != 0 {
                                lhs_value.checked_rem(*rhs_value)
                            } else {
                                None
                            }
                        }
                        UnresolvedIntOp::BitAnd => Some(lhs_value & rhs_value),
                        UnresolvedIntOp::BitOr => Some(lhs_value | rhs_value),
                        UnresolvedIntOp::BitXor => Some(lhs_value ^ rhs_value),
                        UnresolvedIntOp::Shl => u32::try_from(*rhs_value)
                            .ok()
                            .and_then(|shift| lhs_value.checked_shl(shift)),
                        UnresolvedIntOp::Shr => u32::try_from(*rhs_value)
                            .ok()
                            .and_then(|shift| lhs_value.checked_shr(shift)),
                    };
                    if let Some(value) = computed {
                        return TUnion::new(TAtomic::TLiteralInt { value });
                    }
                }
                TUnion::int()
            }
        }
    }

    fn lookup_constant_type(
        codebase: &CodebaseInfo,
        interner: &Interner,
        class_id: StrId,
        const_id: StrId,
        visiting: &mut FxHashSet<(StrId, StrId)>,
        hit_cycle: &mut bool,
        failures: &mut Vec<ConstResolutionFailure>,
    ) -> TUnion {
        // The collect-time FQCN may differ in casing from the declared name.
        let class_info = codebase.get_class(class_id).or_else(|| {
            codebase
                .classlike_name_lookup
                .get(
                    &interner
                        .lookup(class_id)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase(),
                )
                .and_then(|resolved_id| codebase.get_class(*resolved_id))
        });
        let Some(class_info) = class_info else {
            failures.push(ConstResolutionFailure::MissingClass(class_id));
            return TUnion::mixed();
        };

        // Constants are inherited; walk the declaring class then ancestors.
        let const_info = class_info.constants.get(&const_id).or_else(|| {
            class_info
                .all_parent_classes
                .iter()
                .chain(class_info.interfaces.iter())
                .find_map(|ancestor_id| {
                    codebase
                        .get_class(*ancestor_id)
                        .and_then(|ancestor| ancestor.constants.get(&const_id))
                })
        });
        let Some(const_info) = const_info else {
            failures.push(ConstResolutionFailure::MissingClassConstant(
                class_info.name,
                const_id,
            ));
            return TUnion::mixed();
        };

        // An enum case's pending initializer is its backed VALUE; the
        // constant itself is the case (`Bar::BAR` stays `enum(Bar::BAR)`).
        if let Some(initializer) = &const_info.unresolved_initializer
            && const_info.enum_case_value.is_none()
        {
            if !visiting.insert((class_info.name, const_id)) {
                // Initializer cycle — Psalm throws CircularReferenceException.
                *hit_cycle = true;
                return TUnion::mixed();
            }
            return resolve_const_expr(
                initializer,
                codebase,
                interner,
                visiting,
                hit_cycle,
                failures,
            );
        }

        const_info.constant_type.clone()
    }

    /// The backed value of `class::case`, resolving a pending case-value
    /// initializer in place (Psalm's ConstantTypeResolver EnumValueFetch).
    fn lookup_enum_case_value(
        codebase: &CodebaseInfo,
        interner: &Interner,
        class_id: StrId,
        case_id: StrId,
        visiting: &mut FxHashSet<(StrId, StrId)>,
        hit_cycle: &mut bool,
        failures: &mut Vec<ConstResolutionFailure>,
    ) -> TUnion {
        let class_info = codebase.get_class(class_id).or_else(|| {
            codebase
                .classlike_name_lookup
                .get(
                    &interner
                        .lookup(class_id)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase(),
                )
                .and_then(|resolved_id| codebase.get_class(*resolved_id))
        });
        let Some(class_info) = class_info else {
            failures.push(ConstResolutionFailure::MissingClass(class_id));
            return TUnion::mixed();
        };
        let Some(const_info) = class_info.constants.get(&case_id) else {
            failures.push(ConstResolutionFailure::MissingClassConstant(
                class_info.name,
                case_id,
            ));
            return TUnion::mixed();
        };

        if let Some(initializer) = &const_info.unresolved_initializer {
            if !visiting.insert((class_info.name, case_id)) {
                *hit_cycle = true;
                return TUnion::mixed();
            }
            return resolve_const_expr(
                initializer,
                codebase,
                interner,
                visiting,
                hit_cycle,
                failures,
            );
        }

        const_info
            .enum_case_value
            .clone()
            .unwrap_or_else(TUnion::mixed)
    }

    fn const_union_to_array_key(union: &TUnion) -> Option<pzoom_code_info::t_atomic::ArrayKey> {
        use pzoom_code_info::t_atomic::ArrayKey;
        match union.get_single()? {
            TAtomic::TLiteralInt { value } => Some(ArrayKey::Int(*value)),
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .ok()
                .map(ArrayKey::Int)
                .or_else(|| Some(ArrayKey::String(value.clone()))),
            TAtomic::TLiteralClassString { name } => Some(ArrayKey::String(name.clone())),
            TAtomic::TNull => Some(ArrayKey::String(String::new())),
            _ => None,
        }
    }
}

/// Recursively populate a classlike, ensuring all ancestors are populated first.
/// Follows hakana's `populate_classlike_storage` pattern.
fn populate_classlike_storage(classlike_name: &StrId, codebase: &mut CodebaseInfo) {
    // Remove storage temporarily to allow mutable access during recursion
    let mut storage = match codebase.classlike_infos.remove(classlike_name) {
        Some(storage) => storage,
        None => return,
    };

    if storage.is_populated {
        codebase.classlike_infos.insert(*classlike_name, storage);
        return;
    }

    // Initialize declaring/appearing IDs for properties defined in this class
    for prop_name in storage.properties.keys().copied().collect::<Vec<_>>() {
        storage
            .declaring_property_ids
            .insert(prop_name, *classlike_name);
        storage
            .appearing_property_ids
            .insert(prop_name, *classlike_name);
        storage
            .inheritable_property_ids
            .insert(prop_name, *classlike_name);
    }

    // Initialize declaring/appearing IDs for methods defined in this class
    for method_name in storage.methods.keys().copied().collect::<Vec<_>>() {
        storage
            .declaring_method_ids
            .insert(method_name, *classlike_name);
        storage
            .appearing_method_ids
            .insert(method_name, *classlike_name);
        storage
            .inheritable_method_ids
            .insert(method_name, *classlike_name);
    }

    // Process used traits first (traits take precedence in PHP)
    for trait_name in storage.used_traits.clone() {
        populate_data_from_trait(&mut storage, codebase, &trait_name);
    }

    // Process parent class
    if let Some(parent_name) = storage.parent_class {
        populate_data_from_parent_classlike(&mut storage, codebase, &parent_name);
    }

    // Process interfaces
    if storage.kind == ClassLikeKind::Interface {
        // Interface extending other interfaces
        for iface_name in storage.interfaces.clone() {
            populate_interface_data_from_parent_interface(&mut storage, codebase, &iface_name);
        }
    } else {
        // Class implementing interfaces
        for iface_name in storage.interfaces.clone() {
            populate_data_from_implemented_interface(&mut storage, codebase, &iface_name);
        }
    }

    // Psalm's documenting-method inheritance (Populator's
    // `ClassLikeStorage::$documenting_method_ids`): record, per appearing method
    // name, the ancestor `MethodIdentifier` whose docblock documents the return
    // type, and flag the overriding method as documenting-inherited. The
    // effective return type is resolved lazily — never baked — by
    // `Methods::getMethodReturnType` (pzoom's method-call return-type fetcher).
    populate_documenting_method_ids(&mut storage, codebase);

    // Class-level `@psalm-taint-specialize` (Psalm Populator): every
    // non-static method's taints are tracked per call site.
    if storage.specialize_instance {
        for method_info in storage.methods.values_mut() {
            if !method_info.is_static {
                std::sync::Arc::make_mut(method_info).taints.specialize_call = true;
            }
        }
    }

    // Class-level @psalm-immutable / @psalm-external-mutation-free propagate
    // to non-static methods that don't carry their own
    // @psalm-external-mutation-free (Psalm Populator) — a method tagged
    // external-mutation-free in an immutable class keeps mutation_free false,
    // so its discarded calls aren't UnusedMethodCall.
    if storage.is_immutable || storage.is_external_mutation_free {
        for method_info in storage.methods.values_mut() {
            if !method_info.is_static && !method_info.is_external_mutation_free {
                let method_info = std::sync::Arc::make_mut(method_info);
                method_info.is_mutation_free = storage.is_immutable;
                method_info.is_external_mutation_free = storage.is_external_mutation_free;
                // Class-declared immutability is not an inference: the
                // methods memoize like any declared-@psalm-mutation-free
                // method (Psalm's MethodCallPurityAnalyzer memoizable path).
                if storage.is_immutable {
                    method_info.mutation_free_inferred = false;
                }
            }
        }
    }

    // Shrink collections to fit
    storage.all_parent_interfaces.shrink_to_fit();
    storage.all_parent_classes.shrink_to_fit();
    storage.appearing_method_ids.shrink_to_fit();
    storage.declaring_method_ids.shrink_to_fit();
    storage.appearing_property_ids.shrink_to_fit();
    storage.declaring_property_ids.shrink_to_fit();

    storage.is_populated = true;
    codebase.classlike_infos.insert(*classlike_name, storage);
}

/// Port of the documenting-method section of Psalm's
/// `Populator::populateOverriddenMethods`
/// (`ClassLikeStorage::$documenting_method_ids`): for each method that overrides
/// ancestors but declares no docblock types of its own, record the ancestor
/// `MethodIdentifier` whose docblock documents the return type, and flag the
/// overriding method as documenting-inherited.
///
/// The effective return type is never baked into storage; it is resolved
/// lazily wherever needed by `Methods::getMethodReturnType` (pzoom's method-call
/// return-type fetcher), exactly as Psalm does.
fn populate_documenting_method_ids(storage: &mut ClassLikeInfo, codebase: &CodebaseInfo) {
    storage.documenting_method_ids.clear();

    fn has_docblock_return(method: &pzoom_code_info::FunctionLikeInfo) -> bool {
        method.return_type.is_some()
    }
    fn has_docblock_params(method: &pzoom_code_info::FunctionLikeInfo) -> bool {
        method.params.iter().any(|param| param.has_docblock_type)
    }

    // Deterministic iteration over method names (FxHashMap is unordered).
    let mut method_names: Vec<StrId> = storage.overridden_method_ids.keys().copied().collect();
    method_names.sort_by_key(|method_name| method_name.0);

    // Method names whose documenting id was unset by a conflicting interface —
    // mirrors Psalm's `inherited_return_type === null` guard, which skips a
    // method once unset.
    let mut documenting_unset: FxHashSet<StrId> = FxHashSet::default();

    for method_name in method_names {
        let Some(child) = storage.methods.get(&method_name) else {
            continue;
        };
        if has_docblock_return(child) || has_docblock_params(child) {
            continue;
        }

        // Deterministic ancestor order for the interface tie-break below.
        let mut declaring_classes: Vec<StrId> = storage.overridden_method_ids[&method_name]
            .iter()
            .copied()
            .collect();
        declaring_classes.sort_by_key(|class_id| class_id.0);

        for declaring_class in declaring_classes {
            if documenting_unset.contains(&method_name) {
                break;
            }

            let Some(declaring_storage) = codebase.classlike_infos.get(&declaring_class) else {
                continue;
            };
            let Some(declaring_method) = declaring_storage
                .methods
                .get(&method_name)
                .map(|method| method.as_ref())
                .or_else(|| declaring_storage.pseudo_methods.get(&method_name))
                .or_else(|| declaring_storage.pseudo_static_methods.get(&method_name))
            else {
                continue;
            };

            if !has_docblock_return(declaring_method) && !has_docblock_params(declaring_method) {
                continue;
            }

            let declaring_method_id = MethodIdentifier(declaring_class, method_name);

            match storage.documenting_method_ids.get(&method_name).copied() {
                None => {
                    storage
                        .documenting_method_ids
                        .insert(method_name, declaring_method_id);
                }
                Some(existing) if existing == declaring_method_id => {}
                Some(existing) => {
                    // A nearer declaring interface (the new declaring class
                    // extends the current documenting class) supersedes it.
                    if declaring_storage.interfaces.contains(&existing.0) {
                        storage
                            .documenting_method_ids
                            .insert(method_name, declaring_method_id);
                    } else if let Some(documenting_storage) =
                        codebase.classlike_infos.get(&existing.0)
                        && !documenting_storage.interfaces.contains(&declaring_class)
                        && documenting_storage.kind == ClassLikeKind::Interface
                    {
                        // Two unrelated interfaces disagree — cancel the
                        // inheritance (Psalm unsets the documenting id).
                        storage.documenting_method_ids.remove(&method_name);
                        documenting_unset.insert(method_name);
                    }
                }
            }
        }
    }

    // Psalm sets `MethodStorage::$inherited_return_type = true` for every method
    // that ends up with a documenting id (it gates ReturnTypeAnalyzer's external
    // type check and MethodComparator's docblock-return comparison).
    let documenting_names: Vec<StrId> = storage.documenting_method_ids.keys().copied().collect();
    for method_name in documenting_names {
        if let Some(method_arc) = storage.methods.get_mut(&method_name) {
            std::sync::Arc::make_mut(method_arc).inherited_return_type = true;
        }
    }
}

/// Populate data from a used trait.
/// Follows hakana's `populate_data_from_trait`.
fn populate_data_from_trait(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    trait_name: &StrId,
) {
    // Recursively populate the trait first
    populate_classlike_storage(trait_name, codebase);

    let trait_storage = match codebase.classlike_infos.get(trait_name) {
        Some(s) => s,
        None => {
            storage.invalid_dependencies.push(*trait_name);
            return;
        }
    };

    // Inherit constants from trait. Trait members are flattened into the
    // using class, so the constant's visibility scope becomes the class
    // (a private trait constant is accessible from the class's own code).
    for (const_name, const_info) in &trait_storage.constants {
        if !storage.constants.contains_key(const_name) {
            let mut const_info = const_info.clone();
            const_info.declaring_class = storage.name;
            storage.constants.insert(*const_name, const_info);
        }
    }

    // Inherit interfaces that the trait implements
    storage
        .all_parent_interfaces
        .extend(trait_storage.interfaces.iter().copied());
    storage
        .all_parent_interfaces
        .extend(trait_storage.all_parent_interfaces.iter().copied());

    // Inherit invalid dependencies
    storage
        .invalid_dependencies
        .extend(trait_storage.invalid_dependencies.iter().copied());

    extend_template_params(storage, trait_storage, false);

    // Inherit methods and properties
    let is_trait = storage.kind == ClassLikeKind::Trait;
    inherit_methods_from_parent(storage, trait_storage, is_trait);

    // Psalm scans trait statements in the using class's context, so an
    // inferred-mutation-free trait getter in a *final* class gets
    // mutation_free_inferred = false (firm — memoizable). pzoom scans the
    // trait standalone; recompute the flag against the using class.
    if storage.is_final {
        let trait_method_names: Vec<StrId> = trait_storage.methods.keys().copied().collect();
        for method_name in trait_method_names {
            if let Some(method_info) = storage.methods.get_mut(&method_name) {
                if method_info.mutation_free_inferred {
                    std::sync::Arc::make_mut(method_info).mutation_free_inferred = false;
                }
            }
        }
    }
    inherit_properties_from_parent(storage, trait_storage, true); // from_trait = true
    inherit_pseudo_members_from_parent(storage, trait_storage);
    inherit_mixin_metadata_from_parent(storage, trait_storage);

    apply_trait_method_aliases(storage, trait_name);
}

/// Populate data from a parent class.
/// Follows hakana's `populate_data_from_parent_classlike`.
fn populate_data_from_parent_classlike(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    parent_name: &StrId,
) {
    // Recursively populate the parent first
    populate_classlike_storage(parent_name, codebase);

    let parent_storage = match codebase.classlike_infos.get(parent_name) {
        Some(s) => s,
        None => {
            storage.invalid_dependencies.push(*parent_name);
            return;
        }
    };

    // Build all_parent_classes: parent + parent's ancestors
    storage.all_parent_classes.push(*parent_name);
    storage
        .all_parent_classes
        .extend(parent_storage.all_parent_classes.iter().copied());

    extend_template_params(storage, parent_storage, true);

    // Inherit all parent interfaces
    storage
        .all_parent_interfaces
        .extend(parent_storage.all_parent_interfaces.iter().copied());

    // Inherit invalid dependencies
    storage
        .invalid_dependencies
        .extend(parent_storage.invalid_dependencies.iter().copied());

    // Inherit used traits from parent
    storage
        .used_traits
        .extend(parent_storage.used_traits.iter().copied());

    // Inherit constants (only public and protected)
    for (const_name, const_info) in &parent_storage.constants {
        if !storage.constants.contains_key(const_name)
            && const_info.visibility != Visibility::Private
        {
            storage.constants.insert(*const_name, const_info.clone());
        }
    }

    // `#[AllowDynamicProperties]` / `@psalm-no-seal-properties` are inherited: a subclass
    // of a class that permits dynamic properties permits them too.
    if parent_storage.no_seal_properties {
        storage.no_seal_properties = true;
    }

    // Inherit methods and properties
    let is_trait = storage.kind == ClassLikeKind::Trait;
    inherit_methods_from_parent(storage, parent_storage, is_trait);
    inherit_properties_from_parent(storage, parent_storage, false); // from_trait = false
    inherit_pseudo_members_from_parent(storage, parent_storage);
    inherit_mixin_metadata_from_parent(storage, parent_storage);
}

/// Populate interface data from a parent interface.
/// Follows hakana's `populate_interface_data_from_parent_interface`.
fn populate_interface_data_from_parent_interface(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    parent_iface_name: &StrId,
) {
    // Recursively populate the parent interface first
    populate_classlike_storage(parent_iface_name, codebase);

    let parent_storage = match codebase.classlike_infos.get(parent_iface_name) {
        Some(s) => s,
        None => {
            storage.invalid_dependencies.push(*parent_iface_name);
            return;
        }
    };

    // Use shared helper for interface data
    populate_interface_data_from_parent_or_implemented_interface(storage, parent_storage);

    // Inherit methods
    inherit_methods_from_parent(storage, parent_storage, false);
    inherit_pseudo_members_from_parent(storage, parent_storage);
    inherit_mixin_metadata_from_parent(storage, parent_storage);

    // Build all_parent_interfaces
    storage.all_parent_interfaces.push(*parent_iface_name);
    storage
        .all_parent_interfaces
        .extend(parent_storage.all_parent_interfaces.iter().copied());
}

/// Populate data from an implemented interface.
/// Follows hakana/Psalm pattern for class implementing interface.
fn populate_data_from_implemented_interface(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    iface_name: &StrId,
) {
    // Recursively populate the interface first
    populate_classlike_storage(iface_name, codebase);

    let iface_storage = match codebase.classlike_infos.get(iface_name) {
        Some(s) => s,
        None => {
            storage.invalid_dependencies.push(*iface_name);
            return;
        }
    };

    // Use shared helper for interface data (constants, etc.)
    populate_interface_data_from_parent_or_implemented_interface(storage, iface_storage);

    // Inherit methods from the interface - this allows abstract classes to call
    // interface methods that will be implemented by concrete subclasses
    inherit_methods_from_parent(storage, iface_storage, false);
    inherit_pseudo_members_from_parent(storage, iface_storage);
    inherit_mixin_metadata_from_parent(storage, iface_storage);

    // Build all_parent_interfaces
    storage.all_parent_interfaces.push(*iface_name);
    storage
        .all_parent_interfaces
        .extend(iface_storage.all_parent_interfaces.iter().copied());
}

/// Shared helper for inheriting data from interfaces.
/// Follows hakana's `populate_interface_data_from_parent_or_implemented_interface`.
fn populate_interface_data_from_parent_or_implemented_interface(
    storage: &mut ClassLikeInfo,
    interface_storage: &ClassLikeInfo,
) {
    // Inherit constants from interface
    for (const_name, const_info) in &interface_storage.constants {
        if !storage.constants.contains_key(const_name) {
            storage.constants.insert(*const_name, const_info.clone());
        }
    }

    // Inherit invalid dependencies
    storage
        .invalid_dependencies
        .extend(interface_storage.invalid_dependencies.iter().copied());

    extend_template_params(storage, interface_storage, false);
}

fn extend_template_params(
    storage: &mut ClassLikeInfo,
    parent_storage: &ClassLikeInfo,
    from_direct_parent: bool,
) {
    // Inherit the promised yield type (Psalm's Populator::extendTemplateParams
    // head: `$storage->yield ??= $parent_storage->yield` with the declaring
    // class recorded for template resolution).
    if parent_storage.yield_type.is_some() && storage.yield_type.is_none() {
        storage.yield_type = parent_storage.yield_type.clone();
        if storage.declaring_yield_class.is_none() {
            storage.declaring_yield_class = Some(
                parent_storage
                    .declaring_yield_class
                    .unwrap_or(parent_storage.name),
            );
        }
    }

    if !parent_storage.template_types.is_empty() {
        storage
            .template_extended_params
            .entry(parent_storage.name)
            .or_default();

        if let Some(parent_offsets) = storage.template_extended_offsets.get(&parent_storage.name) {
            for (i, extended_type) in parent_offsets.iter().enumerate() {
                if let Some(parent_template) = parent_storage.template_types.get(i) {
                    let mapped_name = parent_template.name;
                    // Explicit `@template-extends`/`@template-implements` args
                    // are docblock constructs; stamp them so comparisons treat
                    // an explicit `mixed` arg leniently while a *defaulted*
                    // slot (the else branch below, from_docblock cleared per
                    // Psalm) still reports mixed coercions.
                    let mut stamped_extended_type = extended_type.clone();
                    stamped_extended_type.from_docblock = true;
                    storage
                        .template_extended_params
                        .entry(parent_storage.name)
                        .or_default()
                        .insert(mapped_name, stamped_extended_type);

                    if !parent_template.as_type.is_mixed() {
                        for atomic in &extended_type.types {
                            if let TAtomic::TTemplateParam {
                                name,
                                defining_entity,
                                ..
                            } = atomic
                            {
                                if *defining_entity
                                    == pzoom_code_info::GenericParent::ClassLike(storage.name)
                                {
                                    if let Some(storage_template) = storage
                                        .template_types
                                        .iter_mut()
                                        .find(|template_type| template_type.name == *name)
                                    {
                                        if storage_template.as_type.is_mixed() {
                                            storage_template.as_type =
                                                parent_template.as_type.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let existing = storage.template_extended_params.clone();
            for (template_storage_class, type_map) in &parent_storage.template_extended_params {
                for (template_name, type_) in type_map {
                    storage
                        .template_extended_params
                        .entry(*template_storage_class)
                        .or_default()
                        .insert(*template_name, extend_type(type_, &existing));
                }
            }
        } else {
            // No explicit `@template-extends`: each parent template defaults to
            // its constraint (Psalm's `$default_param` with from_docblock
            // cleared), breaking the template chain at this class.
            for parent_template in &parent_storage.template_types {
                let mut default_param = parent_template.as_type.clone();
                default_param.from_docblock = false;
                storage
                    .template_extended_params
                    .entry(parent_storage.name)
                    .or_default()
                    .insert(parent_template.name, default_param);
            }

            // Psalm only merges the parent's own extended params when extending
            // a direct parent class (`$from_direct_parent`), with the parent's
            // entries winning per class-name key (array_merge semantics).
            if from_direct_parent {
                for (key, value) in &parent_storage.template_extended_params {
                    storage.template_extended_params.insert(*key, value.clone());
                }
            }
        }
    } else {
        // Parent declares no templates: inherit its extended params wholesale,
        // parent entries winning per class-name key (array_merge semantics).
        for (key, value) in &parent_storage.template_extended_params {
            storage.template_extended_params.insert(*key, value.clone());
        }
    }
}

fn extend_type(
    type_: &TUnion,
    template_extended_params: &IndexMap<StrId, IndexMap<StrId, TUnion>>,
) -> TUnion {
    let mut changed = false;
    let mut extended_types = Vec::with_capacity(type_.types.len());

    for atomic_type in &type_.types {
        if let TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        } = atomic_type
        {
            if let Some(referenced_type) = defining_entity
                .classlike_name()
                .and_then(|entity_class| template_extended_params.get(&entity_class))
                .and_then(|params| params.get(name))
            {
                // Psalm's extendType only substitutes non-template referenced
                // atomics; template references keep the original atomic, so
                // the extends chain stays linked one level at a time (which
                // getGenericParamForOffset's recursion depends on).
                for atomic_referenced_type in &referenced_type.types {
                    if matches!(atomic_referenced_type, TAtomic::TTemplateParam { .. }) {
                        extended_types.push(atomic_type.clone());
                    } else {
                        changed = true;
                        extended_types.push(atomic_referenced_type.clone());
                    }
                }
                continue;
            }
        }

        extended_types.push(atomic_type.clone());
    }

    if !changed {
        return type_.clone();
    }

    let mut extended = TUnion::from_types(extended_types);
    extended.from_docblock = type_.from_docblock;
    extended.is_resolved = type_.is_resolved;
    extended.parent_nodes = type_.parent_nodes.clone();
    extended.ignore_nullable_issues = type_.ignore_nullable_issues;
    extended.ignore_falsable_issues = type_.ignore_falsable_issues;
    extended
}

/// Inherit methods from a parent (class, interface, or trait).
/// Follows hakana's `inherit_methods_from_parent`.
fn inherit_methods_from_parent(
    storage: &mut ClassLikeInfo,
    parent_storage: &ClassLikeInfo,
    is_trait: bool,
) {
    let classlike_name = storage.name;

    // Inherit appearing_method_ids
    for (method_name, appearing_class) in &parent_storage.appearing_method_ids {
        if storage.appearing_method_ids.contains_key(method_name) {
            continue;
        }

        // Methods imported from traits appear in the consuming class/trait.
        let appearing = if is_trait || parent_storage.kind == ClassLikeKind::Trait {
            classlike_name
        } else {
            *appearing_class
        };
        storage.appearing_method_ids.insert(*method_name, appearing);
    }

    // Inherit declaring_method_ids and inheritable_method_ids
    for (method_name, declaring_class) in &parent_storage.inheritable_method_ids {
        if storage.declaring_method_ids.contains_key(method_name) {
            continue;
        }

        storage
            .declaring_method_ids
            .insert(*method_name, *declaring_class);

        // Traits can pass down methods from other traits,
        // but not from their require extends/implements parents
        if storage.kind != ClassLikeKind::Trait {
            storage
                .inheritable_method_ids
                .insert(*method_name, *declaring_class);
        }
    }

    // Inherit actual method implementations
    for (method_name, method_info) in &parent_storage.methods {
        if storage.methods.contains_key(method_name) {
            continue;
        }
        storage.methods.insert(*method_name, method_info.clone());
    }

    // Register which methods override an ancestor's method. This mirrors Psalm's
    // `Populator::inheritMethodsFromParent`: every inheritable method of a parent
    // class or interface is recorded as overridden, but a used trait only
    // contributes a method when the trait declares it `abstract` (using a
    // concrete trait method is inheritance, not an override). Unlike the
    // declaring/inheritable loops above, this runs even for methods the child
    // redeclares, so a concrete override of an abstract requirement is counted.
    let parent_is_trait = parent_storage.kind == ClassLikeKind::Trait;
    for (method_name, declaring_class) in &parent_storage.inheritable_method_ids {
        // Psalm skips `__construct` here (unless preserve_constructor_signature);
        // pzoom has no such flag, so an `#[Override]` on a constructor is invalid.
        if *method_name == StrId::CONSTRUCT {
            continue;
        }

        let recorded = if parent_is_trait {
            let is_abstract = parent_storage
                .methods
                .get(method_name)
                .is_some_and(|m| m.is_abstract);
            if is_abstract {
                storage
                    .overridden_method_ids
                    .entry(*method_name)
                    .or_default()
                    .insert(*declaring_class);
            }
            is_abstract
        } else {
            // Private methods are not inheritable/overridable. Psalm omits them
            // from `inheritable_method_ids`; pzoom keeps them, so skip them here
            // (a child redeclaring a private parent method is not an override).
            let is_private = parent_storage
                .methods
                .get(method_name)
                .is_some_and(|m| m.visibility == Visibility::Private);
            if is_private {
                continue;
            }
            storage
                .overridden_method_ids
                .entry(*method_name)
                .or_default()
                .insert(*declaring_class);
            true
        };

        // Propagate the parent's own overridden set (transitive overrides),
        // but only when this method was itself recorded as overridden.
        if recorded
            && let Some(parent_overrides) = parent_storage.overridden_method_ids.get(method_name)
        {
            let inherited: Vec<StrId> = parent_overrides.iter().copied().collect();
            storage
                .overridden_method_ids
                .entry(*method_name)
                .or_default()
                .extend(inherited);
        }
    }
}

fn apply_trait_method_aliases(storage: &mut ClassLikeInfo, trait_name: &StrId) {
    for alias in storage.trait_method_aliases.clone() {
        if alias
            .trait_name
            .is_some_and(|referenced_trait| referenced_trait != *trait_name)
        {
            continue;
        }

        let Some(source_method) = storage.methods.get(&alias.original_name).cloned() else {
            // `use T { CONST_NAME as public ALIAS; }` adapts a trait CONSTANT
            // (Psalm resolves these through the same adaptation list).
            if let Some(source_const) = storage.constants.get(&alias.original_name).cloned() {
                if alias.alias_name == alias.original_name {
                    if let Some(visibility) = alias.visibility
                        && let Some(existing_const) =
                            storage.constants.get_mut(&alias.original_name)
                    {
                        existing_const.visibility = visibility;
                    }
                } else if !storage.constants.contains_key(&alias.alias_name) {
                    let mut aliased_const = source_const;
                    aliased_const.name = alias.alias_name;
                    if let Some(visibility) = alias.visibility {
                        aliased_const.visibility = visibility;
                    }
                    storage.constants.insert(alias.alias_name, aliased_const);
                }
            }
            continue;
        };

        // `use T { foo as public; }` mutates the original method visibility.
        if alias.alias_name == alias.original_name {
            if let Some(visibility) = alias.visibility
                && let Some(existing_method) = storage.methods.get_mut(&alias.original_name)
            {
                std::sync::Arc::make_mut(existing_method).visibility = visibility;
            }
            continue;
        }

        if storage.methods.contains_key(&alias.alias_name) {
            continue;
        }

        let mut aliased_method = (*source_method).clone();
        aliased_method.name = alias.alias_name;

        if let Some(visibility) = alias.visibility {
            aliased_method.visibility = visibility;
        }

        storage
            .methods
            .insert(alias.alias_name, std::sync::Arc::new(aliased_method));

        if let Some(declaring_class) = storage
            .declaring_method_ids
            .get(&alias.original_name)
            .copied()
        {
            storage
                .declaring_method_ids
                .insert(alias.alias_name, declaring_class);
            storage
                .inheritable_method_ids
                .insert(alias.alias_name, declaring_class);
        }

        if let Some(appearing_class) = storage
            .appearing_method_ids
            .get(&alias.original_name)
            .copied()
        {
            storage
                .appearing_method_ids
                .insert(alias.alias_name, appearing_class);
        }
    }
}

fn inherit_pseudo_members_from_parent(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    // Psalm's Populator skips a parent pseudo method when the child declares
    // its OWN real method with that name (the real override wins); a method
    // merely inherited from elsewhere does not block the annotation.
    let has_own_real_method = |storage: &ClassLikeInfo, method_name: &pzoom_str::StrId| {
        storage
            .methods
            .get(method_name)
            .is_some_and(|method_info| method_info.declaring_class == Some(storage.name))
    };

    for (method_name, method_info) in &parent_storage.pseudo_methods {
        if has_own_real_method(storage, method_name) {
            continue;
        }
        storage
            .pseudo_methods
            .entry(*method_name)
            .or_insert_with(|| method_info.clone());
    }

    for (method_name, method_info) in &parent_storage.pseudo_static_methods {
        if has_own_real_method(storage, method_name) {
            continue;
        }
        storage
            .pseudo_static_methods
            .entry(*method_name)
            .or_insert_with(|| method_info.clone());
    }

    for (prop_name, prop_type) in &parent_storage.pseudo_property_get_types {
        storage
            .pseudo_property_get_types
            .entry(*prop_name)
            .or_insert_with(|| prop_type.clone());
    }

    for (prop_name, prop_type) in &parent_storage.pseudo_property_set_types {
        storage
            .pseudo_property_set_types
            .entry(*prop_name)
            .or_insert_with(|| prop_type.clone());
    }

    if parent_storage.sealed_methods.is_some() {
        storage.sealed_methods = parent_storage.sealed_methods;
    }

    if parent_storage.sealed_properties.is_some() {
        storage.sealed_properties = parent_storage.sealed_properties;
    }
}

fn inherit_mixin_metadata_from_parent(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    if storage.named_mixins.is_empty() && !parent_storage.named_mixins.is_empty() {
        storage.named_mixins = parent_storage.named_mixins.clone();
        storage.mixin_declaring_class = parent_storage.mixin_declaring_class;
    }
}

/// Inherit properties from a parent (class or trait).
/// Follows hakana's `inherit_properties_from_parent`.
fn inherit_properties_from_parent(
    storage: &mut ClassLikeInfo,
    parent_storage: &ClassLikeInfo,
    from_trait: bool,
) {
    let classlike_name = storage.name;
    let is_trait = storage.kind == ClassLikeKind::Trait;
    let parent_is_trait = parent_storage.kind == ClassLikeKind::Trait;

    // Inherit appearing_property_ids
    for (prop_name, appearing_class) in &parent_storage.appearing_property_ids {
        if storage.appearing_property_ids.contains_key(prop_name) {
            continue;
        }

        // Skip private properties from non-trait parents
        if !parent_is_trait {
            if let Some(prop_info) = parent_storage.properties.get(prop_name) {
                if prop_info.visibility == Visibility::Private {
                    continue;
                }
            }
        }

        // Properties imported from traits appear in the consuming class/trait.
        let appearing = if is_trait || parent_is_trait {
            classlike_name
        } else {
            *appearing_class
        };
        storage.appearing_property_ids.insert(*prop_name, appearing);
    }

    // Inherit declaring_property_ids
    for (prop_name, declaring_class) in &parent_storage.declaring_property_ids {
        if storage.declaring_property_ids.contains_key(prop_name) {
            continue;
        }

        // Skip private properties from non-trait parents
        if !parent_is_trait {
            if let Some(prop_info) = parent_storage.properties.get(prop_name) {
                if prop_info.visibility == Visibility::Private {
                    continue;
                }
            }
        }

        storage
            .declaring_property_ids
            .insert(*prop_name, *declaring_class);
    }

    // Inherit inheritable_property_ids
    for (prop_name, inheritable_class) in &parent_storage.inheritable_property_ids {
        // Skip private properties from non-trait parents
        if !parent_is_trait {
            if let Some(prop_info) = parent_storage.properties.get(prop_name) {
                if prop_info.visibility == Visibility::Private {
                    continue;
                }
            }
        }

        storage
            .inheritable_property_ids
            .insert(*prop_name, *inheritable_class);
    }

    // Inherit actual property storage
    for (prop_name, prop_info) in &parent_storage.properties {
        if storage.properties.contains_key(prop_name) {
            continue;
        }

        // Skip private properties from non-trait parents
        if !from_trait && prop_info.visibility == Visibility::Private {
            continue;
        }

        storage.properties.insert(*prop_name, prop_info.clone());
    }
}

/// Populate a union type, resolving any type references.
/// Follows hakana's `populate_union_type`.
pub fn populate_union_type(t_union: &mut TUnion) {
    for atomic in t_union.types.iter_mut() {
        populate_atomic_type(atomic);
    }
}

/// Populate an atomic type, resolving any type references.
/// Follows hakana's `populate_atomic_type`.
pub fn populate_atomic_type(t_atomic: &mut TAtomic) {
    match t_atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => {
            populate_union_type(key_type);
            populate_union_type(value_type);
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            populate_union_type(value_type);
        }
        TAtomic::TClassStringMap {
            as_type,
            value_param,
            ..
        } => {
            if let Some(inner) = as_type {
                populate_atomic_type(inner);
            }
            populate_union_type(value_param);
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for prop_type in std::sync::Arc::make_mut(properties).values_mut() {
                populate_union_type(prop_type);
            }
            if let Some(key_type) = fallback_key_type {
                populate_union_type(key_type);
            }
            if let Some(value_type) = fallback_value_type {
                populate_union_type(value_type);
            }
        }
        TAtomic::TNamedObject { type_params, .. } => {
            if let Some(params) = type_params {
                for param in params.iter_mut() {
                    populate_union_type(param);
                }
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for intersection_type in types.iter_mut() {
                populate_atomic_type(intersection_type);
            }
        }
        TAtomic::TObjectWithProperties { properties, .. } => {
            for prop_type in properties.values_mut() {
                populate_union_type(prop_type);
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            populate_union_type(as_type);
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            populate_atomic_type(as_type);
        }
        TAtomic::TClosure {
            params,
            return_type,
            ..
        } => {
            if let Some(ps) = params {
                for param in ps.iter_mut() {
                    populate_union_type(&mut param.param_type);
                }
            }
            if let Some(ret_type) = return_type {
                populate_union_type(ret_type);
            }
        }
        TAtomic::TCallable {
            params,
            return_type,
            ..
        } => {
            if let Some(ps) = params {
                for param in ps.iter_mut() {
                    populate_union_type(&mut param.param_type);
                }
            }
            if let Some(ret_type) = return_type {
                populate_union_type(ret_type);
            }
        }
        TAtomic::TClassString { as_type } => {
            if let Some(inner) = as_type {
                populate_atomic_type(inner);
            }
        }
        TAtomic::TDependentGetClass { as_type, .. } => {
            populate_union_type(as_type);
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            populate_union_type(key_type);
            populate_union_type(value_type);
        }
        // Simple types that don't contain nested types
        TAtomic::TInt
        | TAtomic::TNonspecificLiteralInt
        | TAtomic::TFloat
        | TAtomic::TString
        | TAtomic::TBool
        | TAtomic::TTrue
        | TAtomic::TFalse
        | TAtomic::TNull
        | TAtomic::TVoid
        | TAtomic::TNothing
        | TAtomic::TMixed
        | TAtomic::TNonEmptyMixed
        | TAtomic::TMixedFromLoopIsset
        | TAtomic::TObject
        | TAtomic::TResource
        | TAtomic::TClosedResource
        | TAtomic::TArrayKey
        | TAtomic::TScalar
        | TAtomic::TNonEmptyScalar
        | TAtomic::TNumeric
        | TAtomic::TIntRange { .. }
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TDependentGetType { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TCallableString
        | TAtomic::TEnum { .. }
        | TAtomic::TEnumCase { .. } => {}
        TAtomic::TConditional(conditional) => {
            populate_union_type(&mut conditional.if_true_type);
            populate_union_type(&mut conditional.if_false_type);
        }
        TAtomic::TTypeVariable { .. } => {}
        TAtomic::TTemplateKeyOf { as_type, .. } | TAtomic::TTemplateValueOf { as_type, .. } => {
            populate_union_type(as_type);
        }
        TAtomic::TPropertiesOf { .. } | TAtomic::TTemplatePropertiesOf { .. } => {}
    }
}

/// Legacy Populator struct for backwards compatibility.
/// Wraps the `populate_codebase` function.
pub struct Populator<'a> {
    codebase: &'a mut CodebaseInfo,
    interner: &'a Interner,
}

impl<'a> Populator<'a> {
    pub fn new(codebase: &'a mut CodebaseInfo, interner: &'a Interner) -> Self {
        Self { codebase, interner }
    }

    /// Run the population phase.
    pub fn populate(&mut self) {
        populate_codebase(self.codebase, self.interner);
    }
}
