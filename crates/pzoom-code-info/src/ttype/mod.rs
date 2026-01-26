//! Type operations module - combining, comparing, and manipulating types.

mod type_combination;
pub mod type_combiner;

pub use type_combiner::{combine, combine_union_types, add_union_type};
