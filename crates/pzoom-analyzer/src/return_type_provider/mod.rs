//! Return-type providers.
//!
//! These mirror Psalm's return-type provider extension points
//! (`MethodReturnTypeProviderInterface`, `FunctionReturnTypeProviderInterface`):
//! each provider declares the class/function ids it handles and computes a return
//! type for a matching call. The dispatch functions are invoked from the call
//! analyzers, so a developer familiar with Psalm can find and add special-cased
//! return types in one place.
//!
//! Method providers live in [`method`]; function providers in [`function`].

pub mod function;
pub mod method;

pub use function::{
    FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent, dispatch_function_return_type,
};
pub use method::{
    MethodReturnTypeProvider, MethodReturnTypeProviderEvent, dispatch_method_return_type,
};
