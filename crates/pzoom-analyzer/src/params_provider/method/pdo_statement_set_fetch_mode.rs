//! Port of Psalm's `PdoStatementSetFetchMode` method params provider.
//!
//! `PDOStatement::setFetchMode` takes different trailing parameters depending
//! on the literal fetch-mode constant: FETCH_COLUMN takes a column number,
//! FETCH_CLASS a class name plus optional constructor args, FETCH_INTO an
//! object. The stub's generic `mixed ...$args` tail cannot express this.

use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::{TAtomic, TUnion};

use crate::function_analysis_data::FunctionAnalysisData;

use super::{MethodParamsProvider, MethodParamsProviderEvent};

pub(super) struct PdoStatementSetFetchMode;

impl MethodParamsProvider for PdoStatementSetFetchMode {
    fn classlike_names(&self) -> &'static [&'static str] {
        &["PDOStatement"]
    }

    fn get_method_params(
        &self,
        event: &MethodParamsProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<Vec<ParamInfo>> {
        if !event.method_name.eq_ignore_ascii_case("setFetchMode") {
            return None;
        }
        let first_arg_pos = event.arg_positions.first()?;
        let first_arg_type = analysis_data.expr_types.get(&*first_arg_pos).cloned()?;
        let TAtomic::TLiteralInt { value } = first_arg_type.get_single()? else {
            return None;
        };

        let interner = event.analyzer.interner;
        let make_param = |name: &str, param_type: TUnion| ParamInfo {
            name: interner.find(name).unwrap_or(pzoom_str::StrId::EMPTY),
            signature_type: Some(param_type),
            ..Default::default()
        };

        let mut params = vec![make_param("$mode", TUnion::int())];

        match value {
            // PDO::FETCH_COLUMN
            7 => params.push(make_param("$colno", TUnion::int())),
            // PDO::FETCH_CLASS — class name plus variadic constructor args
            8 => {
                params.push(make_param(
                    "$classname",
                    TUnion::new(TAtomic::TClassString { as_type: None }),
                ));
                let mut ctor_args = make_param(
                    "$ctorargs",
                    TUnion::new(TAtomic::array(TUnion::array_key(), TUnion::mixed())),
                );
                ctor_args.is_variadic = true;
                params.push(ctor_args);
            }
            // PDO::FETCH_INTO
            9 => params.push(make_param("$object", TUnion::new(TAtomic::TObject))),
            _ => {}
        }

        Some(params)
    }
}
