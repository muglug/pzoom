//! Composer autoload index, used to scan dependencies the way Psalm does:
//! on demand. Rather than walking the whole `vendor/` tree (which pulls in
//! every package's tests, fixtures, and example data — files that are never
//! autoloadable and that PHP would never load), pzoom resolves a referenced
//! class to its file through Composer's generated maps and scans only that.
//!
//! Parses the maps Composer emits under `vendor/composer/`:
//! - `autoload_classmap.php`   — `'Fully\\Qualified\\Class' => $vendorDir . '/path.php'`
//! - `autoload_psr4.php`       — `'Prefix\\' => array($vendorDir . '/src', …)`
//! - `autoload_namespaces.php` — `'Prefix_' => array($vendorDir . '/lib', …)` (PSR-0)
//! - `autoload_files.php`      — `'hash' => $vendorDir . '/functions.php'` (eager)

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct ComposerAutoload {
    /// Fully-qualified class name (single-backslash, no leading `\`) → file.
    classmap: FxHashMap<String, PathBuf>,
    /// PSR-4 `(namespace prefix, search dirs)`, longest prefix first.
    psr4: Vec<(String, Vec<PathBuf>)>,
    /// PSR-0 `(prefix, search dirs)`, longest prefix first. Underscores in the
    /// class name map to directory separators (PEAR-style), so bundled
    /// `Zend_*` / `Twig_*` style libraries resolve.
    psr0: Vec<(String, Vec<PathBuf>)>,
    /// Files autoloaded eagerly on every request (global functions/bootstrap).
    pub eager_files: Vec<PathBuf>,
}

impl ComposerAutoload {
    /// Load the maps from `<vendor>/composer/`. Returns `None` when the project
    /// has no Composer autoloader (caller falls back to a directory walk).
    pub fn load(vendor_dir: &Path) -> Option<Self> {
        let composer_dir = vendor_dir.join("composer");
        if !composer_dir.join("autoload_classmap.php").is_file() {
            return None;
        }
        let base_dir = vendor_dir.parent().unwrap_or(vendor_dir);

        let mut autoload = ComposerAutoload::default();

        if let Some(text) = read_map_lossy(&composer_dir.join("autoload_classmap.php")) {
            for line in text.lines() {
                if let Some((key, path)) = parse_map_entry(line, vendor_dir, base_dir) {
                    autoload.classmap.insert(unescape_fqn(&key), path);
                }
            }
        }

        if let Some(text) = read_map_lossy(&composer_dir.join("autoload_psr4.php")) {
            for line in text.lines() {
                if let Some((prefix, dirs)) = parse_psr4_entry(line, vendor_dir, base_dir) {
                    autoload.psr4.push((unescape_fqn(&prefix), dirs));
                }
            }
            // Longest prefix wins, like Composer's resolver.
            autoload
                .psr4
                .sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        }

        // PSR-0 (`autoload_namespaces.php`) shares the PSR-4 line shape but
        // resolves paths PEAR-style — see `resolve_class`. Magento bundles
        // Zend Framework 1 (`Zend_Db`, `Zend_Pdf`, …) this way.
        if let Some(text) = read_map_lossy(&composer_dir.join("autoload_namespaces.php")) {
            for line in text.lines() {
                if let Some((prefix, dirs)) = parse_psr4_entry(line, vendor_dir, base_dir) {
                    autoload.psr0.push((unescape_fqn(&prefix), dirs));
                }
            }
            autoload
                .psr0
                .sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        }

        if let Some(text) = read_map_lossy(&composer_dir.join("autoload_files.php")) {
            for line in text.lines() {
                if let Some((_, path)) = parse_map_entry(line, vendor_dir, base_dir) {
                    autoload.eager_files.push(path);
                }
            }
        }

        // Composer's own runtime classes (`Composer\Autoload\ClassLoader`,
        // `Composer\InstalledVersions`) are the autoloader itself, so they appear
        // in no map — but projects reference them. Scan them explicitly.
        for runtime in ["ClassLoader.php", "InstalledVersions.php"] {
            let path = composer_dir.join(runtime);
            if path.is_file() {
                autoload.eager_files.push(path);
            }
        }

        Some(autoload)
    }

