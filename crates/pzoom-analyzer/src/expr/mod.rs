//! Expression-specific analyzers.

pub mod array_analyzer;
pub mod assignment_analyzer;
pub mod binop_analyzer;
pub mod call_analyzer;
pub mod variable_fetch_analyzer;

// New analyzers migrated from Psalm/Hakana
pub mod cast_analyzer;
pub mod clone_analyzer;
pub mod closure_analyzer;
pub mod const_fetch_analyzer;
pub mod output_constructs;
pub mod print_analyzer;
pub mod exit_analyzer;
pub mod include_analyzer;
pub mod empty_analyzer;
pub mod isset_analyzer;
pub mod match_analyzer;
pub mod partial_application_analyzer;
pub mod ternary_analyzer;
pub mod throw_analyzer;
pub mod unop_analyzer;
pub mod yield_analyzer;

// Subdirectory modules
pub mod assignment;
pub mod binop;
pub mod call;
pub mod fetch;
