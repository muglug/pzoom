//! Existing-method static call analysis: resolution up the hierarchy, return type,
//! template context, visibility, argument verification. Mirrors Psalm `ExistingAtomicStaticCallAnalyzer`.

use crate::type_expander::localize_special_class_type_union;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

use super::argument_analyzer;
use super::{arguments_analyzer, function_call_analyzer};

use super::static_call_analyzer::*;
use pzoom_code_info::TemplateResult;

pub(crate) fn can_call_non_static_via_class_scope(
    analyzer: &StatementsAnalyzer<'_>,
    called_class: StrId,
    class_expr: &Expression<'_>,
) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    if function_info.is_static || function_info.declaring_class.is_none() {
        return false;
    }

    if matches!(
        class_expr.unparenthesized(),
        Expression::Parent(_) | Expression::Self_(_) | Expression::Static(_)
    ) {
        return true;
    }

    let Some(calling_class) = function_info.declaring_class else {
        return false;
    };

    if calling_class == called_class {
        return true;
    }

    analyzer
        .codebase
        .get_class(calling_class)
        .is_some_and(|class_info| class_info.all_parent_classes.contains(&called_class))
}

pub(crate) fn resolve_named_object_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
    bool,
)> {
    if let Some(method_info) = get_method_info(analyzer, class_info, method_name) {
        let mut method_info = method_info.clone();
        // Psalm's Methods::getMethodReturnType consults the receiver class's
        // pseudo methods FIRST: an @method annotation overrides an inherited
        // real method's return type.
        if let Some(pseudo_info) = get_pseudo_static_method_info(analyzer, class_info, method_name)
            .or_else(|| get_pseudo_method_info(analyzer, class_info, method_name))
            && pseudo_info.return_type.is_some()
        {
            method_info.return_type = pseudo_info.return_type.clone();
            method_info.declaring_class = pseudo_info.declaring_class;
        }
        return Some((class_info.name, None, method_info, false));
    }

    if class_info.kind == ClassLikeKind::Interface
        || class_has_magic_callstatic(class_info)
        || class_has_magic_call(class_info)
    {
        if let Some(method_info) = get_pseudo_static_method_info(analyzer, class_info, method_name)
        {
            return Some((class_info.name, None, method_info.clone(), false));
        }

        if let Some(method_info) = get_pseudo_method_info(analyzer, class_info, method_name) {
            return Some((class_info.name, None, method_info.clone(), false));
        }
    }

    resolve_named_mixin_static_method(analyzer, class_info, method_name).map(
        |(resolved_class_id, resolved_type_params, method_info)| {
            (
                resolved_class_id,
                resolved_type_params,
                method_info,
                class_has_magic_callstatic(class_info),
            )
        },
    )
}

fn resolve_named_mixin_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    method_name: &str,
) -> Option<(
    StrId,
    Option<Vec<TUnion>>,
    pzoom_code_info::FunctionLikeInfo,
)> {
    if class_info.named_mixins.is_empty() {
        return None;
    }

    let mut class_template_result = function_call_analyzer::get_class_template_defaults(class_info);
    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut class_template_result,
        class_info,
    );

    for mixin_atomic in &class_info.named_mixins {
        let localized_mixin = function_call_analyzer::replace_templates_in_union(
            &TUnion::new(mixin_atomic.clone()),
            &class_template_result,
        );

        for localized_atomic in localized_mixin.types {
            let TAtomic::TNamedObject {
                name: mixin_class_id,
                type_params: mixin_type_params,
                ..
            } = localized_atomic
            else {
                continue;
            };

            let Some(mixin_class_info) = analyzer.codebase.get_class(mixin_class_id) else {
                continue;
            };

            if let Some(method_info) = get_method_info(analyzer, mixin_class_info, method_name) {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }

            if let Some(method_info) =
                get_pseudo_static_method_info(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }

            if let Some(method_info) =
                get_pseudo_method_info(analyzer, mixin_class_info, method_name)
            {
                return Some((mixin_class_id, mixin_type_params, method_info.clone()));
            }
        }
    }

    None
}

