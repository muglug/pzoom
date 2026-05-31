//! SimpleXMLElement::{children,attributes,addChild} return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent, resolve_class_id};

pub(super) struct SimpleXmlElementReturnTypeProvider;

impl MethodReturnTypeProvider for SimpleXmlElementReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["SimpleXMLElement"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !(event.method_name.eq_ignore_ascii_case("children")
            || event.method_name.eq_ignore_ascii_case("attributes")
            || event.method_name.eq_ignore_ascii_case("addChild"))
        {
            return None;
        }

        let simplexml_id = resolve_class_id(event.analyzer, "SimpleXMLElement");
        Some(TUnion::from_types(vec![
            TAtomic::named_object(simplexml_id),
            TAtomic::TNull,
        ]))
    }
}
