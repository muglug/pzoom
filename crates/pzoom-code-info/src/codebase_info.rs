//! Codebase-wide information storage.
//!
//! Stores all collected type information about the codebase.

use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    ClassLikeInfo, FunctionLikeInfo, TAtomic, TUnion, class_type_alias::ClassTypeAlias,
    functionlike_info::AssertionType,
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

    /// Function names where a project declaration replaced a stub's entry
    /// during registration (Psalm's DuplicateFunction for core functions —
    /// the higher-precedence project definition wins the storage slot, so the
    /// clash must be remembered here for the analyzer to report).
    #[serde(default)]
    pub redefined_stub_functions: FxHashSet<StrId>,

    /// Functions whose `if (!function_exists(...))` polyfill declaration was
    /// dropped in favor of an existing definition — their declarations are
    /// not duplicates.
    #[serde(default)]
    pub conditionally_skipped_functions: FxHashSet<StrId>,

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

    /// Interned lowercase classlike name -> interned correctly-cased name, only
    /// for classlikes whose declared name differs from its lowercase form.
    /// pzoom resolves classlike names case-sensitively; this map recovers the
    /// declared casing so UndefinedClass/UndefinedDocblockClass messages can
    /// point at it. Populated during the populate phase.
    #[serde(default)]
    pub classlike_lc_names: FxHashMap<StrId, StrId>,

    /// Same as `classlike_lc_names` for top-level functions (UndefinedFunction
    /// messages). Populated during the populate phase.
    #[serde(default)]
    pub functionlike_lc_names: FxHashMap<StrId, StrId>,

    /// `define()` calls collected anywhere in scanned code (Psalm's
    /// ExpressionScanner). Registered as global constants after populate when
    /// the config sets `allConstantsGlobal` (Psalm's `addGlobalConstantType`).
    #[serde(default)]
    pub global_defines: Vec<GlobalDefine>,

    /// Case-insensitive top-level function lookup (declared names lowercased,
    /// leading backslash trimmed). Populated during the populate phase; lets
    /// string-only contexts (the scalar comparator checking a literal against
    /// `callable-string`) resolve function existence without an interner.
    #[serde(default)]
    pub functionlike_name_lookup: FxHashMap<String, StrId>,
}

/// Information about a global constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub name: StrId,
    pub constant_type: TUnion,
    pub file_path: StrId,
    pub start_offset: u32,
    /// Initializer parts unresolvable at scan time (`const X = Other::CONST;`)
    /// — the populator resolves them once every class is known.
    #[serde(default)]
    pub unresolved_initializer: Option<crate::class_constant_info::UnresolvedConstExpr>,
}

/// A `define()` call collected at scan time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlobalDefine {
    pub name: StrId,
    pub value: GlobalDefineValue,
    pub file_path: StrId,
    pub start_offset: u32,
}

