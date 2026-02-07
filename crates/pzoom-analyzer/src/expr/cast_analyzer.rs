//! Cast expression analyzer.

use mago_syntax::ast::ast::unary::{UnaryPrefix, UnaryPrefixOperator};

use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
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
    let inner_pos = expression_analyzer::analyze(analyzer, unary.operand, analysis_data, context);
    let inner_type = analysis_data.get_expr_type(inner_pos);

    // Check for redundant casts
    if let Some(ref inner) = inner_type {
        if is_redundant_cast(&unary.operator, &inner) {
            let cast_name = get_cast_name(&unary.operator);
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::RedundantCast,
                format!(
                    "Redundant ({}) cast - value is already {}",
                    cast_name, cast_name
                ),
                analyzer.file_path,
                pos.0, // start_offset
                pos.1, // end_offset
                line,
                col,
            ));
        }
    }

    let inner_union = inner_type
        .map(|inner| (*inner).clone())
        .unwrap_or_else(TUnion::mixed);

    let result_type = match &unary.operator {
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            infer_int_cast_type(&inner_union)
        }

        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),

        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            maybe_emit_invalid_string_cast(analyzer, &inner_union, pos, analysis_data);
            infer_string_cast_type(analyzer, &inner_union)
        }

        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }

        UnaryPrefixOperator::ArrayCast(_, _) => infer_array_cast_type(&inner_union),

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

fn infer_int_cast_type(inner_type: &TUnion) -> TUnion {
    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TTrue => casted.push(TAtomic::TLiteralInt { value: 1 }),
            TAtomic::TFalse | TAtomic::TNull => casted.push(TAtomic::TLiteralInt { value: 0 }),
            TAtomic::TBool => {
                casted.push(TAtomic::TLiteralInt { value: 0 });
                casted.push(TAtomic::TLiteralInt { value: 1 });
            }
            TAtomic::TLiteralInt { value } => casted.push(TAtomic::TLiteralInt { value: *value }),
            TAtomic::TLiteralFloat { value } => casted.push(TAtomic::TLiteralInt {
                value: *value as i64,
            }),
            TAtomic::TPositiveInt => casted.push(TAtomic::TPositiveInt),
            TAtomic::TNegativeInt => casted.push(TAtomic::TNegativeInt),
            TAtomic::TIntRange { min, max } => casted.push(TAtomic::TIntRange {
                min: *min,
                max: *max,
            }),
            _ => casted.push(TAtomic::TInt),
        }
    }

    if casted.is_empty() {
        TUnion::int()
    } else {
        TUnion::from_types(casted)
    }
}

fn infer_string_cast_type(analyzer: &StatementsAnalyzer<'_>, inner_type: &TUnion) -> TUnion {
    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TLiteralInt { value } => casted.push(TAtomic::TLiteralString {
                value: value.to_string(),
            }),
            TAtomic::TLiteralFloat { value } => casted.push(TAtomic::TLiteralString {
                value: value.to_string(),
            }),
            TAtomic::TInt
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TFloat
            | TAtomic::TNumeric => casted.push(TAtomic::TNumericString),
            TAtomic::TNamedObject { .. } => {
                if let Some(to_string_type) = get_to_string_return_type(analyzer, atomic) {
                    if union_is_non_empty_string(&to_string_type) {
                        casted.push(TAtomic::TNonEmptyString);
                    } else {
                        casted.push(TAtomic::TString);
                    }
                } else {
                    casted.push(TAtomic::TString);
                }
            }
            _ => casted.push(TAtomic::TString),
        }
    }

    if casted.is_empty() {
        TUnion::string()
    } else {
        TUnion::from_types(casted)
    }
}

fn infer_array_cast_type(inner_type: &TUnion) -> TUnion {
    if inner_type.is_mixed() {
        return TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        });
    }

    let mut casted = Vec::new();

    for atomic in &inner_type.types {
        match atomic {
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. } => casted.push(atomic.clone()),
            TAtomic::TNull => casted.push(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            }),
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                casted.push(TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::mixed()),
                });
            }
            _ => casted.push(TAtomic::TNonEmptyList {
                value_type: Box::new(TUnion::new(atomic.clone())),
            }),
        }
    }

    if casted.is_empty() {
        TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        })
    } else {
        TUnion::from_types(type_combiner::combine(casted, false))
    }
}

fn maybe_emit_invalid_string_cast(
    analyzer: &StatementsAnalyzer<'_>,
    inner_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    for atomic in &inner_type.types {
        if let TAtomic::TNamedObject { name, .. } = atomic
            && should_emit_invalid_cast_for_named_object(analyzer, *name)
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidCast,
                format!(
                    "Cannot cast {} to string",
                    atomic.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }
}

fn should_emit_invalid_cast_for_named_object(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: StrId,
) -> bool {
    let Some(class_info) = analyzer.codebase.get_class(class_name) else {
        return false;
    };

    if class_info.kind == ClassLikeKind::Interface {
        return false;
    }

    !class_info.methods.contains_key(&StrId::TO_STRING)
}

fn get_to_string_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<TUnion> {
    let TAtomic::TNamedObject { name, .. } = atomic else {
        return None;
    };

    let class_info = analyzer.codebase.get_class(*name)?;
    let method = class_info.methods.get(&StrId::TO_STRING)?;
    Some(
        method
            .return_type
            .clone()
            .or_else(|| method.signature_return_type.clone())
            .unwrap_or_else(TUnion::string),
    )
}

fn union_is_non_empty_string(return_type: &TUnion) -> bool {
    !return_type.types.is_empty()
        && return_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TNonEmptyString
                    | TAtomic::TTruthyString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TLiteralString { .. }
            )
        })
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
            matches!(
                inner,
                TAtomic::TArray { .. }
                    | TAtomic::TNonEmptyArray { .. }
                    | TAtomic::TList { .. }
                    | TAtomic::TNonEmptyList { .. }
                    | TAtomic::TKeyedArray { .. }
            )
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
