//! Docblock parsing for pzoom.
//!
//! Split to mirror Psalm's organization:
//! - [`parsed_docblock`] — docblock comment structure (description + tags),
//!   like `DocblockParser.php` / `ParsedDocblock.php`.
//! - [`type_tokenizer`] — type-string tokenizer, like `TypeTokenizer.php`.
//! - [`type_parser`] — PHPDoc type-string parsing, like `TypeParser.php`.

pub mod parse_tree;
pub mod parse_tree_creator;
pub mod parsed_docblock;
pub mod type_parser;
pub mod type_tokenizer;

pub use parsed_docblock::*;
pub use type_parser::*;
