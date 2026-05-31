//! Analyzer for array access expressions ($arr[key]).

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::array::ArrayAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::ttype::type_combiner;
use pzoom_code_info::{ArrayKey, Assertion, ClauseKey, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

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
    let cached_base_key = expression_identifier::get_expression_var_key(access.array);
    let cached_index_key = get_array_index_key(access.index);

    if let (Some(base_key), Some(index_key)) = (cached_base_key.as_ref(), cached_index_key.as_ref())
    {
        if can_reuse_cached_dim_path(base_key, index_key) {
            let full_key = format!("{}[{}]", base_key, index_key);
            let full_key_id = analyzer.interner.intern(&full_key);
            if let Some(existing_type) = context.locals.get(&full_key_id) {
                let has_asserted_dim = context_asserts_isset_state(context, &full_key) == Some(true);
                let base_has_nullable_or_falsable_access = analyzer
                    .interner
                    .find(base_key)
                    .and_then(|base_key_id| context.locals.get(&base_key_id))
                    .is_some_and(|base_type| base_type.is_nullable || base_type.is_falsable);

                if has_asserted_dim {
                    analysis_data.set_expr_type(pos, existing_type.clone());
                    return;
                }

                if existing_type.possibly_undefined {
                    if context_asserts_isset_state(context, &full_key) == Some(true)
                        && !base_has_nullable_or_falsable_access
                    {
                        analysis_data.set_expr_type(pos, existing_type.clone());
                        return;
                    }
                } else {
                    analysis_data.set_expr_type(pos, existing_type.clone());
                    return;
                }
            }
        }
    }

    // Analyze the array expression
    let array_pos = expression_analyzer::analyze(analyzer, access.array, analysis_data, context);

    // Psalm/Hakana do not suppress undefined-variable checks for dim expressions
    // inside isset()/unset() guards.
    let was_inside_isset = context.inside_isset;
    let was_inside_unset = context.inside_unset;
    if context.inside_isset || context.inside_unset {
        context.inside_isset = false;
        context.inside_unset = false;
    }

    // Analyze the index expression
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let index_pos = expression_analyzer::analyze(analyzer, access.index, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    context.inside_isset = was_inside_isset;
    context.inside_unset = was_inside_unset;

    let array_type = analysis_data
        .get_expr_type(array_pos)
        .map(|rc| (*rc).clone());
    let index_type = analysis_data
        .get_expr_type(index_pos)
        .map(|rc| (*rc).clone());

    let base_has_nullable_or_falsable_access = array_type
        .as_ref()
        .is_some_and(|t| t.is_nullable || t.is_falsable);

    let mut cached_possibly_undefined_offset = false;

    if let (Some(base_key), Some(index_key)) = (cached_base_key, cached_index_key) {
        if can_reuse_cached_dim_path(&base_key, &index_key) {
            let full_key = format!("{}[{}]", base_key, index_key);
            let full_key_id = analyzer.interner.intern(&full_key);
            if let Some(existing_type) = context.locals.get(&full_key_id) {
                let has_asserted_dim = context_asserts_isset_state(context, &full_key) == Some(true);
                if has_asserted_dim {
                    analysis_data.set_expr_type(pos, existing_type.clone());
                    return;
                }
                if existing_type.possibly_undefined {
                    if context_asserts_isset_state(context, &full_key) == Some(true) {
                        if !base_has_nullable_or_falsable_access {
                            analysis_data.set_expr_type(pos, existing_type.clone());
                            return;
                        }
                    }
                    cached_possibly_undefined_offset = true;
                } else {
                    analysis_data.set_expr_type(pos, existing_type.clone());
                    return;
                }
            }
        }
    }

    // If we don't know the array type, return mixed
    let Some(array_type) = array_type else {
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    };
    let result_from_docblock = array_type.from_docblock;

    // Check each type in the union
    let mut result_types: Vec<TAtomic> = Vec::new();
    let mut has_valid_access = false;
    let mut has_invalid_access = false;
    let mut has_null = false;
    let mut has_mixed_access = false;
    let mut invalid_type_name = String::new();
    let literal_index_keys = index_type
        .as_ref()
        .and_then(|index_type| get_literal_array_keys_from_union(index_type))
        .or_else(|| get_literal_array_key_from_expr(access.index).map(|key| vec![key]))
        .unwrap_or_default();
    let mut has_possibly_undefined_offset = cached_possibly_undefined_offset;
    let mut has_literal_index_hit = false;
    let mut has_literal_index_miss = false;
    let mut expected_offset_type: Option<TUnion> = None;
    let mut atomic_expected_offset_types: Vec<TUnion> = Vec::new();

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
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }
                let key_type = match atomic {
                    TAtomic::TArray { key_type, .. } | TAtomic::TNonEmptyArray { key_type, .. } => {
                        (**key_type).clone()
                    }
                    _ => TUnion::int(),
                };
                merge_expected_offset_type(&mut expected_offset_type, key_type.clone());
                atomic_expected_offset_types.push(key_type);
                for t in &value_type.types {
                    if !result_types.contains(t) {
                        result_types.push(t.clone());
                    }
                }
            }

            // Keyed array - check specific key or use fallback
            TAtomic::TKeyedArray {
                properties,
                is_list,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                has_valid_access = true;
                let keyed_key_type = extract_keyed_array_key_type(
                    properties,
                    *is_list,
                    fallback_key_type.as_deref(),
                );
                merge_expected_offset_type(&mut expected_offset_type, keyed_key_type.clone());
                atomic_expected_offset_types.push(keyed_key_type);
                if !literal_index_keys.is_empty() {
                    for index_key in &literal_index_keys {
                        if let Some(value_type) = properties.get(index_key) {
                            has_literal_index_hit = true;
                            if value_type.possibly_undefined {
                                if fallback_value_type.is_none() {
                                    has_possibly_undefined_offset = true;
                                }
                                if let Some(fallback) = fallback_value_type {
                                    for t in &fallback.types {
                                        if !result_types.contains(t) {
                                            result_types.push(t.clone());
                                        }
                                    }
                                }
                            }
                            for t in &value_type.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        } else if let Some(fallback) = fallback_value_type {
                            has_literal_index_hit = true;
                            for t in &fallback.types {
                                if !result_types.contains(t) {
                                    result_types.push(t.clone());
                                }
                            }
                        } else {
                            has_literal_index_miss = true;
                            has_possibly_undefined_offset = true;
                        }
                    }
                } else {
                    // Non-literal keyed access is handled via key-type checks; Psalm does not
                    // generally emit possibly-undefined-offset here.
                    for value in properties.values() {
                        for t in &value.types {
                            if !result_types.contains(t) {
                                result_types.push(t.clone());
                            }
                        }
                    }
                    if let Some(fallback) = fallback_value_type {
                        for t in &fallback.types {
                            if !result_types.contains(t) {
                                result_types.push(t.clone());
                            }
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
                merge_expected_offset_type(&mut expected_offset_type, TUnion::int());
                atomic_expected_offset_types.push(TUnion::int());
                if !result_types.contains(&TAtomic::TString) {
                    result_types.push(TAtomic::TString);
                }
            }

            // Mixed - could be anything
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => {
                has_valid_access = true;
                has_mixed_access = true;
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }
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
            | TAtomic::TResource
            | TAtomic::TCallable { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TVoid => {
                has_invalid_access = true;
                invalid_type_name = atomic.get_id(Some(analyzer.interner));
            }
            TAtomic::TNothing => {}

            TAtomic::TNamedObject { name, .. } => {
                if class_supports_array_access(analyzer, *name) {
                    has_valid_access = true;
                    if !literal_index_keys.is_empty() {
                        has_literal_index_hit = true;
                    }
                    let class_name = analyzer.interner.lookup(*name);
                    let is_weak_map = class_name
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("WeakMap");

                    if is_weak_map {
                        if let TAtomic::TNamedObject {
                            type_params: Some(type_params),
                            ..
                        } = atomic
                        {
                            if let Some(key_type) = type_params.first() {
                                merge_expected_offset_type(
                                    &mut expected_offset_type,
                                    key_type.clone(),
                                );
                                atomic_expected_offset_types.push(key_type.clone());
                            }
                            if let Some(value_type) = type_params.get(1) {
                                for t in &value_type.types {
                                    if !result_types.contains(t) {
                                        result_types.push(t.clone());
                                    }
                                }
                            } else if !result_types.contains(&TAtomic::TMixed) {
                                result_types.push(TAtomic::TMixed);
                            }
                        } else if !result_types.contains(&TAtomic::TMixed) {
                            merge_expected_offset_type(&mut expected_offset_type, TUnion::mixed());
                            atomic_expected_offset_types.push(TUnion::mixed());
                            result_types.push(TAtomic::TMixed);
                        }
                    } else {
                        merge_expected_offset_type(&mut expected_offset_type, TUnion::mixed());
                        atomic_expected_offset_types.push(TUnion::mixed());
                        if !result_types.contains(&TAtomic::TMixed) {
                            result_types.push(TAtomic::TMixed);
                        }
                    }
                } else {
                    has_invalid_access = true;
                    invalid_type_name = atomic.get_id(Some(analyzer.interner));
                }
            }

            // Other types - treat as potentially valid for now
            _ => {
                has_valid_access = true;
                if !literal_index_keys.is_empty() {
                    has_literal_index_hit = true;
                }
                result_types.push(TAtomic::TMixed);
            }
        }
    }

    // Report issues based on what we found
    let span = access.array.span();
    let start_line = get_line_number(analyzer.source, span.start.offset);

    // Pure null access
    if has_null
        && !has_valid_access
        && !has_invalid_access
        && !context.inside_isset
        && !context.inside_conditional
    {
        analysis_data.add_issue(Issue::new(
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
    if has_null
        && (has_valid_access || has_invalid_access)
        && !context.inside_isset
        && !context.inside_conditional
    {
        analysis_data.add_issue(Issue::new(
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
    if has_invalid_access && !has_valid_access && !context.inside_isset {
        analysis_data.add_issue(Issue::new(
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
    if has_invalid_access && has_valid_access && !context.inside_isset {
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyInvalidArrayAccess,
            format!(
                "Cannot access array offset on value that may be {}",
                invalid_type_name
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    if has_mixed_access && !context.inside_isset && !context.inside_unset {
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArrayAccess,
            "Cannot access array value on mixed type".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    let should_emit_invalid_literal_offset = !literal_index_keys.is_empty()
        && has_literal_index_miss
        && !has_literal_index_hit
        && !context.inside_unset;

    let mut emitted_offset_issue = false;
    if should_emit_invalid_literal_offset {
        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        let index_type_id = index_type
            .as_ref()
            .map(|t| t.get_id(Some(analyzer.interner)))
            .unwrap_or_else(|| "array-key".to_string());

        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArrayOffset,
            format!("Invalid array offset type: {}", index_type_id),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        emitted_offset_issue = true;
    }

    if has_possibly_undefined_offset
        && !should_emit_invalid_literal_offset
        && !has_invalid_access
        && !context.inside_isset
        && !context.inside_unset
        && !context.inside_conditional
    {
        analysis_data.add_issue(Issue::new(
            IssueKind::PossiblyUndefinedArrayOffset,
            "Possibly undefined array offset".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
    }

    // Check for invalid array offset type
    if let Some(index_type) = index_type.clone() {
        if emitted_offset_issue {
            if result_types.is_empty() {
                analysis_data.set_expr_type(pos, TUnion::nothing());
            } else {
                let combined = type_combiner::combine(result_types, false);
                let mut result_union = TUnion::from_types(combined);
                result_union.from_docblock = result_from_docblock;
                analysis_data.set_expr_type(pos, result_union);
            }
            return;
        }

        if context.inside_unset {
            // Psalm/Hakana do not report array-offset-type issues inside unset guards.
            if result_types.is_empty() {
                analysis_data.set_expr_type(pos, TUnion::mixed());
            } else {
                let combined = type_combiner::combine(result_types, false);
                let mut result_union = TUnion::from_types(combined);
                result_union.from_docblock = result_from_docblock;
                analysis_data.set_expr_type(pos, result_union);
            }
            return;
        }

        if atomic_expected_offset_types.is_empty() {
            emitted_offset_issue = check_array_offset(
                &index_type,
                expected_offset_type.as_ref(),
                analyzer,
                access,
                analysis_data,
                context.inside_isset,
            );
        } else {
            emitted_offset_issue = check_array_offset_against_expected_branches(
                &index_type,
                &atomic_expected_offset_types,
                analyzer,
                access,
                analysis_data,
                context.inside_isset,
            );
        }
    }

    // Set the result type using the type combiner for proper simplification
    if result_types.is_empty() {
        if emitted_offset_issue {
            analysis_data.set_expr_type(pos, TUnion::nothing());
        } else {
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    } else {
        let combined = type_combiner::combine(result_types, false);
        let mut result_union = TUnion::from_types(combined);
        result_union.from_docblock = result_from_docblock;
        analysis_data.set_expr_type(pos, result_union);
    }

    if let (Some(base_key), Some(index_key)) = (
        expression_identifier::get_expression_var_key(access.array),
        get_array_index_key(access.index),
    ) {
        if can_reuse_cached_dim_path(&base_key, &index_key)
            && let Some(expr_type) = analysis_data.get_expr_type(pos).map(|t| (*t).clone())
        {
            let is_cacheable = !expr_type.is_mixed()
                && !expr_type.is_nullable
                && !expr_type.is_falsable
                && !expr_type.possibly_undefined
                && !expr_type.is_nothing();

            if is_cacheable {
                let full_key = format!("{}[{}]", base_key, index_key);
                let full_key_id = analyzer.interner.intern(&full_key);
                context.locals.insert(full_key_id, expr_type);
            }
        }
    }
}

fn get_array_index_key(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => {
            int_lit.value.map(|value| value.to_string())
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                int_value.to_string()
            } else {
                let escaped = value.replace('\'', "\\'");
                format!("'{}'", escaped)
            }
        }),
        Expression::Variable(Variable::Direct(direct)) => Some(direct.name.to_string()),
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            let class_name = match class_const_access.class.unparenthesized() {
                Expression::Identifier(identifier) => identifier.value().to_string(),
                Expression::Self_(_) => "self".to_string(),
                Expression::Static(_) => "static".to_string(),
                Expression::Parent(_) => "parent".to_string(),
                _ => return None,
            };

            let constant_name = match &class_const_access.constant {
                ClassLikeConstantSelector::Identifier(identifier) => identifier.value,
                _ => return None,
            };

            Some(format!("{}::{}", class_name, constant_name))
        }
        _ => None,
    }
}

fn can_reuse_cached_dim_path(base_key: &str, index_key: &str) -> bool {
    let _ = (base_key, index_key);
    true
}

fn context_asserts_isset_state(context: &BlockContext, var_name: &str) -> Option<bool> {
    let descendant_prefix_array = format!("{var_name}[");
    let descendant_prefix_property = format!("{var_name}->");

    for clause in &context.clauses {
        for (name, assertions_by_offset) in &clause.possibilities {
            let ClauseKey::Name(name) = name else {
                continue;
            };

            if name != var_name
                && !name.starts_with(&descendant_prefix_array)
                && !name.starts_with(&descendant_prefix_property)
            {
                continue;
            }

            for assertion in assertions_by_offset.values() {
                match assertion {
                    Assertion::IsIsset | Assertion::IsEqualIsset => return Some(true),
                    Assertion::ArrayKeyExists
                    | Assertion::HasArrayKey(_)
                    | Assertion::HasNonnullEntryForKey(_) => return Some(true),
                    Assertion::IsNotIsset => {
                        if name == var_name {
                            return Some(false);
                        }
                    }
                    Assertion::ArrayKeyDoesNotExist
                    | Assertion::DoesNotHaveArrayKey(_)
                    | Assertion::DoesNotHaveNonnullEntryForKey(_) => {
                        if name == var_name {
                            return Some(false);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

fn get_literal_array_key_from_expr(expr: &Expression<'_>) -> Option<ArrayKey> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(ArrayKey::Int),
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            if let Ok(int_value) = value.parse::<i64>() {
                ArrayKey::Int(int_value)
            } else {
                ArrayKey::String(value.to_string())
            }
        }),
        _ => None,
    }
}

fn get_literal_array_keys_from_union(index_type: &TUnion) -> Option<Vec<ArrayKey>> {
    if index_type.types.is_empty() {
        return None;
    }

    let mut keys = Vec::with_capacity(index_type.types.len());

    for atomic in &index_type.types {
        let key = match atomic {
            TAtomic::TLiteralInt { value } => ArrayKey::Int(*value),
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .map(ArrayKey::Int)
                .unwrap_or_else(|_| ArrayKey::String(value.clone())),
            _ => return None,
        };

        if !keys.contains(&key) {
            keys.push(key);
        }
    }

    Some(keys)
}

fn class_supports_array_access(analyzer: &StatementsAnalyzer<'_>, class_name: StrId) -> bool {
    let array_access = analyzer.interner.intern("ArrayAccess");
    let array_access_fq = analyzer.interner.intern("\\ArrayAccess");

    if class_name == StrId::SIMPLE_XML_ELEMENT {
        return true;
    }

    if class_name == array_access || class_name == array_access_fq {
        return true;
    }

    let Some(class_info) = analyzer.codebase.get_class(class_name) else {
        return false;
    };

    class_info.interfaces.contains(&array_access)
        || class_info.interfaces.contains(&array_access_fq)
        || class_info.all_parent_interfaces.contains(&array_access)
        || class_info.all_parent_interfaces.contains(&array_access_fq)
}

/// Check if the array offset type is valid.
fn check_array_offset(
    index_type: &TUnion,
    expected_offset_type: Option<&TUnion>,
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    analysis_data: &mut FunctionAnalysisData,
    suppress_possible_issue: bool,
) -> bool {
    if let Some(expected_offset_type) = expected_offset_type {
        let coerce_class_strings = should_coerce_class_string_offsets(expected_offset_type);
        let normalized_index_type =
            normalize_array_offset_comparison_union(
                &expand_template_union_bounds(index_type),
                analyzer,
                coerce_class_strings,
            );

        if expected_offset_type
            .types
            .iter()
            .any(|atomic| is_unresolved_expected_offset_atomic(atomic, analyzer))
        {
            let span = access.array.span();
            let start_line = get_line_number(analyzer.source, span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedArrayAccess,
                "Cannot access array value with unresolved key type".to_string(),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }

        if expected_offset_type.is_mixed() || expected_offset_type.is_nothing() {
            return false;
        }

        let mut comparison_result = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected_offset_type,
            false,
            false,
            &mut comparison_result,
        ) {
            return false;
        }

        if suppress_possible_issue {
            let mut reverse_comparison_result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                expected_offset_type,
                &normalized_index_type,
                false,
                false,
                &mut reverse_comparison_result,
            ) {
                return false;
            }

            if expected_offsets_fit_class_string_bound(
                analyzer,
                expected_offset_type,
                &normalized_index_type,
            ) {
                return false;
            }

            if union_contains_class_string_like(&normalized_index_type)
                || union_contains_class_string_like(index_type)
            {
                return false;
            }
        }

        let span = access.index.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);

        if union_type_comparator::can_be_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected_offset_type,
        ) {
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidArrayOffset,
                format!(
                    "Array offset may be invalid type: {}",
                    normalized_index_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        } else {
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!(
                    "Invalid array offset type: {}",
                    normalized_index_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }
    }

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
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TArrayKey
            | TAtomic::TMixed => {
                has_valid_offset = true;
            }

            // Invalid offset types
            TAtomic::TArray { .. }
            | TAtomic::TNonEmptyArray { .. }
            | TAtomic::TKeyedArray { .. }
            | TAtomic::TObject
            | TAtomic::TFloat
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TResource
            | TAtomic::TNull
            | TAtomic::TVoid => {
                has_invalid_offset = true;
                invalid_offset_type = atomic.get_id(Some(analyzer.interner));
            }
            TAtomic::TNamedObject { name, .. } => {
                // Docblocks may carry unresolved class-constant pseudo-types such as `self::FOO`.
                let type_name = analyzer.interner.lookup(*name);
                if type_name.contains("::") {
                    has_valid_offset = true;
                } else {
                    has_invalid_offset = true;
                    invalid_offset_type = atomic.get_id(Some(analyzer.interner));
                }
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
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidArrayOffset,
                format!("Array offset may be invalid type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        } else {
            if suppress_possible_issue {
                return false;
            }
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArrayOffset,
                format!("Invalid array offset type: {}", invalid_offset_type),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                start_line,
                0,
            ));
            return true;
        }
    }

    false
}

fn check_array_offset_against_expected_branches(
    index_type: &TUnion,
    expected_branches: &[TUnion],
    analyzer: &StatementsAnalyzer<'_>,
    access: &ArrayAccess<'_>,
    analysis_data: &mut FunctionAnalysisData,
    suppress_possible_issue: bool,
) -> bool {
    let coerce_class_strings = expected_branches
        .iter()
        .all(should_coerce_class_string_offsets);
    let normalized_index_type =
        normalize_array_offset_comparison_union(
            &expand_template_union_bounds(index_type),
            analyzer,
            coerce_class_strings,
        );

    if expected_branches.iter().any(|expected| {
        expected
            .types
            .iter()
            .any(|atomic| is_unresolved_expected_offset_atomic(atomic, analyzer))
    }) {
        let span = access.array.span();
        let start_line = get_line_number(analyzer.source, span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedArrayAccess,
            "Cannot access array value with unresolved key type".to_string(),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            start_line,
            0,
        ));
        return true;
    }

    let mut has_valid_branch = false;
    let mut has_invalid_branch = false;

    for expected in expected_branches {
        if expected.is_mixed() || expected.is_nothing() {
            has_valid_branch = true;
            continue;
        }

        let mut comparison_result = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected,
            false,
            false,
            &mut comparison_result,
        ) {
            has_valid_branch = true;
            continue;
        }

        if suppress_possible_issue {
            let mut reverse_comparison_result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                expected,
                &normalized_index_type,
                false,
                false,
                &mut reverse_comparison_result,
            ) {
                has_valid_branch = true;
                continue;
            }

            if expected_offsets_fit_class_string_bound(analyzer, expected, &normalized_index_type) {
                has_valid_branch = true;
                continue;
            }
        }

        if union_type_comparator::can_be_contained_by(
            analyzer.codebase,
            &normalized_index_type,
            expected,
        ) {
            has_valid_branch = true;
            has_invalid_branch = true;
        } else {
            has_invalid_branch = true;
        }
    }

    if !has_invalid_branch {
        return false;
    }

    if suppress_possible_issue
        && (union_contains_class_string_like(&normalized_index_type)
            || union_contains_class_string_like(index_type))
    {
        return false;
    }

    if has_valid_branch && suppress_possible_issue {
        return false;
    }

    if suppress_possible_issue {
        return false;
    }

    let span = access.index.span();
    let start_line = get_line_number(analyzer.source, span.start.offset);
    let kind = if has_valid_branch {
        IssueKind::PossiblyInvalidArrayOffset
    } else {
        IssueKind::InvalidArrayOffset
    };

    let message = if has_valid_branch {
        format!(
            "Array offset may be invalid type: {}",
            normalized_index_type.get_id(Some(analyzer.interner))
        )
    } else {
        format!(
            "Invalid array offset type: {}",
            normalized_index_type.get_id(Some(analyzer.interner))
        )
    };

    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        span.start.offset,
        span.end.offset,
        start_line,
        0,
    ));

    true
}

fn merge_expected_offset_type(target: &mut Option<TUnion>, incoming: TUnion) {
    match target {
        Some(existing) => {
            *existing = combine_union_types(existing, &incoming, false);
        }
        None => {
            *target = Some(incoming);
        }
    }
}

fn expand_template_union_bounds(union: &TUnion) -> TUnion {
    let mut expanded = Vec::new();

    for atomic in &union.types {
        match atomic {
            TAtomic::TTemplateParam { as_type, .. } => {
                for nested in &as_type.types {
                    if !expanded.contains(nested) {
                        expanded.push(nested.clone());
                    }
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !expanded.contains(as_type) {
                    expanded.push((**as_type).clone());
                }
            }
            _ => {
                if !expanded.contains(atomic) {
                    expanded.push(atomic.clone());
                }
            }
        }
    }

    if expanded.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(expanded)
    }
}

fn normalize_array_offset_comparison_union(
    union: &TUnion,
    analyzer: &StatementsAnalyzer<'_>,
    coerce_class_strings: bool,
) -> TUnion {
    let mut normalized = Vec::new();

    for atomic in &union.types {
        let next = match atomic {
            TAtomic::TLiteralString { value } => value
                .parse::<i64>()
                .ok()
                .map(|int_value| TAtomic::TLiteralInt { value: int_value })
                .unwrap_or_else(|| atomic.clone()),
            TAtomic::TLiteralClassString { name } if coerce_class_strings => {
                TAtomic::TLiteralString {
                    value: name.trim_start_matches('\\').to_string(),
                }
            }
            TAtomic::TClassString {
                as_type: Some(as_type),
            } if coerce_class_strings => {
                if let TAtomic::TNamedObject { name, .. } = as_type.as_ref() {
                    TAtomic::TLiteralString {
                        value: analyzer
                            .interner
                            .lookup(*name)
                            .trim_start_matches('\\')
                            .to_string(),
                    }
                } else {
                    TAtomic::TString
                }
            }
            TAtomic::TClassString { .. } if coerce_class_strings => TAtomic::TString,
            TAtomic::TNumericString | TAtomic::TNonEmptyNumericString => TAtomic::TInt,
            _ => atomic.clone(),
        };

        if !normalized.contains(&next) {
            normalized.push(next);
        }
    }

    if normalized.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(normalized)
    }
}

fn should_coerce_class_string_offsets(expected_offset_type: &TUnion) -> bool {
    let mut has_literal_string = false;

    for atomic in &expected_offset_type.types {
        match atomic {
            TAtomic::TLiteralString { .. } => {
                has_literal_string = true;
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TArrayKey
            | TAtomic::TMixed
            | TAtomic::TNonEmptyMixed => {
                return false;
            }
            _ => {}
        }
    }

    has_literal_string
}

fn expected_offsets_fit_class_string_bound(
    analyzer: &StatementsAnalyzer<'_>,
    expected_offset_type: &TUnion,
    index_type: &TUnion,
) -> bool {
    let mut bounds: Vec<Option<StrId>> = Vec::new();

    for atomic in &index_type.types {
        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if let TAtomic::TNamedObject { name, .. } = as_type.as_ref() {
                    bounds.push(Some(*name));
                }
            }
            TAtomic::TClassString { as_type: None } => {
                bounds.push(None);
            }
            TAtomic::TLiteralClassString { name } => {
                if let Some(class_id) = resolve_class_id_from_literal_string(analyzer, name) {
                    bounds.push(Some(class_id));
                }
            }
            _ => {}
        }
    }

    if bounds.is_empty() {
        return false;
    }

    let mut expected_class_ids = Vec::new();

    for atomic in &expected_offset_type.types {
        let class_id = match atomic {
            TAtomic::TLiteralClassString { name } => {
                resolve_class_id_from_literal_string(analyzer, name)
            }
            TAtomic::TLiteralString { value } => resolve_class_id_from_literal_string(analyzer, value),
            _ => None,
        };

        let Some(class_id) = class_id else {
            return false;
        };

        expected_class_ids.push(class_id);
    }

    if expected_class_ids.is_empty() {
        return false;
    }

    expected_class_ids.into_iter().all(|expected_class_id| {
        bounds.iter().any(|bound| match bound {
            None => true,
            Some(bound_class_id) => object_type_comparator::is_class_subtype_of(
                expected_class_id,
                *bound_class_id,
                analyzer.codebase,
            ),
        })
    })
}

fn resolve_class_id_from_literal_string(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
) -> Option<StrId> {
    let mut normalized = class_name.trim().trim_start_matches('\\').to_string();

    if normalized
        .to_ascii_lowercase()
        .ends_with("::class")
        && let Some((class_part, _)) = normalized.rsplit_once("::")
    {
        normalized = class_part.trim_start_matches('\\').to_string();
    }

    analyzer
        .interner
        .find(&normalized)
        .or_else(|| analyzer.interner.find(&format!("\\{}", normalized)))
}

fn union_contains_class_string_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TTemplateParamClass { .. }
        )
    })
}

fn extract_keyed_array_key_type(
    properties: &rustc_hash::FxHashMap<ArrayKey, TUnion>,
    is_list: bool,
    fallback_key_type: Option<&TUnion>,
) -> TUnion {
    let mut key_type = fallback_key_type.cloned().unwrap_or_else(TUnion::nothing);

    for key in properties.keys() {
        let literal_key_type = match key {
            ArrayKey::Int(value) => TUnion::new(TAtomic::TLiteralInt { value: *value }),
            ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                value: value.clone(),
            }),
        };

        key_type = if key_type.is_nothing() {
            literal_key_type
        } else {
            combine_union_types(&key_type, &literal_key_type, false)
        };
    }

    if is_list {
        key_type = if key_type.is_nothing() {
            TUnion::int()
        } else {
            combine_union_types(&key_type, &TUnion::int(), false)
        };
    }

    if key_type.is_nothing() {
        TUnion::array_key()
    } else {
        key_type
    }
}

fn is_unresolved_expected_offset_atomic(
    atomic: &TAtomic,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. }
        | TAtomic::TObject => true,
        TAtomic::TNamedObject { name, .. } => {
            let type_name = analyzer.interner.lookup(*name);
            !type_name.contains("::")
        }
        _ => false,
    }
}
