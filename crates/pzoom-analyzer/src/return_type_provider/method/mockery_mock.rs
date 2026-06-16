//! `Mockery::mock` / `Mockery::spy` return-type provider.
//!
//! Mirrors psalm/plugin-mockery's `MockReturnTypeUpdater` hook: when the
//! first argument names a class (`Foo::class` or a literal class-string),
//! the returned `Mockery\MockInterface` becomes `MockInterface&Foo`. pzoom
//! has no plugin runtime, so this activates only when the Mockery plugin's
//! stubs were loaded from psalm.xml's `<pluginClass>` entries.

use pzoom_code_info::{TAtomic, TUnion};

use super::{MethodReturnTypeProvider, MethodReturnTypeProviderEvent};

pub(super) struct MockeryMockReturnTypeProvider;

impl MethodReturnTypeProvider for MockeryMockReturnTypeProvider {
    fn class_names(&self) -> &'static [&'static str] {
        &["Mockery"]
    }

    fn get_method_return_type(
        &self,
        event: &MethodReturnTypeProviderEvent<'_, '_>,
    ) -> Option<TUnion> {
        if !event.method_name.eq_ignore_ascii_case("mock")
            && !event.method_name.eq_ignore_ascii_case("spy")
        {
            return None;
        }

        // Only stand in for psalm/plugin-mockery when its stubs are active.
        if !event
            .analyzer
            .config
            .plugin_stubs
            .iter()
            .any(|stub| stub.contains("plugin-mockery"))
        {
            return None;
        }

        let first_arg_pos = event.arg_positions.first()?;
        let first_arg_type = event
            .analysis_data
            .expr_types
            .get(&*first_arg_pos)
            .cloned()?;

        let mocked_class_id = match first_arg_type.get_single()? {
            TAtomic::TLiteralClassString { name } => event.analyzer.interner.intern(name),
            TAtomic::TLiteralString { value } => {
                let name = value
                    .strip_prefix("alias:")
                    .or_else(|| value.strip_prefix("overload:"))
                    .unwrap_or(value);
                let name = name.split('[').next().unwrap_or(name);
                let name = name.trim_start_matches('\\');
                if name.is_empty() {
                    return None;
                }
                event.analyzer.interner.intern(name)
            }
            _ => return None,
        };

        if event.analyzer.codebase.get_class(mocked_class_id).is_none() {
            return None;
        }

        let mock_interface_id = event.analyzer.interner.intern("Mockery\\MockInterface");

        Some(TUnion::new(TAtomic::TObjectIntersection {
            types: vec![
                TAtomic::TNamedObject {
                    name: mock_interface_id,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                },
                TAtomic::TNamedObject {
                    name: mocked_class_id,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                },
            ],
        }))
    }
}
