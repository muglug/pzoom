//! Analysis pipeline orchestration for pzoom.
//!
//! This crate coordinates the three-phase analysis pipeline:
//! 1. Scanning - Parse files and collect symbols
//! 2. Populating - Resolve inheritance and build type info
//! 3. Analyzing - Type check and detect issues

pub mod scanner;
pub mod populator;
pub mod analyzer;

pub use scanner::{Scanner, ScanResult};
pub use populator::Populator;
pub use analyzer::Analyzer;
