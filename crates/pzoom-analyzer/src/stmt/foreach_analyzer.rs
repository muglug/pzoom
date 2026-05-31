//! Foreach statement analyzer.

use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::r#loop::foreach::{Foreach, ForeachTarget};
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{CodebaseInfo, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::BreakContext;
use crate::stmt::loop_analyzer;

/// Analyze a foreach statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    foreach: &Foreach<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the iterable expression
    let iterable_pos =
        expression_analyzer::analyze(analyzer, foreach.expression, analysis_data, context);
    // `get_expr_type` hands back an owned `Rc<TUnion>`, so this doesn't borrow
    // `analysis_data` — we can still emit issues against it below.
    let iterable_type = analysis_data.get_expr_type(iterable_pos);

    // Create loop context
    let mut foreach_context = context.clone();
    foreach_context.inside_loop = true;
    foreach_context.inside_foreach = true;
    foreach_context.break_types.push(BreakContext::Loop);

    // Determine the value type from the iterable
    let value_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_value_type(iter_type, analyzer)
    } else {
        TUnion::mixed()
    };

    // Determine the key type from the iterable
    let key_type = if let Some(ref iter_type) = iterable_type {
        extract_iterable_key_type(iter_type, analyzer)
    } else {
        TUnion::array_key()
    };

    // Validate that the expression is actually iterable, mirroring Psalm's
    // `ForeachAnalyzer::checkIteratorType` (InvalidIterator, PossiblyNullIterator,
    // RawObjectIteration, ...).
    if let Some(ref iter_type) = iterable_type {
        check_iterator_type(analyzer, analysis_data, iter_type, iterable_pos);
    }

    // Set the iterator variable types in loop context
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            mark_foreach_reference_target(value_target.value, analyzer, &mut foreach_context);
            set_expression_var_type(value_target.value, &value_type, analyzer, &mut foreach_context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_expression_var_type(kv_target.key, &key_type, analyzer, &mut foreach_context);
            mark_foreach_reference_target(kv_target.value, analyzer, &mut foreach_context);
            set_expression_var_type(kv_target.value, &value_type, analyzer, &mut foreach_context);
        }
    }

    // If the iterable is provably empty (its element type is `never`, e.g.
    // `array_keys([])` is `list<never>`), the loop runs zero times and the body is
    // unreachable, so it is not analyzed — matching Psalm/Hakana, which suppress
    // diagnostics in the unreachable body.
    let iterable_is_empty = iterable_type
        .as_ref()
        .is_some_and(|_| value_type.is_nothing());

    if !iterable_is_empty {
        // Analyze the loop body to a fixed point. A foreach may iterate zero times, so
        // `always_enters_loop` is false.
        let loop_scope = LoopScope::new(context.locals.clone());
        let body_stmts = foreach.body.statements();
        let (_loop_scope, _inner) = loop_analyzer::analyze(
            analyzer,
            body_stmts,
            vec![],
            vec![],
            loop_scope,
            &mut foreach_context,
            context,
            analysis_data,
            false,
            false,
        )?;
    }

    // Iterator variables are now visible in the parent scope (PHP quirk).
    // They have the loop's element type after the loop finishes.
    match &foreach.target {
        ForeachTarget::Value(value_target) => {
            set_expression_var_type(value_target.value, &value_type, analyzer, context);
        }
        ForeachTarget::KeyValue(kv_target) => {
            set_expression_var_type(kv_target.key, &key_type, analyzer, context);
            set_expression_var_type(kv_target.value, &value_type, analyzer, context);
        }
    }

    Ok(())
}

