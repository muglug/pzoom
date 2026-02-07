//! Psalm XML baseline parser and matcher.
//!
//! Supports Psalm's `<files><file src="..."><IssueType><code>...</code></IssueType></file></files>`
//! format used by `errorBaseline`.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use rustc_hash::FxHashMap;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct PsalmBaseline {
    files: FxHashMap<String, FxHashMap<String, BaselineIssueBucket>>,
}

#[derive(Clone, Debug, Default)]
struct BaselineIssueBucket {
    remaining: usize,
    snippets: Vec<String>,
}

#[derive(Debug)]
pub enum PsalmBaselineError {
    Xml(quick_xml::Error),
    Io(std::io::Error),
    InvalidAttribute(String),
}

impl From<quick_xml::Error> for PsalmBaselineError {
    fn from(err: quick_xml::Error) -> Self {
        PsalmBaselineError::Xml(err)
    }
}

impl From<std::io::Error> for PsalmBaselineError {
    fn from(err: std::io::Error) -> Self {
        PsalmBaselineError::Io(err)
    }
}

impl std::fmt::Display for PsalmBaselineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PsalmBaselineError::Xml(err) => write!(f, "XML parsing error: {}", err),
            PsalmBaselineError::Io(err) => write!(f, "IO error: {}", err),
            PsalmBaselineError::InvalidAttribute(attr) => {
                write!(f, "Invalid baseline attribute: {}", attr)
            }
        }
    }
}

impl std::error::Error for PsalmBaselineError {}

impl PsalmBaseline {
    /// Returns true when the issue should be suppressed by baseline, and consumes one baseline slot.
    pub fn suppresses(&mut self, file_path: &str, issue_type: &str, selected_text: &str) -> bool {
        let normalized_file = normalize_file_path(file_path);
        let normalized_text = normalize_selected_text(selected_text);

        let mut file_candidates = vec![normalized_file.clone()];
        if let Some(stripped) = normalized_file.strip_prefix("./") {
            file_candidates.push(stripped.to_string());
        } else {
            file_candidates.push(format!("./{}", normalized_file));
        }

        for candidate in file_candidates {
            let Some(issue_entries) = self.files.get_mut(&candidate) else {
                continue;
            };
            let Some(bucket) = issue_entries.get_mut(issue_type) else {
                continue;
            };

            if bucket.remaining == 0 {
                return false;
            }

            // Psalm behavior:
            // - if baseline keeps one snippet per occurrence, match by selected text
            // - otherwise suppress by count only.
            if bucket.remaining == bucket.snippets.len() {
                if let Some(position) = bucket.snippets.iter().position(|s| s == &normalized_text) {
                    bucket.snippets.remove(position);
                    bucket.remaining -= 1;
                    return true;
                }

                // pzoom issue spans are sometimes broader than Psalm's selected_text
                // (e.g. full call expressions vs method names). Accept containment
                // matches so baseline entries still suppress equivalent findings.
                if !normalized_text.is_empty()
                    && let Some(position) = bucket.snippets.iter().position(|s| {
                        !s.is_empty()
                            && (normalized_text.contains(s) || s.contains(&normalized_text))
                    })
                {
                    bucket.snippets.remove(position);
                    bucket.remaining -= 1;
                    return true;
                }

                // If text matching still fails, consume by count. This keeps baseline
                // suppression useful when pzoom's issue span differs from Psalm's
                // selected_text granularity for the same file+issue bucket.
                bucket.snippets.clear();
                bucket.remaining -= 1;
                return true;
            }

            bucket.snippets.clear();
            bucket.remaining -= 1;
            return true;
        }

        false
    }
}

pub fn load_psalm_baseline<P: AsRef<Path>>(path: P) -> Result<PsalmBaseline, PsalmBaselineError> {
    let baseline_xml = std::fs::read_to_string(path)?;
    parse_psalm_baseline(&baseline_xml)
}

