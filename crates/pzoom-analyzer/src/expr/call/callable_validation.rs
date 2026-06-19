//! Callable argument validation, modeled on Psalm's callable checks.
use super::argument_analyzer::*;
use super::function_call_analyzer;
use super::{argument_analyzer, arguments_analyzer};

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{
    FunctionLikeInfo, FunctionLikeParameter, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::callable_type_comparator;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use pzoom_code_info::TemplateResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableValidationOutcome {
    NotApplicable,
    Valid,
    IssueEmitted,
}

pub fn union_has_callable(union: &TUnion) -> bool {
    union.types.iter().any(atomic_has_callable)
}

fn atomic_has_callable(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
        TAtomic::TNamedObject { name, .. } => *name == StrId::CLOSURE,
        TAtomic::TTemplateParam { as_type, .. } => union_has_callable(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => atomic_has_callable(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_has_callable),
        _ => false,
    }
}

pub fn infer_callee_return_type(callee_type: &TUnion) -> Option<TUnion> {
    let mut combined: Option<TUnion> = None;

    for atomic in &callee_type.types {
        let return_type = match atomic {
            TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
                return_type
                    .as_ref()
                    .map(|t| (**t).clone())
                    .unwrap_or_else(TUnion::mixed)
            }
            _ => continue,
        };

        combined = Some(if let Some(existing) = combined {
            combine_union_types(&existing, &return_type, false)
        } else {
            return_type
        });
    }

    combined
}

pub(crate) fn union_has_untyped_mixed_callable(union: &TUnion) -> bool {
    union.types.iter().any(atomic_is_untyped_mixed_callable)
}

fn atomic_is_untyped_mixed_callable(atomic: &TAtomic) -> bool {
    let (params, return_type) = match atomic {
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => (params.as_ref(), return_type.as_ref()),
        _ => return false,
    };

    let params_are_mixed = params.is_none_or(|params| {
        params.iter().all(|param| {
            param.param_type.is_mixed()
                || param
                    .param_type
                    .types
                    .iter()
                    .all(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        })
    });
    let return_is_mixed =
        return_type.is_none_or(|return_type| return_type.is_mixed() || return_type.is_nothing());

    params_are_mixed && return_is_mixed
}

pub(crate) fn file_uses_strict_types(analyzer: &StatementsAnalyzer<'_>) -> bool {
    // Computed once per file in StatementsAnalyzer::new.
    analyzer.file_uses_strict_types
}

pub(crate) fn is_runtime_alias_union_contained(
    analyzer: &StatementsAnalyzer<'_>,
    input_type: &TUnion,
    container_type: &TUnion,
    context: &BlockContext,
) -> bool {
    if input_type.types.is_empty() || container_type.types.is_empty() {
        return false;
    }

    input_type.types.iter().all(|input_atomic| {
        container_type.types.iter().any(|container_atomic| {
            runtime_alias_atomic_is_contained_by(analyzer, input_atomic, container_atomic, context)
        })
    })
}

fn runtime_alias_atomic_is_contained_by(
    analyzer: &StatementsAnalyzer<'_>,
    input_atomic: &TAtomic,
    container_atomic: &TAtomic,
    context: &BlockContext,
) -> bool {
    if input_atomic == container_atomic {
        return true;
    }

    let (
        TAtomic::TNamedObject {
            name: input_name,
            type_params: input_type_params,
            ..
        },
        TAtomic::TNamedObject {
            name: container_name,
            type_params: container_type_params,
            ..
        },
    ) = (input_atomic, container_atomic)
    else {
        return false;
    };

    if input_name == container_name {
        return input_type_params == container_type_params;
    }

    // Runtime class_alias containment is only valid for non-generic object names.
    if input_type_params.is_some() || container_type_params.is_some() {
        return false;
    }

    is_class_subtype_with_runtime_aliases(analyzer, *input_name, *container_name, context)
}

fn is_class_subtype_with_runtime_aliases(
    analyzer: &StatementsAnalyzer<'_>,
    input_class: StrId,
    container_class: StrId,
    context: &BlockContext,
) -> bool {
    let target = resolve_runtime_alias_class(container_class, context);
    let mut to_visit = vec![resolve_runtime_alias_class(input_class, context)];
    let mut visited = FxHashSet::default();

    while let Some(current_class) = to_visit.pop() {
        if !visited.insert(current_class) {
            continue;
        }

        if current_class == target {
            return true;
        }

        if let Some(class_info) = analyzer.codebase.get_class(current_class) {
            if let Some(parent_class) = class_info.parent_class {
                to_visit.push(resolve_runtime_alias_class(parent_class, context));
            }

            for interface_id in &class_info.interfaces {
                to_visit.push(resolve_runtime_alias_class(*interface_id, context));
            }
        } else if let Some(alias_target) = context.class_aliases.get(&current_class).copied() {
            to_visit.push(resolve_runtime_alias_class(alias_target, context));
        }
    }

    false
}

fn resolve_runtime_alias_class(class_id: StrId, context: &BlockContext) -> StrId {
    context
        .class_aliases
        .get(&class_id)
        .copied()
        .unwrap_or(class_id)
}

pub(crate) fn param_allows_string_like(param_type: &TUnion) -> bool {
    param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TTruthyString
                | TAtomic::TCallableString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TNumericString
        )
    })
}

pub(crate) fn union_is_stringable_object(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> bool {
    !union.types.is_empty()
        && union
            .types
            .iter()
            .all(|atomic| atomic_is_stringable_object(analyzer, atomic))
}

pub(crate) fn atomic_is_stringable_object(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, .. } => analyzer
            .codebase
            .get_class(*name)
            .is_some_and(|class_info| class_info.methods.contains_key(&StrId::TO_STRING)),
        TAtomic::TTemplateParam { as_type, .. } => union_is_stringable_object(analyzer, as_type),
        _ => false,
    }
}

pub(crate) fn get_unpacked_iterable_key_type(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<TUnion> {
    match atomic {
        TAtomic::TIterable { key_type, .. } => Some((**key_type).clone()),
        // Any array: the key type is the union of its known entries' literal
        // keys and its typed fallback key (a list's fallback key is int).
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let (key_type, _) = crate::expr::array_analyzer::get_keyed_array_generic_params(
                known_values,
                params.as_deref().map(|(key, _)| key),
                params.as_deref().map(|(_, value)| value),
            );
            Some(key_type)
        }
        // class-string-map iterates with class-string keys (Psalm's standin key param).
        TAtomic::TClassStringMap { .. } => Some(TUnion::new(
            atomic
                .get_class_string_map_standin_key_param()
                .expect("checked TClassStringMap above"),
        )),
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
            if !named_object_is_traversable(analyzer, *name) {
                return None;
            }

            Some(
                type_params
                    .as_ref()
                    .and_then(|params| params.first().cloned())
                    .unwrap_or_else(TUnion::array_key),
            )
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut combined: Option<TUnion> = None;
            for nested in &as_type.types {
                let Some(nested_key_type) = get_unpacked_iterable_key_type(analyzer, nested) else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });
            }

            combined
        }
        TAtomic::TObjectIntersection { types } => {
            let mut combined: Option<TUnion> = None;
            for nested in types {
                let Some(nested_key_type) = get_unpacked_iterable_key_type(analyzer, nested) else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &nested_key_type, false)
                } else {
                    nested_key_type
                });
            }

            combined
        }
        _ => None,
    }
}

pub(crate) fn union_contains_only_array_key(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> bool {
    let mut comparison_result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        union,
        &TUnion::array_key(),
        false,
        false,
        &mut comparison_result,
    )
}

pub(crate) fn union_contains_only_int(analyzer: &StatementsAnalyzer<'_>, union: &TUnion) -> bool {
    let mut comparison_result = TypeComparisonResult::new();
    union_type_comparator::is_contained_by(
        analyzer.codebase,
        union,
        &TUnion::int(),
        false,
        false,
        &mut comparison_result,
    )
}

pub(crate) fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
    if name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
    {
        return true;
    }

    analyzer.codebase.get_class(name).is_some_and(|class_info| {
        class_info.interfaces.contains(&StrId::TRAVERSABLE)
            || class_info
                .all_parent_interfaces
                .iter()
                .any(|interface| *interface == StrId::TRAVERSABLE)
    })
}

pub(crate) fn looks_like_unresolved_conditional_docblock_type(type_id: &str) -> bool {
    if type_id.contains("array{") {
        return false;
    }

    type_id.contains("|:") || type_id.contains(" : ")
}

pub(crate) fn expects_class_string_union(param_type: &TUnion) -> bool {
    union_contains_class_string(param_type)
}

fn union_contains_class_string(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_class_string)
}

fn atomic_contains_class_string(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TTemplateParamClass { .. } => true,
        // Any array: a class-string may sit in a known entry's value or the
        // typed fallback value.
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            known_values
                .values()
                .any(|(_, value)| union_contains_class_string(value))
                || params
                    .as_deref()
                    .is_some_and(|(_, value)| union_contains_class_string(value))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_class_string(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains_class_string),
        _ => false,
    }
}

pub(crate) fn has_plain_string_like_atomic(arg_type: &TUnion) -> bool {
    arg_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TCallableString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLiteralString { .. }
        )
    })
}

pub(crate) fn union_has_template_class_string_argument(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(atomic_has_template_class_string_argument)
}

fn atomic_has_template_class_string_argument(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(inner),
        } => matches!(
            inner.as_ref(),
            TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
        ),
        TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => {
            union_has_template_class_string_argument(as_type)
        }
        _ => false,
    }
}

pub(crate) fn union_is_specific_class_string_set(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TClassString {
                as_type: Some(inner),
            } => matches!(inner.as_ref(), TAtomic::TNamedObject { .. }),
            TAtomic::TLiteralClassString { .. } => true,
            _ => false,
        })
}

pub(crate) fn accepts_unconstrained_class_string(param_type: &TUnion) -> bool {
    let mut saw_class_string = false;

    for atomic in &param_type.types {
        match atomic {
            TAtomic::TClassString { as_type: None } => saw_class_string = true,
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if !atomic_is_unconstrained_class_bound(as_type.as_ref()) {
                    return false;
                }
                saw_class_string = true;
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                if !atomic_is_unconstrained_class_bound(as_type.as_ref()) {
                    return false;
                }
                saw_class_string = true;
            }
            _ => return false,
        }
    }

    saw_class_string
}

fn atomic_is_unconstrained_class_bound(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TObject => true,
        TAtomic::TTemplateParam { as_type, .. } => union_is_unconstrained_class_bound(as_type),
        _ => false,
    }
}

fn union_is_unconstrained_class_bound(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(atomic_is_unconstrained_class_bound)
}

pub(crate) fn is_unconstrained_template_union(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TTemplateParam { as_type, .. } => as_type.is_mixed(),
            TAtomic::TTemplateParamClass { as_type, .. } => {
                atomic_is_unconstrained_class_bound(as_type)
            }
            _ => false,
        })
}

pub(crate) fn is_likely_unresolved_template_named_object_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TNamedObject {
                name,
                type_params: None,
                ..
            } => {
                if analyzer.codebase.get_class(*name).is_some() {
                    return false;
                }

                let name_str = analyzer.interner.lookup(*name);
                let raw = name_str.as_ref();
                !raw.contains('\\') && !raw.contains("::") && is_template_identifier_like(raw)
            }
            _ => false,
        })
}

fn is_template_identifier_like(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    first.is_ascii_uppercase() && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(crate) fn normalize_class_constant_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
    callable_name: &str,
) -> TUnion {
    let mut changed = false;
    let mut normalized_types = Vec::new();

    for atomic in &param_type.types {
        if let Some(constant_type) =
            resolve_class_constant_like_atomic(analyzer, atomic, callable_name)
        {
            changed = true;
            for constant_atomic in constant_type.types {
                if !normalized_types.contains(&constant_atomic) {
                    normalized_types.push(constant_atomic);
                }
            }
            continue;
        }

        if !normalized_types.contains(atomic) {
            normalized_types.push(atomic.clone());
        }
    }

    if !changed {
        return param_type.clone();
    }

    let mut normalized = TUnion::from_types(normalized_types);
    normalized.from_docblock = param_type.from_docblock;
    normalized.is_resolved = param_type.is_resolved;
    normalized.parent_nodes = param_type.parent_nodes.clone();
    normalized.ignore_nullable_issues = param_type.ignore_nullable_issues;
    normalized.ignore_falsable_issues = param_type.ignore_falsable_issues;
    normalized
}

