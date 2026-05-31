//! Codebase-wide information storage.
//!
//! Stores all collected type information about the codebase.

use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    ClassLikeInfo, FunctionLikeInfo, TAtomic, TUnion,
    class_type_alias::ClassTypeAlias,
    functionlike_info::{AssertionType, ConditionalReturnCondition},
};

/// Central storage for all codebase type information.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodebaseInfo {
    /// All classes, interfaces, traits, and enums.
    pub classlike_infos: FxHashMap<StrId, ClassLikeInfo>,

    /// All top-level functions.
    pub functionlike_infos: FxHashMap<StrId, FunctionLikeInfo>,

    /// Global constants.
    pub constants: FxHashMap<StrId, ConstantInfo>,

    /// Type aliases.
    pub type_aliases: FxHashMap<StrId, ClassTypeAlias>,

    /// Files that have been scanned.
    pub files: FxHashMap<StrId, FileInfo>,

    /// Map from classlike to all its descendants (classes, interfaces extending/implementing it).
    /// Populated during the populate phase.
    pub all_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,

    /// Map from classlike to its direct descendants only.
    /// Populated during the populate phase.
    pub direct_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,

    /// Case-insensitive classlike name lookup map.
    ///
    /// Keys are fully-qualified classlike names normalized by trimming a leading
    /// backslash and lowercasing. Populated during the populate phase.
    #[serde(default)]
    pub classlike_name_lookup: FxHashMap<String, StrId>,
}

/// Information about a global constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub name: StrId,
    pub constant_type: TUnion,
    pub file_path: StrId,
    pub start_offset: u32,
}

// `FileInfo` and the inline-annotation structs live in [`crate::file_info`]
// (mirroring Hakana's `file_info.rs`). They are re-exported here so existing
// `codebase_info::FileInfo` / `codebase_info::Inline*` paths keep resolving.
pub use crate::file_info::{
    FileInfo, InlineCallableParamType, InlineCallableTypeAnnotation, InlineCheckTypeAnnotation,
    InlineTraceAnnotation, InlineTypeAnnotations, InlineVarTypeAnnotation,
};

