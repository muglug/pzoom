//! Type comparison module.
//!
//! This module provides comprehensive type comparison functionality,
//! modeled after Psalm's Type\Comparator and Hakana's ttype::comparison.
//!
//! # Module Structure
//!
//! - `union_type_comparator` - Compares union types (TUnion)
//! - `atomic_type_comparator` - Compares atomic types (TAtomic)
//! - `scalar_type_comparator` - Compares scalar types (int, string, etc.)
//! - `object_type_comparator` - Compares object/class types
//! - `array_type_comparator` - Compares array types
//! - `callable_type_comparator` - Compares callable/closure types
//! - `type_comparison_result` - Result type with coercion info

pub mod array_type_comparator;
pub mod atomic_type_comparator;
pub mod callable_type_comparator;
pub mod object_type_comparator;
pub mod scalar_type_comparator;
pub mod type_comparison_result;
pub mod union_type_comparator;

// Re-export commonly used items
pub use type_comparison_result::TypeComparisonResult;
pub use union_type_comparator::is_contained_by;

use pzoom_code_info::{CodebaseInfo, TUnion};

/// Simple wrapper for backward compatibility.
/// Check if input_type is contained by container_type with codebase access.
pub fn is_contained_by_with_codebase(
    input_type: &TUnion,
    container_type: &TUnion,
    codebase: &CodebaseInfo,
) -> bool {
    let mut result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        codebase,
        input_type,
        container_type,
        false, // ignore_null
        false, // ignore_false
        &mut result,
    )
}