fn resolve_class_constant_like_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    callable_name: &str,
) -> Option<TUnion> {
    let TAtomic::TNamedObject {
        name,
        type_params: None,
        ..
    } = atomic
    else {
        return None;
    };

    let raw = analyzer.interner.lookup(*name);
    let (class_part, const_part) = raw.rsplit_once("::")?;
    let class_id = resolve_class_reference_for_constant(analyzer, class_part, callable_name)?;
    let class_info = analyzer.codebase.get_class(class_id)?;
    if let Some(prefix) = const_part.strip_suffix('*') {
        let mut combined: Option<TUnion> = None;
        for (constant_name, constant_info) in &class_info.constants {
            let candidate_name = analyzer.interner.lookup(*constant_name);
            if !candidate_name.starts_with(prefix) {
                continue;
            }

            combined = Some(match combined {
                Some(existing) => {
                    combine_union_types(&existing, &constant_info.constant_type, false)
                }
                None => constant_info.constant_type.clone(),
            });
        }

        return combined;
    }

    let constant_id = analyzer.interner.intern(const_part);
    class_info
        .constants
        .get(&constant_id)
        .map(|constant_info| constant_info.constant_type.clone())
}

fn resolve_class_reference_for_constant(
    analyzer: &StatementsAnalyzer<'_>,
    class_part: &str,
    callable_name: &str,
) -> Option<StrId> {
    let self_class = resolve_self_class_for_callable(analyzer, callable_name);

    if class_part.eq_ignore_ascii_case("self") || class_part.eq_ignore_ascii_case("static") {
        return self_class;
    }

    if class_part.eq_ignore_ascii_case("parent") {
        let self_class = self_class?;
        return analyzer
            .codebase
            .get_class(self_class)
            .and_then(|class_info| class_info.parent_class);
    }

    let class_id = analyzer.interner.intern(class_part);
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let fq = format!("\\{}", class_part.trim_start_matches('\\'));
    let fq_id = analyzer.interner.intern(&fq);
    analyzer.codebase.get_class(fq_id).map(|_| fq_id)
}

fn resolve_self_class_for_callable(
    analyzer: &StatementsAnalyzer<'_>,
    callable_name: &str,
) -> Option<StrId> {
    if let Some((class_name, _)) = callable_name.split_once("::") {
        let class_id = analyzer.interner.intern(class_name);
        if analyzer.codebase.get_class(class_id).is_some() {
            return Some(class_id);
        }

        let fq = format!("\\{}", class_name.trim_start_matches('\\'));
        let fq_id = analyzer.interner.intern(&fq);
        if analyzer.codebase.get_class(fq_id).is_some() {
            return Some(fq_id);
        }
    }

    analyzer.get_declaring_class()
}

/// Collect the function-level template params (`GenericParent::FunctionLike`
/// defining entities) a callable's signature mentions — the callable's *own*
/// templates, as opposed to the callee's standins.
fn collect_own_callable_templates(
    union: &TUnion,
    template_result: &mut pzoom_code_info::TemplateResult,
) {
    for atomic in &union.types {
        match atomic {
            TAtomic::TTemplateParam {
                name,
                defining_entity: defining_entity @ pzoom_code_info::GenericParent::FunctionLike(_),
                as_type,
            } => {
                crate::template::template_types_insert(
                    template_result,
                    *name,
                    *defining_entity,
                    (**as_type).clone(),
                );
            }
            TAtomic::TNamedObject {
                type_params: Some(type_params),
                ..
            } => {
                for type_param in type_params {
                    collect_own_callable_templates(type_param, template_result);
                }
            }
            // Only generic arrays/lists are descended into (old code matched
            // the generic array variants, not TKeyedArray); a shape's known
            // entries are not inspected, preserving prior behaviour.
            // TODO(unify-array): old code ignored shape properties here.
            TAtomic::TArray {
                known_values,
                params: Some(params),
                ..
            } if known_values.is_empty() => {
                collect_own_callable_templates(&params.0, template_result);
                collect_own_callable_templates(&params.1, template_result);
            }
            TAtomic::TIterable {
                key_type,
                value_type,
            } => {
                collect_own_callable_templates(key_type, template_result);
                collect_own_callable_templates(value_type, template_result);
            }
            _ => {}
        }
    }
}

/// Psalm's `HighOrderFunctionArgHandler::remapLowerBounds` +
/// `enhanceCallableArgType`: a callable argument that carries its *own*
/// function-level templates (a named function/method reference like
/// `"from_other"` or `[$class, "method"]`) has those templates solved against
/// the expected callable's parameter types — params only, the contravariant
/// positions — and substituted throughout, so `from_other(ThingType): ThingType|Bar`
/// checked against `callable(FooChild)` becomes
/// `callable(FooChild): FooChild|Bar`. Returns `None` when the candidate has
/// no templates of its own (e.g. closures) or nothing bound.
pub(crate) fn enhance_high_order_callable_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    candidate: &TAtomic,
    expected_callables: &[&TAtomic],
) -> Option<TAtomic> {
    let candidate_params = get_callable_params(candidate)?;

    let mut input_templates = pzoom_code_info::TemplateResult::default();
    for candidate_param in candidate_params {
        collect_own_callable_templates(&candidate_param.param_type, &mut input_templates);
    }
    if let TAtomic::TCallable {
        return_type: Some(return_type),
        ..
    }
    | TAtomic::TClosure {
        return_type: Some(return_type),
        ..
    } = candidate
    {
        collect_own_callable_templates(return_type, &mut input_templates);
    }

    if input_templates.template_types.is_empty() {
        return None;
    }

    for expected_callable in expected_callables {
        let Some(expected_params) = get_callable_params(expected_callable) else {
            continue;
        };

        for (offset, candidate_param) in candidate_params.iter().enumerate() {
            let Some(expected_param) = expected_params.get(offset) else {
                break;
            };

            crate::template::standin_type_replacer::infer_template_replacements_from_union(
                analyzer,
                &candidate_param.param_type,
                &expected_param.param_type,
                &mut input_templates,
            );
        }
    }

    if input_templates.lower_bounds.is_empty() {
        return None;
    }

    let replace = |union: &TUnion| {
        function_call_analyzer::replace_templates_in_union(union, &input_templates)
    };

    Some(match candidate {
        TAtomic::TCallable {
            params: Some(params),
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: Some(
                params
                    .iter()
                    .map(|param| FunctionLikeParameter {
                        param_type: replace(&param.param_type),
                        ..param.clone()
                    })
                    .collect(),
            ),
            return_type: return_type
                .as_ref()
                .map(|return_type| Box::new(replace(return_type))),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params: Some(params),
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: Some(
                params
                    .iter()
                    .map(|param| FunctionLikeParameter {
                        param_type: replace(&param.param_type),
                        ..param.clone()
                    })
                    .collect(),
            ),
            return_type: return_type
                .as_ref()
                .map(|return_type| Box::new(replace(return_type))),
            is_pure: *is_pure,
        },
        _ => return None,
    })
}

/// The function receiving a callable argument, for Psalm's
/// verifyCallableInContext scoping: its class (a method callee) and whether
/// it is one of PHP's native callback-taking functions (call_user_func,
/// usort, ...), which accept in-class non-public callables.
#[derive(Clone, Copy)]
pub(crate) struct CalleeContext {
    pub class: Option<StrId>,
    pub is_native_callback: bool,
    /// The function-like containing this callable expression, so a method
    /// referenced via a callable is attributed to it in the symbol graph.
    pub referencing_id: Option<pzoom_code_info::data_flow::node::FunctionLikeIdentifier>,
    pub referencing_class: Option<StrId>,
}

const PHP_NATIVE_NON_PUBLIC_CB: &[&str] = &[
    "array_filter",
    "array_diff_uassoc",
    "array_diff_ukey",
    "array_intersect_uassoc",
    "array_intersect_ukey",
    "array_map",
    "array_reduce",
    "array_udiff",
    "array_udiff_assoc",
    "array_udiff_uassoc",
    "array_uintersect",
    "array_uintersect_assoc",
    "array_uintersect_uassoc",
    "array_walk",
    "array_walk_recursive",
    "preg_replace_callback",
    "preg_replace_callback_array",
    "call_user_func",
    "call_user_func_array",
    "forward_static_call",
    "forward_static_call_array",
    "is_callable",
    "ob_start",
    "register_shutdown_function",
    "register_tick_function",
    "session_set_save_handler",
    "set_error_handler",
    "set_exception_handler",
    "spl_autoload_register",
    "spl_autoload_unregister",
    "uasort",
    "uksort",
    "usort",
];

pub(crate) fn validate_callable_argument(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_pos: Pos,
    arg_type: &TUnion,
    expected_type: &TUnion,
    argument_offset: usize,
    callable_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    prefer_invalid_argument_for_undefined: bool,
) -> CallableValidationOutcome {
    let expected_callables = get_expected_callable_atomics(expected_type);
    if expected_callables.is_empty() {
        return CallableValidationOutcome::NotApplicable;
    }

    // `string|callable` accepts any string: a string argument satisfies the
    // plain-string side, so it is not validated as a callable reference
    // (Psalm only resolves string callables for callable-only params).
    let expected_accepts_plain_string = expected_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
        )
    });
    let arg_is_all_strings = !arg_type.types.is_empty()
        && arg_type.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TNonEmptyString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
            )
        });
    if expected_accepts_plain_string && arg_is_all_strings {
        return CallableValidationOutcome::NotApplicable;
    }

    // More generally: when the param union's NON-callable side already
    // accepts the argument (e.g. `array{class-string, string}|callable`
    // given a tuple), the arg is not forced through callable validation.
    let non_callable_members: Vec<TAtomic> = expected_type
        .types
        .iter()
        .filter(|atomic| {
            !matches!(
                atomic,
                TAtomic::TCallable { .. } | TAtomic::TClosure { .. } | TAtomic::TCallableString
            )
        })
        .cloned()
        .collect();
    if !non_callable_members.is_empty() {
        let non_callable_union = TUnion::from_types(non_callable_members);
        let mut comparison_result = TypeComparisonResult::new();
        if crate::type_comparator::union_type_comparator::is_contained_by(
            analyzer.codebase,
            arg_type,
            &non_callable_union,
            false,
            false,
            &mut comparison_result,
        ) {
            return CallableValidationOutcome::NotApplicable;
        }
    }

    // The function receiving the callable (Psalm's verifyCallableInContext
    // compares its class to $context->self; PHP's native callback-takers
    // accept in-class non-public callables).
    let callee_class = callable_name.split_once("::").map(|(class_part, _)| {
        analyzer
            .interner
            .intern(class_part.trim_start_matches('\\'))
    });
    let callee_context = CalleeContext {
        class: callee_class,
        is_native_callback: callee_class.is_none()
            && PHP_NATIVE_NON_PUBLIC_CB.contains(&callable_name.to_ascii_lowercase().as_str()),
        referencing_id: context.function_context.referencing_id(),
        referencing_class: context.function_context.calling_class,
    };

    // `map($xs, id())`: the recorded arg type collapsed the callee's unbound
    // templates; validate against the STORAGE return type instead, whose
    // templates the enhancement below can solve (Psalm's
    // HighOrderFunctionArgHandler TYPE_CALLABLE path).
    let raw_high_order =
        function_call_analyzer::high_order_call_arg_raw_callable(analyzer, arg.value(), context)
            .and_then(|raw_union| {
                raw_union
                    .types
                    .iter()
                    .find(|atomic| {
                        matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. })
                    })
                    .cloned()
            });

    let candidate = if let Some(candidate) = raw_high_order {
        candidate
    } else if let Some(candidate) = resolve_callable_from_concat_expr(
        analyzer,
        arg.value(),
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
        callee_context,
    ) {
        candidate
    } else if let Some(candidate) = resolve_candidate_from_union(
        analyzer,
        arg_type,
        &expected_callables,
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
        context,
        callee_context,
    ) {
        candidate
    } else {
        return CallableValidationOutcome::NotApplicable;
    };

    // Psalm solves a named callable's own templates against the expected
    // callable before comparing (HighOrderFunctionArgHandler).
    let candidate = enhance_high_order_callable_atomic(analyzer, &candidate, &expected_callables)
        .unwrap_or(candidate);

    let mut selected_issue_kind: Option<IssueKind> = None;
    let candidate_from_resolved_reference = !arg_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }));

    // A plain, signature-less `callable`/`Closure` (no declared params or return)
    // is compatible with any expected callable shape — mirror Psalm, which does
    // not report a coercion-from-mixed when an untyped callable is passed where a
    // specific `callable(...)` is expected.
    if matches!(
        &candidate,
        TAtomic::TCallable {
            params: None,
            return_type: None,
            ..
        } | TAtomic::TClosure {
            params: None,
            return_type: None,
            ..
        }
    ) {
        return CallableValidationOutcome::Valid;
    }

    for expected_callable in expected_callables {
        let mut comparison_result = TypeComparisonResult::new();
        let is_match = callable_type_comparator::is_contained_by(
            analyzer.codebase,
            &candidate,
            expected_callable,
            &mut comparison_result,
        );

        if is_match {
            let arg_is_literal_reference = matches!(
                arg.value().unparenthesized(),
                Expression::Literal(_) | Expression::Array(_) | Expression::LegacyArray(_)
            );
            if candidate_from_resolved_reference
                && (is_optional_param_gap_mismatch(&candidate, expected_callable)
                    || has_required_param_for_optional_expected(&candidate, expected_callable)
                    || (arg_is_literal_reference
                        && callback_accepts_fewer_than_expected(&candidate, expected_callable)))
            {
                // A string-resolved callback whose signature accepts fewer
                // params than the container passes may behave differently at
                // runtime — Psalm grades it possibly invalid.
                selected_issue_kind = Some(select_preferred_callable_issue_kind(
                    selected_issue_kind,
                    IssueKind::PossiblyInvalidArgument,
                ));
                continue;
            }

            return CallableValidationOutcome::Valid;
        }

        let issue_kind = determine_callable_mismatch_issue_kind(
            analyzer,
            &candidate,
            expected_callable,
            &comparison_result,
        );

        selected_issue_kind = Some(select_preferred_callable_issue_kind(
            selected_issue_kind,
            issue_kind,
        ));
    }

    let kind = selected_issue_kind.unwrap_or(IssueKind::InvalidArgument);

    add_issue(
        analyzer,
        analysis_data,
        arg_pos,
        kind,
        format!(
            "Argument {} of {} expects {}, but {} provided",
            argument_offset + 1,
            callable_name,
            expected_type.get_id(Some(analyzer.interner)),
            candidate.get_id(Some(analyzer.interner))
        ),
    );

    CallableValidationOutcome::IssueEmitted
}

