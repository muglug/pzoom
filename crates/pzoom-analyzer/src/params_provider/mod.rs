//! Parameter providers.
//!
//! These mirror Psalm's params-provider extension points
//! (`FunctionParamsProviderInterface`, `MethodParamsProviderInterface`): some
//! builtin calls have parameter lists that depend on the *call site* (argument
//! count, literal flag values), which a static stub signature cannot express.
//! Each provider declares the function/class ids it handles and builds the
//! per-call parameter list — or asks for the generic parameter validation to
//! be skipped, exactly like a Psalm provider returning `null` params (which
//! disables downstream argument checking for the call).
//!
//! Function providers live in [`function`]; method providers in [`method`].
//! The dispatch functions are consulted from the call analyzers at the same
//! point Psalm consults `$codebase->functions->params_provider` /
//! `$codebase->methods->params_provider`.

pub mod function;
pub mod method;

pub use function::{
    FunctionParamsProviderEvent, FunctionParamsProviderResult, dispatch_function_params,
};
pub use method::{MethodParamsProviderEvent, dispatch_method_params};