/// Validate that `iter_type` can be iterated over, emitting the same family of
/// issues Psalm's `ForeachAnalyzer::checkIteratorType` does.
///
/// Each atomic member is classified as a valid iterable (array/iterable/object
/// implementing `Traversable`), `null`, a non-Traversable "raw" object (PHP
/// iterates its public properties), or an outright invalid value (a scalar). The
/// emitted issue then depends on whether the offending members are the whole
/// type (`InvalidIterator`/`NullIterator`/`RawObjectIteration`) or only part of
/// it (`PossiblyInvalidIterator`/`PossiblyNullIterator`).
fn check_iterator_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    iter_type: &TUnion,
    pos: Pos,
) {
    // `mixed` carries no information to check against.
    if iter_type.is_mixed() {
        return;
    }

    let interner = Some(analyzer.interner);
    let mut has_valid_iterator = false;
    let mut has_null = false;
    let mut invalid_types: Vec<String> = Vec::new();
    let mut raw_object_types: Vec<String> = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TNull => has_null = true,

            // Arrays, `iterable`, and anything whose runtime value could be a
            // Traversable (`object`, a template parameter, `mixed`) are accepted.
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TList { .. }
            | TAtomic::TNonEmptyList { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TIterable { .. }
            | TAtomic::TObject
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TMixed
            | TAtomic::TNonEmptyMixed => {
                has_valid_iterator = true;
            }

            TAtomic::TNamedObject { name, .. } => {
                if *name == StrId::STDCLASS
                    || !analyzer.codebase.class_exists(*name)
                    || class_is_traversable(analyzer.codebase, *name)
                {
                    // Implements Traversable, or an unknown class we can't
                    // disprove — assume it is iterable.
                    has_valid_iterator = true;
                } else {
                    // A concrete object that does not implement Traversable: PHP
                    // iterates its public properties (Psalm: RawObjectIteration).
                    raw_object_types.push(TUnion::new(atomic.clone()).get_id(interner));
                }
            }

            // Scalars and other non-iterable values cannot be iterated at all.
            TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TIntRange { .. }
            | TAtomic::TArrayKey
            | TAtomic::TScalar
            | TAtomic::TNumeric
            | TAtomic::TVoid
            | TAtomic::TResource
            | TAtomic::TClosedResource
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. } => {
                invalid_types.push(TUnion::new(atomic.clone()).get_id(interner));
            }

            // Anything else (enums, intersections, conditionals, …): be
            // conservative and don't flag it, to avoid false positives.
            _ => {}
        }
    }

    let (start_offset, end_offset) = pos;
    let (start_line, start_column) = analyzer.get_line_column(start_offset);
    let mut emit = |kind: IssueKind, message: String| {
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            start_offset,
            end_offset,
            start_line,
            start_column,
        ));
    };

    if !invalid_types.is_empty() {
        // If only *some* of the union can't be iterated, it's a possible error.
        let kind = if has_valid_iterator || has_null || !raw_object_types.is_empty() {
            IssueKind::PossiblyInvalidIterator
        } else {
            IssueKind::InvalidIterator
        };
        emit(kind, format!("Cannot iterate over {}", invalid_types.join("|")));
    }

    if !raw_object_types.is_empty() {
        emit(
            IssueKind::RawObjectIteration,
            format!(
                "Trying to iterate over the non-Traversable object {}",
                raw_object_types.join("|"),
            ),
        );
    }

    if iter_type.is_null() {
        emit(
            IssueKind::NullIterator,
            "Cannot iterate over null".to_string(),
        );
    } else if has_null && (has_valid_iterator || !raw_object_types.is_empty()) {
        emit(
            IssueKind::PossiblyNullIterator,
            "Cannot iterate over a nullable value".to_string(),
        );
    }
}

/// Whether a class (by interned name) is — or implements/extends — `Traversable`,
/// and may therefore be used directly in `foreach`.
fn class_is_traversable(codebase: &CodebaseInfo, name: StrId) -> bool {
    if matches!(
        name,
        StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
    ) {
        return true;
    }

    let Some(class_info) = codebase.get_class(name) else {
        return false;
    };

    class_info.interfaces.contains(&StrId::TRAVERSABLE)
        || class_info
            .all_parent_interfaces
            .iter()
            .any(|interface| *interface == StrId::TRAVERSABLE)
}

/// Extract the value type from an iterable type.
fn extract_iterable_value_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut value_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNonEmptyArray { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TList { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNonEmptyList { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                // Union of all property types
                for prop_type in properties.values() {
                    value_types.push(prop_type.clone());
                }
                if let Some(fallback) = fallback_value_type {
                    value_types.push((**fallback).clone());
                }
            }
            TAtomic::TIterable { value_type, .. } => value_types.push((**value_type).clone()),
            TAtomic::TNamedObject { type_params, .. } => {
                if let Some(type_params) = type_params {
                    if type_params.len() >= 2 {
                        value_types.push(type_params[1].clone());
                    } else if let Some(first) = type_params.first() {
                        value_types.push(first.clone());
                    } else {
                        value_types.push(TUnion::mixed());
                    }
                } else {
                    value_types.push(TUnion::mixed());
                }
            }
            _ => {}
        }
    }

    if value_types.is_empty() {
        TUnion::mixed()
    } else {
        // Combine all value types using the type combiner
        let mut result = value_types.remove(0);
        for t in value_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Extract the key type from an iterable type.
fn extract_iterable_key_type(iter_type: &TUnion, _analyzer: &StatementsAnalyzer<'_>) -> TUnion {
    let mut key_types = Vec::new();

    for atomic in &iter_type.types {
        match atomic {
            TAtomic::TArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TNonEmptyArray { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => key_types.push(TUnion::int()),
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                ..
            } => {
                // Union of all property key types
                for key in properties.keys() {
                    match key {
                        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                            key_types.push(TUnion::new(TAtomic::TLiteralInt {
                                value: *value,
                            }));
                        }
                        pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                            if let Ok(int_value) = value.parse::<i64>() {
                                key_types.push(TUnion::new(TAtomic::TLiteralInt {
                                    value: int_value,
                                }));
                            } else {
                                key_types.push(TUnion::new(TAtomic::TLiteralString {
                                    value: value.clone(),
                                }));
                            }
                        }
                    }
                }
                if let Some(fallback) = fallback_key_type {
                    key_types.push((**fallback).clone());
                }
            }
            TAtomic::TIterable { key_type, .. } => key_types.push((**key_type).clone()),
            TAtomic::TNamedObject { type_params, .. } => {
                if let Some(type_params) = type_params {
                    if type_params.len() >= 2 {
                        key_types.push(type_params[0].clone());
                    } else {
                        key_types.push(TUnion::array_key());
                    }
                } else {
                    key_types.push(TUnion::array_key());
                }
            }
            _ => {}
        }
    }

    if key_types.is_empty() {
        TUnion::array_key()
    } else {
        // Combine all key types using the type combiner
        let mut result = key_types.remove(0);
        for t in key_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    }
}

