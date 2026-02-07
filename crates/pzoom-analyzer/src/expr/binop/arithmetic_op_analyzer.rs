//! Arithmetic operation helpers for binary operators.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

pub use super::arithmetic_analyzer::analyze;

pub(crate) fn infer_arithmetic_type(left: Option<&TUnion>, right: Option<&TUnion>) -> TUnion {
    let has_float = |t: &TUnion| {
        t.types
            .iter()
            .any(|a| matches!(a, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
    };

    match (left, right) {
        (Some(lt), Some(rt)) => {
            if has_float(lt) || has_float(rt) {
                TUnion::float()
            } else {
                TUnion::int_from_calculation()
            }
        }
        _ => TUnion::new(TAtomic::TNumeric),
    }
}

pub(crate) fn emit_arithmetic_operand_issue(
    analyzer: &StatementsAnalyzer<'_>,
    union: Option<&TUnion>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(union) = union else {
        return;
    };

    if union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNull))
        && !union.ignore_nullable_issues
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyNullOperand,
            format!(
                "Cannot use arithmetic on possibly null type {}",
                union.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if union.is_single() && matches!(union.get_single(), Some(TAtomic::TFalse)) {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::FalseOperand,
            "Cannot use arithmetic on false".to_string(),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return;
    }

    // Psalm does not raise InvalidOperand for arithmetic on mixed; it is handled
    // by mixed-flow issue types elsewhere.
    if union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
    {
        return;
    }

    let mut has_valid = false;
    let mut has_invalid = false;
    for atomic in &union.types {
        if matches!(atomic, TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing) {
            continue;
        }

        if is_valid_arithmetic_atomic(atomic) {
            has_valid = true;
        } else {
            has_invalid = true;
        }
    }

    if !has_invalid {
        return;
    }

    let kind = if has_valid {
        IssueKind::PossiblyInvalidOperand
    } else {
        IssueKind::InvalidOperand
    };

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        format!(
            "Cannot use arithmetic on type {}",
            union.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn is_valid_arithmetic_atomic(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TInt
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TFloat
        | TAtomic::TLiteralFloat { .. }
        | TAtomic::TNumeric
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString => true,
        TAtomic::TLiteralString { value } => value.parse::<f64>().is_ok(),
        TAtomic::TTemplateParam { as_type, .. } => {
            !as_type.types.is_empty() && as_type.types.iter().all(is_valid_arithmetic_atomic)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => is_valid_arithmetic_atomic(as_type),
        _ => false,
    }
}
