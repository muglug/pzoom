//! Psalm's CallMap, ported (InternalCallMapHandler).
//!
//! Psalm types every PHP builtin through per-version CallMaps
//! (`dictionaries/CallMap_{70..85}.php`), with its own stub files overriding a
//! small curated subset. pzoom's JetBrains-derived stubs play the CallMap's
//! role structurally, but their docblock types are neither version-aware nor
//! CallMap-accurate, and they carry docblock provenance where Psalm's CallMap
//! types are native.
//!
//! This module applies the real CallMap over the scanned stub functions:
//!
//! - `dictionaries/callmap_85.json` is Psalm's newest composed map; downgrade
//!   deltas (`callmap_deltas.json`) reconstruct each older version exactly,
//!   selected like Psalm by `min(85, max(70, major.minor))`.
//! - Functions Psalm declares in its own stubs (`psalm_stub_functions.json`)
//!   are skipped: pzoom's storage for them came from those same Psalm
//!   docblocks and stays authoritative (templates, conditional returns,
//!   assertions).
//! - For everything else the CallMap signature replaces the stub's param and
//!   return types with non-docblock provenance. Stub-only metadata that
//!   models Psalm's side tables (purity, taint sinks, assertions) is kept.
//! - Project-code redefinitions of builtins are never touched (the storage's
//!   file is no longer a stub file).
//!
//! Methods (`Class::method` entries) and arity-variant signatures
//! (`name'1`) are not applied yet.

use pzoom_code_info::CodebaseInfo;
use pzoom_str::ThreadedInterner;
use rustc_hash::{FxHashMap, FxHashSet};

static CALLMAP_85: &str = include_str!("../../../dictionaries/callmap_85.json");
/// Downgrade deltas, newest first, parallel to [`DELTA_STEPS`].
static CALLMAP_DELTA_SOURCES: [&str; 10] = [
    include_str!("../../../dictionaries/callmap_delta_84.json"),
    include_str!("../../../dictionaries/callmap_delta_83.json"),
    include_str!("../../../dictionaries/callmap_delta_82.json"),
    include_str!("../../../dictionaries/callmap_delta_81.json"),
    include_str!("../../../dictionaries/callmap_delta_80.json"),
    include_str!("../../../dictionaries/callmap_delta_74.json"),
    include_str!("../../../dictionaries/callmap_delta_73.json"),
    include_str!("../../../dictionaries/callmap_delta_72.json"),
    include_str!("../../../dictionaries/callmap_delta_71.json"),
    include_str!("../../../dictionaries/callmap_delta_70.json"),
];
static PSALM_STUB_FUNCTIONS: &str = include_str!("../../../dictionaries/psalm_stub_functions.json");
static PSALM_SPECIAL_FUNCTIONS: &str =
    include_str!("../../../dictionaries/psalm_special_functions.json");

/// `[["", return_type], [param_key, param_type], ...]`
type RawEntry = Vec<(String, String)>;

/// Downgrade steps in application order (newest first). `deltas["84"]`
/// transforms the 85 map into the 84 map, and so on down the chain.
const DELTA_STEPS: [u32; 10] = [84, 83, 82, 81, 80, 74, 73, 72, 71, 70];

#[derive(serde::Deserialize)]
struct Delta {
    set: FxHashMap<String, RawEntry>,
    remove: Vec<String>,
}

/// Psalm's CallMap version selection: `min(85, max(70, major.minor))`.
fn callmap_version(php_version_id: u32) -> u32 {
    let major = php_version_id / 10_000;
    let minor = (php_version_id % 10_000) / 100;
    (major * 10 + minor).clamp(70, 85)
}

/// The composed CallMap for a version, reconstructed from the newest map by
/// walking downgrade deltas (the same composition Psalm ships pre-built).
/// Cached per version — the test runner applies per test.
fn composed_map(version: u32) -> std::sync::Arc<FxHashMap<String, RawEntry>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<FxHashMap<u32, std::sync::Arc<FxHashMap<String, RawEntry>>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(map) = cache.lock().unwrap().get(&version) {
        return std::sync::Arc::clone(map);
    }

    let mut map: FxHashMap<String, RawEntry> =
        serde_json::from_str(CALLMAP_85).expect("dictionaries/callmap_85.json parses");

    if version < 85 {
        for (index, step) in DELTA_STEPS.into_iter().enumerate() {
            if step < version {
                break;
            }
            let delta: Delta = serde_json::from_str(CALLMAP_DELTA_SOURCES[index])
                .expect("callmap delta file parses");
            for name in delta.remove {
                map.remove(&name);
            }
            for (name, entry) in delta.set {
                map.insert(name, entry);
            }
        }
    }

    let map = std::sync::Arc::new(map);
    cache
        .lock()
        .unwrap()
        .insert(version, std::sync::Arc::clone(&map));
    map
}

