//! Psalm XML configuration file parser.
//!
//! This module parses Psalm's XML configuration format (psalm.xml) and converts
//! it to pzoom's Config struct.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use rustc_hash::FxHashSet;
use std::path::Path;

use crate::config::{Config, ErrorLevel};

/// Error type for Psalm config parsing.
#[derive(Debug)]
pub enum PsalmConfigError {
    Xml(quick_xml::Error),
    Io(std::io::Error),
    InvalidAttribute(String),
    MissingRootElement,
}

impl From<quick_xml::Error> for PsalmConfigError {
    fn from(err: quick_xml::Error) -> Self {
        PsalmConfigError::Xml(err)
    }
}

impl From<std::io::Error> for PsalmConfigError {
    fn from(err: std::io::Error) -> Self {
        PsalmConfigError::Io(err)
    }
}

impl std::fmt::Display for PsalmConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PsalmConfigError::Xml(e) => write!(f, "XML parsing error: {}", e),
            PsalmConfigError::Io(e) => write!(f, "IO error: {}", e),
            PsalmConfigError::InvalidAttribute(s) => write!(f, "Invalid attribute: {}", s),
            PsalmConfigError::MissingRootElement => write!(f, "Missing <psalm> root element"),
        }
    }
}

impl std::error::Error for PsalmConfigError {}