/// The defined value of a scan-time `define()`. Psalm's SimpleTypeInferer
/// leaves call values mixed, but the constants Psalm-on-itself resolves at
/// runtime via `get_defined_constants()` (e.g. PSALM_VERSION) come from
/// single calls, so pzoom remembers the callee and substitutes its declared
/// return type once the codebase is populated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GlobalDefineValue {
    Resolved(TUnion),
    /// Value is a call to a named function; use its declared return type.
    FunctionReturn(StrId),
    /// Value is a static method call; use its declared return type.
    MethodReturn(StrId, StrId),
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

    /// Case-insensitive top-level function resolution by declared name.
    pub fn resolve_functionlike_name(&self, name: &str) -> Option<StrId> {
        let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
        self.functionlike_name_lookup.get(&normalized).copied()
    }

    /// Find the correctly-cased classlike for a reference that failed exact
    /// lookup. Returns the declared name when it differs only by case from
    /// `requested` (for "did you mean" diagnostics); never returns `requested`.
    pub fn cased_classlike_for(
        &self,
        interner: &pzoom_str::Interner,
        requested: StrId,
    ) -> Option<StrId> {
        let lc = interner.lookup(requested).to_ascii_lowercase();
        let lc_id = interner.intern(&lc);
        let cased = if self.classlike_infos.contains_key(&lc_id) {
            lc_id
        } else {
            *self.classlike_lc_names.get(&lc_id)?
        };
        (cased != requested).then_some(cased)
    }

    /// Same as [`Self::cased_classlike_for`] for top-level functions.
    pub fn cased_functionlike_for(
        &self,
        interner: &pzoom_str::Interner,
        requested: StrId,
    ) -> Option<StrId> {
        let lc = interner.lookup(requested).to_ascii_lowercase();
        let lc_id = interner.intern(&lc);
        let cased = if self.functionlike_infos.contains_key(&lc_id) {
            lc_id
        } else {
            *self.functionlike_lc_names.get(&lc_id)?
        };
        (cased != requested).then_some(cased)
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
                let incoming_in_project_dirs = self
                    .files
                    .get(&info.file_path)
                    .is_none_or(|file_info| file_info.is_in_project_dirs);

                // The real declaration becomes the base — its hierarchy and real
                // members are authoritative — and the stub *augments* it. A stub
                // always contributes its magic members (`@property`/`@method`/
                // `@mixin`), letting a stub provider annotate a class it doesn't
                // own (e.g. a generated `@property` for an Eloquent model). A
                // dependency class additionally takes the real members the stub
                // declares (Psalm's "stub files override the original
                // definitions", e.g. phpparser.phpstub refining vendor php-parser
                // signatures); a project class keeps its own real members, since
                // Psalm holds analyzed-project declarations authoritative — and
                // built-in stubs carry no magic members, so a project polyfill of
                // a stubbed name is unaffected.
                let stub = std::mem::replace(existing, info);
                if !incoming_in_project_dirs {
                    existing.is_stubbed = true;
                }
                merge_stub_into_real(existing, stub, !incoming_in_project_dirs);
                return;
            }

            if !existing_is_stub && incoming_is_stub {
                // The mirror case — a stub scanned after the real class augments
                // it the same way.
                let existing_in_project_dirs = self
                    .files
                    .get(&existing.file_path)
                    .is_none_or(|file_info| file_info.is_in_project_dirs);
                existing.is_stubbed = true;
                merge_stub_into_real(existing, info, !existing_in_project_dirs);
                return;
            }

            // Psalm's scanning is composer-autoload driven, so it only ever
            // sees ONE declaration of a class per run; a duplicate in another
            // dependency file (e.g. psalm/plugin-mockery's conditional trait
            // stubs redeclaring Mockery's trait) never loads. Approximate
            // that: among non-stub files, a redeclaration outside the project
            // dirs never merges — the project declaration (or the first
            // dependency one) wins.
            if !existing_is_stub && !incoming_is_stub {
                let incoming_in_project_dirs = self
                    .files
                    .get(&info.file_path)
                    .is_none_or(|file_info| file_info.is_in_project_dirs);
                let existing_in_project_dirs = self
                    .files
                    .get(&existing.file_path)
                    .is_none_or(|file_info| file_info.is_in_project_dirs);
                if !incoming_in_project_dirs {
                    return;
                }
                if !existing_in_project_dirs {
                    *existing = info;
                    return;
                }
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
                        merge_functionlike_info(
                            std::sync::Arc::make_mut(existing_method_info),
                            std::sync::Arc::try_unwrap(method_info)
                                .unwrap_or_else(|shared| (*shared).clone()),
                            false,
                        );
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
                        merge_functionlike_info(existing_method_info, method_info, false);
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
                        merge_functionlike_info(existing_method_info, method_info, false);
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
            // An `if (!function_exists(...))` polyfill never runs when the
            // function already exists: keep the existing definition, no
            // DuplicateFunction (Psalm skips the branch entirely).
            if info.declared_if_not_exists {
                self.conditionally_skipped_functions.insert(info.name);
                return;
            }
            let existing_prec = file_precedence(&self.files, existing.file_path);
            let incoming_prec = file_precedence(&self.files, info.file_path);

            // A higher-precedence source (project code > curated stubs >
            // phpstorm-derived `extensions/*` stubs) takes the storage slot; a
            // lower-precedence source is ignored. Mirrors Psalm's stub
            // precedence — except a stub *augments* a real function it shares a
            // name with, folding in the type info (return/param types,
            // assertions) the real declaration lacks rather than being discarded.
            if incoming_prec > existing_prec {
                if incoming_prec == 3 {
                    // The arriving real declaration takes the slot but keeps the
                    // stub's extra type info. Project code redefining a stubbed
                    // function is also Psalm's DuplicateFunction — remember the
                    // clash. When the real declaration is a vendor (non-project)
                    // one, the stub stays authoritative for the return type.
                    self.redefined_stub_functions.insert(info.name);
                    let stub_overrides_return = file_is_stub(&self.files, existing.file_path)
                        && !file_is_in_project_dirs(&self.files, info.file_path);
                    let stub = std::mem::replace(existing, info);
                    merge_functionlike_info(existing, stub, stub_overrides_return);
                } else {
                    // Stub over a lower-precedence stub: replace outright.
                    *existing = info;
                }
                return;
            }
            if incoming_prec < existing_prec {
                // A stub for an existing real function augments it; a lower stub
                // beneath a higher stub is ignored. A stub augmenting a vendor
                // (non-project) declaration overrides its return type.
                if existing_prec == 3 {
                    let stub_overrides_return = file_is_stub(&self.files, info.file_path)
                        && !file_is_in_project_dirs(&self.files, existing.file_path);
                    merge_functionlike_info(existing, info, stub_overrides_return);
                }
                return;
            }

            if functionlike_info_quality(&info) > functionlike_info_quality(existing) {
                *existing = info;
            } else {
                merge_functionlike_info(existing, info, false);
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

/// Whether a declaration's source file is a stub file.
fn file_is_stub(files: &FxHashMap<StrId, FileInfo>, file_path: StrId) -> bool {
    files.get(&file_path).is_some_and(|f| f.is_stub)
}

/// Whether a declaration's source file lives in the analyzed project (as opposed
/// to a vendored dependency). A missing file entry is treated as in-project, so
/// the stub-override path stays conservative and only fires for declarations
/// known to be vendored. Mirrors the convention used by `register_class`.
fn file_is_in_project_dirs(files: &FxHashMap<StrId, FileInfo>, file_path: StrId) -> bool {
    files.get(&file_path).is_none_or(|f| f.is_in_project_dirs)
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

    if info.return_type.as_ref().is_some_and(|return_type| {
        return_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, crate::TAtomic::TConditional(_)))
    }) {
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

/// Fold a `stub`'s members onto a real (project or dependency) class `base`,
/// additively — a stub *augments* the real class rather than replacing it.
///
/// The stub's magic members are always merged in (the real class's own
/// declarations win on a clash): `@method` (pseudo methods), `@property`
/// (pseudo-property get/set types) and `@mixin`s. Real (non-magic) members are
/// merged only when `include_real_members` is set — for dependency sources,
/// where a stub may override the original definition; a project class keeps its
/// own real members. Built-in stubs declare no magic members, so a project
/// polyfill of a stubbed name is unaffected.
fn merge_stub_into_real(base: &mut ClassLikeInfo, stub: ClassLikeInfo, include_real_members: bool) {
    if include_real_members {
        for (method_name, method_info) in stub.methods {
            base.method_names.insert(method_name);
            base.methods.insert(method_name, method_info);
        }
        for (prop_name, prop_info) in stub.properties {
            base.properties.insert(prop_name, prop_info);
        }
        for (const_name, const_info) in stub.constants {
            base.constants.insert(const_name, const_info);
        }
    }

    for (method_name, method_info) in stub.pseudo_methods {
        base.pseudo_methods
            .entry(method_name)
            .or_insert(method_info);
    }
    for (method_name, method_info) in stub.pseudo_static_methods {
        base.pseudo_static_methods
            .entry(method_name)
            .or_insert(method_info);
    }
    for (prop_name, prop_type) in stub.pseudo_property_get_types {
        base.pseudo_property_get_types
            .entry(prop_name)
            .or_insert(prop_type);
    }
    for (prop_name, prop_type) in stub.pseudo_property_set_types {
        base.pseudo_property_set_types
            .entry(prop_name)
            .or_insert(prop_type);
    }
    for mixin in stub.named_mixins {
        if !base.named_mixins.contains(&mixin) {
            base.named_mixins.push(mixin);
        }
    }
    if base.template_types.is_empty() && !stub.template_types.is_empty() {
        base.template_types = stub.template_types;
    }
}

/// Fold `incoming`'s type info into `existing`. `incoming_overrides_return` is
/// set when `incoming` is a stub augmenting a *non-project* (vendor/dependency)
/// real declaration: Psalm holds stub files authoritative over the original
/// definitions, so the stub's return type displaces the vendor's rather than
/// merely filling a gap. A project declaration stays authoritative for its own
/// return type, so the flag is left false there and the stub only fills a
/// missing type.
fn merge_functionlike_info(
    existing: &mut FunctionLikeInfo,
    incoming: FunctionLikeInfo,
    incoming_overrides_return: bool,
) {
    if existing.template_types.is_empty() && !incoming.template_types.is_empty() {
        existing.template_types = incoming.template_types;
    }

    if (existing.return_type.is_none() || incoming_overrides_return)
        && incoming.return_type.is_some()
    {
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

        if (existing_param.param_type.is_none()
            || (incoming_param_has_template && !existing_param_has_template))
            && let Some(incoming_param_type) = incoming_param.param_type
        {
            existing_param.param_type = Some(incoming_param_type);
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
        remap_functionlike_info_template_names(std::sync::Arc::make_mut(method_info), remap);
    }

    for method_info in info.pseudo_methods.values_mut() {
        remap_functionlike_info_template_names(method_info, remap);
    }

    for method_info in info.pseudo_static_methods.values_mut() {
        remap_functionlike_info_template_names(method_info, remap);
    }

    for property_info in info.properties.values_mut() {
        let property_info = std::sync::Arc::make_mut(property_info);
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
            if let Some(mapped_name) = remap.get(&conditional.param_name) {
                conditional.param_name = *mapped_name;
            }
            remap_union_template_names(&mut conditional.as_type, remap);
            remap_union_template_names(&mut conditional.conditional_type, remap);
            remap_union_template_names(&mut conditional.if_true_type, remap);
            remap_union_template_names(&mut conditional.if_false_type, remap);
        }
        TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            remap_union_template_names(key_type, remap);
            remap_union_template_names(value_type, remap);
        }
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            for (_, value) in std::sync::Arc::make_mut(known_values).values_mut() {
                remap_union_template_names(value, remap);
            }
            if let Some(params) = params.as_mut() {
                remap_union_template_names(&mut params.0, remap);
                remap_union_template_names(&mut params.1, remap);
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

#[cfg(test)]
mod stub_augmentation_tests {
    use super::*;
    use crate::file_info::FileInfo;
    use crate::functionlike_info::FunctionLikeInfo;
    use crate::t_atomic::TAtomic;
    use crate::t_union::TUnion;

    fn file(path: StrId, is_stub: bool, in_project: bool) -> FileInfo {
        FileInfo {
            path,
            classes: Vec::new(),
            functions: Vec::new(),
            constants: Vec::new(),
            content_hash: String::new(),
            contents: String::new(),
            parse_errors: Vec::new(),
            docblock_parse_issues: Vec::new(),
            is_stub,
            is_low_precedence_stub: false,
            is_in_project_dirs: in_project,
            inline_annotations: Default::default(),
            type_alias_imports: Vec::new(),
        }
    }

    /// A stub `@property`/`@mixin` augments a project class while the project
    /// keeps its own (real) members — the case a stub provider relies on.
    #[test]
    fn stub_augments_project_class() {
        let (stub_path, project_path) = (StrId(1), StrId(2));
        let (class_name, real_method, magic_prop) = (StrId(100), StrId(101), StrId(102));

        for (existing_first, label) in [(true, "stub first"), (false, "project first")] {
            let mut codebase = CodebaseInfo::default();
            codebase
                .files
                .insert(stub_path, file(stub_path, true, false));
            codebase
                .files
                .insert(project_path, file(project_path, false, true));

            let mut stub = ClassLikeInfo {
                name: class_name,
                file_path: stub_path,
                ..Default::default()
            };
            stub.pseudo_property_get_types
                .insert(magic_prop, TUnion::new(TAtomic::TString));
            stub.named_mixins.push(TAtomic::TString);

            let mut project = ClassLikeInfo {
                name: class_name,
                file_path: project_path,
                ..Default::default()
            };
            project.method_names.insert(real_method);

            // Augmentation must hold regardless of scan order.
            if existing_first {
                codebase.register_class(stub);
                codebase.register_class(project);
            } else {
                codebase.register_class(project);
                codebase.register_class(stub);
            }

            let merged = codebase.classlike_infos.get(&class_name).unwrap();
            assert_eq!(
                merged.file_path, project_path,
                "{label}: real class is base"
            );
            assert!(
                merged.method_names.contains(&real_method),
                "{label}: project member kept",
            );
            assert!(
                merged.pseudo_property_get_types.contains_key(&magic_prop),
                "{label}: stub @property merged in",
            );
            assert!(
                merged.named_mixins.contains(&TAtomic::TString),
                "{label}: stub @mixin merged in",
            );
        }
    }

    /// A stub return type fills in a project function that lacks one, without
    /// displacing the project declaration.
    #[test]
    fn stub_augments_project_function() {
        let (stub_path, project_path, fn_name) = (StrId(1), StrId(2), StrId(200));

        for (existing_first, label) in [(true, "stub first"), (false, "project first")] {
            let mut codebase = CodebaseInfo::default();
            codebase
                .files
                .insert(stub_path, file(stub_path, true, false));
            codebase
                .files
                .insert(project_path, file(project_path, false, true));

            let project = FunctionLikeInfo {
                name: fn_name,
                file_path: project_path,
                return_type: None,
                ..Default::default()
            };
            let stub = FunctionLikeInfo {
                name: fn_name,
                file_path: stub_path,
                return_type: Some(TUnion::new(TAtomic::TString)),
                ..Default::default()
            };

            if existing_first {
                codebase.register_function(stub);
                codebase.register_function(project);
            } else {
                codebase.register_function(project);
                codebase.register_function(stub);
            }

            let merged = codebase.functionlike_infos.get(&fn_name).unwrap();
            assert_eq!(merged.file_path, project_path, "{label}: project is base");
            assert!(
                merged.return_type.is_some(),
                "{label}: stub return type merged in",
            );
        }
    }

    /// A stub return type *overrides* a vendor (non-project) function's own
    /// return type — Psalm holds stub files authoritative over the original
    /// definitions ("stub files override the original definitions").
    #[test]
    fn stub_overrides_vendor_function_return() {
        let (stub_path, vendor_path, fn_name) = (StrId(1), StrId(2), StrId(200));

        for (existing_first, label) in [(true, "stub first"), (false, "vendor first")] {
            let mut codebase = CodebaseInfo::default();
            codebase
                .files
                .insert(stub_path, file(stub_path, true, false));
            // Vendored dependency: a real (non-stub) declaration, not in project.
            codebase
                .files
                .insert(vendor_path, file(vendor_path, false, false));

            let vendor = FunctionLikeInfo {
                name: fn_name,
                file_path: vendor_path,
                return_type: Some(TUnion::new(TAtomic::TInt)),
                ..Default::default()
            };
            let stub = FunctionLikeInfo {
                name: fn_name,
                file_path: stub_path,
                return_type: Some(TUnion::new(TAtomic::TString)),
                ..Default::default()
            };

            if existing_first {
                codebase.register_function(stub);
                codebase.register_function(vendor);
            } else {
                codebase.register_function(vendor);
                codebase.register_function(stub);
            }

            let merged = codebase.functionlike_infos.get(&fn_name).unwrap();
            assert_eq!(merged.file_path, vendor_path, "{label}: vendor is base");
            assert_eq!(
                merged.return_type,
                Some(TUnion::new(TAtomic::TString)),
                "{label}: stub return type overrides the vendor's",
            );
        }
    }

    /// The mirror guarantee: a stub does *not* override a project function's own
    /// declared return type — analyzed-project declarations stay authoritative.
    #[test]
    fn stub_does_not_override_project_function_return() {
        let (stub_path, project_path, fn_name) = (StrId(1), StrId(2), StrId(200));

        for (existing_first, label) in [(true, "stub first"), (false, "project first")] {
            let mut codebase = CodebaseInfo::default();
            codebase
                .files
                .insert(stub_path, file(stub_path, true, false));
            codebase
                .files
                .insert(project_path, file(project_path, false, true));

            let project = FunctionLikeInfo {
                name: fn_name,
                file_path: project_path,
                return_type: Some(TUnion::new(TAtomic::TInt)),
                ..Default::default()
            };
            let stub = FunctionLikeInfo {
                name: fn_name,
                file_path: stub_path,
                return_type: Some(TUnion::new(TAtomic::TString)),
                ..Default::default()
            };

            if existing_first {
                codebase.register_function(stub);
                codebase.register_function(project);
            } else {
                codebase.register_function(project);
                codebase.register_function(stub);
            }

            let merged = codebase.functionlike_infos.get(&fn_name).unwrap();
            assert_eq!(merged.file_path, project_path, "{label}: project is base");
            assert_eq!(
                merged.return_type,
                Some(TUnion::new(TAtomic::TInt)),
                "{label}: project return type is kept",
            );
        }
    }
}
