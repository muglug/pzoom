//! DateTime / DateTimeImmutable return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent};

pub(super) struct DateTimeReturnTypeProvider;

impl MethodReturnTypeProvider for DateTimeReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["DateTime", "DateTimeImmutable"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("createFromFormat") {
            return None;
        }

        let mut false_or_datetime =
            TUnion::from_types(vec![TAtomic::named_object(event.class_id), TAtomic::TFalse]);
        false_or_datetime.ignore_falsable_issues = true;
        Some(false_or_datetime)
    }
}
