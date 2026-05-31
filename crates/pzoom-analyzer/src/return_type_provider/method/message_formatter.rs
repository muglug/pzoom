//! MessageFormatter::formatMessage return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent};

pub(super) struct MessageFormatterReturnTypeProvider;

impl MethodReturnTypeProvider for MessageFormatterReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["MessageFormatter"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("formatMessage") {
            return None;
        }

        let mut false_or_string = TUnion::from_types(vec![TAtomic::TString, TAtomic::TFalse]);
        false_or_string.ignore_falsable_issues = true;
        Some(false_or_string)
    }
}