fn resolve_callable_from_concat_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    prefer_invalid_argument_for_undefined: bool,
    callee_context: CalleeContext,
) -> Option<TAtomic> {
    let Expression::Binary(binary) = expr else {
        return None;
    };

    if !matches!(binary.operator, BinaryOperator::StringConcat(_)) {
        return None;
    }

    let class_id = get_class_from_class_const_expr(analyzer, binary.lhs)?;
    let method_name = get_literal_string(binary.rhs)?
        .strip_prefix("::")?
        .to_string();

    resolve_method_callable(
        analyzer,
        class_id,
        &method_name,
        true,
        arg_pos,
        analysis_data,
        prefer_invalid_argument_for_undefined,
        callee_context,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_candidate_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    expected_callables: &[&TAtomic],
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    prefer_invalid_argument_for_undefined: bool,
    context: &BlockContext,
    callee_context: CalleeContext,
) -> Option<TAtomic> {
    for atomic in &arg_type.types {
        match atomic {
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => return Some(atomic.clone()),
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(candidate) =
                    resolve_invokable_object_callable(analyzer, *name, type_params.as_deref())
                {
                    return Some(candidate);
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(candidate) = resolve_candidate_from_union(
                    analyzer,
                    as_type,
                    expected_callables,
                    arg_pos,
                    analysis_data,
                    prefer_invalid_argument_for_undefined,
                    context,
                    callee_context,
                ) {
                    return Some(candidate);
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for nested_atomic in types {
                    if let Some(candidate) = resolve_candidate_from_union(
                        analyzer,
                        &TUnion::new(nested_atomic.clone()),
                        expected_callables,
                        arg_pos,
                        analysis_data,
                        prefer_invalid_argument_for_undefined,
                        context,
                        callee_context,
                    ) {
                        return Some(candidate);
                    }
                }
            }
            TAtomic::TLiteralString { value } => {
                if let Some(candidate) = resolve_string_callable(
                    analyzer,
                    value,
                    expected_callables,
                    arg_pos,
                    analysis_data,
                    prefer_invalid_argument_for_undefined,
                    context,
                    callee_context,
                ) {
                    return Some(candidate);
                }
            }
            // A `[$class_or_obj, "method"]` callable is a known-shape array
            // (old TKeyedArray); a generic array is not resolved here.
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                if let Some(candidate) = resolve_array_callable(
                    analyzer,
                    known_values,
                    arg_pos,
                    analysis_data,
                    context,
                    callee_context,
                ) {
                    return Some(candidate);
                }
            }
            _ => {}
        }
    }

    None
}

fn resolve_array_callable(
    analyzer: &StatementsAnalyzer<'_>,
    known_values: &rustc_hash::FxHashMap<ArrayKey, (bool, TUnion)>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    callee_context: CalleeContext,
) -> Option<TAtomic> {
    if known_values.len() != 2 {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable must have exactly two elements",
        );
        return None;
    }

    let Some((_, first)) = known_values.get(&ArrayKey::Int(0)) else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable first element must be at offset 0",
        );
        return None;
    };

    let Some((_, second)) = known_values.get(&ArrayKey::Int(1)) else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable second element must be at offset 1",
        );
        return None;
    };

    // PHP 8.2 deprecated ["self"/"parent"/"static", "method"] callables.
    if let Some(TAtomic::TLiteralString { value }) = first.get_single() {
        emit_deprecated_callable_magic_class(analyzer, value, arg_pos, analysis_data);
    }

    // A non-literal STRING method name can't be resolved but may well be
    // callable — Psalm records a potential dynamic reference and stays
    // silent. A non-string element is never a valid method name.
    let Some(method_name) = get_literal_string_from_union(second) else {
        let second_is_stringish = second.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TString
                    | TAtomic::TNonEmptyString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TLowercaseString
                    | TAtomic::TNonEmptyLowercaseString
                    | TAtomic::TTruthyString
                    | TAtomic::TCallableString
                    | TAtomic::TNumericString
                    | TAtomic::TNonEmptyNumericString
                    | TAtomic::TClassString { .. }
                    | TAtomic::TLiteralClassString { .. }
                    | TAtomic::TMixed
                    | TAtomic::TNonEmptyMixed
            )
        });
        if !second_is_stringish {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Array callable method name must be a literal string",
            );
        }
        return None;
    };

    if method_name.starts_with("::") || method_name.is_empty() {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Array callable method name is malformed",
        );
        return None;
    }

    // ["parent", "m"] in a parentless class (Psalm's ParentNotFound).
    if let Some(TAtomic::TLiteralString { value }) = first.get_single()
        && value.eq_ignore_ascii_case("parent")
        && get_class_id_from_union(analyzer, first, context).is_none()
    {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::ParentNotFound,
            "Cannot resolve parent reference in a class without a parent",
        );
        return None;
    }

    if let Some(class_id) = get_class_id_from_union(analyzer, first, context) {
        let resolved = resolve_method_callable(
            analyzer,
            class_id,
            method_name,
            true,
            arg_pos,
            analysis_data,
            true,
            callee_context,
        );

        // A `[$class, "method"]` callable through a templated class
        // reference (`class-string<T1>` / `T1`) late-binds `static` returns
        // to the template — `[$class, "fromString"]` with
        // `class-string<T1 of Id>` yields `callable(...): T1`, so
        // `array_map` infers `list<T1>` (Psalm's $static_type carries the
        // template).
        if let Some(mut callable) = resolved {
            if let Some(static_binding) = template_static_binding_from_union(first)
                && let TAtomic::TCallable {
                    return_type: Some(return_type),
                    ..
                } = &mut callable
            {
                let parent_class_id = analyzer
                    .codebase
                    .get_class(class_id)
                    .and_then(|class_info| class_info.parent_class);
                **return_type =
                    crate::type_expander::localize_special_class_type_union_with_static_object(
                        analyzer.codebase,
                        analyzer.interner,
                        return_type,
                        class_id,
                        static_binding,
                        parent_class_id,
                    );
            }
            return Some(callable);
        }
        return None;
    }

    if let Some(object_class_id) = get_object_class_id_from_union(first) {
        let resolved = resolve_method_callable(
            analyzer,
            object_class_id,
            method_name,
            false,
            arg_pos,
            analysis_data,
            true,
            callee_context,
        );

        // `[$obj, "method"]` on a `T1`-typed receiver late-binds `static`
        // returns to the template, mirroring the class-string branch above.
        if let Some(mut callable) = resolved {
            if let Some(static_binding) = template_static_binding_from_union(first)
                && let TAtomic::TCallable {
                    return_type: Some(return_type),
                    ..
                } = &mut callable
            {
                let parent_class_id = analyzer
                    .codebase
                    .get_class(object_class_id)
                    .and_then(|class_info| class_info.parent_class);
                **return_type =
                    crate::type_expander::localize_special_class_type_union_with_static_object(
                        analyzer.codebase,
                        analyzer.interner,
                        return_type,
                        object_class_id,
                        static_binding,
                        parent_class_id,
                    );
            }
            return Some(callable);
        }
        return None;
    }

    // A generic class-string / plain object first element can't be resolved
    // to a concrete method but IS a valid callable shape (Psalm's
    // getCallableFromAtomic returns a signatureless callable).
    if first.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TClassString { .. }
                | TAtomic::TObject
                | TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TTemplateParam { .. }
        )
    }) {
        return Some(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: None,
        });
    }

    add_issue(
        analyzer,
        analysis_data,
        arg_pos,
        IssueKind::InvalidArgument,
        "Array callable first element must be a class string or object",
    );

    None
}

/// An invokable object's `__invoke` as a callable atomic, with the declaring
/// class's templates localized through the instance's type params (Psalm's
/// `CallableTypeComparator::getCallableFromAtomic`). The method's own
/// templates stay intact for the high-order solve against the expected
/// callable.
fn resolve_invokable_object_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    type_params: Option<&[TUnion]>,
) -> Option<TAtomic> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let invoke_method = class_info.methods.get(&StrId::INVOKE)?;
    if invoke_method.visibility != Visibility::Public {
        return None;
    }
    let callable = functionlike_to_callable_atomic(invoke_method);

    let mut template_result =
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            type_params,
        );
    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut template_result,
        class_info,
    );
    let localized =
        crate::template::inferred_type_replacer::replace(&TUnion::new(callable), &template_result);
    localized.get_single().cloned()
}

