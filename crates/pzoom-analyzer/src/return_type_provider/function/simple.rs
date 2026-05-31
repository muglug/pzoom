//! Built-ins with a fixed return type: utf8_encode, tmpfile, fopen, getopt,
//! filter_input(_array), explode.

use pzoom_code_info::{TAtomic, TUnion};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::named_function_call_handler;
use crate::function_analysis_data::FunctionAnalysisData;

pub(super) struct Utf8EncodeReturnTypeProvider;
impl FunctionReturnTypeProvider for Utf8EncodeReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["utf8_encode"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        Some(TUnion::string())
    }
}

pub(super) struct TmpfileReturnTypeProvider;
impl FunctionReturnTypeProvider for TmpfileReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["tmpfile"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let mut return_type = TUnion::from_types(vec![TAtomic::TResource, TAtomic::TFalse]);
        return_type.ignore_falsable_issues = true;
        Some(return_type)
    }
}

pub(super) struct FopenReturnTypeProvider;
impl FunctionReturnTypeProvider for FopenReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["fopen"]
    }
    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        if event
            .args
            .first()
            .is_some_and(|arg| named_function_call_handler::is_php_stream_literal_argument(arg))
        {
            Some(TUnion::new(TAtomic::TResource))
        } else {
            None
        }
    }
}

pub(super) struct GetoptReturnTypeProvider;
impl FunctionReturnTypeProvider for GetoptReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["getopt"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        let getopt_value_type = TUnion::from_types(vec![
            TAtomic::TString,
            TAtomic::TFalse,
            TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
        ]);
        Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::string()),
            value_type: Box::new(getopt_value_type),
        }))
    }
}

pub(super) struct FilterInputReturnTypeProvider;
impl FunctionReturnTypeProvider for FilterInputReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["filter_input"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        Some(TUnion::from_types(vec![
            TAtomic::TString,
            TAtomic::TFalse,
            TAtomic::TNull,
        ]))
    }
}

pub(super) struct FilterInputArrayReturnTypeProvider;
impl FunctionReturnTypeProvider for FilterInputArrayReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["filter_input_array"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        Some(TUnion::from_types(vec![
            TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            TAtomic::TNull,
        ]))
    }
}

pub(super) struct ExplodeReturnTypeProvider;
impl FunctionReturnTypeProvider for ExplodeReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["explode"]
    }
    fn get_function_return_type(
        &self,
        _event: &FunctionReturnTypeProviderEvent<'_, '_>,
        _analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        Some(TUnion::new(TAtomic::TNonEmptyList {
            value_type: Box::new(TUnion::string()),
        }))
    }
}