impl CodebaseInfo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get information about a class by name.
    pub fn get_class(&self, name: StrId) -> Option<&ClassLikeInfo> {
        self.classlike_infos.get(&name)
    }

    /// Get mutable information about a class by name.
    pub fn get_class_mut(&mut self, name: StrId) -> Option<&mut ClassLikeInfo> {
        self.classlike_infos.get_mut(&name)
    }

    /// Returns the storage of the class-like that declares `method_name` as
    /// seen from `fq_class_name` — Psalm's
    /// `Methods::getClassLikeStorageForMethod`. For an inherited method this
    /// is the ancestor that declares it; trait methods resolve to the using
    /// class (matching `declaring_method_ids` semantics). Falls back to the
    /// class's own storage when no declaring id is recorded.
    pub fn get_classlike_storage_for_method(
        &self,
        fq_class_name: StrId,
        method_name: StrId,
    ) -> Option<&ClassLikeInfo> {
        let class_info = self.get_class(fq_class_name)?;

        match class_info.declaring_method_ids.get(&method_name) {
            Some(declaring_class) if *declaring_class != fq_class_name => {
                self.get_class(*declaring_class).or(Some(class_info))
            }
            _ => Some(class_info),
        }
    }

    /// Get information about a function by name.
    pub fn get_function(&self, name: StrId) -> Option<&FunctionLikeInfo> {
        self.functionlike_infos.get(&name)
    }

    /// Get mutable information about a function by name.
    pub fn get_function_mut(&mut self, name: StrId) -> Option<&mut FunctionLikeInfo> {
        self.functionlike_infos.get_mut(&name)
    }

    /// Check if a class exists.
    pub fn class_exists(&self, name: StrId) -> bool {
        self.classlike_infos.contains_key(&name)
    }

    /// Resolve a classlike by case-insensitive string name.
    pub fn resolve_classlike_name(&self, name: &str) -> Option<StrId> {
        let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
        self.classlike_name_lookup.get(&normalized).copied()
    }

    /// Check if a function exists.
    pub fn function_exists(&self, name: StrId) -> bool {
        self.functionlike_infos.contains_key(&name)
    }

    /// Register a class in the codebase.
    pub fn register_class(&mut self, mut info: ClassLikeInfo) {
        if let Some(existing) = self.classlike_infos.get_mut(&info.name) {
            let existing_is_stub = self
                .files
                .get(&existing.file_path)
                .is_some_and(|file_info| file_info.is_stub);
            let incoming_is_stub = self
                .files
                .get(&info.file_path)
                .is_some_and(|file_info| file_info.is_stub);

            if existing_is_stub && !incoming_is_stub {
                *existing = info;
                return;
            }

            if !existing_is_stub && incoming_is_stub {
                return;
            }

            let template_name_remap = get_class_template_name_remap(existing, &info);
            if !template_name_remap.is_empty() {
                remap_classlike_info_template_names(&mut info, &template_name_remap);
            }

            if existing.parent_class.is_none() {
                existing.parent_class = info.parent_class;
            }

            existing.interfaces.extend(info.interfaces);
            existing.used_traits.extend(info.used_traits);
            existing
                .trait_method_aliases
                .extend(info.trait_method_aliases);
            existing.method_names.extend(info.method_names);

            for (method_name, method_info) in info.methods {
                if let Some(existing_method_info) = existing.methods.get_mut(&method_name) {
                    if functionlike_info_quality(&method_info)
                        > functionlike_info_quality(existing_method_info)
                    {
                        *existing_method_info = method_info;
                    } else {
                        merge_functionlike_info(existing_method_info, method_info);
                    }
                } else {
                    existing.methods.insert(method_name, method_info);
                }
            }

            for (method_name, method_info) in info.pseudo_methods {
                if let Some(existing_method_info) = existing.pseudo_methods.get_mut(&method_name) {
                    if functionlike_info_quality(&method_info)
                        > functionlike_info_quality(existing_method_info)
                    {
                        *existing_method_info = method_info;
                    } else {
                        merge_functionlike_info(existing_method_info, method_info);
                    }
                } else {
                    existing.pseudo_methods.insert(method_name, method_info);
                }
            }

            for (method_name, method_info) in info.pseudo_static_methods {
                if let Some(existing_method_info) =
                    existing.pseudo_static_methods.get_mut(&method_name)
                {
                    if functionlike_info_quality(&method_info)
                        > functionlike_info_quality(existing_method_info)
                    {
                        *existing_method_info = method_info;
                    } else {
                        merge_functionlike_info(existing_method_info, method_info);
                    }
                } else {
                    existing
                        .pseudo_static_methods
                        .insert(method_name, method_info);
                }
            }

            for (prop_name, prop_info) in info.properties {
                existing.properties.entry(prop_name).or_insert(prop_info);
            }

            for (prop_name, prop_type) in info.pseudo_property_set_types {
                existing
                    .pseudo_property_set_types
                    .entry(prop_name)
                    .or_insert(prop_type);
            }

            for (prop_name, prop_type) in info.pseudo_property_get_types {
                existing
                    .pseudo_property_get_types
                    .entry(prop_name)
                    .or_insert(prop_type);
            }

            for (const_name, const_info) in info.constants {
                existing.constants.entry(const_name).or_insert(const_info);
            }

            if class_template_types_quality(&info.template_types, &info.named_mixins)
                > class_template_types_quality(&existing.template_types, &existing.named_mixins)
            {
                existing.template_types = info.template_types;
            }

            for (classlike_name, offsets) in info.template_extended_offsets {
                existing
                    .template_extended_offsets
                    .entry(classlike_name)
                    .or_insert(offsets);
            }

            for (classlike_name, template_map) in info.template_extended_params {
                existing
                    .template_extended_params
                    .entry(classlike_name)
                    .or_insert(template_map);
            }

            existing.is_final |= info.is_final;
            existing.is_abstract |= info.is_abstract;
            existing.is_readonly |= info.is_readonly;
            existing.is_immutable |= info.is_immutable;
            existing.is_external_mutation_free |= info.is_external_mutation_free;
            existing.is_deprecated |= info.is_deprecated;
            existing.is_internal |= info.is_internal;
            for internal_scope in info.internal {
                if !existing.internal.contains(&internal_scope) {
                    existing.internal.push(internal_scope);
                }
            }
            for mixin in info.named_mixins {
                if !existing.named_mixins.contains(&mixin) {
                    existing.named_mixins.push(mixin);
                }
            }
            existing.docblock_issues.extend(info.docblock_issues);
            existing
                .duplicate_property_issues
                .extend(info.duplicate_property_issues);

            if existing.mixin_declaring_class.is_none() {
                existing.mixin_declaring_class = info.mixin_declaring_class;
            }

            if existing.sealed_methods.is_none() {
                existing.sealed_methods = info.sealed_methods;
            }

            if existing.sealed_properties.is_none() {
                existing.sealed_properties = info.sealed_properties;
            }

            if existing.deprecation_message.is_none() {
                existing.deprecation_message = info.deprecation_message;
            }

            return;
        }

        self.classlike_infos.insert(info.name, info);
    }

    /// Register a function in the codebase.
    pub fn register_function(&mut self, info: FunctionLikeInfo) {
        if let Some(existing) = self.functionlike_infos.get_mut(&info.name) {
            let existing_prec = file_precedence(&self.files, existing.file_path);
            let incoming_prec = file_precedence(&self.files, info.file_path);

            // A higher-precedence source (project code > curated stubs >
            // phpstorm-derived `extensions/*` stubs) replaces a lower one outright;
            // a lower-precedence source is ignored. Mirrors Psalm's stub precedence.
            if incoming_prec > existing_prec {
                *existing = info;
                return;
            }
            if incoming_prec < existing_prec {
                return;
            }

            if functionlike_info_quality(&info) > functionlike_info_quality(existing) {
                *existing = info;
            } else {
                merge_functionlike_info(existing, info);
            }
            return;
        }

        self.functionlike_infos.insert(info.name, info);
    }
}