/// PHP 8.2 deprecated "self"/"parent"/"static" in callable strings and
/// callable-array class elements (Psalm: DeprecatedConstant).
fn emit_deprecated_callable_magic_class(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let normalized = class_name.trim_start_matches('\\');
    if analyzer.config.php_version_id() >= 80200
        && matches!(
            normalized.to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        )
    {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::DeprecatedConstant,
            format!("Use of \"{}\" in callables is deprecated", normalized),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_string_callable(
    analyzer: &StatementsAnalyzer<'_>,
    raw: &str,
    expected_callables: &[&TAtomic],
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_invalid_argument: bool,
    context: &BlockContext,
    callee_context: CalleeContext,
) -> Option<TAtomic> {
    let cleaned = raw.strip_prefix('\\').unwrap_or(raw);

    if cleaned.is_empty() {
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Callable string cannot be empty",
            );
        }
        return None;
    }

    if let Some((class_name, method_name)) = cleaned.split_once("::") {
        emit_deprecated_callable_magic_class(analyzer, class_name, arg_pos, analysis_data);
        let Some(class_id) = resolve_class_id(analyzer, class_name, context) else {
            if class_name.eq_ignore_ascii_case("parent") {
                // Psalm's ParentNotFound for "parent::m" callables in
                // parentless classes.
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::ParentNotFound,
                    "Cannot resolve parent reference in a class without a parent",
                );
            } else if emit_invalid_argument {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::InvalidArgument,
                    "Invalid callable class reference",
                );
            } else {
                add_issue(
                    analyzer,
                    analysis_data,
                    arg_pos,
                    IssueKind::UndefinedClass,
                    crate::class_casing::undefined_class_message(analyzer, &class_name),
                );
            }
            return None;
        };

        return resolve_method_callable(
            analyzer,
            class_id,
            method_name,
            true,
            arg_pos,
            analysis_data,
            emit_invalid_argument,
            callee_context,
        );
    }

    if let Some(function_info) = resolve_callable_function(analyzer, cleaned, context) {
        return Some(functionlike_to_callable(function_info));
    }

    if has_local_function_declaration(analyzer, cleaned) {
        if let Some(expected_callable) = expected_callables.first() {
            return Some((*expected_callable).clone());
        }

        return Some(TAtomic::TCallable {
            params: None,
            return_type: None,
            is_pure: Some(false),
        });
    }

    // A function_exists() guard vouches for the callable (Psalm's phantom
    // functions): treat it as a signatureless callable.
    {
        let guard_key = pzoom_code_info::VarName::new(&format!(
            "@function_exists({})",
            cleaned.trim_start_matches('\\').to_ascii_lowercase()
        ));
        if context
            .locals
            .get(&guard_key)
            .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
        {
            return Some(TAtomic::TCallable {
                params: None,
                return_type: None,
                is_pure: None,
            });
        }
    }

    if emit_invalid_argument {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            "Invalid callable function reference",
        );
    } else {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::UndefinedFunction,
            crate::class_casing::undefined_function_message(analyzer, &cleaned, None),
        );
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn resolve_method_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: pzoom_str::StrId,
    method_name: &str,
    class_style: bool,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    emit_invalid_argument: bool,
    callee_context: CalleeContext,
) -> Option<TAtomic> {
    let Some(class_info) = analyzer.codebase.get_class(class_id) else {
        let class_name = analyzer.interner.lookup(class_id);
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Invalid callable class reference",
            );
        } else {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedClass,
                crate::class_casing::undefined_class_message(analyzer, &class_name),
            );
        }
        return None;
    };

    if analyzer.config.find_unused_code
        && let Some(method_info) = get_method_info(analyzer, class_info, method_name)
    {
        // A callable referencing a method counts as a call for
        // find_unused_code (Psalm's methodExists records the reference).
        let method_lc = analyzer.interner.intern(&method_name.to_lowercase());
        analysis_data
            .referenced_class_members
            .insert((class_id, method_lc));
        analysis_data
            .symbol_references
            .add_reference_to_class_member(
                callee_context.referencing_id.as_ref(),
                callee_context.referencing_class,
                (class_id, method_lc),
                false,
            );
        if let Some(declaring) = method_info.declaring_class {
            analysis_data
                .referenced_class_members
                .insert((declaring, method_lc));
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    callee_context.referencing_id.as_ref(),
                    callee_context.referencing_class,
                    (declaring, method_lc),
                    false,
                );
        }
        analysis_data.referenced_classes.insert(class_id);
        analysis_data
            .method_returns_used
            .insert((class_id, method_lc));
        if let Some(declaring) = method_info.declaring_class {
            analysis_data
                .method_returns_used
                .insert((declaring, method_lc));
        }
    }

    // PHP method names are case-insensitive: "A::BARBAR" references barBar.
    let resolved_method_info = get_method_info(analyzer, class_info, method_name).or_else(|| {
        class_info.methods.iter().find_map(|(existing_id, method)| {
            analyzer
                .interner
                .lookup(*existing_id)
                .eq_ignore_ascii_case(method_name)
                .then(|| &**method)
        })
    });
    let Some(method_info) = resolved_method_info else {
        let class_name = analyzer.interner.lookup(class_id);
        if emit_invalid_argument {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                "Invalid callable method reference",
            );
        } else {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::UndefinedMethod,
                crate::class_casing::undefined_method_message(analyzer, &class_name, method_name),
            );
        }
        return None;
    };

    // Psalm's verifyCallableInContext: a callable referencing the CURRENT
    // class ($context->self) skips the checks entirely when the receiving
    // function is a method of the same class or one of PHP's native
    // callback-takers (call_user_func, usort, ...). Anything else requires a
    // public (and, for class-style references, static) method.
    // Visibility scoping: a private method is in scope only in its declaring
    // class; a protected one anywhere in the hierarchy ([$this, "m"]).
    let references_own_class = match method_info.visibility {
        Visibility::Private => {
            analyzer.get_declaring_class().is_some()
                && analyzer.get_declaring_class() == method_info.declaring_class
        }
        _ => {
            analyzer.get_declaring_class() == Some(class_id)
                || analyzer.get_declaring_class().is_some_and(|declaring| {
                    crate::type_comparator::object_type_comparator::is_class_subtype_of(
                        declaring,
                        class_id,
                        analyzer.codebase,
                    ) || crate::type_comparator::object_type_comparator::is_class_subtype_of(
                        class_id,
                        declaring,
                        analyzer.codebase,
                    )
                })
        }
    };
    let callee_exempt = references_own_class
        && (callee_context.class == Some(class_id)
            || callee_context.is_native_callback
            // Closure::fromCallable binds the calling scope like the native
            // callback-takers (Psalm's verifyCallableInContext).
            || callee_context
                .class
                .is_some_and(|callee_class| callee_class == pzoom_str::StrId::CLOSURE));

    if callee_exempt {
        // no visibility/staticness checks
    } else if class_style {
        if !method_info.is_static || method_info.visibility != Visibility::Public {
            add_issue(
                analyzer,
                analysis_data,
                arg_pos,
                IssueKind::InvalidArgument,
                format!(
                    "Callable {}::{} must reference a public static method",
                    analyzer.interner.lookup(class_id),
                    method_name
                ),
            );
            return None;
        }
    } else if method_info.visibility != Visibility::Public {
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InvalidArgument,
            format!(
                "Callable {}::{} must reference a public method",
                analyzer.interner.lookup(class_id),
                method_name
            ),
        );
        return None;
    }

    Some(functionlike_to_callable(method_info))
}

/// The template param a callable's class element names, if any —
/// `class-string<T1>` or a `T1`-typed object reference. Used to late-bind
/// `static` in the resolved callable's return type.
fn template_static_binding_from_union(class_union: &TUnion) -> Option<TAtomic> {
    let atomic = class_union.get_single()?;
    match atomic {
        template @ TAtomic::TTemplateParam { .. } => Some(template.clone()),
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => Some(match as_type.as_ref() {
            template @ TAtomic::TTemplateParam { .. } => template.clone(),
            bound => TAtomic::TTemplateParam {
                name: *name,
                defining_entity: *defining_entity,
                as_type: Box::new(TUnion::new(bound.clone())),
            },
        }),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => match as_type.as_ref() {
            template @ TAtomic::TTemplateParam { .. } => Some(template.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn functionlike_to_callable(function_info: &FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TCallable {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure || function_info.is_mutation_free),
    }
}

fn is_optional_param_gap_mismatch(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    if candidate_params.len() >= expected_params.len() {
        return false;
    }

    expected_params[candidate_params.len()..]
        .iter()
        .all(|p| p.is_optional || p.is_variadic)
}

/// Whether the candidate requires a parameter at a position the expected
/// callable marks optional (the callable contract allows omitting it).
fn has_required_param_for_optional_expected(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    candidate_params
        .iter()
        .enumerate()
        .any(|(i, candidate_param)| {
            !candidate_param.is_optional
                && !candidate_param.is_variadic
                && expected_params
                    .get(i)
                    .is_some_and(|expected_param| expected_param.is_optional)
        })
}

pub(crate) fn get_callable_params(atomic: &TAtomic) -> Option<&Vec<FunctionLikeParameter>> {
    match atomic {
        TAtomic::TCallable { params, .. } | TAtomic::TClosure { params, .. } => params.as_ref(),
        _ => None,
    }
}

fn has_scalar_callable_mismatch(
    codebase: &pzoom_code_info::CodebaseInfo,
    candidate: &TAtomic,
    expected: &TAtomic,
) -> bool {
    let candidate_params = get_callable_params(candidate);
    let expected_params = get_callable_params(expected);

    if let (Some(candidate_params), Some(expected_params)) = (candidate_params, expected_params) {
        let shared = candidate_params.len().min(expected_params.len());
        for idx in 0..shared {
            let candidate_param = &candidate_params[idx].param_type;
            let expected_param = &expected_params[idx].param_type;
            if is_scalar_only_union(candidate_param)
                && is_scalar_only_union(expected_param)
                && candidate_param.get_id(None) != expected_param.get_id(None)
            {
                return true;
            }
        }
    }

    let candidate_return = match candidate {
        TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
            return_type
        }
        _ => return false,
    };

    let expected_return = match expected {
        TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => {
            return_type
        }
        _ => return false,
    };

    if let (Some(candidate_return), Some(expected_return)) = (candidate_return, expected_return) {
        // Return types are covariant: a candidate returning a subtype of the
        // expected return (e.g. `string` where `int|string` is expected) is fine,
        // not a scalar mismatch. Only flag a genuine incompatibility.
        if is_scalar_only_union(candidate_return)
            && is_scalar_only_union(expected_return)
            && candidate_return.get_id(None) != expected_return.get_id(None)
            && !union_type_comparator::is_contained_by(
                codebase,
                candidate_return,
                expected_return,
                false,
                false,
                &mut TypeComparisonResult::new(),
            )
        {
            return true;
        }
    }

    false
}

fn is_scalar_only_union(union: &TUnion) -> bool {
    !union.types.is_empty() && union.types.iter().all(is_scalar_atomic)
}

pub(crate) fn is_scalar_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TInt
            | TAtomic::TFloat
            | TAtomic::TString
            | TAtomic::TBool
            | TAtomic::TTrue
            | TAtomic::TFalse
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TLiteralString { .. }
    )
}

pub(crate) fn union_is_string_like(union: &TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    matches!(
        union.get_single(),
        Some(
            TAtomic::TString
                | TAtomic::TNonEmptyString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
        )
    )
}

pub(crate) fn callable_allows_unknown_runtime_class(callable_name: &str) -> bool {
    callable_name.eq_ignore_ascii_case("class_exists")
        || callable_name.eq_ignore_ascii_case("interface_exists")
        || callable_name.eq_ignore_ascii_case("trait_exists")
        || callable_name.eq_ignore_ascii_case("enum_exists")
        || callable_name.eq_ignore_ascii_case("is_a")
        || callable_name.eq_ignore_ascii_case("is_subclass_of")
}

pub(crate) fn union_is_array_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TArray { .. } => true,
            TAtomic::TTemplateParam { as_type, .. } => union_is_array_like(as_type),
            _ => false,
        })
}

pub(crate) fn union_is_list_like(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TArray { is_list, .. } => *is_list,
            TAtomic::TTemplateParam { as_type, .. } => union_is_list_like(as_type),
            _ => false,
        })
}

pub(crate) fn is_untyped_callable_union(union: &TUnion) -> bool {
    if !union.is_single() {
        return false;
    }

    match union.get_single() {
        Some(
            TAtomic::TCallable {
                params: None,
                return_type: None,
                ..
            }
            | TAtomic::TClosure {
                params: None,
                return_type: None,
                ..
            },
        ) => true,
        Some(TAtomic::TNamedObject { name, .. }) => *name == StrId::CLOSURE,
        _ => false,
    }
}

pub(crate) fn has_typed_callable_signature_union(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| match atomic {
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => params.is_some() || return_type.is_some(),
        _ => false,
    })
}

pub(crate) fn get_expected_callable_atomics(union: &TUnion) -> Vec<&TAtomic> {
    union
        .types
        .iter()
        .filter(|t| matches!(t, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
        .collect()
}

fn determine_callable_mismatch_issue_kind(
    analyzer: &StatementsAnalyzer<'_>,
    candidate: &TAtomic,
    expected_callable: &TAtomic,
    comparison_result: &TypeComparisonResult,
) -> IssueKind {
    if has_non_overlapping_callable_arity(candidate, expected_callable) {
        return IssueKind::InvalidArgument;
    }

    if is_optional_param_gap_mismatch(candidate, expected_callable) {
        return IssueKind::PossiblyInvalidArgument;
    }

    if has_scalar_callable_mismatch(analyzer.codebase, candidate, expected_callable) {
        return IssueKind::InvalidScalarArgument;
    }

    if comparison_result.type_coerced_from_mixed.unwrap_or(false) {
        return IssueKind::MixedArgumentTypeCoercion;
    }

    // A coercion that isn't from mixed (e.g. a contravariant callable parameter
    // that accepts only a subtype of what the container parameter requires) is a
    // soft `ArgumentTypeCoercion`, matching Psalm's `ArgumentAnalyzer` which emits
    // it whenever `type_coerced` is set without `type_coerced_from_mixed`.
    if comparison_result.type_coerced.unwrap_or(false) {
        return IssueKind::ArgumentTypeCoercion;
    }

    let candidate_union = TUnion::new(candidate.clone());
    let expected_union = TUnion::new(expected_callable.clone());

    if union_type_comparator::can_be_contained_by(
        analyzer.codebase,
        &candidate_union,
        &expected_union,
    ) {
        IssueKind::PossiblyInvalidArgument
    } else {
        IssueKind::InvalidArgument
    }
}

fn has_non_overlapping_callable_arity(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    let candidate_required = candidate_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();
    let expected_required = expected_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    let candidate_max = if candidate_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(candidate_params.len())
    };
    let expected_max = if expected_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(expected_params.len())
    };

    if let Some(expected_max) = expected_max
        && candidate_required > expected_max
    {
        return true;
    }

    if let Some(candidate_max) = candidate_max
        && candidate_max < expected_required
    {
        return true;
    }

    false
}

