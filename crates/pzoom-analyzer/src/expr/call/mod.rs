//! Call expression analyzers.

pub mod argument_analyzer;
pub mod arguments_analyzer;
pub mod array_function_arguments_analyzer;
pub mod atomic_method_call_analyzer;
pub mod atomic_static_call_analyzer;
pub mod callable_validation;
pub mod class_template_param_collector;
pub mod existing_atomic_method_call_analyzer;
pub mod existing_atomic_static_call_analyzer;
pub mod function_call_analyzer;
pub mod function_call_assertion_analyzer;
pub mod function_call_return_type_fetcher;
pub(crate) mod impure_functions_list;
pub mod method_call_analyzer;
pub mod method_call_prohibition_analyzer;
pub mod method_call_purity_analyzer;
pub mod method_call_return_type_fetcher;
pub mod method_visibility_analyzer;
pub mod missing_method_call_handler;
pub mod named_function_call_handler;
pub mod new_analyzer;
pub mod static_call_analyzer;
