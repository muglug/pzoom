//! Core analysis engine for pzoom.
//!
//! This crate contains the main analysis logic, including:
//! - File analysis
//! - Statement analysis
//! - Expression analysis
//! - Type inference and checking

pub mod assertion_finder;
pub mod config;
pub mod context;
pub mod docblock;
pub mod expr;
pub mod expr_analyzer;
pub mod function_analysis_data;
pub mod psalm_config;
pub mod reconciler;
pub mod scope;
pub mod statements_analyzer;
pub mod stmt;
pub mod stmt_analyzer;
pub mod type_comparator;

pub use config::Config;
pub use docblock::{parse_docblock, parse_type_string, DocblockTag, ParsedDocblock};
pub use psalm_config::{find_and_load_psalm_config, load_psalm_config, parse_psalm_xml};
pub use context::{BlockContext, FunctionContext, FunctionContextInfo, FunctionLikeId};
pub use function_analysis_data::FunctionAnalysisData;
pub use statements_analyzer::StatementsAnalyzer;
pub use scope::{IfScope, IfConditionalScope};
