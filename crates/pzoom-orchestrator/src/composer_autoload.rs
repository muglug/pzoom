//! Composer autoload index, used to scan dependencies the way Psalm does:
//! on demand. Rather than walking the whole `vendor/` tree (which pulls in
//! every package's tests, fixtures, and example data — files that are never
//! autoloadable and that PHP would never load), pzoom resolves a referenced
//! class to its file through Composer's generated maps and scans only that.
//!
//! Parses the three maps Composer emits under `vendor/composer/`:
//! - `autoload_classmap.php` — `'Fully\\Qualified\\Class' => $vendorDir . '/path.php'`
//! - `autoload_psr4.php`      — `'Prefix\\' => array($vendorDir . '/src', …)`
//! - `autoload_files.php`     — `'hash' => $vendorDir . '/functions.php'` (eager)

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct ComposerAutoload {
    /// Fully-qualified class name (single-backslash, no leading `\`) → file.
    classmap: FxHashMap<String, PathBuf>,
    /// `(namespace prefix, search dirs)`, longest prefix first.
    psr4: Vec<(String, Vec<PathBuf>)>,
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

        if let Ok(text) = std::fs::read_to_string(composer_dir.join("autoload_classmap.php")) {
            for line in text.lines() {
                if let Some((key, path)) = parse_map_entry(line, vendor_dir, base_dir) {
                    autoload.classmap.insert(unescape_fqn(&key), path);
                }
            }
        }

        if let Ok(text) = std::fs::read_to_string(composer_dir.join("autoload_psr4.php")) {
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

        if let Ok(text) = std::fs::read_to_string(composer_dir.join("autoload_files.php")) {
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
        None
    }
}

/// Composer escapes the `\` in a PHP single-quoted string as `\\`.
fn unescape_fqn(raw: &str) -> String {
    raw.replace("\\\\", "\\")
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