fn select_preferred_callable_issue_kind(
    current: Option<IssueKind>,
    incoming: IssueKind,
) -> IssueKind {
    let incoming_priority = callable_issue_priority(incoming);

    match current {
        None => incoming,
        Some(existing) => {
            if incoming_priority < callable_issue_priority(existing) {
                incoming
            } else {
                existing
            }
        }
    }
}

fn callable_issue_priority(kind: IssueKind) -> u8 {
    match kind {
        IssueKind::PossiblyInvalidArgument => 0,
        IssueKind::MixedArgumentTypeCoercion => 1,
        IssueKind::InvalidScalarArgument => 2,
        IssueKind::InvalidArgument => 3,
        _ => 4,
    }
}

fn get_literal_string<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Literal(Literal::String(s)) => s.value,
        _ => None,
    }
}

fn get_literal_string_from_union(union: &TUnion) -> Option<&str> {
    if !union.is_single() {
        return None;
    }

    match union.get_single()? {
        TAtomic::TLiteralString { value } => Some(value.as_str()),
        _ => None,
    }
}

fn get_class_from_class_const_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<pzoom_str::StrId> {
    let Expression::Access(access) = expr else {
        return None;
    };

    let mago_syntax::ast::ast::access::Access::ClassConstant(const_access) = access else {
        return None;
    };

    let mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector::Identifier(id) =
        &const_access.constant
    else {
        return None;
    };

    if !id.value.eq_ignore_ascii_case("class") {
        return None;
    }

    resolve_class_id_from_expr(analyzer, const_access.class)
}

fn resolve_class_id_from_expr(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> Option<pzoom_str::StrId> {
    match expr {
        Expression::Identifier(id) => analyzer
            .get_resolved_name(id.span().start.offset)
            .or_else(|| Some(analyzer.interner.intern(id.value()))),
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer
            .get_declaring_class()
            .and_then(|class_id| analyzer.codebase.get_class(class_id))
            .and_then(|class_info| class_info.parent_class),
        Expression::MagicConstant(mc) => {
            if matches!(
                mc,
                mago_syntax::ast::ast::magic_constant::MagicConstant::Class(_)
            ) {
                analyzer.get_declaring_class()
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn resolve_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    let normalized = class_name.strip_prefix('\\').unwrap_or(class_name);
    if normalized.is_empty() {
        return None;
    }

    match normalized.to_ascii_lowercase().as_str() {
        "self" | "static" => analyzer.get_declaring_class(),
        "parent" => analyzer
            .get_declaring_class()
            .and_then(|class_id| analyzer.codebase.get_class(class_id))
            .and_then(|class_info| class_info.parent_class),
        _ => {
            let class_id = analyzer.interner.intern(normalized);
            if analyzer.codebase.get_class(class_id).is_some() {
                return Some(class_id);
            }

            if !normalized.contains('\\') {
                if let Some(namespace_id) = context.namespace {
                    let namespace = analyzer.interner.lookup(namespace_id);
                    let namespaced_id = analyzer
                        .interner
                        .intern(&format!("{}\\{}", namespace, normalized));
                    if analyzer.codebase.get_class(namespaced_id).is_some() {
                        return Some(namespaced_id);
                    }
                }
            }

            None
        }
    }
}

fn get_class_id_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    let mut class_id = None;

    for atomic in &union.types {
        let atomic_class_id = get_class_id_from_atomic(analyzer, atomic, context)?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_object_class_id_from_union(union: &TUnion) -> Option<pzoom_str::StrId> {
    let mut class_id = None;

    for atomic in &union.types {
        let atomic_class_id = get_object_class_id_from_atomic(atomic)?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_class_id_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    context: &BlockContext,
) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TLiteralClassString { name } => resolve_class_id(analyzer, name, context),
        TAtomic::TLiteralString { value } => resolve_class_id(analyzer, value, context),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => get_named_class_id_from_atomic(as_type).map(|class_id| {
            // `static::class` / `self::class` produce class-string<static>;
            // resolve the placeholder to the enclosing class (Psalm's
            // getFunctionIdsFromCallableArg resolves $this_class_name).
            if class_id == StrId::STATIC || class_id == StrId::SELF {
                analyzer
                    .get_declaring_class()
                    .or(context.self_class)
                    .unwrap_or(class_id)
            } else {
                class_id
            }
        }),
        TAtomic::TTemplateParam { as_type, .. } => {
            get_class_id_from_union(analyzer, as_type, context)
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn get_object_class_id_from_atomic(atomic: &TAtomic) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut class_id = None;

            for nested in &as_type.types {
                let nested_id = get_object_class_id_from_atomic(nested)?;

                if let Some(existing) = class_id {
                    if existing != nested_id {
                        return None;
                    }
                } else {
                    class_id = Some(nested_id);
                }
            }

            class_id
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn get_named_class_id_from_atomic(atomic: &TAtomic) -> Option<pzoom_str::StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            if as_type.is_single() {
                get_named_class_id_from_atomic(as_type.get_single()?)
            } else {
                None
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_id_from_atomic(as_type),
        _ => None,
    }
}

fn resolve_callable_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    _context: &BlockContext,
) -> Option<&'a FunctionLikeInfo> {
    let name = name.strip_prefix('\\').unwrap_or(name);
    let function_id = analyzer.interner.intern(name);
    if let Some(function_info) = analyzer.codebase.get_function(function_id) {
        return Some(function_info);
    }

    // PHP function names are case-insensitive: "fooBar" references foobar.
    if let Some(cased_id) = analyzer
        .codebase
        .cased_functionlike_for(analyzer.interner, function_id)
        && let Some(function_info) = analyzer.codebase.get_function(cased_id)
    {
        return Some(function_info);
    }
    None
}

fn has_local_function_declaration(analyzer: &StatementsAnalyzer<'_>, target_name: &str) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let start = function_info.start_offset as usize;
    let end = function_info.end_offset as usize;
    if start >= end || end > analyzer.source.len() {
        return false;
    }

    let source_window = &analyzer.source[start..end];
    has_function_declaration_in_source(source_window, target_name)
}

fn has_function_declaration_in_source(source: &str, target_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;

    while i + 8 <= bytes.len() {
        if bytes[i..i + 8].eq_ignore_ascii_case(b"function") {
            // Ensure "function" token boundary.
            if i > 0 && is_ident_byte(bytes[i - 1]) {
                i += 1;
                continue;
            }
            if i + 8 < bytes.len() && is_ident_byte(bytes[i + 8]) {
                i += 1;
                continue;
            }

            let mut cursor = i + 8;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }

            if cursor < bytes.len() && bytes[cursor] == b'&' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
            }

            let name_start = cursor;
            while cursor < bytes.len() && is_ident_byte(bytes[cursor]) {
                cursor += 1;
            }

            if name_start == cursor {
                i += 1;
                continue;
            }

            let declared_name = &source[name_start..cursor];
            // PHP function names are case-insensitive.
            if declared_name.eq_ignore_ascii_case(target_name) {
                let mut after_name = cursor;
                while after_name < bytes.len() && bytes[after_name].is_ascii_whitespace() {
                    after_name += 1;
                }

                if after_name < bytes.len() && bytes[after_name] == b'(' {
                    return true;
                }
            }
        }

        i += 1;
    }

    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(crate) fn get_method_info<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a pzoom_code_info::ClassLikeInfo,
    method_name: &str,
) -> Option<&'a FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    class_info.methods.get(&method_id).map(|method| &**method)
}

pub(crate) fn find_undefined_class_string_literal_in_argument(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    param_type: &TUnion,
    context: &BlockContext,
) -> Option<String> {
    let expr = expr.unparenthesized();

    if !matches!(expr, Expression::Array(_) | Expression::LegacyArray(_)) {
        let single_param_atomic = param_type.get_single()?;
        return find_undefined_class_string_literal_in_argument_for_atomic(
            analyzer,
            expr,
            single_param_atomic,
            context,
        );
    }

    for atomic in &param_type.types {
        if let Some(name) = find_undefined_class_string_literal_in_argument_for_atomic(
            analyzer, expr, atomic, context,
        ) {
            return Some(name);
        }
    }

    None
}

fn find_undefined_class_string_literal_in_argument_for_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    param_atomic: &TAtomic,
    context: &BlockContext,
) -> Option<String> {
    let expr = expr.unparenthesized();

    match param_atomic {
        TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TTemplateParamClass { .. } => {
            let literal = get_literal_string_value(expr)?;
            if classlike_exists_case_insensitive(analyzer, &literal, context) {
                None
            } else {
                Some(literal)
            }
        }
        // A generic array/list (old TArray / TNonEmptyArray / TList /
        // TNonEmptyList): the element type is the typed fallback value.
        TAtomic::TArray {
            known_values,
            params: Some(params),
            ..
        } if known_values.is_empty() => find_undefined_class_string_literal_in_array_argument(
            analyzer, expr, &params.1, context,
        ),
        // A shape (old TKeyedArray), or the sealed empty array `[]`
        // (params None).
        TAtomic::TArray {
            known_values,
            params,
            ..
        } => {
            let fallback_value_type = params.as_deref().map(|(_, value)| value);
            let all_properties_expect_class_string = !known_values.is_empty()
                && known_values
                    .values()
                    .all(|(_, value)| union_contains_class_string(value));
            let fallback_expects_class_string =
                fallback_value_type.is_some_and(|fallback| union_contains_class_string(fallback));

            if all_properties_expect_class_string || fallback_expects_class_string {
                let value_type = fallback_value_type
                    .or_else(|| known_values.values().next().map(|(_, value)| value))?;

                return find_undefined_class_string_literal_in_array_argument(
                    analyzer, expr, value_type, context,
                );
            }

            None
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            find_undefined_class_string_literal_in_argument(analyzer, expr, as_type, context)
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                if let Some(name) = find_undefined_class_string_literal_in_argument_for_atomic(
                    analyzer, expr, nested, context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        _ => None,
    }
}

fn find_undefined_class_string_literal_in_array_argument(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    value_param_type: &TUnion,
    context: &BlockContext,
) -> Option<String> {
    if !union_contains_class_string(value_param_type) {
        return None;
    }

    let expr = expr.unparenthesized();

    match expr {
        Expression::Array(array) => {
            for element in array.elements.iter() {
                let value_expr = match element {
                    ArrayElement::KeyValue(kv) => kv.value,
                    ArrayElement::Value(value) => value.value,
                    ArrayElement::Variadic(_) | ArrayElement::Missing(_) => continue,
                };

                if let Some(name) = find_undefined_class_string_literal_in_argument(
                    analyzer,
                    value_expr,
                    value_param_type,
                    context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        Expression::LegacyArray(array) => {
            for element in array.elements.iter() {
                let value_expr = match element {
                    ArrayElement::KeyValue(kv) => kv.value,
                    ArrayElement::Value(value) => value.value,
                    ArrayElement::Variadic(_) | ArrayElement::Missing(_) => continue,
                };

                if let Some(name) = find_undefined_class_string_literal_in_argument(
                    analyzer,
                    value_expr,
                    value_param_type,
                    context,
                ) {
                    return Some(name);
                }
            }

            None
        }
        _ => None,
    }
}

fn get_literal_string_value(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::String(string_literal)) => {
            string_literal.value.map(ToString::to_string)
        }
        Expression::Binary(binary)
            if matches!(binary.operator, BinaryOperator::StringConcat(_)) =>
        {
            let lhs = get_literal_string_value(binary.lhs)?;
            let rhs = get_literal_string_value(binary.rhs)?;
            Some(format!("{}{}", lhs, rhs))
        }
        _ => None,
    }
}

fn classlike_exists_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if normalized.is_empty() {
        return false;
    }

    if resolve_class_id(analyzer, normalized, context).is_some() {
        return true;
    }

    let normalized_id = analyzer.interner.intern(normalized);
    if analyzer.codebase.get_class(normalized_id).is_some() {
        return true;
    }

    if !normalized.contains('\\')
        && let Some(namespace_id) = context.namespace
    {
        let namespace = analyzer.interner.lookup(namespace_id);
        let namespaced_candidate = format!("{}\\{}", namespace, normalized);
        let namespaced_id = analyzer.interner.intern(&namespaced_candidate);
        return analyzer.codebase.get_class(namespaced_id).is_some();
    }

    false
}

pub(crate) fn is_valid_by_ref_arg(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    context: &BlockContext,
) -> bool {
    let expr = arg.value().unparenthesized();
    if matches!(
        expr,
        Expression::Variable(_)
            | Expression::ArrayAccess(_)
            | Expression::Access(_)
            | Expression::Assignment(_)
    ) {
        return true;
    }

    let Expression::Call(call) = expr else {
        return false;
    };

    call_returns_by_ref(analyzer, call, context)
}

fn call_returns_by_ref(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    context: &BlockContext,
) -> bool {
    match call {
        Call::Function(function_call) => {
            let Expression::Identifier(function_id) = function_call.function.unparenthesized()
            else {
                return false;
            };

            let resolved_id = analyzer
                .get_resolved_name(function_id.span().start.offset)
                .unwrap_or_else(|| analyzer.interner.intern(function_id.value()));

            analyzer
                .codebase
                .get_function(resolved_id)
                .is_some_and(|function_info| function_info.returns_by_ref)
        }
        Call::Method(method_call) => {
            let ClassLikeMemberSelector::Identifier(method_id) = &method_call.method else {
                return false;
            };

            let method_name_id = analyzer.interner.intern(method_id.value);
            let Expression::Variable(Variable::Direct(direct)) =
                method_call.object.unparenthesized()
            else {
                return false;
            };

            let var_id = VarName::new(direct.name);
            let Some(object_type) = context.get_var_type(&var_id) else {
                return false;
            };

            object_type.types.iter().any(|atomic| {
                matches!(atomic, TAtomic::TNamedObject { name, .. } if analyzer
                    .codebase
                    .get_class(*name)
                    .and_then(|class_info| class_info.methods.get(&method_name_id))
                    .is_some_and(|method_info| method_info.returns_by_ref))
            })
        }
        _ => false,
    }
}

pub(crate) fn check_by_ref_property_mutability(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Expression::Access(access) = arg.value() else {
        return;
    };

    let Access::Property(property_access) = access else {
        return;
    };

    let ClassLikeMemberSelector::Identifier(id) = &property_access.property else {
        return;
    };

    let object_span = property_access.object.span();
    let Some(object_type) = analysis_data
        .expr_types
        .get(&(object_span.start.offset, object_span.end.offset))
        .cloned()
    else {
        return;
    };

    let property_id = analyzer.interner.intern(id.value);

    // Passing a property by reference is a write, so Psalm routes it through the
    // same readonly check as an assignment. Unlike a direct assignment, taking a
    // reference defeats the receiver's reference-freedom, so the pure-compatible
    // (fresh `new`/`clone`) exemption never applies here — every restricted
    // by-ref pass is reported. The one place Psalm does not reach the check is a
    // method of a class that does not own the appearing property class (e.g.
    // `array_shift($other->items)` where `$other` is some other immutable
    // class); a free-function / global scope, or a method that owns the class,
    // is policed.
    let in_class_method = analyzer
        .function_info
        .is_some_and(|info| info.declaring_class.is_some());

    for atomic in &object_type.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };

        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };

        let Some(prop_info) = class_info.properties.get(&property_id) else {
            continue;
        };

        // In a trait body `$this` is generic, so only a property the trait
        // *itself* declares is resolved against the readonly check — a using-class
        // property's readonly-ness (native `@psalm-readonly` or via the using
        // class's `@psalm-immutable`) isn't known to Psalm's open trait receiver,
        // and the same trait may be used by a mutable class too (e.g. an immutable
        // `Union` and a `MutableUnion`).
        let is_readonly = if analysis_data.in_trait_body {
            let prop_from_trait = analyzer
                .codebase
                .get_class(prop_info.declaring_class)
                .is_some_and(|info| {
                    info.kind == pzoom_code_info::class_like_info::ClassLikeKind::Trait
                });
            prop_from_trait && prop_info.is_readonly
        } else {
            prop_info.is_readonly || class_info.is_immutable
        };
        if !is_readonly {
            continue;
        }

        let owns_class =
            crate::expr::fetch::atomic_property_fetch_analyzer::calling_context_owns_class(
                analyzer, *name,
            );
        if in_class_method && !owns_class {
            continue;
        }

        // The property-assignment readonly check permits writes from the owning
        // class's special init methods (constructor / unserialize / __clone),
        // and for properties that allow private mutation.
        if owns_class
            && (crate::expr::assignment::instance_property_assignment_analyzer::is_special_write_method(analyzer)
                || prop_info.readonly_allow_private_mutation)
        {
            break;
        }

        let class_name = analyzer.interner.lookup(*name);
        add_issue(
            analyzer,
            analysis_data,
            arg_pos,
            IssueKind::InaccessibleProperty,
            format!("{}::${} is marked readonly", class_name, id.value),
        );
        break;
    }
}