/// Precedence tier of a declaration's source file (higher wins). Mirrors Psalm's
/// stub precedence: project code > pzoom's curated stubs > phpstorm-derived
/// (`stubs/extensions/*`) stubs.
fn file_precedence(files: &FxHashMap<StrId, FileInfo>, file_path: StrId) -> u8 {
    match files.get(&file_path) {
        Some(f) if !f.is_stub => 3,
        Some(f) if !f.is_low_precedence_stub => 2,
        Some(_) => 1,
        None => 2,
    }
}

fn functionlike_info_quality(info: &FunctionLikeInfo) -> usize {
    let mut score = 0usize;

    // Prefer richer generic declarations (e.g. CoreGeneric stubs).
    score += info.template_types.len() * 1_000;
    score += info.assertions.len() * 10;
    score += info.if_true_assertions.len() * 10;
    score += info.if_false_assertions.len() * 10;

    if info.is_pure {
        score += 10;
    }

    if info.is_mutation_free {
        score += 10;
    }

    if let Some(return_type) = &info.return_type {
        score += 50;
        if info
            .signature_return_type
            .as_ref()
            .is_some_and(|sig| sig != return_type)
        {
            score += 50;
        }
    }

    if info
        .return_type
        .as_ref()
        .is_some_and(|return_type| return_type.types.iter().any(|atomic| {
            matches!(atomic, crate::TAtomic::TConditional(_))
        }))
    {
        score += 100;
    }
    if info.if_this_is_type.is_some() {
        score += 80;
    }

    for param in &info.params {
        if param.param_type.is_some() {
            score += 5;
            if param
                .param_type
                .as_ref()
                .is_some_and(union_has_top_level_template_param)
            {
                score += 40;
            }
        }
        if param.signature_type.is_some() {
            score += 3;
        }
        if param.has_docblock_type {
            score += 20;
        }
        if param.param_type.is_some() && param.param_type != param.signature_type {
            score += 15;
        }
    }

    score
}

