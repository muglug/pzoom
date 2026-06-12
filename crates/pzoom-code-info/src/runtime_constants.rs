//! Types of PHP's runtime-defined global constants.
//!
//! Their stub declarations are self-referential placeholders
//! (`const PHP_BINARY = PHP_BINARY;`), so scan-time inference can't type
//! them; this table mirrors Psalm's `ConstFetchAnalyzer::getGlobalConstType`
//! and is applied when the declaration collector stores them, so every
//! consumer (constant fetches, enum case-value checks, ...) sees the same
//! types from `codebase.constants`.

use crate::{TAtomic, TUnion};

/// The type of a PHP runtime global constant (by lowercased name), or `None`
/// for constants whose stub initializer types them.
pub fn runtime_global_constant_type(name_lowercase: &str) -> Option<TUnion> {
    Some(match name_lowercase {
        "php_version" | "php_extra_version" => TUnion::new(TAtomic::TNonEmptyString),
        "php_major_version"
        | "php_minor_version"
        | "php_release_version"
        | "php_int_min"
        | "php_float_dig"
        | "php_debug"
        | "php_zts" => TUnion::int(),
        "php_version_id" | "php_int_max" | "php_int_size" | "php_maxpathlen" => {
            TUnion::new(TAtomic::TIntRange {
                min: Some(1),
                max: None,
            })
        }
        "php_float_epsilon" | "php_float_max" | "php_float_min" => TUnion::float(),
        "php_os" | "php_os_family" => TUnion::string(),
        "php_sapi" | "php_binary" => TUnion::new(TAtomic::TNonEmptyString),
        // Psalm types the separators and PHP_EOL as TSingleLetter (a
        // one-character, hence non-empty, string).
        "php_eol" | "directory_separator" | "path_separator" => {
            TUnion::new(TAtomic::TNonEmptyString)
        }
        _ => return None,
    })
}