pub(crate) fn add_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
    kind: IssueKind,
    message: impl Into<String>,
) {
    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

pub(crate) fn analyze_arguments_with_callable_context(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: Option<pzoom_str::StrId>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    template_defaults: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let mut next_positional = 0usize;
    for arg in args {
        if is_closure_like_argument(arg) {
            if matches!(arg, Argument::Positional(_)) {
                next_positional += 1;
            }
            continue;
        }

        // Psalm's handleByRefFunctionArg skips read-analysis of a by-ref
        // argument whose var path isn't in scope; flag it so the array fetch
        // doesn't report an empty-array read the call is about to write.
        let param_is_by_ref = params
            .get(next_positional)
            .or_else(|| params.last().filter(|param| param.is_variadic))
            .is_some_and(|param| param.by_ref);
        if matches!(arg, Argument::Positional(_)) {
            next_positional += 1;
        }

        let was_inside_by_ref_argument = context.inside_by_ref_argument;
        if param_is_by_ref {
            context.inside_by_ref_argument = true;
        }
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        context.inside_by_ref_argument = was_inside_by_ref_argument;
    }

    let mut template_result = template_defaults.clone();
    function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        params,
        &mut template_result,
        analysis_data,
        context,
    );

    // Psalm's ARRAY_FILTERLIKE handling (handleArrayMapFilterArrayArg +
    // handleClosureArg): the filter family's callback params are typed from
    // the already-analyzed array argument, even when the resolved signature
    // (e.g. a vendor polyfill's plain `callable`) carries no param types.
    let filter_callback_type = function_id.and_then(|function_id| {
        crate::expr::call::arguments_analyzer::infer_array_filter_callback_param_type_for_closure_inference(
            analyzer,
            function_id,
            args,
            arg_positions,
            analysis_data,
        )
    });

    for (idx, arg) in args.iter().enumerate() {
        let Some(closure_offset) = get_closure_like_argument_offset(arg) else {
            continue;
        };

        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };

        let expected_param_type = if idx == 1 && filter_callback_type.is_some() {
            filter_callback_type.clone()
        } else {
            param.and_then(|param| param.get_type()).map(|param_type| {
                if crate::template::template_result_is_empty(&template_result) {
                    param_type.clone()
                } else {
                    function_call_analyzer::replace_templates_in_union(param_type, &template_result)
                }
            })
        };

        // An entry seeded by an OUTER high-order pass (the enclosing call
        // solved this callee's templates against its own expectation) is
        // more informed than the local unsolved param type — keep it.
        let mut inserted = false;
        if let Some(expected_param_type) = expected_param_type {
            if union_has_callable(&expected_param_type)
                && !context
                    .expected_callable_arg_types
                    .contains_key(&closure_offset)
            {
                context
                    .expected_callable_arg_types
                    .insert(closure_offset, expected_param_type);
                inserted = true;
            }
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        if inserted {
            context.expected_callable_arg_types.remove(&closure_offset);
        }
    }
}

