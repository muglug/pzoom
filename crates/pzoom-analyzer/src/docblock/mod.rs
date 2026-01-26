//! Docblock parsing for PHPDoc and Psalm annotations.
//!
//! This module parses PHP docblocks to extract type information from
//! annotations like @param, @return, @var, and Psalm-specific tags.
//!
//! The type parser is based on Psalm's TypeParser.php and uses a two-phase approach:
//! 1. TypeTokenizer - tokenizes type strings into tokens
//! 2. ParseTreeCreator - builds a parse tree from tokens
//! 3. TypeParser - converts parse trees to TUnion types

pub mod parse_tree;
pub mod parse_tree_creator;
pub mod parser;
pub mod type_parser;
pub mod type_tokenizer;

pub use parser::{parse_docblock, DocblockTag, ParsedDocblock};
pub use type_parser::parse_type_string;
