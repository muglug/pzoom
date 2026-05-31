//! Analysis pipeline orchestration for pzoom.
//!
//! This crate coordinates the three-phase analysis pipeline:
//! 1. Scanning - Parse files and collect symbols
//! 2. Populating - Resolve inheritance and build type info
//! 3. Analyzing - Type check and detect issues

pub mod analyzer;
pub mod ast_differ;
pub mod cache;
pub mod populator;
pub mod scanner;

pub use analyzer::Analyzer;
pub use populator::Populator;
pub use scanner::{ScanResult, Scanner};