pub(crate) fn resolve_descendant_static_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    call_pos: Pos,
) -> Option<TUnion> {
    let descendants = analyzer.codebase.all_classlike_descendants.get(&class_id)?;
    let mut combined_return_type: Option<TUnion> = None;
    let mut found = false;

    for descendant_id in descendants {
        let Some(descendant_info) = analyzer.codebase.get_class(*descendant_id) else {
            continue;
        };
        let Some(method_info) = get_method_info(analyzer, descendant_info, method_name) else {
            continue;
        };
        if !method_info.is_static {
            continue;
        }

        found = true;
        let descendant_name = analyzer.interner.lookup(*descendant_id);
        let template_result = build_static_method_template_context(
            analyzer,
            descendant_info,
            None,
            analyzer
                .get_declaring_class()
                .and_then(|class_id| analyzer.codebase.get_class(class_id)),
            method_info,
            args,
            arg_positions,
            analysis_data,
            context,
        );
        analyze_pending_closure_args_for_static_method(
            analyzer,
            args,
            arg_positions,
            method_info,
            &template_result,
            *descendant_id,
            class_id,
            descendant_info.parent_class,
            analysis_data,
            context,
        );
        verify_method_arguments(
            analyzer,
            args,
            arg_positions,
            method_info,
            descendant_name.as_ref(),
            method_name,
            analysis_data,
            context,
            call_pos,
            &template_result,
            *descendant_id,
            class_id,
            descendant_info.parent_class,
        );
        crate::expr::call::atomic_method_call_analyzer::apply_post_static_call_assertions(
            analyzer,
            analysis_data,
            args,
            method_info,
            context,
            &template_result,
            *descendant_id,
            class_id,
            descendant_info.parent_class,
        );

        let return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            method_info,
            &template_result,
            &FxHashMap::default(),
            args.len(),
        )
        .unwrap_or_else(TUnion::mixed);
        combined_return_type = Some(if let Some(existing) = combined_return_type {
            combine_union_types(&existing, &return_type, false)
        } else {
            return_type
        });
    }

    if found {
        Some(combined_return_type.unwrap_or_else(TUnion::mixed))
    } else {
        None
    }
}

/// A magic method call (resolved via `__callStatic`/`__call`) is mutation-free when
/// its backing magic handler is pure or mutation-free. Returns false for real methods
/// (which are handled by the normal purity check).
pub(crate) fn magic_call_handler_is_mutation_free(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &ClassLikeInfo,
    method_name: &str,
) -> bool {
    // Real declared methods are validated normally, not via a magic handler.
    if get_method_info(analyzer, class_info, method_name).is_some() {
        return false;
    }

    ["__callStatic", "__call"].iter().any(|handler| {
        get_method_info(analyzer, class_info, handler)
            .is_some_and(|handler_info| handler_info.is_pure || handler_info.is_mutation_free)
    })
}

pub(crate) fn get_method_info<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    class_info.methods.get(&method_id).map(|method| &**method)
}

pub(crate) fn get_pseudo_method_info<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    class_info.pseudo_methods.get(&method_id)
}

fn get_pseudo_static_method_info<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    class_info.pseudo_static_methods.get(&method_id)
}

pub(crate) fn class_has_magic_callstatic(class_info: &ClassLikeInfo) -> bool {
    class_info
        .methods
        .contains_key(&pzoom_str::StrId::CALL_STATIC)
}

pub(crate) fn class_has_magic_call(class_info: &ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::CALL)
}

pub(crate) fn get_method_visibility_scope_class_id(
    class_info: &ClassLikeInfo,
    method_info: &pzoom_code_info::FunctionLikeInfo,
) -> StrId {
    class_info
        .appearing_method_ids
        .get(&method_info.name)
        .copied()
        .or(method_info.declaring_class)
        .unwrap_or(class_info.name)
}

pub(crate) fn can_access_protected_member_visibility(
    analyzer: &StatementsAnalyzer<'_>,
    caller_class: StrId,
    visibility_scope_class: StrId,
) -> bool {
    caller_class == visibility_scope_class
        || object_type_comparator::is_class_subtype_of(
            caller_class,
            visibility_scope_class,
            analyzer.codebase,
        )
        || object_type_comparator::is_class_subtype_of(
            visibility_scope_class,
            caller_class,
            analyzer.codebase,
        )
}

pub(crate) fn build_static_method_template_context(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    called_type_params: Option<&[TUnion]>,
    invoking_class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> TemplateResult {
    let mut template_result = function_call_analyzer::get_class_template_defaults(class_info);
    for template_type in &method_info.template_types {
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
            called_type_params,
        ),
    );

    if let Some(invoking_class_info) = invoking_class_info {
        if let Some(invoking_template_map) = invoking_class_info
            .template_extended_params
            .get(&class_info.name)
        {
            for (template_name, replacement) in invoking_template_map {
                crate::template::lower_bounds_insert(
                    &mut template_result,
                    *template_name,
                    pzoom_code_info::GenericParent::ClassLike(class_info.name),
                    replacement.clone(),
                );
            }
        }
    }

    let mut arg_template_result = TemplateResult {
        template_types: template_result.template_types.clone(),
        ..Default::default()
    };
    function_call_analyzer::infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        &method_info.params,
        &mut arg_template_result,
        analysis_data,
        context,
    );
    // Class templates already pinned by the receiver/extends chain are
    // READONLY during argument standin replacement (Psalm's
    // $class_generic_params readonly TemplateResult): an `object` argument
    // must not widen `T` past the binding `@template-extends Stringer<A>`
    // fixed. Argument-derived bounds only fill templates without one.
    for (template_name, entities) in arg_template_result.lower_bounds {
        for (defining_entity, bounds) in entities {
            template_result
                .lower_bounds
                .entry(template_name)
                .or_default()
                .entry(defining_entity)
                .or_insert(bounds);
        }
    }

    template_result
}