/// Set a variable's type in the context from an expression.
fn set_expression_var_type(
    expr: &Expression<'_>,
    var_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let target = unwrap_reference_target(expr);

    match target.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            let var_id = analyzer.interner.intern(direct.name);
            context.set_var_type(var_id, var_type.clone());
        }
        Expression::List(list) => {
            for (offset, element) in list.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        Expression::Array(array) => {
            for (offset, element) in array.elements.iter().enumerate() {
                set_destructuring_element_var_type(element, offset, var_type, analyzer, context);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
enum DestructuringLookupKey {
    Int(i64),
    String(String),
    Unknown,
}

fn set_destructuring_element_var_type(
    element: &ArrayElement<'_>,
    offset: usize,
    source_type: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let (target_expr, lookup_key) = match element {
        ArrayElement::Missing(_) | ArrayElement::Variadic(_) => return,
        ArrayElement::Value(value_element) => (
            value_element.value,
            DestructuringLookupKey::Int(offset as i64),
        ),
        ArrayElement::KeyValue(key_value) => (
            key_value.value,
            extract_destructuring_key(key_value.key).unwrap_or(DestructuringLookupKey::Unknown),
        ),
    };

    let target_type = infer_destructured_value_type(source_type, &lookup_key);
    set_expression_var_type(target_expr, &target_type, analyzer, context);
}

fn extract_destructuring_key(expr: &Expression<'_>) -> Option<DestructuringLookupKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .map(|value| DestructuringLookupKey::Int(value as i64)),
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| DestructuringLookupKey::String(value.to_string())),
        _ => None,
    }
}

fn infer_destructured_value_type(
    source_type: &TUnion,
    lookup_key: &DestructuringLookupKey,
) -> TUnion {
    let mut inferred_type: Option<TUnion> = None;
    let mut saw_destructurable_type = false;

    for atomic in &source_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                saw_destructurable_type = true;
                add_inferred_union(&mut inferred_type, value_type);
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                saw_destructurable_type = true;
                if let Some(array_key) = lookup_key_to_array_key(lookup_key) {
                    if let Some(property_type) = properties.get(&array_key) {
                        add_inferred_union(&mut inferred_type, property_type);
                    } else if let Some(fallback_value_type) = fallback_value_type {
                        add_inferred_union(&mut inferred_type, fallback_value_type);
                    }
                } else if let Some(fallback_value_type) = fallback_value_type {
                    add_inferred_union(&mut inferred_type, fallback_value_type);
                } else {
                    for property_type in properties.values() {
                        add_inferred_union(&mut inferred_type, property_type);
                    }
                }
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return TUnion::mixed(),
            _ => {}
        }
    }

    if let Some(inferred_type) = inferred_type {
        inferred_type
    } else if saw_destructurable_type {
        TUnion::mixed()
    } else {
        source_type.clone()
    }
}

fn lookup_key_to_array_key(key: &DestructuringLookupKey) -> Option<ArrayKey> {
    match key {
        DestructuringLookupKey::Int(value) => Some(ArrayKey::Int(*value)),
        DestructuringLookupKey::String(value) => Some(ArrayKey::String(value.clone())),
        DestructuringLookupKey::Unknown => None,
    }
}

fn add_inferred_union(target: &mut Option<TUnion>, next: &TUnion) {
    if let Some(existing) = target {
        *existing = combine_union_types(existing, next, false);
    } else {
        *target = Some(next.clone());
    }
}

fn unwrap_reference_target<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    if let Expression::UnaryPrefix(unary) = expr.unparenthesized()
        && matches!(unary.operator, UnaryPrefixOperator::Reference(_))
    {
        return unary.operand;
    }

    expr
}

fn mark_foreach_reference_target(
    expr: &Expression<'_>,
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
) {
    let Expression::UnaryPrefix(unary) = expr.unparenthesized() else {
        return;
    };

    if !matches!(unary.operator, UnaryPrefixOperator::Reference(_)) {
        return;
    }

    let Expression::Variable(Variable::Direct(direct)) = unary.operand.unparenthesized() else {
        return;
    };

    let var_id = analyzer.interner.intern(direct.name);
    context.clear_confusing_reference(var_id);
    context.mark_external_reference(var_id);
}
