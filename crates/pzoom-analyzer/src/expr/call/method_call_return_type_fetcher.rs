//! Method call return type fetcher.
//!
//! Mirrors Psalm/Hakana special return-type handling for selected internal methods.

use pzoom_code_info::{TAtomic, TUnion};
use pzoom_str::StrId;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn fetch(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> Option<TUnion> {
    if method_name.eq_ignore_ascii_case("createfromformat")
        && (class_matches_or_descends_from(analyzer, class_id, "DateTime")
            || class_matches_or_descends_from(analyzer, class_id, "DateTimeImmutable"))
    {
        let mut false_or_datetime = TUnion::from_types(vec![
            TAtomic::TNamedObject {
                name: class_id,
                type_params: None,
            },
            TAtomic::TFalse,
        ]);
        false_or_datetime.ignore_falsable_issues = true;
        return Some(false_or_datetime);
    }

    if method_name.eq_ignore_ascii_case("createelement")
        && class_matches_or_descends_from(analyzer, class_id, "DOMDocument")
    {
        let dom_element_id = resolve_class_id(analyzer, "DOMElement");
        let mut false_or_domelement = TUnion::from_types(vec![
            TAtomic::TNamedObject {
                name: dom_element_id,
                type_params: None,
            },
            TAtomic::TFalse,
        ]);
        false_or_domelement.ignore_falsable_issues = true;
        return Some(false_or_domelement);
    }

    if (method_name.eq_ignore_ascii_case("children")
        || method_name.eq_ignore_ascii_case("attributes")
        || method_name.eq_ignore_ascii_case("addchild"))
        && class_matches_or_descends_from(analyzer, class_id, "SimpleXMLElement")
    {
        let simplexml_id = resolve_class_id(analyzer, "SimpleXMLElement");
        return Some(TUnion::from_types(vec![
            TAtomic::TNamedObject {
                name: simplexml_id,
                type_params: None,
            },
            TAtomic::TNull,
        ]));
    }

    if method_name.eq_ignore_ascii_case("formatmessage")
        && class_matches_or_descends_from(analyzer, class_id, "MessageFormatter")
    {
        let mut false_or_string = TUnion::from_types(vec![TAtomic::TString, TAtomic::TFalse]);
        false_or_string.ignore_falsable_issues = true;
        return Some(false_or_string);
    }

    None
}

fn class_matches_or_descends_from(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    target_class: &str,
) -> bool {
    let direct = resolve_class_id(analyzer, target_class);
    if class_id == direct {
        return true;
    }

    analyzer
        .codebase
        .all_classlike_descendants
        .get(&direct)
        .is_some_and(|descendants| descendants.contains(&class_id))
}

fn resolve_class_id(analyzer: &StatementsAnalyzer<'_>, class_name: &str) -> StrId {
    let direct = analyzer.interner.intern(class_name);
    if analyzer.codebase.get_class(direct).is_some() {
        return direct;
    }

    let fq = analyzer.interner.intern(&format!("\\{}", class_name));
    if analyzer.codebase.get_class(fq).is_some() {
        return fq;
    }

    direct
}