fn merge_functionlike_info(existing: &mut FunctionLikeInfo, incoming: FunctionLikeInfo) {
    if existing.template_types.is_empty() && !incoming.template_types.is_empty() {
        existing.template_types = incoming.template_types;
    }

    if existing.return_type.is_none() && incoming.return_type.is_some() {
        existing.return_type = incoming.return_type;
    }

    if existing.if_this_is_type.is_none() && incoming.if_this_is_type.is_some() {
        existing.if_this_is_type = incoming.if_this_is_type;
    }

    if existing.signature_return_type.is_none() && incoming.signature_return_type.is_some() {
        existing.signature_return_type = incoming.signature_return_type;
    }

    if !existing.is_pure && incoming.is_pure {
        existing.is_pure = true;
    }

    if !existing.is_mutation_free && incoming.is_mutation_free {
        existing.is_mutation_free = true;
    }

    if !existing.no_named_arguments && incoming.no_named_arguments {
        existing.no_named_arguments = true;
    }

    if existing.assertions.is_empty() && !incoming.assertions.is_empty() {
        existing.assertions = incoming.assertions;
    }

    if existing.if_true_assertions.is_empty() && !incoming.if_true_assertions.is_empty() {
        existing.if_true_assertions = incoming.if_true_assertions;
    }

    if existing.if_false_assertions.is_empty() && !incoming.if_false_assertions.is_empty() {
        existing.if_false_assertions = incoming.if_false_assertions;
    }

    existing.docblock_issues.extend(incoming.docblock_issues);

    if !existing.is_internal && incoming.is_internal {
        existing.is_internal = true;
    }

    for internal_scope in incoming.internal {
        if !existing.internal.contains(&internal_scope) {
            existing.internal.push(internal_scope);
        }
    }

    if incoming.params.len() > existing.params.len() {
        existing
            .params
            .resize(incoming.params.len(), Default::default());
    }

    for (idx, incoming_param) in incoming.params.into_iter().enumerate() {
        let existing_param = &mut existing.params[idx];

        let incoming_param_has_template = incoming_param
            .param_type
            .as_ref()
            .is_some_and(union_has_top_level_template_param);
        let existing_param_has_template = existing_param
            .param_type
            .as_ref()
            .is_some_and(union_has_top_level_template_param);

        if existing_param.param_type.is_none()
            || (incoming_param_has_template && !existing_param_has_template)
        {
            if let Some(incoming_param_type) = incoming_param.param_type {
                existing_param.param_type = Some(incoming_param_type);
            }
        }

        if existing_param.param_out_type.is_none() && incoming_param.param_out_type.is_some() {
            existing_param.param_out_type = incoming_param.param_out_type;
        }

        if existing_param.signature_type.is_none() && incoming_param.signature_type.is_some() {
            existing_param.signature_type = incoming_param.signature_type;
        }

        if !existing_param.has_docblock_type && incoming_param.has_docblock_type {
            existing_param.has_docblock_type = true;
        }

        existing_param.is_optional |= incoming_param.is_optional;
        existing_param.is_variadic |= incoming_param.is_variadic;
        existing_param.by_ref |= incoming_param.by_ref;
        existing_param.is_promoted |= incoming_param.is_promoted;
    }
}

fn class_template_types_quality(
    template_types: &[crate::class_like_info::TemplateType],
    named_mixins: &[TAtomic],
) -> usize {
    let mut score = template_types.len() * 100;

    score += template_types
        .iter()
        .filter(|template_type| !template_type.as_type.is_mixed())
        .count()
        * 20;

    let mut mixin_template_names = FxHashSet::default();
    for mixin in named_mixins {
        collect_template_names_from_atomic(mixin, &mut mixin_template_names);
    }

    if !mixin_template_names.is_empty() {
        let covered_count = template_types
            .iter()
            .filter(|template_type| mixin_template_names.contains(&template_type.name))
            .count();
        score += covered_count * 500;

        if covered_count == mixin_template_names.len() {
            score += 5_000;
        }
    }

    score
}

