//! `"array_filter"` return-type provider.

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, VarName};

use super::{FunctionReturnTypeProvider, FunctionReturnTypeProviderEvent};
use crate::expr::call::function_call_analyzer as fca;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use mago_syntax::cst::cst::argument::Argument;
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::statement::Statement;
pub(super) struct ArrayFilterReturnTypeProvider;

impl FunctionReturnTypeProvider for ArrayFilterReturnTypeProvider {
    fn function_ids(&self) -> &'static [&'static str] {
        &["array_filter"]
    }

    fn get_function_return_type(
        &self,
        event: &FunctionReturnTypeProviderEvent<'_, '_>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Option<TUnion> {
        infer_array_filter_return_type(
            event.analyzer,
            event.args,
            event.arg_positions,
            analysis_data,
        )
    }
}

fn infer_array_filter_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.expr_types.get(&array_pos).cloned()?;
    let callback_is_default =
        fca::is_default_array_filter_callback(args, arg_positions, analysis_data);

    // Psalm validates the filter callback against the array's element type;
    // resolving that element on a `mixed` array reports MixedArrayAccess on the
    // array argument (ArrayFunctionArgumentsAnalyzer). A bare
    // `array_filter($mixed)` with the default callback does not access elements.
    if !callback_is_default
        && array_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
    {
        let (start, end) = array_pos;
        let (line, column) = analyzer.get_line_column(start);
        let message = match args
            .first()
            .and_then(|arg| crate::expression_identifier::get_expression_var_key(arg.value()))
        {
            Some(var) => format!("Cannot access array value on mixed variable {var}"),
            None => "Cannot access array value on mixed type".to_string(),
        };
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArrayAccess,
            message,
            analyzer.file_path,
            start,
            end,
            line,
            column,
        ));
    }

    let callback_assertions = if callback_is_default {
        None
    } else {
        callback_param_assertions(analyzer, args, analysis_data)
    };

    let mut filtered_types = Vec::new();

    for atomic in &array_type.types {
        let Some(mut filtered_atomic) =
            fca::infer_array_filter_return_atomic(atomic, callback_is_default)
        else {
            continue;
        };

        if let Some(callback_assertions) = &callback_assertions {
            // `infer_array_filter_return_atomic` yields a generic array/list
            // whose value type is the typed fallback (`params.1`); narrow it.
            if let TAtomic::TArray {
                params: Some(params),
                ..
            } = &mut filtered_atomic
            {
                let narrowed = apply_assertions_to_union(
                    analyzer,
                    callback_assertions,
                    &params.1,
                    analysis_data,
                );
                params.1 = narrowed;
            }
        }

        if !filtered_types.contains(&filtered_atomic) {
            filtered_types.push(filtered_atomic);
        }
    }

    if filtered_types.is_empty() {
        let array_info = fca::extract_array_like_info_from_union(&array_type)?;

        let key_type = if array_info.key_type.is_nothing() {
            TUnion::array_key()
        } else {
            fca::normalize_array_key_union(&array_info.key_type)
        };

        let value_type = if callback_is_default {
            fca::narrow_union_to_truthy(&array_info.value_type)
        } else if let Some(callback_assertions) = &callback_assertions {
            apply_assertions_to_union(
                analyzer,
                callback_assertions,
                &array_info.value_type,
                analysis_data,
            )
        } else {
            array_info.value_type
        };

        return Some(TUnion::new(TAtomic::array(key_type, value_type)));
    }

    Some(TUnion::from_types(filtered_types))
}

/// Psalm's ArrayFilterReturnTypeProvider closure handling: a closure or arrow
/// function callback whose body is a single returned condition has that
/// condition's truths extracted for the first parameter (e.g.
/// `fn ($value) => is_string($value)` asserts string).
fn callback_param_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    analysis_data: &FunctionAnalysisData,
) -> Option<Vec<Vec<pzoom_code_info::Assertion>>> {
    let callback_expr = args.get(1)?.value().unparenthesized();

    // A literal-string callback naming a type-check function asserts that
    // type on the values (Psalm routes these through
    // getFunctionIdsFromCallableArg + the function's effects).
    if let Expression::Literal(mago_syntax::cst::cst::literal::Literal::String(string_lit)) =
        callback_expr
    {
        let assertion_type = match pzoom_syntax::bytes_to_str(string_lit.value?).trim_start_matches('\\') {
            "is_string" => TAtomic::TString,
            "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
            "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
            "is_bool" => TAtomic::TBool,
            "is_object" => TAtomic::TObject,
            "is_null" => TAtomic::TNull,
            "is_numeric" => TAtomic::TNumeric,
            "is_scalar" => TAtomic::TScalar,
            "is_resource" => TAtomic::TResource,
            "is_callable" => TAtomic::TCallable {
                params: None,
                return_type: None,
                is_pure: None,
            },
            "is_array" => TAtomic::array(TUnion::array_key(), TUnion::mixed()),
            "is_iterable" => TAtomic::TIterable {
                key_type: Box::new(TUnion::mixed()),
                value_type: Box::new(TUnion::mixed()),
            },
            _ => return None,
        };
        return Some(vec![vec![pzoom_code_info::Assertion::IsType(
            assertion_type,
        )]]);
    }

    let (first_param, return_expr) = match callback_expr {
        Expression::ArrowFunction(arrow) => {
            (arrow.parameter_list.parameters.first()?, arrow.expression)
        }
        Expression::Closure(closure) => {
            let [Statement::Return(return_stmt)] = closure.body.statements.as_slice() else {
                return None;
            };
            (
                closure.parameter_list.parameters.first()?,
                return_stmt.value?,
            )
        }
        _ => return None,
    };

    if first_param.ellipsis.is_some() {
        return None;
    }

    let assertions = crate::assertion_finder::get_assertions(analyzer, return_expr, analysis_data);
    let param_assertions = assertions
        .if_true
        .get(&VarName::new(pzoom_syntax::bytes_to_str(first_param.variable.name)))?;

    if param_assertions.is_empty() {
        return None;
    }

    Some(param_assertions.clone())
}

fn apply_assertions_to_union(
    analyzer: &StatementsAnalyzer<'_>,
    assertion_groups: &[Vec<pzoom_code_info::Assertion>],
    value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    let mut reconciled = value_type.clone();
    for group in assertion_groups {
        if let [assertion] = group.as_slice() {
            reconciled = crate::reconciler::assertion_reconciler::reconcile(
                assertion,
                Some(&reconciled),
                false,
                None,
                analyzer,
                analysis_data,
                false,
                false,
            );
        } else {
            // An OR group (e.g. a `@psalm-assert-if-true A|B` callback): like
            // Psalm's reconcileKeyedTypes, reconcile each alternative against
            // the pre-group type and union the results.
            let mut result: Option<TUnion> = None;
            for assertion in group {
                let narrowed = crate::reconciler::assertion_reconciler::reconcile(
                    assertion,
                    Some(&reconciled),
                    false,
                    None,
                    analyzer,
                    analysis_data,
                    false,
                    false,
                );
                result = Some(match result {
                    None => narrowed,
                    Some(existing) => {
                        pzoom_code_info::combine_union_types(&existing, &narrowed, false)
                    }
                });
            }
            if let Some(result) = result {
                reconciled = result;
            }
        }
    }
    reconciled
}
