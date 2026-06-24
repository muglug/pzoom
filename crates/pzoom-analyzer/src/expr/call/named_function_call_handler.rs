//! Named function call handler.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;

use pzoom_code_info::VarName;
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
    // `\get_class(...)`-style fully-qualified calls match too.
    let function_name = function_name.trim_start_matches('\\');
    if function_name.eq_ignore_ascii_case("get_class")
        || function_name.eq_ignore_ascii_case("gettype")
        || function_name.eq_ignore_ascii_case("get_debug_type")
    {
        return handle_dependent_type_function(
            analyzer,
            function_name,
            func_call,
            arg_positions,
            analysis_data,
            context,
        );
    }

    if function_name.eq_ignore_ascii_case("var_dump")
        || function_name.eq_ignore_ascii_case("shell_exec")
    {
        // Psalm's NamedFunctionCallHandler flags these unconditionally.
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ForbiddenCode,
            format!("Unsafe {}", function_name.to_ascii_lowercase()),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if function_name.eq_ignore_ascii_case("get_called_class") {
        return Some(infer_get_called_class_return_type(analyzer, context));
    }

    if function_name.eq_ignore_ascii_case("get_parent_class")
        && func_call.argument_list.arguments.is_empty()
    {
        return Some(infer_get_parent_class_return_type(analyzer, context));
    }

    if (function_name.eq_ignore_ascii_case("array_walk")
        || function_name.eq_ignore_ascii_case("array_walk_recursive"))
        && let Some(first_arg_pos) = arg_positions.first()
        && let Some(first_arg_type) = analysis_data.expr_types.get(&*first_arg_pos).cloned()
        && first_arg_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                pzoom_code_info::TAtomic::TNamedObject { .. } | pzoom_code_info::TAtomic::TObject
            )
        })
    {
        // Psalm's NamedFunctionCallHandler: array_walk over an object iterates
        // its properties, which is rarely intended.
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            if first_arg_type.is_single() {
                IssueKind::RawObjectIteration
            } else {
                IssueKind::PossibleRawObjectIteration
            },
            "Possibly undesired iteration over object properties".to_string(),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    if function_name.eq_ignore_ascii_case("define") {
        apply_define_side_effect(func_call, arg_positions, analysis_data, context, analyzer);
    }

    if function_name.eq_ignore_ascii_case("is_file")
        || function_name.eq_ignore_ascii_case("file_exists")
    {
        register_phantom_file(analyzer, func_call, analysis_data, context);
    }

    if function_name.eq_ignore_ascii_case("constant") {
        // Psalm's NamedFunctionCallHandler resolves `constant($name)` through
        // ConstFetchAnalyzer::getConstName/getConstType when the name is a
        // known literal string.
        if let Some(const_type) =
            handle_constant_call(analyzer, func_call, arg_positions, analysis_data, context)
        {
            return Some(const_type);
        }
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

    if function_name.eq_ignore_ascii_case("compact") {
        return analyze_compact_call(analyzer, func_call, analysis_data, context);
    }

    if function_name.eq_ignore_ascii_case("extract") {
        // Side effects only: the normal call path still runs (return type,
        // arg checks, the `@psalm-taint-sink extract` sink).
        analyze_extract_call(func_call, arg_positions, analysis_data, context);
        return None;
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
///
/// When the argument is a plain in-scope variable `$x` (and not a template),
/// `get_class($x)`/`gettype($x)` return a *dependent* type that remembers `$x`,
/// so a later `get_class($x) === Foo::class` / `switch (gettype($x))` can narrow
/// `$x` (Psalm's `TDependentGetClass`/`TDependentGetType`).
fn handle_dependent_type_function(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    if !function_name.eq_ignore_ascii_case("get_class")
        && !function_name.eq_ignore_ascii_case("gettype")
        && !function_name.eq_ignore_ascii_case("get_debug_type")
    {
        return None;
    }

    if let Some(first_arg_pos) = arg_positions.first().copied()
        && let Some(first_arg_type) = analysis_data.expr_types.get(&first_arg_pos).cloned()
    {
        let first_arg_type = first_arg_type.as_ref().clone();

        // Produce a dependent type when the argument is a simple variable that is
        // in scope and not a template parameter. The dependent type remembers
        // `$x` so a later `get_class($x) === Foo::class` / `gettype($x) ===
        // "string"` (incl. via `switch`) narrows `$x`. Mirrors Psalm's
        // `TDependentGetClass` / `TDependentGetType`.
        if let Some(var_id) = dependent_arg_var_id(analyzer, func_call)
            && context.locals.contains_key(&var_id)
            && !first_arg_type.types.iter().any(is_template_atomic)
        {
            if function_name.eq_ignore_ascii_case("get_class") {
                let as_type = if first_arg_type.is_mixed() {
                    TUnion::new(TAtomic::TObject)
                } else {
                    first_arg_type.clone()
                };
                return Some(TUnion::new(TAtomic::TDependentGetClass {
                    var_id: var_id.clone(),
                    as_type: Box::new(as_type),
                }));
            }
            // gettype($x) / get_debug_type($x)
            return Some(TUnion::new(TAtomic::TDependentGetType { var_id }));
        }

        if function_name.eq_ignore_ascii_case("get_class") {
            return Some(infer_get_class_return_type(&first_arg_type));
        }

        return None;
    }

    if function_name.eq_ignore_ascii_case("get_class") {
        if let Some(self_class_id) = analyzer.get_declaring_class().or(context.self_class) {
            return Some(TUnion::new(TAtomic::TClassString {
                as_type: Some(Box::new(TAtomic::TNamedObject {
                    name: self_class_id,
                    type_params: None,
                    is_static: false,
                    remapped_params: false,
                })),
            }));
        }

        // Psalm: get_class() without arguments only works inside a class.
        let span = func_call.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(pzoom_code_info::Issue::new(
            pzoom_code_info::IssueKind::TooFewArguments,
            "Cannot call get_class() without argument outside of class scope",
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));

        return Some(TUnion::new(TAtomic::TClassString { as_type: None }));
    }

    None
}

pub(crate) fn is_php_stream_literal_argument(
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

pub(crate) fn apply_class_alias_side_effect(
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

/// Psalm's `constant()` call handling: the constant name comes from a literal
/// string argument (ConstFetchAnalyzer::getConstName also accepts any
/// expression whose inferred type is a single string literal), and the call's
/// type is the resolved constant's type (ConstFetchAnalyzer::getConstType).
/// An unknown name leaves the default (mixed) return type in place.
fn handle_constant_call(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let first_arg = func_call.argument_list.arguments.first()?;

    let const_name = extract_literal_string_value(first_arg.value()).or_else(|| {
        let first_arg_type = analysis_data.expr_types.get(&*arg_positions.first()?)?;
        if let [TAtomic::TLiteralString { value }] = first_arg_type.types.as_slice() {
            Some(value.clone())
        } else {
            None
        }
    })?;

    // Runtime constant names are always fully qualified.
    let const_name = const_name.trim_start_matches('\\');
    if const_name.is_empty() {
        return None;
    }

    let const_id = analyzer
        .interner
        .find(const_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);

    // `define()`-created constants tracked in this scope take precedence,
    // matching the `$context->hasVariable($fq_const_name)` branch of
    // Psalm's getConstType.
    if let Some(runtime_type) = context.defined_constants.get(&const_id) {
        return Some(runtime_type.clone());
    }

    analyzer
        .codebase
        .constants
        .get(&const_id)
        .map(|const_info| const_info.constant_type.clone())
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
        .and_then(|pos| analysis_data.expr_types.get(&*pos).cloned())
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let const_id = analyzer
        .interner
        .find(const_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    context.defined_constants.insert(const_id, const_value_type);
}

/// Mirrors Psalm's `NamedFunctionCallHandler` is_file/file_exists branch: record
/// the argument as a phantom file so a later `include`/`require` of the same
/// expression is not reported Unresolvable/Missing. Psalm prefers the extended
/// var id (`$argv[0]`); failing that it falls back to the statically-resolved
/// path string (literal/const/`__DIR__`).
fn register_phantom_file(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    analysis_data: &FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(first_arg) = func_call.argument_list.arguments.first() else {
        return;
    };
    if first_arg.is_unpacked() {
        return;
    }
    let arg_expr = first_arg.value();

    if let Some(var_id) = crate::expression_identifier::get_expression_var_key(arg_expr) {
        context.phantom_files.insert(var_id.to_string());
        return;
    }

    if let Some(path) =
        crate::expr::include_analyzer::get_path_to(analyzer, arg_expr, analysis_data)
    {
        context.phantom_files.insert(path);
    }
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

            Some(
                analyzer
                    .interner
                    .find(normalized.as_str())
                    .unwrap_or(pzoom_str::StrId::EMPTY),
            )
        }
        Expression::Identifier(identifier) => {
            let offset = identifier.span().start.offset;
            let class_id = analyzer.get_resolved_name(offset).unwrap_or_else(|| {
                analyzer
                    .interner
                    .find(identifier.value())
                    .unwrap_or(pzoom_str::StrId::EMPTY)
            });
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
            analyzer.get_resolved_name(offset).unwrap_or_else(|| {
                analyzer
                    .interner
                    .find(id.value())
                    .unwrap_or(pzoom_str::StrId::EMPTY)
            })
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
                    is_static: false,
                    remapped_params: false,
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

/// The interned id of the first argument when it is a plain `$var`, else `None`.
fn dependent_arg_var_id(
    _analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
) -> Option<VarName> {
    use mago_syntax::ast::ast::variable::Variable;
    let first_arg = func_call.argument_list.arguments.first()?;
    match first_arg.value().unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => Some(VarName::new(direct.name)),
        _ => None,
    }
}

fn is_template_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TTemplateParam { .. }
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TTemplateKeyOf { .. }
            | TAtomic::TTemplateValueOf { .. }
    )
}

fn infer_get_called_class_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
) -> TUnion {
    // Mirror Psalm's FunctionCallReturnTypeFetcher: `get_called_class()` is
    // `class-string<$context->self>` where the constraint is the concrete enclosing
    // class marked as the late-static-bound type (`new TNamedObject($self, true)`).
    if let Some(self_class_id) = analyzer.get_declaring_class().or(context.self_class) {
        return TUnion::new(TAtomic::TClassString {
            as_type: Some(Box::new(TAtomic::TNamedObject {
                name: self_class_id,
                type_params: None,
                is_static: true,
                remapped_params: false,
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
                is_static: false,
                remapped_params: false,
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

    let Some(target_type) = analysis_data.expr_types.get(&target_arg_pos).cloned() else {
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
            } else if class_could_have_undeclared_method(analyzer, *name) {
                // A non-final class (or one with a magic `__call`) may have the method at
                // runtime via a subclass or magic dispatch, so `method_exists` is not
                // provably false - it is `bool`. Matches Psalm.
                (true, true)
            } else {
                (false, true)
            }
        }
        TAtomic::TLiteralClassString { name } => {
            let class_id = analyzer
                .interner
                .find(name.trim_start_matches('\\'))
                .unwrap_or(pzoom_str::StrId::EMPTY);
            if class_or_ancestor_has_method(analyzer, class_id, method_name) {
                (true, false)
            } else if class_could_have_undeclared_method(analyzer, class_id) {
                (true, true)
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
                } else if class_could_have_undeclared_method(analyzer, *name) {
                    (true, true)
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

/// Whether `class_id` could gain a method beyond those declared - i.e. it is not final
/// (a subclass could declare it) or it defines a magic `__call`. Such classes make
/// `method_exists` indeterminate rather than provably false.
fn class_could_have_undeclared_method(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> bool {
    let mut current_class = Some(class_id);
    let mut is_final = false;
    while let Some(current_class_id) = current_class {
        let Some(class_info) = analyzer.codebase.get_class(current_class_id) else {
            return true;
        };
        if current_class_id == class_id {
            is_final = class_info.is_final;
        }
        if class_info.methods.contains_key(&StrId::CALL) {
            return true;
        }
        current_class = class_info.parent_class;
    }
    !is_final
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
    let method_id = analyzer
        .interner
        .find(method_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    // method_exists() reflects PHP runtime semantics, which are
    // case-insensitive, regardless of pzoom's case-sensitive resolution.
    class_info.methods.contains_key(&method_id)
        || class_info
            .cased_method_for(analyzer.interner, method_id)
            .is_some()
        || class_info.pseudo_methods.keys().any(|method_id| {
            analyzer
                .interner
                .lookup(*method_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
        })
        || class_info.pseudo_static_methods.keys().any(|method_id| {
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

/// Psalm's `compact()` handling (NamedFunctionCallHandler): when every
/// argument is a literal string, the call reads the named variables — a
/// missing one reports UndefinedVariable, and the result is the shape of the
/// in-scope variables.
/// Psalm's `NamedFunctionCallHandler` extract() handling: a known
/// keyed-array argument defines (or, under EXTR_SKIP, fills in) the matching
/// variables; anything less knowable stops undefined-variable checking and
/// (without EXTR_SKIP) widens every local to mixed.
fn analyze_extract_call(
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    const EXTR_OVERWRITE: i64 = 0;
    const EXTR_SKIP: i64 = 1;

    let flag_value = if func_call.argument_list.arguments.len() < 2 {
        Some(EXTR_OVERWRITE)
    } else {
        arg_positions
            .get(1)
            .and_then(|flag_pos| analysis_data.expr_types.get(&*flag_pos).cloned())
            .and_then(|flag_type| match flag_type.get_single() {
                Some(TAtomic::TLiteralInt { value }) if *value == EXTR_SKIP => Some(EXTR_SKIP),
                Some(TAtomic::TLiteralInt { value }) if *value == EXTR_OVERWRITE => {
                    Some(EXTR_OVERWRITE)
                }
                _ => None,
            })
    };

    let mut is_unsealed = true;
    let mut validated_var_ids: Vec<pzoom_code_info::VarName> = Vec::new();

    if flag_value.is_some()
        && let Some(array_pos) = arg_positions.first()
        && let Some(array_type) = analysis_data.expr_types.get(&*array_pos).cloned()
        && array_type.types.len() == 1
    {
        if let Some(TAtomic::TArray {
            known_values,
            is_sealed,
            ..
        }) = array_type.get_single()
            // Only a shape (old TKeyedArray) defines named variables.
            && !known_values.is_empty()
        {
            for (key, (_possibly_undefined, value_type)) in known_values.iter() {
                let pzoom_code_info::t_atomic::ArrayKey::String(key) = key else {
                    continue;
                };
                // Variables must start with a letter or underscore.
                if !key
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                {
                    continue;
                }

                let var_id = pzoom_code_info::VarName::from(format!("${}", key));
                validated_var_ids.push(var_id.clone());

                if context.locals.contains_key(&var_id) && flag_value == Some(EXTR_SKIP) {
                    continue;
                }

                let mut assigned_type = value_type.clone();
                assigned_type.possibly_undefined_from_try = false;
                context.locals.insert(var_id, assigned_type);
            }

            if *is_sealed {
                is_unsealed = false;
            }
        }
    }

    if matches!(flag_value, Some(EXTR_OVERWRITE) | Some(EXTR_SKIP)) && !is_unsealed {
        return;
    }

    context.check_variables = false;

    if flag_value == Some(EXTR_SKIP) {
        return;
    }

    // Unknown keys may overwrite anything: every plain local becomes mixed.
    let plain_locals: Vec<pzoom_code_info::VarName> = context
        .locals
        .keys()
        .filter(|var_id| {
            var_id.as_ref() != "$this"
                && !var_id.contains('[')
                && !var_id.contains('>')
                && !validated_var_ids.contains(var_id)
        })
        .cloned()
        .collect();
    for var_id in plain_locals {
        let parent_nodes = context.locals[&var_id].parent_nodes.clone();
        let mut mixed_type = TUnion::mixed();
        mixed_type.parent_nodes = parent_nodes;
        context.locals.insert(var_id, mixed_type);
    }
}

fn analyze_compact_call(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    use pzoom_code_info::ArrayKey;

    let mut properties = rustc_hash::FxHashMap::default();

    for arg in func_call.argument_list.arguments.iter() {
        if arg.is_unpacked() {
            return None;
        }

        let Expression::Literal(Literal::String(string_literal)) = arg.value().unparenthesized()
        else {
            return None;
        };
        let Some(var_name) = string_literal.value else {
            return None;
        };

        let var_type = context
            .locals
            .get(var_name)
            .or_else(|| context.locals.get(format!("${}", var_name).as_str()));

        if let Some(var_type) = var_type {
            properties.insert(
                ArrayKey::String(var_name.to_string()),
                (var_type.possibly_undefined_from_try, (**var_type).clone()),
            );
        } else {
            let span = arg.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedVariable,
                format!("Cannot find referenced variable ${}", var_name),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
    }

    if properties.is_empty() {
        return Some(TUnion::new(TAtomic::array(
            TUnion::string(),
            TUnion::mixed(),
        )));
    }

    Some(TUnion::new(TAtomic::keyed_array(
        properties, false, true, None, None,
    )))
}