/// Parse a Psalm XML config file and return a Config.
pub fn parse_psalm_xml(xml: &str) -> Result<Config, PsalmConfigError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut config = Config::default();
    let mut found_psalm_root = false;
    let mut buf = Vec::new();
    let mut current_path: Vec<String> = Vec::new();
    let mut ignore_files: Vec<String> = Vec::new();
    let mut stubs: Vec<String> = Vec::new();
    let mut forbidden_functions: FxHashSet<String> = FxHashSet::default();
    let mut active_issue_handler_suppression: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if name == "psalm" && current_path.is_empty() {
                    found_psalm_root = true;
                    parse_psalm_attributes(e, &mut config)?;
                }

                current_path.push(name.clone());

                // Parse elements based on current path
                let path_strs: Vec<&str> = current_path.iter().map(|s| s.as_str()).collect();
                match path_strs.as_slice() {
                    [.., "projectFiles", "directory"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            config.project_dirs.push(name);
                        }
                    }
                    [.., "projectFiles", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            config.project_dirs.push(name);
                        }
                    }
                    [.., "projectFiles", "ignoreFiles", "directory"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            ignore_files.push(format!("{}/**", name));
                        }
                    }
                    [.., "projectFiles", "ignoreFiles", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            ignore_files.push(name);
                        }
                    }
                    [.., "issueHandlers", issue_name] => {
                        // Check if this issue is suppressed (errorLevel="suppress")
                        if let Some(level) = get_attribute(e, "errorLevel")? {
                            if level == "suppress" {
                                config.suppressed_issues.insert(issue_name.to_string());
                            }
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel"] => {
                        let level = get_attribute(e, "type")?.or(get_attribute(e, "errorLevel")?);
                        if level.as_deref() == Some("suppress") {
                            active_issue_handler_suppression = Some(issue_name.to_string());
                        } else {
                            active_issue_handler_suppression = None;
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel", "directory"] => {
                        if active_issue_handler_suppression.as_deref() == Some(issue_name)
                            && let Some(name) = get_attribute(e, "name")?
                        {
                            config.add_issue_handler_suppression_pattern(
                                issue_name,
                                format!("{}/**", name),
                            );
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel", "file"] => {
                        if active_issue_handler_suppression.as_deref() == Some(issue_name)
                            && let Some(name) = get_attribute(e, "name")?
                        {
                            config.add_issue_handler_suppression_pattern(issue_name, name);
                        }
                    }
                    [.., "stubs", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            stubs.push(name);
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                // Handle self-closing tags
                let full_path: Vec<&str> = current_path
                    .iter()
                    .map(|s| s.as_str())
                    .chain(std::iter::once(name.as_str()))
                    .collect();

                match full_path.as_slice() {
                    [.., "projectFiles", "directory"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            config.project_dirs.push(name);
                        }
                    }
                    [.., "projectFiles", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            config.project_dirs.push(name);
                        }
                    }
                    [.., "projectFiles", "ignoreFiles", "directory"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            ignore_files.push(format!("{}/**", name));
                        }
                    }
                    [.., "projectFiles", "ignoreFiles", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            ignore_files.push(name);
                        }
                    }
                    [.., "issueHandlers", issue_name] => {
                        if let Some(level) = get_attribute(e, "errorLevel")? {
                            if level == "suppress" {
                                config.suppressed_issues.insert(issue_name.to_string());
                            }
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel"] => {
                        let level = get_attribute(e, "type")?.or(get_attribute(e, "errorLevel")?);
                        if level.as_deref() == Some("suppress") {
                            config.suppressed_issues.insert(issue_name.to_string());
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel", "directory"] => {
                        if active_issue_handler_suppression.as_deref() == Some(issue_name)
                            && let Some(name) = get_attribute(e, "name")?
                        {
                            config.add_issue_handler_suppression_pattern(
                                issue_name,
                                format!("{}/**", name),
                            );
                        }
                    }
                    [.., "issueHandlers", issue_name, "errorLevel", "file"] => {
                        if active_issue_handler_suppression.as_deref() == Some(issue_name)
                            && let Some(name) = get_attribute(e, "name")?
                        {
                            config.add_issue_handler_suppression_pattern(issue_name, name);
                        }
                    }
                    [.., "stubs", "file"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            stubs.push(name);
                        }
                    }
                    [.., "forbiddenFunctions", "function"] => {
                        if let Some(name) = get_attribute(e, "name")? {
                            forbidden_functions.insert(name);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) => {
                if e.name().as_ref() == b"errorLevel" {
                    active_issue_handler_suppression = None;
                }
                current_path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if !found_psalm_root {
        return Err(PsalmConfigError::MissingRootElement);
    }

    // Remove default project_dirs if we found any in the XML
    if config.project_dirs.len() > 1 {
        config.project_dirs.remove(0); // Remove default "."
    }

    // Set exclude patterns from ignoreFiles
    if !ignore_files.is_empty() {
        config.exclude_patterns = ignore_files;
    }

    // Set stubs
    if !stubs.is_empty() {
        config.stubs = stubs;
    }

    // Set forbidden functions
    if !forbidden_functions.is_empty() {
        config.forbidden_functions = forbidden_functions;
    }

    Ok(config)
}

/// Parse attributes from the root <psalm> element.
fn parse_psalm_attributes(e: &BytesStart<'_>, config: &mut Config) -> Result<(), PsalmConfigError> {
    for attr in e.attributes() {
        let attr = attr.map_err(|e| PsalmConfigError::InvalidAttribute(format!("{:?}", e)))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = String::from_utf8_lossy(&attr.value).to_string();

        match key.as_str() {
            "phpVersion" => {
                config.php_version = value;
            }
            "errorLevel" => {
                if let Ok(n) = value.parse::<u8>() {
                    config.error_level = ErrorLevel::from_int(n);
                }
            }
            "findUnusedCode" | "findUnusedVariablesAndParams" => {
                config.report_unused = value == "true";
            }
            "findUnusedPsalmSuppress" => {
                config.find_unused_suppress = value == "true";
            }
            "findUnusedBaselineEntry" => {
                config.find_unused_baseline_entry = value == "true";
            }
            "runTaintAnalysis" => {
                config.taint_analysis = value == "true";
            }
            "useDocblockTypes" => {
                config.use_docblock_types = value == "true";
            }
            "reportMixedIssues" => {
                config.report_mixed_issues = value == "true";
            }
            "cacheDirectory" => {
                config.cache_dir = Some(value);
            }
            "errorBaseline" => {
                config.error_baseline = Some(value);
            }
            "threads" => {
                if let Ok(n) = value.parse::<usize>() {
                    config.threads = n;
                }
            }
            _ => {
                // Ignore other attributes for now
            }
        }
    }
    Ok(())
}

/// Get an attribute value from an element.
fn get_attribute(e: &BytesStart<'_>, name: &str) -> Result<Option<String>, PsalmConfigError> {
    for attr in e.attributes() {
        let attr = attr.map_err(|e| PsalmConfigError::InvalidAttribute(format!("{:?}", e)))?;
        let key = String::from_utf8_lossy(attr.key.as_ref());
        if key == name {
            return Ok(Some(String::from_utf8_lossy(&attr.value).to_string()));
        }
    }
    Ok(None)
}

/// Load a Psalm config from a file path.
pub fn load_psalm_config<P: AsRef<Path>>(path: P) -> Result<Config, PsalmConfigError> {
    let config_path = path.as_ref();
    let content = std::fs::read_to_string(config_path)?;
    let mut config = parse_psalm_xml(&content)?;

    if let Some(error_baseline) = config.error_baseline.clone() {
        let error_baseline_path = Path::new(&error_baseline);
        if error_baseline_path.is_relative()
            && let Some(config_dir) = config_path.parent()
        {
            config.error_baseline = Some(
                config_dir
                    .join(error_baseline_path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }

    Ok(config)
}

/// Try to find and load a Psalm config file in the given directory.
/// Looks for psalm.xml, psalm.xml.dist, or psalm.dist.xml in order.
pub fn find_and_load_psalm_config<P: AsRef<Path>>(dir: P) -> Option<Config> {
    let dir = dir.as_ref();

    for filename in &["psalm.xml", "psalm.xml.dist", "psalm.dist.xml"] {
        let path = dir.join(filename);
        if path.exists() {
            if let Ok(config) = load_psalm_config(&path) {
                return Some(config);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_psalm_xml() {
        let xml = r#"<?xml version="1.0"?>
<psalm
    phpVersion="8.1"
    findUnusedCode="true"
    runTaintAnalysis="true"
>
    <projectFiles>
        <directory name="src" />
        <directory name="lib" />
        <ignoreFiles>
            <directory name="vendor" />
            <file name="legacy.php" />
        </ignoreFiles>
    </projectFiles>
    <issueHandlers>
        <MixedAssignment errorLevel="suppress" />
        <MixedArgument errorLevel="suppress" />
    </issueHandlers>
</psalm>"#;

        let config = parse_psalm_xml(xml).unwrap();

        assert_eq!(config.php_version, "8.1");
        assert!(config.report_unused);
        assert!(config.taint_analysis);
        assert!(config.project_dirs.contains(&"src".to_string()));
        assert!(config.project_dirs.contains(&"lib".to_string()));
        assert!(config.exclude_patterns.contains(&"vendor/**".to_string()));
        assert!(config.exclude_patterns.contains(&"legacy.php".to_string()));
        assert!(config.suppressed_issues.contains("MixedAssignment"));
        assert!(config.suppressed_issues.contains("MixedArgument"));
    }

    #[test]
    fn test_parse_minimal_psalm_xml() {
        let xml = r#"<?xml version="1.0"?>
<psalm>
    <projectFiles>
        <directory name="app" />
    </projectFiles>
</psalm>"#;

        let config = parse_psalm_xml(xml).unwrap();

        assert!(config.project_dirs.contains(&"app".to_string()));
        assert!(!config.report_unused);
        assert!(!config.taint_analysis);
    }

    #[test]
    fn test_missing_root_element() {
        let xml = r#"<?xml version="1.0"?>
<config>
    <projectFiles>
        <directory name="src" />
    </projectFiles>
</config>"#;

        let result = parse_psalm_xml(xml);
        assert!(matches!(result, Err(PsalmConfigError::MissingRootElement)));
    }

    #[test]
    fn test_parse_error_level_and_advanced_options() {
        let xml = r#"<?xml version="1.0"?>
<psalm
    errorLevel="3"
    useDocblockTypes="false"
    reportMixedIssues="false"
    findUnusedPsalmSuppress="true"
    findUnusedBaselineEntry="true"
    errorBaseline="psalm-baseline.xml"
>
    <projectFiles>
        <directory name="src" />
    </projectFiles>
</psalm>"#;

        let config = parse_psalm_xml(xml).unwrap();

        assert_eq!(config.error_level, ErrorLevel::Level3);
        assert!(!config.use_docblock_types);
        assert!(!config.report_mixed_issues);
        assert!(config.find_unused_suppress);
        assert!(config.find_unused_baseline_entry);
        assert_eq!(config.error_baseline.as_deref(), Some("psalm-baseline.xml"));
    }

    #[test]
    fn test_parse_stubs_and_forbidden_functions() {
        let xml = r#"<?xml version="1.0"?>
<psalm>
    <projectFiles>
        <directory name="src" />
    </projectFiles>
    <stubs>
        <file name="stubs/phpstan.php" />
        <file name="stubs/doctrine.php" />
    </stubs>
    <forbiddenFunctions>
        <function name="var_dump" />
        <function name="print_r" />
        <function name="dd" />
    </forbiddenFunctions>
</psalm>"#;

        let config = parse_psalm_xml(xml).unwrap();

        assert!(config.stubs.contains(&"stubs/phpstan.php".to_string()));
        assert!(config.stubs.contains(&"stubs/doctrine.php".to_string()));
        assert!(config.forbidden_functions.contains("var_dump"));
        assert!(config.forbidden_functions.contains("print_r"));
        assert!(config.forbidden_functions.contains("dd"));
    }

    #[test]
    fn test_parse_scoped_issue_handler_suppressions() {
        let xml = r#"<?xml version="1.0"?>
<psalm>
    <projectFiles>
        <directory name="src" />
    </projectFiles>
    <issueHandlers>
        <InternalMethod>
            <errorLevel type="suppress">
                <directory name="tests" />
                <file name="src/Foo.php" />
            </errorLevel>
        </InternalMethod>
    </issueHandlers>
</psalm>"#;

        let config = parse_psalm_xml(xml).unwrap();

        assert!(config.is_issue_suppressed_for_path("InternalMethod", "tests/A.php"));
        assert!(config.is_issue_suppressed_for_path("InternalMethod", "src/Foo.php"));
        assert!(!config.is_issue_suppressed_for_path("InternalMethod", "src/Bar.php"));
    }

    #[test]
    fn test_load_psalm_config_resolves_relative_baseline_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pzoom_psalm_config_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let config_path = temp_dir.join("psalm.xml");
        std::fs::write(
            &config_path,
            r#"<?xml version="1.0"?>
<psalm errorBaseline="psalm-baseline.xml">
    <projectFiles>
        <directory name="src" />
    </projectFiles>
</psalm>"#,
        )
        .unwrap();

        let config = load_psalm_config(&config_path).unwrap();
        let baseline = config.error_baseline.expect("baseline should be resolved");
        assert_eq!(
            std::path::Path::new(&baseline),
            temp_dir.join("psalm-baseline.xml")
        );

        let _ = std::fs::remove_file(config_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