    /// Resolve a fully-qualified class name to the file that defines it.
    pub fn resolve_class(&self, fqcn: &str) -> Option<PathBuf> {
        let fqcn = fqcn.strip_prefix('\\').unwrap_or(fqcn);
        if let Some(path) = self.classmap.get(fqcn) {
            return Some(path.clone());
        }
        // PSR-4: the longest matching prefix maps to `<dir>/<rest>.php`, with the
        // namespace separators after the prefix turned into directory separators.
        for (prefix, dirs) in &self.psr4 {
            if let Some(rest) = fqcn.strip_prefix(prefix.as_str()) {
                let relative = rest.replace('\\', "/");
                for dir in dirs {
                    let candidate = dir.join(format!("{relative}.php"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        // PSR-0: the whole class name maps to a path under the search dir.
        // Namespace separators become directory separators, and — PEAR-style —
        // so do underscores in the class-name segment (the part after the last
        // `\`). `Zend_Db_Expr` → `Zend/Db/Expr.php`.
        for (prefix, dirs) in &self.psr0 {
            if fqcn.starts_with(prefix.as_str()) {
                let (namespace, class) = match fqcn.rfind('\\') {
                    Some(pos) => (&fqcn[..pos], &fqcn[pos + 1..]),
                    None => ("", fqcn),
                };
                let mut relative = namespace.replace('\\', "/");
                if !relative.is_empty() {
                    relative.push('/');
                }
                relative.push_str(&class.replace('_', "/"));
                for dir in dirs {
                    let candidate = dir.join(format!("{relative}.php"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }
}

/// Composer escapes the `\` in a PHP single-quoted string as `\\`.
fn unescape_fqn(raw: &str) -> String {
    raw.replace("\\\\", "\\")
}

/// Read a Composer-generated map, tolerating stray non-UTF-8 bytes.
///
/// Composer records every class name verbatim, and a dependency may legally
/// declare a class whose name is not valid UTF-8: `symfony/cache`, for one,
/// ships `Traits/ValueWrapper.php` whose class is the single byte `0xA9`, so
/// the generated `autoload_classmap.php` is not valid UTF-8 as a whole. Reading
/// it with `read_to_string` fails outright, which would silently drop *every*
/// classmap entry (and with it every classmap-autoloaded dependency — PHPUnit,
/// the AWS SDK, …). Read the bytes and decode lossily instead: the one
/// malformed key becomes U+FFFD — it is never referenced by well-formed code —
/// while every other entry still loads.
fn read_map_lossy(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Resolve `$vendorDir . '/x'` / `$baseDir . '/x'` to an absolute path.
///
/// The `$vendorDir`/`$baseDir` variable need not be at the very start of the
/// piece: PSR-4 values are wrapped as `array($vendorDir . '/x', …)`, so the
/// first directory is preceded by `array(`. Locate the variable anywhere in
/// the piece rather than requiring it as a prefix.
fn join_var_path(value: &str, vendor_dir: &Path, base_dir: &Path) -> Option<PathBuf> {
    let (root, rest) = match (value.find("$vendorDir"), value.find("$baseDir")) {
        (Some(i), v) if v.is_none_or(|j| i < j) => {
            (vendor_dir, &value[i + "$vendorDir".len()..])
        }
        (_, Some(j)) => (base_dir, &value[j + "$baseDir".len()..]),
        _ => return None,
    };
    let rest = rest.trim_start().strip_prefix('.')?.trim_start();
    let segment = single_quoted(rest)?;
    Some(root.join(segment.trim_start_matches('/')))
}

/// Extract the contents of the first `'…'` single-quoted string in `s`.
fn single_quoted(s: &str) -> Option<&str> {
    let start = s.find('\'')? + 1;
    let end = start + s[start..].find('\'')?;
    Some(&s[start..end])
}

/// Parse a `'key' => $vendorDir . '/path',` line (classmap / files).
fn parse_map_entry(line: &str, vendor_dir: &Path, base_dir: &Path) -> Option<(String, PathBuf)> {
    let line = line.trim();
    let (key_part, value_part) = line.split_once("=>")?;
    let key = single_quoted(key_part)?.to_string();
    let path = join_var_path(
        value_part.trim().trim_end_matches(','),
        vendor_dir,
        base_dir,
    )?;
    Some((key, path))
}

/// Parse a `'Prefix\\' => array($vendorDir . '/a', $vendorDir . '/b'),` line.
fn parse_psr4_entry(
    line: &str,
    vendor_dir: &Path,
    base_dir: &Path,
) -> Option<(String, Vec<PathBuf>)> {
    let line = line.trim();
    let (key_part, value_part) = line.split_once("=>")?;
    let prefix = single_quoted(key_part)?.to_string();
    let dirs = value_part
        .split(',')
        .filter_map(|piece| join_var_path(piece, vendor_dir, base_dir))
        .collect::<Vec<_>>();
    if dirs.is_empty() {
        return None;
    }
    Some((prefix, dirs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A classmap line whose key is not valid UTF-8 (`symfony/cache` ships a
    /// class named by the single byte `0xA9`) must not drop the rest of the
    /// classmap, and PSR-0 (`autoload_namespaces.php`, used by bundled
    /// `Zend_*` libraries) must resolve PEAR-style.
    #[test]
    fn loads_classmap_past_non_utf8_key_and_resolves_psr0() {
        let base = std::env::temp_dir().join(format!("pzoom_autoload_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vendor = base.join("vendor");
        let composer = vendor.join("composer");
        fs::create_dir_all(&composer).unwrap();

        // Real target for a classmap entry.
        let widget = vendor.join("acme/lib/src/Widget.php");
        fs::create_dir_all(widget.parent().unwrap()).unwrap();
        fs::write(&widget, "<?php\n").unwrap();

        // autoload_classmap.php: one valid entry, then a sibling whose key is a
        // lone 0xA9 byte (invalid standalone UTF-8) — the whole file is then not
        // valid UTF-8, which used to make `read_to_string` discard every entry.
        let mut classmap = Vec::new();
        classmap.extend_from_slice(b"<?php\n$vendorDir = dirname(__DIR__);\nreturn array(\n");
        classmap.extend_from_slice(b"    'Acme\\\\Widget' => $vendorDir . '/acme/lib/src/Widget.php',\n");
        classmap.extend_from_slice(b"    '");
        classmap.push(0xA9);
        classmap.extend_from_slice(b"' => $vendorDir . '/symfony/cache/Traits/ValueWrapper.php',\n);\n");
        fs::write(composer.join("autoload_classmap.php"), &classmap).unwrap();

        fs::write(composer.join("autoload_psr4.php"), b"<?php\nreturn array(\n);\n").unwrap();

        // PSR-0 Zend-style library on disk.
        let expr = vendor.join("acme/zend/library/Zend/Db/Expr.php");
        fs::create_dir_all(expr.parent().unwrap()).unwrap();
        fs::write(&expr, "<?php\n").unwrap();
        fs::write(
            composer.join("autoload_namespaces.php"),
            b"<?php\nreturn array(\n    'Zend_' => array($vendorDir . '/acme/zend/library'),\n);\n"
                .as_slice(),
        )
        .unwrap();

        let autoload = ComposerAutoload::load(&vendor).expect("autoload should load");

        // The valid classmap entry survived the non-UTF-8 sibling line.
        assert_eq!(autoload.resolve_class("Acme\\Widget"), Some(widget.clone()));
        // A leading `\` on the reference is tolerated.
        assert_eq!(autoload.resolve_class("\\Acme\\Widget"), Some(widget));
        // PSR-0 underscores become directory separators.
        assert_eq!(autoload.resolve_class("Zend_Db_Expr"), Some(expr));
        // A genuinely unknown class still resolves to nothing.
        assert_eq!(autoload.resolve_class("Acme\\Nope"), None);

        let _ = fs::remove_dir_all(&base);
    }
}
