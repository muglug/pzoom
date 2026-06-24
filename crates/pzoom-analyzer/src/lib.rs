//! Core analysis engine for pzoom.
//!
//! This crate contains the main analysis logic, including:
//! - File analysis
//! - Statement analysis
//! - Expression analysis
//! - Type inference and checking

pub mod algebra_analyzer;
pub mod assertion_finder;
pub mod class_casing;
pub mod config;
pub mod context;
pub(crate) mod data_flow;
pub mod expr;
pub mod expression_analyzer;
pub mod expression_identifier;
pub mod file_analyzer;
pub mod formula_generator;
pub mod function_analysis_data;
pub mod function_like_analyzer;
pub mod init_collector;
pub(crate) mod internal_access;
pub(crate) mod issue_suppression;
pub mod methods;
pub mod params_provider;
pub mod plugin;
pub mod profiling;
pub mod psalm_baseline;
pub mod psalm_config;
pub mod reconciler;
pub mod return_type_provider;
pub mod scope;
pub mod statements_analyzer;
pub mod stmt;
pub mod stmt_analyzer;
pub mod taint_analyzer;
pub mod template;
pub mod type_comparator;
pub mod type_coverage;
pub mod type_expander;
pub mod unused_symbols;
pub(crate) mod unused_variable_analyzer;

pub use config::Config;
pub use context::{BlockContext, FunctionContext, FunctionContextInfo, FunctionLikeId};
pub use function_analysis_data::FunctionAnalysisData;
pub use psalm_baseline::{PsalmBaseline, load_psalm_baseline};
pub use psalm_config::{find_and_load_psalm_config, load_psalm_config, parse_psalm_xml};
pub use scope::{IfConditionalScope, IfScope};
pub use statements_analyzer::StatementsAnalyzer;
