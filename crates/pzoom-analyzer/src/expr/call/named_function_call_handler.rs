//! Named function call handler.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Mirrors Psalm's `NamedFunctionCallHandler::handle`.
pub(super) fn handle(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    if function_name.eq_ignore_ascii_case("get_class")
        || function_name.eq_ignore_ascii_case("gettype")
        || function_name.eq_ignore_ascii_case("get_debug_type")
    {
        return handle_dependent_type_function(
            analyzer,
            function_name,
            arg_positions,
            analysis_data,
            context,
        );
    }

    if function_name.eq_ignore_ascii_case("get_called_class") {
        return Some(infer_get_called_class_return_type(analyzer, context));
    }

    if function_name.eq_ignore_ascii_case("get_parent_class")
        && func_call.argument_list.arguments.is_empty()
    {
        return Some(infer_get_parent_class_return_type(analyzer, context));
    }

    if function_name.eq_ignore_ascii_case("define") {
        apply_define_side_effect(func_call, arg_positions, analysis_data, context, analyzer);
    }

    // Keep runtime aliases flow-sensitive even when function metadata is unavailable.
    if function_name.eq_ignore_ascii_case("class_alias") {
        apply_class_alias_side_effect(analyzer, func_call, context);
    }

    if function_name.eq_ignore_ascii_case("file_get_contents")
        && func_call
            .argument_list
            .arguments
            .first()
            .is_some_and(is_php_stream_literal_argument)
    {
        return Some(TUnion::string());
    }

    if function_name.eq_ignore_ascii_case("method_exists") {
        return Some(analyze_method_exists_call(
            analyzer,
            func_call,
            arg_positions,
            pos,
            analysis_data,
        ));
    }

    None
}

/// Mirrors Psalm's `NamedFunctionCallHandler::handleDependentTypeFunction`.
fn handle_dependent_type_function(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    if !function_name.eq_ignore_ascii_case("get_class")
        && !function_name.eq_ignore_ascii_case("gettype")
        && !function_name.eq_ignore_ascii_case("get_debug_type")
    {
        return None;
    }

    if let Some(first_arg_pos) = arg_positions.first().copied()
        && let Some(first_arg_type) = analysis_data.get_expr_type(first_arg_pos)
    {
        if function_name.eq_ignore_ascii_case("get_class") {
            return Some(infer_get_class_return_type(first_arg_type.as_ref()));
        }

        return None;
    }

    if function_name.eq_ignore_ascii_case("get_class") {
        if let Some(self_class_id) = analyzer.get_declaring_class().or(context.self_class) {
            return Some(TUnion::new(TAtomic::TClassString {
                as_type: Some(Box::new(TAtomic::TNamedObject {
                    name: self_class_id,
                    type_params: None,
                })),
            }));
        }

        return Some(TUnion::new(TAtomic::TClassString { as_type: None }));
    }

    None
}

pub(super) fn is_php_stream_literal_argument(
    arg: &mago_syntax::ast::ast::argument::Argument<'_>,
) -> bool {
    let expr = arg.value().unparenthesized();
    let Expression::Literal(Literal::String(string_lit)) = expr else {
        return false;
    };

    let Some(value) = string_lit.value else {
        return false;
    };

    let stream_name = value.to_ascii_lowercase();
    stream_name == "php://input" || stream_name == "php://stdin"
}

fn apply_class_alias_side_effect(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    context: &mut BlockContext,
) {
    if context.inside_conditional {
        return;
    }

    let Some(source_arg) = func_call.argument_list.arguments.first() else {
        return;
    };
    let Some(alias_arg) = func_call.argument_list.arguments.get(1) else {
        return;
    };

    let Some(source_class) = extract_class_alias_name(analyzer, source_arg.value(), context) else {
        return;
    };
    let Some(alias_class) = extract_class_alias_name(analyzer, alias_arg.value(), context) else {
        return;
    };

    let source_class = context
        .class_aliases
        .get(&source_class)
        .copied()
        .unwrap_or(source_class);

    context.class_aliases.insert(alias_class, source_class);
}