pub(crate) fn is_closure_like_argument(arg: &Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

pub(crate) fn get_closure_like_argument_offset(arg: &Argument<'_>) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

pub(crate) fn validate_direct_callable_invocation(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    pos: Pos,
) {
    check_callable_union_invocability(analyzer, callee_type, analysis_data, pos);

    let Some(callable_signature) = get_first_callable_signature(analyzer, callee_type) else {
        return;
    };
    let callable_params = &callable_signature.params;

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = callable_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments for callable, {} expected, {} provided",
                required_params,
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = callable_params
        .last()
        .is_some_and(|param| param.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > callable_params.len() {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments for callable, {} expected, {} provided",
                callable_params.len(),
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let callable_function_info = callable_signature.to_function_info();

    arguments_analyzer::check_arguments_match(
        analyzer,
        args,
        arg_positions,
        &callable_function_info,
        "callable",
        analysis_data,
        context,
        None,
        pos,
        false,
        true,
    );
}

pub(crate) struct DirectCallableSignature {
    params: Vec<pzoom_code_info::FunctionLikeParameter>,
    // TCallable signatures generally originate from docblock callable(...) annotations.
    // TClosure signatures come from concrete closure definitions and should retain
    // scalar mismatch diagnostics.
    from_callable_docblock: bool,
}

impl DirectCallableSignature {
    fn to_function_info(&self) -> pzoom_code_info::FunctionLikeInfo {
        let mut info = pzoom_code_info::FunctionLikeInfo::default();
        info.params = self
            .params
            .iter()
            .map(|param| {
                let mut param_info = ParamInfo::default();
                param_info.name = param.name.unwrap_or(StrId::EMPTY);
                param_info.param_type = Some(param.param_type.clone());
                param_info.signature_type = None;
                param_info.has_docblock_type = self.from_callable_docblock;
                param_info.is_optional = param.is_optional;
                param_info.is_variadic = param.is_variadic;
                param_info.by_ref = param.by_ref;
                param_info
            })
            .collect();
        info.is_variadic = self.params.last().is_some_and(|param| param.is_variadic);
        info
    }
}

/// The callee union's callable signature as a synthetic FunctionLikeInfo, so a
/// direct callable/invokable-object call can seed its closure arguments via
/// analyze_arguments_with_callable_context like a named call.
pub(crate) fn direct_callable_function_info(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
) -> Option<pzoom_code_info::FunctionLikeInfo> {
    get_first_callable_signature(analyzer, callee_type)
        .map(|signature| signature.to_function_info())
}

fn get_first_callable_signature(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
) -> Option<DirectCallableSignature> {
    for atomic in &callee_type.types {
        match atomic {
            TAtomic::TCallable {
                params: Some(params),
                ..
            } => {
                return Some(DirectCallableSignature {
                    params: params.clone(),
                    from_callable_docblock: true,
                });
            }
            TAtomic::TClosure {
                params: Some(params),
                ..
            } => {
                return Some(DirectCallableSignature {
                    params: params.clone(),
                    from_callable_docblock: false,
                });
            }
            // An invokable object's __invoke signature drives validation and
            // closure-argument seeding, as for Psalm's getCallableFromAtomic.
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(
                    TAtomic::TClosure {
                        params: Some(params),
                        ..
                    }
                    | TAtomic::TCallable {
                        params: Some(params),
                        ..
                    },
                ) = resolve_invokable_object_callable(analyzer, *name, type_params.as_deref())
                {
                    return Some(DirectCallableSignature {
                        params,
                        from_callable_docblock: false,
                    });
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(signature) = get_first_callable_signature(analyzer, as_type) {
                    return Some(signature);
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for nested_atomic in types {
                    if let Some(signature) =
                        get_first_callable_signature(analyzer, &TUnion::new(nested_atomic.clone()))
                    {
                        return Some(signature);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

pub(crate) fn widen_literal_scalar_union_for_callable(union: &TUnion) -> TUnion {
    let mut widened = Vec::new();

    for atomic in &union.types {
        let mapped = match atomic {
            TAtomic::TLiteralInt { .. } => TAtomic::TInt,
            TAtomic::TLiteralFloat { .. } => TAtomic::TFloat,
            TAtomic::TLiteralString { .. } => TAtomic::TString,
            _ => atomic.clone(),
        };

        if !widened.contains(&mapped) {
            widened.push(mapped);
        }
    }

    if widened.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(widened)
    }
}

pub(crate) fn infer_array_map_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    callback_type: &TUnion,
    callback_input_types: &[TUnion],
    context: &BlockContext,
) -> Option<TUnion> {
    let mut resolved_return_type = infer_callee_return_type(callback_type);

    for atomic in &callback_type.types {
        let callable_return = match atomic {
            TAtomic::TLiteralString { value } => {
                let is_fq = value.starts_with('\\');
                function_call_analyzer::resolve_function(analyzer, value, is_fq, None, context)
                    .and_then(|f| resolve_callable_return_type(analyzer, f, callback_input_types))
            }
            // A `[$class_or_obj, "method"]` callable (old TKeyedArray shape).
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                resolve_array_callable_method(analyzer, known_values, context)
                    .and_then(|m| resolve_callable_return_type(analyzer, m, callback_input_types))
                    .map(|callable_return| {
                        // `static` through a templated class element resolves
                        // to the template, not the concrete class (same
                        // late-binding as resolve_array_callable).
                        if let Some((_, class_union)) =
                            known_values.get(&pzoom_code_info::ArrayKey::Int(0))
                            && let Some(class_id) =
                                get_callable_class_from_union(analyzer, class_union, context)
                        {
                            let parent_class_id = analyzer
                                .codebase
                                .get_class(class_id)
                                .and_then(|class_info| class_info.parent_class);
                            // The raw declared return still carries `static`;
                            // re-localize from the method storage. A template
                            // element late-binds to the template; a literally
                            // named class binds firmly
                            // ([CriterionId::class, 'fromString'] returns
                            // CriterionId even when declared on Id).
                            if let Some(method_info) =
                                resolve_array_callable_method(analyzer, known_values, context)
                                && method_info.get_return_type().is_some_and(|return_type| {
                                    super::atomic_method_call_analyzer::
                                            union_contains_static_reference(return_type)
                                })
                                && let Some(raw_return) = method_info.get_return_type()
                            {
                                if let Some(static_binding) =
                                    template_static_binding_from_union(class_union)
                                {
                                    return crate::type_expander::
                                        localize_special_class_type_union_with_static_object(
                                            analyzer.codebase,
                                            analyzer.interner,
                                            raw_return,
                                            class_id,
                                            static_binding,
                                            parent_class_id,
                                        );
                                }
                                return crate::type_expander::
                                    localize_special_class_type_union_final(
                                        analyzer.codebase,
                                        analyzer.interner,
                                        raw_return,
                                        class_id,
                                        class_id,
                                        parent_class_id,
                                        true,
                                    );
                            }
                        }
                        // An OBJECT element ([\$container, 'get']) localizes
                        // the declaring class's templates through the
                        // instance's type params (Container<stdClass>::get
                        // returns stdClass).
                        if let Some((_, class_union)) =
                            known_values.get(&pzoom_code_info::ArrayKey::Int(0))
                            && let Some(TAtomic::TNamedObject {
                                name,
                                type_params: Some(object_params),
                                ..
                            }) = class_union.get_single()
                            && let Some(class_info) = analyzer.codebase.get_class(*name)
                        {
                            let mut template_result = function_call_analyzer::
                                infer_class_template_replacements_from_type_params(
                                    class_info,
                                    Some(object_params),
                                );
                            function_call_analyzer::
                                infer_class_template_replacements_from_extended_params(
                                    &mut template_result,
                                    class_info,
                                );
                            if !crate::template::template_result_is_empty(&template_result)
                                && let Some(method_info) =
                                    resolve_array_callable_method(analyzer, known_values, context)
                                && let Some(raw_return) = method_info.get_return_type()
                            {
                                return crate::template::inferred_type_replacer::replace(
                                    raw_return,
                                    &template_result,
                                );
                            }
                        }
                        callable_return
                    })
            }
            _ => None,
        };

        if let Some(callable_return) = callable_return {
            resolved_return_type = Some(if let Some(existing) = resolved_return_type {
                combine_union_types(&existing, &callable_return, false)
            } else {
                callable_return
            });
        }
    }

    resolved_return_type
}

pub(crate) fn infer_invokable_object_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut combined_return_type: Option<TUnion> = None;

    for atomic in &callee_type.types {
        let return_type = match atomic {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => infer_invokable_named_object_return_type(
                analyzer,
                *name,
                type_params.as_deref(),
                args,
                arg_positions,
                analysis_data,
                context,
            ),
            TAtomic::TTemplateParam { as_type, .. } => infer_invokable_object_return_type(
                analyzer,
                as_type,
                args,
                arg_positions,
                analysis_data,
                context,
            ),
            // A `callable():R` (or `Closure():R`) member of an intersection like
            // `object&callable():int` carries the invocation's return type
            // directly (Psalm reads the TCallable/TClosure return type).
            TAtomic::TCallable { return_type, .. } | TAtomic::TClosure { return_type, .. } => Some(
                return_type
                    .as_ref()
                    .map(|return_type| (**return_type).clone())
                    .unwrap_or_else(TUnion::mixed),
            ),
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_return: Option<TUnion> = None;
                for intersection_atomic in types {
                    let intersection_union = TUnion::new(intersection_atomic.clone());
                    let Some(this_return_type) = infer_invokable_object_return_type(
                        analyzer,
                        &intersection_union,
                        args,
                        arg_positions,
                        analysis_data,
                        context,
                    ) else {
                        continue;
                    };

                    intersection_return = Some(if let Some(existing) = intersection_return {
                        combine_union_types(&existing, &this_return_type, false)
                    } else {
                        this_return_type
                    });
                }

                intersection_return
            }
            _ => None,
        };

        if let Some(return_type) = return_type {
            combined_return_type = Some(if let Some(existing) = combined_return_type {
                combine_union_types(&existing, &return_type, false)
            } else {
                return_type
            });
        }
    }

    combined_return_type
}

fn infer_invokable_named_object_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    object_type_params: Option<&[TUnion]>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let invoke_method = class_info.methods.get(&StrId::INVOKE)?;
    invoke_method.get_return_type()?;

    let mut template_result = function_call_analyzer::get_class_template_defaults(class_info);
    for template_type in &invoke_method.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut template_result,
        class_info,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_result,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            object_type_params,
        ),
    );
    let mut arg_template_result = TemplateResult {
        template_types: template_result.template_types.clone(),
        ..Default::default()
    };
    function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        &invoke_method.params,
        &mut arg_template_result,
        analysis_data,
        context,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_result,
        arg_template_result,
    );

    let callable_name = format!("{}::__invoke", analyzer.interner.lookup(class_id));
    for (idx, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            continue;
        }

        let param = if idx < invoke_method.params.len() {
            Some(&invoke_method.params[idx])
        } else {
            invoke_method
                .params
                .last()
                .filter(|param| param.is_variadic)
        };
        let Some(param) = param else {
            continue;
        };

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned() else {
            continue;
        };

        let mut effective_param = param.clone();
        if let Some(param_type) = param.get_type() {
            effective_param.param_type = Some(function_call_analyzer::replace_templates_in_union(
                param_type,
                &template_result,
            ));
        }

        verify_type(
            analyzer,
            arg,
            arg_pos,
            &arg_type,
            &effective_param,
            idx,
            &callable_name,
            analysis_data,
            context,
            // Synthetic re-verification against the callable's signature; the
            // real call's argument dataflow is attached by the outer call.
            None,
        );
    }

    let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        invoke_method,
        &template_result,
        &FxHashMap::default(),
        args.len(),
    )
    .unwrap_or_else(TUnion::mixed);

    Some(localize_special_class_type_union_for_callable(
        analyzer.codebase,
        analyzer.interner,
        &resolved_return_type,
        class_id,
        class_info.parent_class,
    ))
}

/// Localize `self`/`static`/`parent` in a callable's return type to its defining
/// class. Unlike a method call, a callable reference captures the class at
/// definition, so `static` is *not* late-bound — equivalent to expanding with
/// `function_is_final: true`. Thin wrapper over the single [`type_expander`] mechanism.
fn localize_special_class_type_union_for_callable(
    codebase: &pzoom_code_info::CodebaseInfo,
    interner: &pzoom_str::Interner,
    union: &TUnion,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = union.clone();
    crate::type_expander::expand_union(
        codebase,
        interner,
        &mut localized,
        &crate::type_expander::TypeExpansionOptions {
            self_class: Some(self_class_id),
            static_class_type: crate::type_expander::StaticClassType::Name(self_class_id),
            parent_class: parent_class_id,
            function_is_final: true,
            evaluate_conditional_types: false,
        },
    );
    localized
}

fn resolve_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    arg_types: &[TUnion],
) -> Option<TUnion> {
    function_info.get_return_type()?;
    let mut template_result = function_call_analyzer::get_template_defaults(function_info);

    // Conditional returns name their subject param (`$string is class-string
    // ? ...`); bind each param to the corresponding element type so the
    // branch resolves against the real input rather than an injected
    // `nothing` (which is contained in everything and picks the if branch).
    let mut param_arg_types: FxHashMap<pzoom_str::StrId, TUnion> = FxHashMap::default();
    for (idx, param) in function_info.params.iter().enumerate() {
        let Some(arg_type) = arg_types.get(idx) else {
            continue;
        };
        param_arg_types.insert(param.name, arg_type.clone());

        let Some(param_type) = param.get_type() else {
            continue;
        };

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            arg_type,
            &mut template_result,
        );
    }

    let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
        analyzer,
        function_info,
        &template_result,
        &param_arg_types,
        arg_types.len(),
    )?;

    if let Some(self_class_id) = function_info.declaring_class {
        let parent_class_id = analyzer
            .codebase
            .get_class(self_class_id)
            .and_then(|class_info| class_info.parent_class);

        return Some(localize_special_class_type_union_for_callable(
            analyzer.codebase,
            analyzer.interner,
            &resolved_return_type,
            self_class_id,
            parent_class_id,
        ));
    }

    Some(resolved_return_type)
}

fn resolve_array_callable_method<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    known_values: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, (bool, TUnion)>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let (_, first) = known_values.get(&pzoom_code_info::ArrayKey::Int(0))?;
    let (_, second) = known_values.get(&pzoom_code_info::ArrayKey::Int(1))?;

    let method_name = get_literal_string_from_union(second)?;
    let class_id = get_callable_class_from_union(analyzer, first, context)?;

    let class_info = analyzer.codebase.get_class(class_id)?;
    get_method_info(analyzer, class_info, method_name)
}

