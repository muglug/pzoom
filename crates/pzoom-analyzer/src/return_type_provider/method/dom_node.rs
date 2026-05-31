//! DOMNode::appendChild return-type provider.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent};

pub(super) struct DomNodeReturnTypeProvider;

impl MethodReturnTypeProvider for DomNodeReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["DOMNode"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("appendChild") {
            return None;
        }

        let first_arg_pos = *event.arg_positions.first()?;
        let arg_type = event.analysis_data.get_expr_type(first_arg_pos)?;

        if !arg_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TNamedObject { .. }
                    | TAtomic::TObject
                    | TAtomic::TObjectIntersection { .. }
            )
        }) {
            return None;
        }

        Some((*arg_type).clone())
    }
}
