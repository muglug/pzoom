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

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Main entry point for the population phase.
/// Follows hakana's `populate_codebase` function.
pub fn populate_codebase(codebase: &mut CodebaseInfo, interner: &Interner) {
    // Rebuild case-insensitive classlike lookup used by type comparators.
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

    // First, reset population state for classlikes that need repopulation
    let classlike_names: Vec<_> = codebase
        .classlike_infos
        .iter()
        .filter(|(_, storage)| !storage.is_populated)
        .map(|(k, _)| *k)
        .collect();

    for name in &classlike_names {
        if let Some(info) = codebase.classlike_infos.get_mut(name) {
            info.is_populated = false;
            info.declaring_property_ids = FxHashMap::default();
            info.appearing_property_ids = FxHashMap::default();
            info.declaring_method_ids = FxHashMap::default();
            info.appearing_method_ids = FxHashMap::default();
        }
    }

    // Populate all classlikes (recursive to handle inheritance order)
    for name in &classlike_names {
        populate_classlike_storage(name, codebase);
    }

    // Populate types in properties
    for (_, storage) in codebase.classlike_infos.iter_mut() {
        for (_, prop_info) in storage.properties.iter_mut() {
            if let Some(ref mut prop_type) = prop_info.property_type {
                populate_union_type(prop_type);
            }
            if let Some(ref mut sig_type) = prop_info.signature_type {
                populate_union_type(sig_type);
            }
        }

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

    // Populate function/method types
    for (_, func_info) in codebase.functionlike_infos.iter_mut() {
        if let Some(ref mut return_type) = func_info.return_type {
            populate_union_type(return_type);
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
    for (_, storage) in codebase.classlike_infos.iter_mut() {
        for (_, method_info) in storage.pseudo_methods.iter_mut() {
            if let Some(ref mut return_type) = method_info.return_type {
                populate_union_type(return_type);
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

    let _ = interner; // Will be used for filtering HH\\ types in Hack mode
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

    // Inherit constants from trait
    for (const_name, const_info) in &trait_storage.constants {
        if !storage.constants.contains_key(const_name) {
            storage.constants.insert(*const_name, const_info.clone());
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

    extend_template_params(storage, trait_storage);

    // Inherit methods and properties
    let is_trait = storage.kind == ClassLikeKind::Trait;
    inherit_methods_from_parent(storage, trait_storage, is_trait);
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

    extend_template_params(storage, parent_storage);

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

    extend_template_params(storage, interface_storage);
}

fn extend_template_params(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    if !parent_storage.template_types.is_empty() {
        storage
            .template_extended_params
            .entry(parent_storage.name)
            .or_default();

        if let Some(parent_offsets) = storage.template_extended_offsets.get(&parent_storage.name) {
            for (i, extended_type) in parent_offsets.iter().enumerate() {
                if let Some(parent_template) = parent_storage.template_types.get(i) {
                    let mapped_name = parent_template.name;
                    storage
                        .template_extended_params
                        .entry(parent_storage.name)
                        .or_default()
                        .insert(mapped_name, extended_type.clone());

                    if !parent_template.as_type.is_mixed() {
                        for atomic in &extended_type.types {
                            if let TAtomic::TTemplateParam {
                                name,
                                defining_entity,
                                ..
                            } = atomic
                            {
                                if *defining_entity == storage.name {
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
            for (key, value) in &parent_storage.template_extended_params {
                storage
                    .template_extended_params
                    .entry(*key)
                    .or_insert_with(|| value.clone());
            }
        }
    } else {
        for (key, value) in &parent_storage.template_extended_params {
            storage
                .template_extended_params
                .entry(*key)
                .or_insert_with(|| value.clone());
        }
    }
}

fn extend_type(
    type_: &TUnion,
    template_extended_params: &FxHashMap<StrId, FxHashMap<StrId, TUnion>>,
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
            if let Some(referenced_type) = template_extended_params
                .get(defining_entity)
                .and_then(|params| params.get(name))
            {
                changed = true;
                extended_types.extend(referenced_type.types.clone());
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
            continue;
        };

        // `use T { foo as public; }` mutates the original method visibility.
        if alias.alias_name == alias.original_name {
            if let Some(visibility) = alias.visibility
                && let Some(existing_method) = storage.methods.get_mut(&alias.original_name)
            {
                existing_method.visibility = visibility;
            }
            continue;
        }

        if storage.methods.contains_key(&alias.alias_name) {
            continue;
        }

        let mut aliased_method = source_method.clone();
        aliased_method.name = alias.alias_name;

        if let Some(visibility) = alias.visibility {
            aliased_method.visibility = visibility;
        }

        storage.methods.insert(alias.alias_name, aliased_method);

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
    for (method_name, method_info) in &parent_storage.pseudo_methods {
        storage
            .pseudo_methods
            .entry(*method_name)
            .or_insert_with(|| method_info.clone());
    }

    for (method_name, method_info) in &parent_storage.pseudo_static_methods {
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
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for prop_type in properties.values_mut() {
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
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            populate_union_type(key_type);
            populate_union_type(value_type);
        }
        // Simple types that don't contain nested types
        TAtomic::TInt
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
        | TAtomic::TObject
        | TAtomic::TResource
        | TAtomic::TClosedResource
        | TAtomic::TArrayKey
        | TAtomic::TScalar
        | TAtomic::TNumeric
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TEnum { .. }
        | TAtomic::TEnumCase { .. } => {}
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