fn collect_template_names_from_atomic(atomic: &TAtomic, template_names: &mut FxHashSet<StrId>) {
    match atomic {
        TAtomic::TTemplateParam { name, .. } | TAtomic::TTemplateParamClass { name, .. } => {
            template_names.insert(*name);
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => {
            for type_param in type_params {
                for nested in &type_param.types {
                    collect_template_names_from_atomic(nested, template_names);
                }
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                collect_template_names_from_atomic(nested, template_names);
            }
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => collect_template_names_from_atomic(as_type, template_names),
        _ => {}
    }
}

fn get_class_template_name_remap(
    existing: &ClassLikeInfo,
    incoming: &ClassLikeInfo,
) -> FxHashMap<StrId, StrId> {
    let mut remap = FxHashMap::default();

    if existing.template_types.is_empty()
        || incoming.template_types.is_empty()
        || existing.template_types.len() != incoming.template_types.len()
    {
        return remap;
    }

    for (incoming_template, existing_template) in incoming
        .template_types
        .iter()
        .zip(existing.template_types.iter())
    {
        if incoming_template.name != existing_template.name {
            remap.insert(incoming_template.name, existing_template.name);
        }
    }

    remap
}

fn remap_classlike_info_template_names(info: &mut ClassLikeInfo, remap: &FxHashMap<StrId, StrId>) {
    for template_type in &mut info.template_types {
        if let Some(mapped_name) = remap.get(&template_type.name) {
            template_type.name = *mapped_name;
        }
        remap_union_template_names(&mut template_type.as_type, remap);
    }

    for method_info in info.methods.values_mut() {
        remap_functionlike_info_template_names(method_info, remap);
    }

    for method_info in info.pseudo_methods.values_mut() {
        remap_functionlike_info_template_names(method_info, remap);
    }

    for method_info in info.pseudo_static_methods.values_mut() {
        remap_functionlike_info_template_names(method_info, remap);
    }

    for property_info in info.properties.values_mut() {
        if let Some(property_type) = property_info.property_type.as_mut() {
            remap_union_template_names(property_type, remap);
        }
        if let Some(signature_type) = property_info.signature_type.as_mut() {
            remap_union_template_names(signature_type, remap);
        }
    }

    for pseudo_property_type in info.pseudo_property_set_types.values_mut() {
        remap_union_template_names(pseudo_property_type, remap);
    }

    for pseudo_property_type in info.pseudo_property_get_types.values_mut() {
        remap_union_template_names(pseudo_property_type, remap);
    }

    for template_offsets in info.template_extended_offsets.values_mut() {
        for offset_type in template_offsets {
            remap_union_template_names(offset_type, remap);
        }
    }

    for template_map in info.template_extended_params.values_mut() {
        for template_type in template_map.values_mut() {
            remap_union_template_names(template_type, remap);
        }
    }

    for mixin_type in &mut info.named_mixins {
        remap_atomic_template_names(mixin_type, remap);
    }
}

fn remap_functionlike_info_template_names(
    info: &mut FunctionLikeInfo,
    remap: &FxHashMap<StrId, StrId>,
) {
    for template_type in &mut info.template_types {
        if let Some(mapped_name) = remap.get(&template_type.name) {
            template_type.name = *mapped_name;
        }
        remap_union_template_names(&mut template_type.as_type, remap);
    }

    if let Some(return_type) = info.return_type.as_mut() {
        remap_union_template_names(return_type, remap);
    }
    if let Some(signature_return_type) = info.signature_return_type.as_mut() {
        remap_union_template_names(signature_return_type, remap);
    }

    if let Some(if_this_is_type) = info.if_this_is_type.as_mut() {
        remap_union_template_names(if_this_is_type, remap);
    }

    for param in &mut info.params {
        if let Some(param_type) = param.param_type.as_mut() {
            remap_union_template_names(param_type, remap);
        }
        if let Some(param_out_type) = param.param_out_type.as_mut() {
            remap_union_template_names(param_out_type, remap);
        }
        if let Some(signature_type) = param.signature_type.as_mut() {
            remap_union_template_names(signature_type, remap);
        }
        if let Some(default_type) = param.default_type.as_mut() {
            remap_union_template_names(default_type, remap);
        }
    }

    for assertion in &mut info.assertions {
        remap_assertion_type_template_names(&mut assertion.assertion_type, remap);
    }
    for assertion in &mut info.if_true_assertions {
        remap_assertion_type_template_names(&mut assertion.assertion_type, remap);
    }
    for assertion in &mut info.if_false_assertions {
        remap_assertion_type_template_names(&mut assertion.assertion_type, remap);
    }
}

fn remap_assertion_type_template_names(
    assertion_type: &mut AssertionType,
    remap: &FxHashMap<StrId, StrId>,
) {
    match assertion_type {
        AssertionType::IsType(union)
        | AssertionType::IsEqual(union)
        | AssertionType::IsLooselyEqual(union)
        | AssertionType::IsNotType(union)
        | AssertionType::IsNotEqual(union)
        | AssertionType::IsNotLooselyEqual(union) => {
            remap_union_template_names(union, remap);
        }
        AssertionType::Truthy
        | AssertionType::Falsy
        | AssertionType::NotNull
        | AssertionType::NotEmpty => {}
    }
}

fn remap_union_template_names(union: &mut TUnion, remap: &FxHashMap<StrId, StrId>) {
    for atomic in &mut union.types {
        remap_atomic_template_names(atomic, remap);
    }
}

fn remap_atomic_template_names(atomic: &mut TAtomic, remap: &FxHashMap<StrId, StrId>) {
    match atomic {
        TAtomic::TConditional(conditional) => {
            match &mut conditional.condition {
                ConditionalReturnCondition::TemplateIs {
                    template_name,
                    asserted_type,
                } => {
                    if let Some(mapped_name) = remap.get(template_name) {
                        *template_name = *mapped_name;
                    }
                    remap_union_template_names(asserted_type, remap);
                }
                ConditionalReturnCondition::ParamIs { asserted_type, .. } => {
                    remap_union_template_names(asserted_type, remap);
                }
                ConditionalReturnCondition::FuncNumArgsIs { .. } => {}
            }
            remap_union_template_names(&mut conditional.if_true_type, remap);
            remap_union_template_names(&mut conditional.if_false_type, remap);
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
            remap_union_template_names(key_type, remap);
            remap_union_template_names(value_type, remap);
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            remap_union_template_names(value_type, remap);
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for property_type in properties.values_mut() {
                remap_union_template_names(property_type, remap);
            }
            if let Some(fallback_key_type) = fallback_key_type.as_mut() {
                remap_union_template_names(fallback_key_type, remap);
            }
            if let Some(fallback_value_type) = fallback_value_type.as_mut() {
                remap_union_template_names(fallback_value_type, remap);
            }
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => {
            for type_param in type_params {
                remap_union_template_names(type_param, remap);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                remap_atomic_template_names(nested, remap);
            }
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
            if let Some(params) = params {
                for param in params {
                    remap_union_template_names(&mut param.param_type, remap);
                }
            }

            if let Some(return_type) = return_type {
                remap_union_template_names(return_type, remap);
            }
        }
        TAtomic::TTemplateParam { name, as_type, .. } => {
            if let Some(mapped_name) = remap.get(name) {
                *name = *mapped_name;
            }
            remap_union_template_names(as_type, remap);
        }
        TAtomic::TTemplateParamClass { name, as_type, .. } => {
            if let Some(mapped_name) = remap.get(name) {
                *name = *mapped_name;
            }
            remap_atomic_template_names(as_type, remap);
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            remap_atomic_template_names(as_type, remap);
        }
        _ => {}
    }
}

fn union_has_top_level_template_param(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
        )
    })
}