pub(crate) fn method_is_mutation_free(
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    method_info.is_pure
        || method_info.is_mutation_free
        || (class_info.is_immutable && !method_info.is_static)
}

pub(crate) fn get_inherited_method_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
    template_result: &TemplateResult,
    param_arg_types: &rustc_hash::FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let mut candidate_class_ids = Vec::new();

    if let Some(parent_class_id) = class_info.parent_class {
        candidate_class_ids.push(parent_class_id);
    }

    candidate_class_ids.extend(
        class_info
            .all_parent_classes
            .iter()
            .copied()
            .filter(|parent_class_id| Some(*parent_class_id) != class_info.parent_class),
    );
    candidate_class_ids.extend(class_info.all_parent_interfaces.iter().copied());

    let mut seen = FxHashSet::default();
    for candidate_class_id in candidate_class_ids {
        if !seen.insert(candidate_class_id) {
            continue;
        }

        let Some(candidate_class_info) = analyzer.codebase.get_class(candidate_class_id) else {
            continue;
        };

        let Some(candidate_method_info) =
            get_method_info(analyzer, candidate_class_info, method_name)
        else {
            continue;
        };

        if candidate_method_info.get_return_type().is_none() {
            continue;
        }

        let mut candidate_result = template_result.clone();
        for (template_name, entries) in
            function_call_analyzer::get_class_template_defaults(candidate_class_info).template_types
        {
            candidate_result
                .template_types
                .entry(template_name)
                .or_insert(entries);
        }
        for (template_name, entries) in
            function_call_analyzer::get_template_defaults(candidate_method_info).template_types
        {
            candidate_result
                .template_types
                .entry(template_name)
                .or_insert(entries);
        }

        if let Some(candidate_template_map) =
            class_info.template_extended_params.get(&candidate_class_id)
        {
            for (template_name, replacement) in candidate_template_map {
                crate::template::lower_bounds_insert(
                    &mut candidate_result,
                    *template_name,
                    pzoom_code_info::GenericParent::ClassLike(candidate_class_id),
                    replacement.clone(),
                );
            }
        }

        let resolved_return_type = function_call_analyzer::resolve_functionlike_return_type(
            analyzer,
            candidate_method_info,
            &candidate_result,
            param_arg_types,
            arg_count,
        )
        .unwrap_or_else(TUnion::mixed);

        return Some(resolved_return_type);
    }

    None
}

pub(crate) fn verify_method_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_name: &str,
    method_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    call_pos: Pos,
    template_result: &TemplateResult,
    self_class_id: StrId,
    static_class_id: StrId,
    parent_class_id: Option<StrId>,
) {
    let callable_name = format!("{}::{}", class_name, method_name);
    let arg_param_indices = arguments_analyzer::check_arguments_match(
        analyzer,
        args,
        arg_positions,
        method_info,
        &callable_name,
        analysis_data,
        context,
        Some(template_result),
        call_pos,
        false,
        false,
    );

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = method_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let issue_pos = arg_positions.first().copied().unwrap_or(call_pos);
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to method {}, {} expected, {} provided",
                callable_name,
                required_params,
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = method_info.params.last().is_some_and(|p| p.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > method_info.params.len() {
        let issue_pos = arg_positions
            .get(method_info.params.len())
            .copied()
            .unwrap_or(call_pos);
        let (line, col) = analyzer.get_line_column(issue_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to method {}, {} expected, {} provided",
                callable_name,
                method_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            issue_pos.0,
            issue_pos.1,
            line,
            col,
        ));
    }

    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        if arg.is_unpacked() {
            if let Some(arg_type) =
                arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
            {
                argument_analyzer::verify_unpacked_argument(
                    analyzer,
                    arg_pos,
                    &arg_type,
                    &callable_name,
                    method_info.no_named_arguments,
                    analysis_data,
                );
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| method_info.params.get(mapped_index))
            .or_else(|| method_info.params.last().filter(|p| p.is_variadic));

        if let (Some(param), Some(arg_type)) = (
            param,
            arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos),
        ) {
            let mut effective_param = param.clone();
            if let Some(param_type) = param.get_type() {
                let replaced_param_type =
                    if crate::template::template_result_is_empty(template_result) {
                        param_type.clone()
                    } else {
                        function_call_analyzer::replace_templates_in_union(
                            param_type,
                            template_result,
                        )
                    };

                effective_param.param_type = Some(localize_special_class_type_union(
                    analyzer.codebase,
                    analyzer.interner,
                    &replaced_param_type,
                    self_class_id,
                    static_class_id,
                    parent_class_id,
                ));
            }

            argument_analyzer::verify_type(
                analyzer,
                arg,
                arg_pos,
                &arg_type,
                &effective_param,
                param_index.unwrap_or(idx),
                &callable_name,
                analysis_data,
                context,
                Some(arguments_analyzer::call_dataflow_for_method_call(
                    static_class_id,
                    method_info,
                    call_pos,
                )),
            );
        }
    }
}
