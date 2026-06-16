//! Enabled-extension resolution for optional extension stubs.
//!
//! Stubs for extensions bundled with PHP are always loaded; stubs under
//! `stubs/extensions/optional/` (PECL/third-party — swoole, redis, xdebug, …)
//! are loaded only when the extension is enabled. Mirrors Psalm, which loads
//! `stubs/extensions/*.phpstub` only for extensions enabled via config or
//! composer.json, with `php -m` standing in for Psalm's in-process
//! `extension_loaded()` checks.
//!
//! Resolution order (later sources only ever add, except disable which wins):
//!  1. `php -m` on the local PHP binary, when one is available
//!  2. a `php.ini` next to the project root (`extension=` / `zend_extension=`)
//!  3. composer.json `require` entries of the form `ext-<name>`
//!  4. psalm.xml `<enableExtensions>` entries
//!  5. psalm.xml `<disableExtensions>` entries remove extensions

use rustc_hash::FxHashSet;
use std::path::Path;
use std::process::Command;

/// Resolve the set of enabled optional extensions for a project.
///
/// `enable`/`disable` come from psalm.xml's `<enableExtensions>` /
/// `<disableExtensions>` elements.
pub fn resolve_enabled_extensions(
    project_root: &Path,
    enable: &[String],
    disable: &[String],
) -> FxHashSet<String> {
    let mut enabled = FxHashSet::default();

    enabled.extend(php_loaded_extensions());
    enabled.extend(php_ini_extensions(project_root));
    enabled.extend(composer_required_extensions(project_root));
    enabled.extend(enable.iter().map(|name| normalize_extension_name(name)));

    for name in disable {
        enabled.remove(&normalize_extension_name(name));
    }

    enabled
}

/// Extensions reported by `php -m`, when a `php` binary is on PATH.
fn php_loaded_extensions() -> FxHashSet<String> {
    let Ok(output) = Command::new("php").arg("-m").output() else {
        return FxHashSet::default();
    };
    if !output.status.success() {
        return FxHashSet::default();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('['))
        .map(normalize_extension_name)
        .collect()
}

/// Extensions enabled by a `php.ini` in the project root.
fn php_ini_extensions(project_root: &Path) -> FxHashSet<String> {
    let mut extensions = FxHashSet::default();
    let ini_path = project_root.join("php.ini");
    let Ok(contents) = std::fs::read_to_string(&ini_path) else {
        return extensions;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !key.eq_ignore_ascii_case("extension") && !key.eq_ignore_ascii_case("zend_extension") {
            continue;
        }
        // Strip an inline comment, quotes, any directory prefix, a `php_`
        // prefix (Windows) and the shared-object suffix:
        //   extension=redis | redis.so | "php_redis.dll" | /usr/lib/php/redis.so
        let value = value.split(';').next().unwrap_or("").trim();
        let value = value.trim_matches(['"', '\'']);
        let value = value.rsplit(['/', '\\']).next().unwrap_or(value);
        let value = value
            .strip_suffix(".so")
            .or_else(|| value.strip_suffix(".dll"))
            .unwrap_or(value);
        let value = value.strip_prefix("php_").unwrap_or(value);
        if !value.is_empty() {
            extensions.insert(normalize_extension_name(value));
        }
    }
    extensions
}

/// Extensions required by composer.json (`"require": {"ext-foo": "*"}`).
fn composer_required_extensions(project_root: &Path) -> FxHashSet<String> {
    let mut extensions = FxHashSet::default();
    let composer_path = project_root.join("composer.json");
    let Ok(contents) = std::fs::read_to_string(&composer_path) else {
        return extensions;
    };
    // Minimal extraction: keys of the top-level "require" object that start
    // with "ext-". A full JSON parser is overkill for this shape.
    let Some(require_start) = contents.find("\"require\"") else {
        return extensions;
    };
    let Some(brace_start) = contents[require_start..].find('{') else {
        return extensions;
    };
    let object_start = require_start + brace_start;
    let mut depth = 0usize;
    let mut object_end = contents.len();
    for (offset, ch) in contents[object_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    object_end = object_start + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    for key_match in contents[object_start..object_end]
        .split('"')
        .skip(1)
        .step_by(2)
    {
        if let Some(name) = key_match.strip_prefix("ext-") {
            extensions.insert(normalize_extension_name(name));
        }
    }
    extensions
}

/// Lowercase and map known aliases onto stub file stems
/// (`php -m` prints e.g. "Zend OPcache").
fn normalize_extension_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match lower.as_str() {
        "zend opcache" => "opcache".to_string(),
        _ => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_php_ini_extension_forms() {
        let dir = std::env::temp_dir().join("pzoom_ext_test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("php.ini"),
            r#"
; a comment
extension=redis
extension=swoole.so
extension="php_memcached.dll"
zend_extension=/usr/lib/php/xdebug.so
extension = imagick ; trailing comment
;extension=disabled_one
memory_limit=512M
"#,
        )
        .unwrap();

        let extensions = php_ini_extensions(&dir);
        for expected in ["redis", "swoole", "memcached", "xdebug", "imagick"] {
            assert!(extensions.contains(expected), "missing {expected}");
        }
        assert!(!extensions.contains("disabled_one"));
        assert!(!extensions.contains("memory_limit"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_enable_disable_precedence() {
        let dir = std::env::temp_dir().join("pzoom_ext_test_empty");
        std::fs::create_dir_all(&dir).unwrap();

        let enabled = resolve_enabled_extensions(&dir, &["Redis".to_string()], &[]);
        assert!(enabled.contains("redis"));

        let disabled =
            resolve_enabled_extensions(&dir, &["redis".to_string()], &["redis".to_string()]);
        assert!(!disabled.contains("redis"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
