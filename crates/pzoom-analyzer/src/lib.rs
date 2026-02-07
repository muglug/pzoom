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
pub(crate) mod data_flow;
pub mod expr;
pub mod expression_analyzer;
pub mod expression_identifier;
pub mod function_analysis_data;
pub(crate) mod internal_access;
pub(crate) mod issue_suppression;
pub mod psalm_baseline;
pub mod psalm_config;
pub mod reconciler;
pub mod scope;
pub mod statements_analyzer;
pub mod stmt;
pub mod stmt_analyzer;
pub mod template;
pub mod type_comparator;

pub use config::Config;
pub use context::{BlockContext, FunctionContext, FunctionContextInfo, FunctionLikeId};
pub use function_analysis_data::FunctionAnalysisData;
pub use psalm_baseline::{PsalmBaseline, load_psalm_baseline};
pub use psalm_config::{find_and_load_psalm_config, load_psalm_config, parse_psalm_xml};
pub use scope::{IfConditionalScope, IfScope};
pub use statements_analyzer::StatementsAnalyzer;
