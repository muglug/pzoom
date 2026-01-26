//! Cast expression analyzer.

use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a cast expression.
///
/// This handles type casts like (int), (string), (array), etc.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &UnaryPrefix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the inner expression
    let inner_pos = expr_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let inner_type = analysis_data.get_expr_type(inner_pos);

    // Check for redundant casts
    if let Some(inner) = inner_type {
        if is_redundant_cast(&unary.operator, &inner) {
            let cast_name = get_cast_name(&unary.operator);
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::RedundantCast,
                format!("Redundant ({}) cast - value is already {}", cast_name, cast_name),
                analyzer.file_path,
                pos.0, // start_offset
                pos.1, // end_offset
                line,
                col,
            ));
        }
    }

    let result_type = match &unary.operator {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            TUnion::int()
        }

        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),

        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            TUnion::string()
        }

        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }

        UnaryPrefixOperator::ArrayCast(_, _) => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),

        UnaryPrefixOperator::ObjectCast(_, _) => {
            // (object) cast creates stdClass
            TUnion::new(TAtomic::TObject)
        }

        UnaryPrefixOperator::UnsetCast(_, _) => {
            // (unset) cast always returns null (deprecated in PHP 8)
            TUnion::null()
        }

        UnaryPrefixOperator::VoidCast(_, _) => {
            // (void) cast (for completeness, rarely used)
            TUnion::void()
        }

        // Non-cast operators should not reach here
        _ => TUnion::mixed(),
    };

    analysis_data.set_expr_type(pos, result_type);
}

/// Check if an operator is a cast operator.
pub fn is_cast_operator(op: &UnaryPrefixOperator) -> bool {
    matches!(
        op,
        UnaryPrefixOperator::IntCast(_, _)
            | UnaryPrefixOperator::IntegerCast(_, _)
            | UnaryPrefixOperator::FloatCast(_, _)
            | UnaryPrefixOperator::DoubleCast(_, _)
            | UnaryPrefixOperator::RealCast(_, _)
            | UnaryPrefixOperator::StringCast(_, _)
            | UnaryPrefixOperator::BinaryCast(_, _)
            | UnaryPrefixOperator::BoolCast(_, _)
            | UnaryPrefixOperator::BooleanCast(_, _)
            | UnaryPrefixOperator::ArrayCast(_, _)
            | UnaryPrefixOperator::ObjectCast(_, _)
            | UnaryPrefixOperator::UnsetCast(_, _)
            | UnaryPrefixOperator::VoidCast(_, _)
    )
}

/// Check if a cast is redundant given the inner type.
fn is_redundant_cast(op: &UnaryPrefixOperator, inner_type: &TUnion) -> bool {
    // Only consider single-type unions for redundant cast detection
    if !inner_type.is_single() {
        return false;
    }

    let inner = match inner_type.get_single() {
        Some(t) => t,
        None => return false,
    };

    match op {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            matches!(inner, TAtomic::TInt | TAtomic::TLiteralInt { .. })
        }

        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => {
            matches!(inner, TAtomic::TFloat | TAtomic::TLiteralFloat { .. })
        }

        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            matches!(inner, TAtomic::TString | TAtomic::TLiteralString { .. })
        }

        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            matches!(inner, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse)
        }

        UnaryPrefixOperator::ArrayCast(_, _) => {
            matches!(inner, TAtomic::TArray { .. } | TAtomic::TKeyedArray { .. })
        }

        UnaryPrefixOperator::ObjectCast(_, _) => {
            matches!(inner, TAtomic::TObject | TAtomic::TNamedObject { .. })
        }

        _ => false,
    }
}

/// Get the display name for a cast operator.
fn get_cast_name(op: &UnaryPrefixOperator) -> &'static str {
    match op {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => "int",
        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => "float",
        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => "string",
        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => "bool",
        UnaryPrefixOperator::ArrayCast(_, _) => "array",
        UnaryPrefixOperator::ObjectCast(_, _) => "object",
        UnaryPrefixOperator::UnsetCast(_, _) => "unset",
        UnaryPrefixOperator::VoidCast(_, _) => "void",
        _ => "unknown",
    }
}
