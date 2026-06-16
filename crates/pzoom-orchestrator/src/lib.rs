//! Analysis pipeline orchestration for pzoom.
//!
//! This crate coordinates the three-phase analysis pipeline:
//! 1. Scanning - Parse files and collect symbols
//! 2. Populating - Resolve inheritance and build type info
//! 3. Analyzing - Type check and detect issues

pub mod analyzer;
pub mod ast_differ;
pub mod cache;
pub mod callmap;
pub mod composer_autoload;
pub mod extensions;
pub mod populator;
pub mod scanner;

pub use analyzer::Analyzer;
pub use callmap::apply_call_map;
pub use composer_autoload::ComposerAutoload;
pub use extensions::resolve_enabled_extensions;
pub use populator::{Populator, register_global_defined_constants};
pub use scanner::{ScanResult, Scanner};