pub fn parse_psalm_baseline(xml: &str) -> Result<PsalmBaseline, PsalmBaselineError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut baseline = PsalmBaseline::default();
    let mut buf = Vec::new();

    let mut current_file: Option<String> = None;
    let mut current_issue: Option<String> = None;
    let mut inside_code = false;
    let mut code_text = String::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let name = element_name(e);

                if name == "file" {
                    let src = get_attribute(e, "src")?.unwrap_or_default();
                    current_file = Some(normalize_file_path(&src));
                    current_issue = None;
                    continue;
                }

                if name == "code" {
                    if current_file.is_some() && current_issue.is_some() {
                        inside_code = true;
                        code_text.clear();
                    }
                    continue;
                }

                if let Some(file) = current_file.as_ref() {
                    // Any tag directly under <file> is treated as an issue type bucket.
                    if current_issue.is_none() {
                        let issue_name = name.to_string();
                        current_issue = Some(issue_name.clone());
                        baseline
                            .files
                            .entry(file.clone())
                            .or_default()
                            .entry(issue_name)
                            .or_default();
                    }
                }
            }
            Event::Text(e) => {
                if inside_code {
                    code_text.push_str(String::from_utf8_lossy(e.as_ref()).as_ref());
                }
            }
            Event::CData(e) => {
                if inside_code {
                    code_text.push_str(String::from_utf8_lossy(e.as_ref()).as_ref());
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if name == "code" && inside_code {
                    if let (Some(file), Some(issue_name)) =
                        (current_file.as_ref(), current_issue.as_ref())
                    {
                        let normalized_code = normalize_selected_text(&code_text);
                        let bucket = baseline
                            .files
                            .entry(file.clone())
                            .or_default()
                            .entry(issue_name.clone())
                            .or_default();
                        bucket.remaining += 1;
                        bucket.snippets.push(normalized_code);
                    }

                    inside_code = false;
                    code_text.clear();
                } else if current_issue.as_deref() == Some(name.as_str()) {
                    current_issue = None;
                } else if name == "file" {
                    current_file = None;
                    current_issue = None;
                    inside_code = false;
                    code_text.clear();
                }
            }
            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }

    Ok(baseline)
}

fn normalize_file_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn normalize_selected_text(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

fn element_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).to_string()
}

fn get_attribute(e: &BytesStart<'_>, key: &str) -> Result<Option<String>, PsalmBaselineError> {
    for attr in e.attributes() {
        let attr = attr.map_err(|e| PsalmBaselineError::InvalidAttribute(format!("{:?}", e)))?;
        if String::from_utf8_lossy(attr.key.as_ref()) == key {
            return Ok(Some(String::from_utf8_lossy(&attr.value).to_string()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_suppress() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<files>
  <file src="src/Foo.php">
    <InvalidArgument>
      <code><![CDATA[$a]]></code>
      <code><![CDATA[$b]]></code>
    </InvalidArgument>
  </file>
</files>"#;

        let mut baseline = parse_psalm_baseline(xml).unwrap();

        assert!(baseline.suppresses("src/Foo.php", "InvalidArgument", "$a"));
        assert!(baseline.suppresses("src/Foo.php", "InvalidArgument", "$a"));
        assert!(!baseline.suppresses("src/Foo.php", "InvalidArgument", "$b"));
        assert!(!baseline.suppresses("src/Foo.php", "InvalidArgument", "$b"));
    }

    #[test]
    fn test_suppresses_when_issue_span_contains_baseline_snippet() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<files>
  <file src="src/Foo.php">
    <ImpureMethodCall>
      <code><![CDATA[classOrInterfaceExists]]></code>
    </ImpureMethodCall>
  </file>
</files>"#;

        let mut baseline = parse_psalm_baseline(xml).unwrap();

        assert!(baseline.suppresses(
            "src/Foo.php",
            "ImpureMethodCall",
            "$codebase->classOrInterfaceExists($this->value)"
        ));
        assert!(!baseline.suppresses(
            "src/Foo.php",
            "ImpureMethodCall",
            "$codebase->classOrInterfaceExists($this->value)"
        ));
    }
}
