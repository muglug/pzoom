//! Analyzer for array access expressions ($arr[key]).

use mago_span::HasSpan;
use mago_syntax::ast::ast::array::ArrayAccess;

use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::expr_analyzer;

/// Compute line number from byte offset in source.
fn get_line_number(source: &str, offset: u32) -> u32 {
    let offset = offset as usize;
    source[..offset.min(source.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1
}

/// Analyze an array access expression like $arr[0] or $arr['key'].
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the array expression
    let array_pos = expr_analyzer::analyze(analyzer, access.array, analysis_data, context);

    // Analyze the index expression
    let index_pos = expr_analyzer::analyze(analyzer, access.index, analysis_data, context);

    let array_type = analysis_data.get_expr_type(array_pos).map(|rc| (*rc).clone());
    let index_type = analysis_data.get_expr_type(index_pos).map(|rc| (*rc).clone());

    // If we don't know the array type, return mixed
    let Some(array_type) = array_type else {
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    };

    // Check each type in the union
    let mut result_types: Vec<TAtomic> = Vec::new();
    let mut has_valid_access = false;
    let mut has_invalid_access = false;
    let mut has_null = false;
    let mut invalid_type_name = String::new();

    for atomic in &array_type.types {
        match atomic {
            // Null access
            TAtomic::TNull => {
                has_null = true;
            }

            // Array types - valid access
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                has_valid_access = true;
                for t in &value_type.types {
                    if !result_types.contains(t) {
                        result_types.push(t.clone());
                    }
                }
            }

            // Keyed array - check specific key or use fallback
            TAtomic::TKeyedArray { properties, fallback_value_type, .. } => {
                has_valid_access = true;
                // Collect all possible value types from properties
                for value in properties.values() {
                    for t in &value.types {
                        if !result_types.contains(t) {
                            result_types.push(t.clone());
                        }
                    }
                }
                // Also include fallback type if present
                if let Some(fallback) = fallback_value_type {
                    for t in &fallback.types {
                        if !result_types.contains(t) {
                            result_types.push(t.clone());
                        }
                    }
                }
            }

            // String access - returns string
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TTruthyString
            | TAtomic::TClassString { .. }
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString => {
                has_valid_access = true;
                if !result_types.contains(&TAtomic::TString) {
                    result_types.push(TAtomic::TString);
                }
            }

            // Mixed - could be anything
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                has_valid_access = true;
                result_types.clear();
                result_types.push(TAtomic::TMixed);
            }

            // Invalid array access types
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TResource
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TVoid
            | TAtomic::TNothing => {
                has_invalid_access = true;
                invalid_type_name = atomic.get_id();
            }

            // Other types - treat as potentially valid for now
            _ => {
                has_valid_access = true;
                result_types.push(TAtomic::TMixed);
            }
        }
    }

    // Report issues based on what we found
    let span = access.array.span();
    let start_line = get_line_number(analyzer.source, span.start.offset);

    // Pure null access
    if has_null && !has_valid_access && !has_invalid_access {
        analysis_data.issues.push(Issue::new(
            IssueKind::NullArrayAccess,
            "Cannot access array offset on null".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    }

    // Possibly null access
    if has_null && (has_valid_access || has_invalid_access) {
        analysis_data.issues.push(Issue::new(
            IssueKind::PossiblyNullArrayAccess,
            "Cannot access array offset on possibly null value".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    // Pure invalid access (non-array type)
    if has_invalid_access && !has_valid_access {
        analysis_data.issues.push(Issue::new(
            IssueKind::InvalidArrayAccess,
            format!("Cannot access array offset on {}", invalid_type_name),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    }

    // Possibly invalid access (union with non-array type)
    if has_invalid_access && has_valid_access {
        analysis_data.issues.push(Issue::new(
            IssueKind::PossiblyInvalidArrayAccess,
            format!("Cannot access array offset on value that may be {}", invalid_type_name),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    // Check for invalid array offset type
    if let Some(index_type) = index_type {
        check_array_offset(&index_type, analyzer, access, analysis_data);
    }

    // Set the result type using the type combiner for proper simplification
    if result_types.is_empty() {
        analysis_data.set_expr_type(pos, TUnion::mixed());
    } else {
        let combined = type_combiner::combine(result_types, false);
        analysis_data.set_expr_type(pos, TUnion::from_types(combined));
    }
}

/// Check if the array offset type is valid.
fn check_array_offset(
    index_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut has_valid_offset = false;
    let mut has_invalid_offset = false;
    let mut invalid_offset_type = String::new();

    for atomic in &index_type.types {
        match atomic {
            // Valid offset types
            TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TNumericString
            | TAtomic::TArrayKey
            | TAtomic::TMixed => {
                has_valid_offset = true;
            }

            // Invalid offset types
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TObject
            | TAtomic::TNamedObject { .. }
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TResource
            | TAtomic::TNull
            | TAtomic::TVoid => {
                has_invalid_offset = true;
                invalid_offset_type = atomic.get_id();
            }

            // Bool can be used as offset (converts to 0/1)
            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => {
                has_valid_offset = true;
            }

            _ => {
                has_valid_offset = true;
            }
        }
    }

    if has_invalid_offset {
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);

        if has_valid_offset {
            analysis_data.issues.push(Issue::new(
                IssueKind::PossiblyInvalidArrayOffset,
                format!("Array offset may be invalid type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
        } else {
            analysis_data.issues.push(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!("Invalid array offset type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
        }
    }
}
