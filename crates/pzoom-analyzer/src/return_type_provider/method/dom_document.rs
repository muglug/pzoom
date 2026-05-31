//! DOMDocument::createElement return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent, resolve_class_id};

pub(super) struct DomDocumentReturnTypeProvider;

impl MethodReturnTypeProvider for DomDocumentReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["DOMDocument"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("createElement") {
            return None;
        }

        let dom_element_id = resolve_class_id(event.analyzer, "DOMElement");
        let mut false_or_domelement =
            TUnion::from_types(vec![TAtomic::named_object(dom_element_id), TAtomic::TFalse]);
        false_or_domelement.ignore_falsable_issues = true;
        Some(false_or_domelement)
    }
}