fn apply_define_side_effect(
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
) {
    let Some(const_name_arg) = func_call.argument_list.arguments.first() else {
        return;
    };
    let Some(const_name) = extract_literal_string_value(const_name_arg.value()) else {
        return;
    };

    let const_name = const_name.trim_start_matches('\\');
    if const_name.is_empty() {
        return;
    }

    let const_value_type = arg_positions
        .get(1)
        .and_then(|pos| analysis_data.get_expr_type(*pos))
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let const_id = analyzer.interner.intern(const_name);
    context.defined_constants.insert(const_id, const_value_type);
}

fn extract_class_alias_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    match expr.unparenthesized() {
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            if !matches!(
                class_const_access.constant,
                ClassLikeConstantSelector::Identifier(identifier)
                    if identifier.value.eq_ignore_ascii_case("class")
            ) {
                return None;
            }

            resolve_aliasable_class_id(analyzer, class_const_access.class, context)
        }
        Expression::Literal(Literal::String(string_lit)) => {
            let mut normalized = string_lit.value?.trim_start_matches('\\').to_string();
            if !normalized.contains('\\') {
                let span = string_lit.span();
                let raw_literal = analyzer
                    .get_source_substring(span.start.offset as usize, span.end.offset as usize);
                if let Some(raw_inner) = strip_wrapping_quotes(raw_literal.trim())
                    && raw_inner.contains('\\')
                {
                    normalized = raw_inner.trim_start_matches('\\').to_string();
                }
            }

            Some(analyzer.interner.intern(normalized.as_str()))
        }
        Expression::Identifier(identifier) => {
            let offset = identifier.span().start.offset;
            let class_id = analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(identifier.value()));
            Some(
                context
                    .class_aliases
                    .get(&class_id)
                    .copied()
                    .unwrap_or(class_id),
            )
        }
        _ => None,
    }
}

fn resolve_aliasable_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    let class_id = match expr.unparenthesized() {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(id.value()))
        }
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class()?,
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|class_id| {
            analyzer
                .codebase
                .get_class(class_id)
                .and_then(|class_info| class_info.parent_class)
        })?,
        _ => return None,
    };

    Some(
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .unwrap_or(class_id),
    )
}

fn strip_wrapping_quotes(raw: &str) -> Option<&str> {
    if raw.len() < 2 {
        return None;
    }

    let first = raw.as_bytes()[0] as char;
    let last = raw.as_bytes()[raw.len() - 1] as char;
    if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
        Some(&raw[1..raw.len() - 1])
    } else {
        None
    }
}

fn infer_get_class_return_type(arg_type: &TUnion) -> TUnion {
    if arg_type.is_single() {
        return match arg_type.get_single() {
            Some(TAtomic::TNamedObject { name, .. }) => TUnion::new(TAtomic::TClassString {
                as_type: Some(Box::new(TAtomic::TNamedObject {
                    name: *name,
                    type_params: None,
                })),
            }),
            Some(
                template @ TAtomic::TTemplateParam { .. }
                | template @ TAtomic::TTemplateParamClass { .. },
            ) => TUnion::new(TAtomic::TClassString {
                as_type: Some(Box::new(template.clone())),
            }),
            _ => TUnion::new(TAtomic::TClassString { as_type: None }),
        };
    }

    TUnion::new(TAtomic::TClassString { as_type: None })
}

fn infer_get_called_class_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
) -> TUnion {
    if analyzer.get_declaring_class().or(context.self_class).is_some() {
        return TUnion::new(TAtomic::TClassString {
            as_type: Some(Box::new(TAtomic::TNamedObject {
                name: StrId::STATIC,
                type_params: None,
            })),
        });
    }

    TUnion::new(TAtomic::TClassString { as_type: None })
}

fn infer_get_parent_class_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
) -> TUnion {
    let self_class_id = analyzer.get_declaring_class().or(context.self_class);
    let parent_class_id = self_class_id.and_then(|class_id| {
        analyzer
            .codebase
            .get_class(class_id)
            .and_then(|class_info| class_info.parent_class)
    });

    if let Some(parent_class_id) = parent_class_id {
        return TUnion::new(TAtomic::TClassString {
            as_type: Some(Box::new(TAtomic::TNamedObject {
                name: parent_class_id,
                type_params: None,
            })),
        });
    }

    TUnion::from_types(vec![
        TAtomic::TClassString { as_type: None },
        TAtomic::TFalse,
    ])
}