fn get_callable_class_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    class_union: &TUnion,
    context: &BlockContext,
) -> Option<StrId> {
    let mut class_id = None;

    for atomic in &class_union.types {
        let atomic_class_id = match atomic {
            TAtomic::TLiteralClassString { name } => {
                let class_name = name.strip_prefix('\\').unwrap_or(name);
                Some(analyzer.interner.intern(class_name))
            }
            TAtomic::TLiteralString { value } => {
                let class_name = value.strip_prefix('\\').unwrap_or(value);
                resolve_class_name_for_callable(analyzer, class_name, context)
            }
            TAtomic::TNamedObject { name, .. } => Some(*name),
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => get_named_class_from_atomic(as_type),
            TAtomic::TTemplateParam { as_type, .. } => {
                get_callable_class_from_union(analyzer, as_type, context)
            }
            TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_from_atomic(as_type),
            _ => None,
        }?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_named_class_from_atomic(atomic: &TAtomic) -> Option<StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            if as_type.is_single() {
                get_named_class_from_atomic(as_type.get_single()?)
            } else {
                None
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_from_atomic(as_type),
        _ => None,
    }
}

fn resolve_class_name_for_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> Option<StrId> {
    let normalized = class_name.strip_prefix('\\').unwrap_or(class_name);
    let class_id = analyzer.interner.intern(normalized);

    if analyzer.codebase.classlike_infos.contains_key(&class_id) {
        return Some(class_id);
    }

    if let Some(ns_id) = context.namespace {
        let ns = analyzer.interner.lookup(ns_id);
        let qualified = format!("{}\\{}", ns, normalized);
        let qualified_id = analyzer.interner.intern(&qualified);
        if analyzer
            .codebase
            .classlike_infos
            .contains_key(&qualified_id)
        {
            return Some(qualified_id);
        }
    }

    None
}

pub(crate) fn resolve_callable_union_for_template_inference(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut callable_union: Option<TUnion> = None;

    for atomic in &arg_type.types {
        let callable_atomic = match atomic {
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => Some(atomic.clone()),
            TAtomic::TLiteralString { value } => {
                let cleaned = value.strip_prefix('\\').unwrap_or(value);

                if let Some((class_name, method_name)) = cleaned.split_once("::") {
                    let class_id = resolve_class_name_for_callable(analyzer, class_name, context)?;
                    let class_info = analyzer.codebase.get_class(class_id)?;
                    let method_id = analyzer.interner.intern(method_name);
                    class_info
                        .methods
                        .get(&method_id)
                        .map(|method| functionlike_to_callable_atomic(method))
                } else {
                    let is_fq = value.starts_with('\\');
                    function_call_analyzer::resolve_function(analyzer, value, is_fq, None, context)
                        .map(functionlike_to_callable_atomic)
                }
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } => resolve_invokable_object_callable(analyzer, *name, type_params.as_deref()),
            // A `[$class_or_obj, "method"]` callable (old TKeyedArray shape).
            TAtomic::TArray { known_values, .. } if !known_values.is_empty() => {
                resolve_array_callable_method(analyzer, known_values, context).map(|method_info| {
                    let mut callable = functionlike_to_callable_atomic(method_info);
                    // Late-bind `static` returns to the template a
                    // `class-string<T1>` element names (same as
                    // resolve_array_callable).
                    if let Some((_, class_union)) =
                        known_values.get(&pzoom_code_info::ArrayKey::Int(0))
                        && let Some(static_binding) =
                            template_static_binding_from_union(class_union)
                        && let Some(class_id) =
                            get_callable_class_from_union(analyzer, class_union, context)
                        && let TAtomic::TCallable {
                            return_type: Some(return_type),
                            ..
                        } = &mut callable
                    {
                        let parent_class_id = analyzer
                            .codebase
                            .get_class(class_id)
                            .and_then(|class_info| class_info.parent_class);
                        **return_type = crate::type_expander::
                            localize_special_class_type_union_with_static_object(
                                analyzer.codebase,
                                analyzer.interner,
                                return_type,
                                class_id,
                                static_binding,
                                parent_class_id,
                            );
                    }
                    callable
                })
            }
            _ => None,
        };

        if let Some(callable_atomic) = callable_atomic {
            callable_union = Some(if let Some(existing) = callable_union {
                combine_union_types(&existing, &TUnion::new(callable_atomic), false)
            } else {
                TUnion::new(callable_atomic)
            });
        }
    }

    callable_union
}

fn functionlike_to_callable_atomic(function_info: &pzoom_code_info::FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| pzoom_code_info::FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TCallable {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure || function_info.is_mutation_free),
    }
}

pub(crate) fn union_contains_non_pure_callable(union: &TUnion) -> bool {
    union.types.iter().any(atomic_is_non_pure_callable)
}

fn atomic_is_non_pure_callable(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { is_pure, .. } | TAtomic::TClosure { is_pure, .. } => {
            !matches!(is_pure, Some(true))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_non_pure_callable(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_is_non_pure_callable),
        _ => false,
    }
}

pub(crate) fn maybe_check_builtin_callable_arity(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !func_name.eq_ignore_ascii_case("array_map") {
        return;
    }

    if args.len() < 2 || args.iter().skip(1).any(|arg| arg.is_unpacked()) {
        return;
    }

    let callback_pos = if let Some(pos) = arg_positions.first().copied() {
        pos
    } else {
        return;
    };

    let Some(callback_type) = analysis_data.expr_types.get(&callback_pos).cloned() else {
        return;
    };

    let callback_arity = args.len().saturating_sub(1);
    match callable_arity_status(analyzer, &callback_type, callback_arity, context) {
        CallableArityStatus::TooFew { required } => {
            let (line, col) = analyzer.get_line_column(callback_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooFewArguments,
                format!(
                    "Too few arguments for callable passed to array_map, {} expected, {} provided",
                    required, callback_arity
                ),
                analyzer.file_path,
                callback_pos.0,
                callback_pos.1,
                line,
                col,
            ));
        }
        CallableArityStatus::TooMany { max } => {
            let (line, col) = analyzer.get_line_column(callback_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooManyArguments,
                format!(
                    "Too many arguments for callable passed to array_map, {} expected, {} provided",
                    max, callback_arity
                ),
                analyzer.file_path,
                callback_pos.0,
                callback_pos.1,
                line,
                col,
            ));
        }
        CallableArityStatus::Supported | CallableArityStatus::Unknown => {}
    }
}

fn callable_arity_status(
    analyzer: &StatementsAnalyzer<'_>,
    callback_type: &TUnion,
    arity: usize,
    context: &BlockContext,
) -> CallableArityStatus {
    let mut saw_unknown = false;
    let mut saw_known = false;
    let mut min_required_above: Option<usize> = None;
    let mut max_allowed_below: Option<usize> = None;

    for atomic in &callback_type.types {
        match atomic {
            TAtomic::TNull => {}
            TAtomic::TCallable { params, .. } | TAtomic::TClosure { params, .. } => {
                let Some(params) = params.as_ref() else {
                    saw_unknown = true;
                    continue;
                };

                saw_known = true;
                let required_count = params
                    .iter()
                    .filter(|param| !param.is_optional && !param.is_variadic)
                    .count();
                let param_count = params.len();
                let is_variadic = params.last().is_some_and(|param| param.is_variadic);

                if params_accept_arity(required_count, param_count, is_variadic, arity) {
                    return CallableArityStatus::Supported;
                }

                if arity < required_count {
                    min_required_above = Some(
                        min_required_above
                            .map_or(required_count, |existing| existing.min(required_count)),
                    );
                } else if !is_variadic && arity > param_count {
                    max_allowed_below = Some(
                        max_allowed_below.map_or(param_count, |existing| existing.max(param_count)),
                    );
                }
            }
            TAtomic::TLiteralString { value } => {
                let Some(function_info) =
                    function_call_analyzer::resolve_function(analyzer, value, false, None, context)
                else {
                    saw_unknown = true;
                    continue;
                };

                saw_known = true;
                let required_count = function_info
                    .params
                    .iter()
                    .filter(|param| !param.is_optional && !param.is_variadic)
                    .count();
                let param_count = function_info.params.len();
                let is_variadic = function_info
                    .params
                    .last()
                    .is_some_and(|param| param.is_variadic);

                if params_accept_arity(required_count, param_count, is_variadic, arity) {
                    return CallableArityStatus::Supported;
                }

                if arity < required_count {
                    min_required_above = Some(
                        min_required_above
                            .map_or(required_count, |existing| existing.min(required_count)),
                    );
                } else if !is_variadic && arity > param_count {
                    max_allowed_below = Some(
                        max_allowed_below.map_or(param_count, |existing| existing.max(param_count)),
                    );
                }
            }
            _ => {
                saw_unknown = true;
            }
        }
    }

    if min_required_above.is_some() && max_allowed_below.is_none() {
        return CallableArityStatus::TooFew {
            required: min_required_above.unwrap_or(arity + 1),
        };
    }

    if max_allowed_below.is_some() && min_required_above.is_none() {
        return CallableArityStatus::TooMany {
            max: max_allowed_below.unwrap_or(arity.saturating_sub(1)),
        };
    }

    if saw_known || saw_unknown {
        CallableArityStatus::Unknown
    } else {
        CallableArityStatus::Supported
    }
}

fn params_accept_arity(
    required_count: usize,
    param_count: usize,
    variadic: bool,
    arity: usize,
) -> bool {
    arity >= required_count && (variadic || arity <= param_count)
}

pub(crate) fn callable_union_is_pure(union: &TUnion) -> bool {
    let mut saw_non_null_candidate = false;
    let mut saw_callable_candidate = false;

    for atomic in &union.types {
        match atomic {
            TAtomic::TNull => {}
            TAtomic::TCallable { is_pure, .. } | TAtomic::TClosure { is_pure, .. } => {
                saw_non_null_candidate = true;
                saw_callable_candidate = true;
                if !matches!(is_pure, Some(true)) {
                    return false;
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                saw_non_null_candidate = true;
                if !callable_union_is_pure(as_type) {
                    return false;
                }
                saw_callable_candidate = true;
            }
            _ => {
                return false;
            }
        }
    }

    !saw_non_null_candidate || saw_callable_candidate
}

pub(crate) enum CallableArityStatus {
    Supported,
    TooFew { required: usize },
    TooMany { max: usize },
    Unknown,
}

/// Psalm's variable-call invocability checks (FunctionCallAnalyzer when the
/// callee is an expression): a mixed callee is a MixedFunctionCall, a null
/// possibility among others is a PossiblyNullFunctionCall, and a non-callable
/// possibility is a PossiblyInvalidFunctionCall (InvalidFunctionCall when
/// nothing in the union is callable).
fn check_callable_union_invocability(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    pos: Pos,
) {
    // By-ref placeholders (undefined vars captured by reference) are
    // typeless in Psalm and skip these checks entirely.
    if callee_type.from_undefined_by_ref {
        return;
    }

    let mut has_callable = false;
    let mut has_null = false;
    let mut has_mixed = false;
    let mut has_invalid = false;

    fn atomic_is_callable_like(
        analyzer: &StatementsAnalyzer<'_>,
        atomic: &TAtomic,
    ) -> Option<bool> {
        match atomic {
            TAtomic::TClosure { .. }
            | TAtomic::TCallable { .. }
            | TAtomic::TCallableString
            | TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TClassString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TArray { .. }
            | TAtomic::TObject
            | TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. } => Some(true),
            // An intersection is callable when any part is.
            TAtomic::TObjectIntersection { types } => Some(
                types
                    .iter()
                    .any(|part| atomic_is_callable_like(analyzer, part) == Some(true)),
            ),
            // An unknown class is not callable (Psalm reports
            // InvalidFunctionCall alongside the UndefinedClass).
            TAtomic::TNamedObject { name, .. } => Some(
                analyzer
                    .codebase
                    .get_class(*name)
                    .is_some_and(|class_info| class_info.methods.contains_key(&StrId::INVOKE)),
            ),
            _ => None,
        }
    }

    for atomic in &callee_type.types {
        match atomic {
            TAtomic::TMixed | TAtomic::TNonEmptyMixed | TAtomic::TMixedFromLoopIsset => {
                has_mixed = true
            }
            TAtomic::TNull | TAtomic::TVoid => has_null = true,
            other => match atomic_is_callable_like(analyzer, other) {
                Some(true) => has_callable = true,
                _ => has_invalid = true,
            },
        }
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    if has_mixed {
        analysis_data.add_issue(Issue::new(
            IssueKind::MixedFunctionCall,
            "Cannot call function on mixed",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if has_null {
        // Psalm: a callee that can only be null is a NullFunctionCall; null
        // among other possibilities is a PossiblyNullFunctionCall.
        let only_null = !has_callable && !has_mixed && !has_invalid;
        analysis_data.add_issue(if only_null {
            Issue::new(
                IssueKind::NullFunctionCall,
                "Cannot call function on null value",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            )
        } else {
            Issue::new(
                IssueKind::PossiblyNullFunctionCall,
                "Cannot call function on possibly null value",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            )
        });
    }

    if has_invalid {
        analysis_data.add_issue(Issue::new(
            if has_callable || has_mixed || has_null {
                IssueKind::PossiblyInvalidFunctionCall
            } else {
                IssueKind::InvalidFunctionCall
            },
            format!(
                "Cannot treat type {} as callable",
                callee_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }
}

/// Whether the candidate callable's maximum arity is below the expected
/// callable's required arity (the only-fewer-params direction of an arity
/// mismatch).
fn callback_accepts_fewer_than_expected(candidate: &TAtomic, expected: &TAtomic) -> bool {
    let (Some(candidate_params), Some(expected_params)) = (
        get_callable_params(candidate),
        get_callable_params(expected),
    ) else {
        return false;
    };

    let candidate_required = candidate_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();
    let expected_max = if expected_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(expected_params.len())
    };
    // Requiring more than the container provides is the fatal direction.
    if let Some(expected_max) = expected_max
        && candidate_required > expected_max
    {
        return false;
    }

    let candidate_max = if candidate_params
        .last()
        .is_some_and(|param| param.is_variadic)
    {
        None
    } else {
        Some(candidate_params.len())
    };
    let expected_required = expected_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();
    candidate_max.is_some_and(|candidate_max| candidate_max < expected_required)
}