/// Apply the CallMap for the analysis PHP version over the scanned codebase.
pub fn apply_call_map(
    codebase: &mut CodebaseInfo,
    interner: &ThreadedInterner,
    php_version_id: u32,
) {
    let map = composed_map(callmap_version(php_version_id));
    // Functions whose pzoom stubs stand in for Psalm's own stubs or its
    // dedicated machinery (ReturnTypeProviders, closure-argument special
    // cases): the CallMap signature must not overwrite them.
    let psalm_stubbed: FxHashSet<&str> = {
        let stub_names: Vec<&str> =
            serde_json::from_str(PSALM_STUB_FUNCTIONS).expect("psalm_stub_functions.json parses");
        let special_names: Vec<&str> = serde_json::from_str(PSALM_SPECIAL_FUNCTIONS)
            .expect("psalm_special_functions.json parses");
        stub_names.into_iter().chain(special_names).collect()
    };

    for (name, entry) in map.iter() {
        // Methods and arity variants are not applied yet.
        if name.contains("::") || name.contains('\'') {
            continue;
        }

        let function_id = interner.intern(name);
        // CallMap membership marks the function builtin for purity purposes
        // even when a vendor polyfill supplied the declaration.
        if let Some(info) = codebase.functionlike_infos.get_mut(&function_id) {
            info.in_call_map = true;
        }

        if psalm_stubbed.contains(name.as_str()) {
            continue;
        }

        let Some(info) = codebase.functionlike_infos.get(&function_id) else {
            // Not scanned (e.g. a disabled optional extension): the extension
            // gating decides what exists, not the CallMap.
            continue;
        };
        // A project redefinition of a builtin owns the storage slot.
        let is_stub_storage = codebase
            .files
            .get(&info.file_path)
            .is_some_and(|file_info| file_info.is_stub);
        if !is_stub_storage {
            continue;
        }

        let mut return_type = None;
        let mut param_types: Vec<(usize, &str, &str)> = Vec::new();
        for (index, (key, type_str)) in entry.iter().enumerate() {
            if index == 0 && key.is_empty() {
                return_type = Some(type_str.as_str());
            } else {
                param_types.push((param_types.len(), key.as_str(), type_str.as_str()));
            }
        }

        // Arity/value variants (`name'1`, ...): Psalm picks the matching
        // signature per call; pzoom stores one signature, so each param
        // accepts the union across variants (hrtime's as_number false|true
        // = bool; min's non-empty-array|mixed = mixed). Return types keep
        // the base entry (per-call providers refine where it matters).
        let mut variant_param_types: Vec<Vec<&str>> = vec![Vec::new(); param_types.len()];
        for variant_index in 1..=4u32 {
            let Some(variant_entry) = map.get(&format!("{}'{}", name, variant_index)) else {
                break;
            };
            for (index, (key, type_str)) in variant_entry.iter().enumerate() {
                if index == 0 && key.is_empty() {
                    continue;
                }
                let position = index - 1;
                if let Some(types) = variant_param_types.get_mut(position) {
                    types.push(type_str.as_str());
                }
            }
        }

        let info = codebase
            .functionlike_infos
            .get_mut(&function_id)
            .expect("checked above");

        if let Some(return_type_str) = return_type
            && let Ok(mut parsed) =
                pzoom_syntax::docblock::parse_type_string(return_type_str, interner)
        {
            pzoom_syntax::docblock::clear_union_from_docblock_deep(&mut parsed);
            // The CallMap return is the native signature (Psalm builds the
            // storage with non-docblock provenance); no docblock slot remains.
            info.return_type = None;
            info.signature_return_type = Some(parsed);
        }

        for (position, key, type_str) in param_types {
            let Some(param) = info.params.get_mut(position) else {
                // CallMap lists more optionals than the stub declares; the
                // stub's arity (php-src) wins.
                break;
            };
            let Ok(mut parsed) = pzoom_syntax::docblock::parse_type_string(type_str, interner)
            else {
                continue;
            };
            for variant_type_str in &variant_param_types[position] {
                if let Ok(variant_parsed) =
                    pzoom_syntax::docblock::parse_type_string(variant_type_str, interner)
                {
                    parsed = pzoom_code_info::ttype::type_combiner::combine_union_types(
                        &parsed,
                        &variant_parsed,
                        false,
                    );
                }
            }
            pzoom_syntax::docblock::clear_union_from_docblock_deep(&mut parsed);

            // '&w_name' / '&rw_name': the CallMap type describes what the
            // function writes back through the reference.
            let by_ref_out = key.starts_with("&w_") || key.starts_with("&rw_");
            if by_ref_out && param.param_out_type.is_none() {
                param.param_out_type = Some(parsed.clone());
            }

            param.param_type = None;
            param.has_docblock_type = false;
            param.signature_type = Some(parsed);
        }

        // Any leftover template machinery came from the JetBrains stub (e.g.
        // in_array's `@template V`); Psalm's CallMap functions have none.
        info.template_types.clear();
    }

    // Stable CallMap entries live in the stub files
    // (tools/callmap_to_stubs.php), written as docblock tags because most
    // CallMap types aren't native-hint expressible. They model Psalm's
    // CallMap storage all the same: non-docblock provenance. Flip every stub
    // function outside Psalm's stub/special sets.
    let function_ids: Vec<pzoom_str::StrId> = codebase.functionlike_infos.keys().copied().collect();
    for function_id in function_ids {
        let name = interner.lookup(function_id);
        if name.contains("::") || psalm_stubbed.contains(name.as_ref()) {
            continue;
        }
        let Some(info) = codebase.functionlike_infos.get(&function_id) else {
            continue;
        };
        let is_stub_storage = codebase
            .files
            .get(&info.file_path)
            .is_some_and(|file_info| file_info.is_stub);
        if !is_stub_storage {
            continue;
        }
        let info = codebase
            .functionlike_infos
            .get_mut(&function_id)
            .expect("checked above");
        if let Some(return_type) = info.return_type.as_mut() {
            pzoom_syntax::docblock::clear_union_from_docblock_deep(return_type);
        }
        for param in info.params.iter_mut() {
            if let Some(param_type) = param.param_type.as_mut() {
                pzoom_syntax::docblock::clear_union_from_docblock_deep(param_type);
            }
        }
    }
}