fn analyze_method_exists_call(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    let Some(method_arg) = func_call.argument_list.arguments.get(1) else {
        return TUnion::bool();
    };

    let Some(method_name) = extract_literal_string_arg(method_arg.value()) else {
        return TUnion::bool();
    };

    let Some(target_arg_pos) = arg_positions.first().copied() else {
        return TUnion::bool();
    };

    let Some(target_type) = analysis_data.get_expr_type(target_arg_pos) else {
        return TUnion::bool();
    };

    let mut can_be_true = false;
    let mut can_be_false = false;

    for atomic in &target_type.types {
        let (atomic_true, atomic_false) =
            atomic_method_exists_possibility(analyzer, atomic, &method_name);
        can_be_true |= atomic_true;
        can_be_false |= atomic_false;
    }

    if can_be_true && !can_be_false {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RedundantCondition,
            format!(
                "Call to method_exists(..., \"{}\") is always true",
                method_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return TUnion::new(TAtomic::TTrue);
    }

    if !can_be_true && can_be_false {
        return TUnion::new(TAtomic::TFalse);
    }

    TUnion::bool()
}

fn atomic_method_exists_possibility(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    method_name: &str,
) -> (bool, bool) {
    match atomic {
        TAtomic::TNamedObject { name, .. } => {
            if class_or_ancestor_has_method(analyzer, *name, method_name) {
                (true, false)
            } else {
                (false, true)
            }
        }
        TAtomic::TLiteralClassString { name } => {
            let class_id = analyzer.interner.intern(name.trim_start_matches('\\'));
            if class_or_ancestor_has_method(analyzer, class_id, method_name) {
                (true, false)
            } else {
                (false, true)
            }
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        }
        | TAtomic::TTemplateParamClass { as_type, .. } => {
            if let TAtomic::TNamedObject { name, .. } = &**as_type {
                if class_or_ancestor_has_method(analyzer, *name, method_name) {
                    (true, false)
                } else {
                    (false, true)
                }
            } else {
                (true, true)
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut can_be_true = false;
            let mut can_be_false = false;
            for bound_atomic in &as_type.types {
                let (bound_true, bound_false) =
                    atomic_method_exists_possibility(analyzer, bound_atomic, method_name);
                can_be_true |= bound_true;
                can_be_false |= bound_false;
            }
            (can_be_true, can_be_false)
        }
        TAtomic::TMixed
        | TAtomic::TObject
        | TAtomic::TClassString { .. }
        | TAtomic::TString
        | TAtomic::TLiteralString { .. } => (true, true),
        _ => (false, true),
    }
}

fn class_or_ancestor_has_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: &str,
) -> bool {
    let mut current_class = Some(class_id);

    while let Some(current_class_id) = current_class {
        let Some(class_info) = analyzer.codebase.get_class(current_class_id) else {
            return false;
        };

        if class_has_method_case_insensitive(analyzer, class_info, method_name) {
            return true;
        }

        for interface_id in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
        {
            let Some(interface_info) = analyzer.codebase.get_class(*interface_id) else {
                continue;
            };

            if class_has_method_case_insensitive(analyzer, interface_info, method_name) {
                return true;
            }
        }

        current_class = class_info.parent_class;
    }

    false
}

fn class_has_method_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: &str,
) -> bool {
    class_info.methods.keys().any(|method_id| {
        analyzer
            .interner
            .lookup(*method_id)
            .as_ref()
            .eq_ignore_ascii_case(method_name)
    }) || class_info.pseudo_methods.keys().any(|method_id| {
        analyzer
            .interner
            .lookup(*method_id)
            .as_ref()
            .eq_ignore_ascii_case(method_name)
    }) || class_info.pseudo_static_methods.keys().any(|method_id| {
        analyzer
            .interner
            .lookup(*method_id)
            .as_ref()
            .eq_ignore_ascii_case(method_name)
    })
}

fn extract_literal_string_arg(expr: &Expression<'_>) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    Some(string_lit.value?.to_ascii_lowercase())
}

fn extract_literal_string_value(expr: &Expression<'_>) -> Option<String> {
    let Expression::Literal(Literal::String(string_lit)) = expr.unparenthesized() else {
        return None;
    };

    Some(string_lit.value?.to_string())
}
