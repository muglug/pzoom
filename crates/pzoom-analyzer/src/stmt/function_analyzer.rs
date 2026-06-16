//! Function declaration analyzer.
//!
//! Analyzes function bodies with proper return type context.

use mago_span::HasSpan;
use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, VarId, VariableSourceKind};
use pzoom_code_info::VarName;
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::attribute_analyzer;
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a function declaration. The enclosing namespace (if any) is read from
/// `context`, so the same entry point serves a top-level or a namespaced function.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func: &Function<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // The enclosing namespace as a string, recovered from `context` (set by the
    // namespace analyzer). `namespace_owned` keeps the interned string alive for
    // the `Option<&str>` borrow used throughout this function.
    let namespace_owned = context.namespace.map(|id| analyzer.interner.lookup(id));
    let namespace = namespace_owned.as_deref();

    // Get the function name - use FQN if in a namespace
    let func_name = func.name.value;
    let fqn = match namespace {
        Some(ns) => format!("{}\\{}", ns, func_name),
        None => func_name.to_string(),
    };

    // Look up the function info from the codebase
    let func_name_id = analyzer.interner.intern(&fqn);
    let mut function_info = analyzer.codebase.get_function(func_name_id);

    // A stored definition at a different location means this declaration
    // redefines an existing function (a sibling in this file, or a PHP core
    // function from the stubs): Psalm's DuplicateFunction. The stored
    // signature belongs to the other definition, so don't analyze this body
    // against it.
    if let Some(info) = function_info
        && (info.file_path != analyzer.file_path
            || info.start_offset != func.span().start.offset
            || analyzer
                .codebase
                .redefined_stub_functions
                .contains(&func_name_id))
        && !analyzer
            .codebase
            .conditionally_skipped_functions
            .contains(&func_name_id)
    {
        let span = func.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::DuplicateFunction,
            format!("Method {} has already been defined", fqn),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
        function_info = None;
    }
    let function_info = function_info;

    // Duplicate parameter names in the signature (Psalm's DuplicateParam).
    {
        let mut seen_param_names: FxHashSet<&str> = FxHashSet::default();
        for param in func.parameter_list.parameters.iter() {
            if !seen_param_names.insert(param.variable.name) {
                let span = param.variable.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::DuplicateParam,
                    format!("Duplicate param {} in {}", param.variable.name, fqn),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
        }
    }

    attribute_analyzer::analyze_function_attributes(analyzer, func, context, analysis_data);
    if let Some(info) = function_info {
        emit_docblock_issues(analyzer, info, analysis_data);
        emit_unused_docblock_params(
            analyzer,
            info,
            &analyzer.interner.lookup(info.name),
            analysis_data,
        );
        check_undefined_docblock_types(analyzer, info, analysis_data);
        check_param_class_casing(analyzer, info, analysis_data);
        check_invalid_param_defaults(analyzer, info, &fqn, analysis_data);
        check_template_param_bounds(analyzer, info, analysis_data);
        check_reserved_signature_words(analyzer, info, namespace, analysis_data);
    }

    // Snapshot top-level variable types for `global $x` lookups inside the
    // body (Psalm's global_context). Only the outermost scope contributes.
    if analyzer.function_info.is_none() {
        for (var_name, var_type) in &context.locals {
            analysis_data
                .file_global_types
                .insert(var_name.clone(), var_type.clone());
        }
    }

    // Create a new analyzer with the function context
    let func_analyzer = analyzer.for_nested_function(function_info);

    // Create a new context for the function body, preserving namespace
    let mut func_context = BlockContext::new();
    func_context.namespace = context.namespace;
    let no_named_arguments = function_info.is_some_and(|info| info.no_named_arguments);

    // Add parameters to context
    for (param_index, param) in func.parameter_list.parameters.iter().enumerate() {
        let param_name = param.variable.name;
        let param_name_id = VarName::new(param_name);

        // Get parameter info from function info
        let param_info =
            function_info.and_then(|info| {
            info.params
                .iter()
                .find(|p| analyzer.interner.lookup(p.name).as_ref() == param_name_id.as_str())
        });

        // Get parameter type - for variadic params, wrap in array type
        let mut param_type = if let Some(info) = param_info {
            // The from_docblock provenance was decided at scan time
            // (FunctionLikeDocblockScanner's typehint-matching rule). Psalm's
            // processParams expands the stored type at function entry
            // (TypeExpander with evaluate_class_constants=true), which resolves
            // class-constant references/wildcards in nested positions.
            let mut base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);
            crate::type_expander::expand_union(
                analyzer.codebase,
                analyzer.interner,
                &mut base_type,
                &crate::type_expander::TypeExpansionOptions::default(),
            );
            if info.is_variadic {
                // Match Psalm: variadics accept named args unless explicitly disabled.
                if no_named_arguments {
                    TUnion::new(TAtomic::TList {
                        value_type: Box::new(base_type),
                    })
                } else {
                    TUnion::new(TAtomic::TArray {
                        key_type: Box::new(TUnion::array_key()),
                        value_type: Box::new(base_type),
                    })
                }
            } else {
                base_type
            }
        } else {
            TUnion::mixed()
        };

        // Call-site argument types are used to narrow a parameter's declared
        // type inside the body, but a templated parameter (e.g. `Collection<T>`)
        // must stay abstract: `T` is universally quantified over every caller, so
        // narrowing it to a single observed call site would be unsound. Psalm
        // analyses such bodies with the abstract template parameter.
        if !crate::type_comparator::generic_type_comparator::union_has_template(&param_type)
            && let Some(callsite_type) =
                analysis_data.function_argument_callsite_types.get(&(func_name_id, param_index))
        {
            param_type =
                assertion_reconciler::intersect_union_with_union(&param_type, callsite_type)
                    .unwrap_or_else(|| callsite_type.clone());
        }

        // Hakana `functionlike_analyzer` seeds a `Param`-id `VariableUseSource`
        // node per parameter. `get_param_source_kind`: inout (≈ PHP by-ref) →
        // InoutParam; closures → ClosureParam; "simple" function-likes (plain
        // functions, private methods) → PrivateParam; else NonPrivateParam.
        let source_kind = if param_info.is_some_and(|info| info.by_ref) {
            VariableSourceKind::InoutParam
        } else {
            // A plain (non-method) function is always `is_simple_fn` in Hakana.
            VariableSourceKind::PrivateParam
        };
        let param_span = param.variable.span();
        let parent_node = crate::data_flow::add_param_dataflow_node(
            &mut analysis_data.data_flow_graph,
            source_kind,
            VarId(analyzer.interner.intern(&param_name_id)),
            make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            Some(&pzoom_code_info::data_flow::node::FunctionLikeIdentifier::Function(func_name_id)),
            param_index,
            param_info.and_then(|info| info.signature_type.as_ref()),
        );
        analysis_data
            .param_sources
            .push(crate::function_analysis_data::ParamSourceInfo {
                node_id: parent_node.id.clone(),
                function_key: func.span().start.offset,
                param_index,
                is_closure: false,
                reportable: true,
                is_promoted: param_info.is_some_and(|info| info.is_promoted),
                by_ref: param_info.is_some_and(|info| info.by_ref),
                function_end: func.span().end.offset,
                name: param_name.to_string(),
                span: (param_span.start.offset, param_span.end.offset),
                method_param_meta: None,
            });
        param_type.parent_nodes.push(parent_node);

        if param_info.is_some_and(|info| info.by_ref) {
            // Writes to a by-ref param are visible to the caller — treat like
            // a by-ref closure use for unused-assignment purposes.
            func_context.mark_external_reference(param_name_id.clone());
        }
        func_context.set_var_type(param_name_id.clone(), param_type.clone());
        if let Some(alt_param_name_id) = get_alternate_param_var_id(analyzer, param_name)
            && alt_param_name_id.as_str() != param_name_id.as_str()
        {
            func_context.set_var_type(alt_param_name_id, param_type.clone());
        }
    }

    // Analyze parameter default expressions in function scope.
    for param in func.parameter_list.parameters.iter() {
        let Some(default_value) = &param.default_value else {
            continue;
        };
        let _ = expression_analyzer::analyze(
            &func_analyzer,
            &default_value.value,
            analysis_data,
            &mut func_context,
        );
    }

    // Analyze the function body
    let yield_types_start = analysis_data.inferred_yield_types.len();
    let return_types_start = analysis_data.inferred_return_types.len();
    let prev_is_generator = analysis_data.current_function_is_generator;
    let body_has_yield = stmt_analyzer::body_contains_yield(func.body.statements.as_slice());
    analysis_data.current_function_is_generator = body_has_yield;
    let saved_var_appearances = std::mem::take(&mut analysis_data.first_var_appearances);
    stmt_analyzer::analyze_stmts(
        &func_analyzer,
        func.body.statements.as_slice(),
        analysis_data,
        &mut func_context,
    )?;
    analysis_data.first_var_appearances = saved_var_appearances;
    analysis_data.current_function_is_generator = prev_is_generator;
    // Syntactic, like Psalm's storage->has_yield: a value-less `yield;` makes
    // a generator without recording an inferred yield type.
    let has_yield =
        body_has_yield || analysis_data.inferred_yield_types.len() > yield_types_start;

    // Hakana `functionlike_analyzer`: a body that falls through without
    // returning still flows by-ref (inout) param values out of the function.
    if !func_context.has_returned {
        crate::stmt::return_analyzer::handle_byref_at_return(
            &func_analyzer,
            analysis_data,
            &func_context,
        );
    }

    if let Some(info) = function_info {
        emit_invalid_by_ref_param_out_types(&func_analyzer, info, &func_context, analysis_data);
        // The statement walk no longer marks `has_returned` after break-less
        // infinite loops (Psalm analyzes the code after them), so the
        // all-paths-leave fact comes from the control-action scan.
        // Psalm's verifyReturnType control-action scan (return distinct from
        // exit, so the no-return / always-exits checks can tell them apart).
        let control_actions = crate::stmt::scope_analyzer::get_control_actions(
            func.body.statements.as_slice(),
            analysis_data,
            &[],
            false,
        );

        let exit_control_actions = crate::stmt::scope_analyzer::get_control_actions(
            func.body.statements.as_slice(),
            analysis_data,
            &[],
            true,
        );

        verify_missing_return_checks(
            &func_analyzer,
            info,
            analysis_data,
            func.name.span().start.offset as u32,
            &fqn,
            has_yield,
            func_context.has_returned,
            analysis_data.inferred_return_types.len() > return_types_start,
            &control_actions,
            &exit_control_actions,
            crate::stmt::scope_analyzer::only_throws(func.body.statements.as_slice()),
            crate::stmt::scope_analyzer::only_throws_or_exits(
                func.body.statements.as_slice(),
                analysis_data,
            ),
            return_types_start,
            yield_types_start,
            None,
            true,
        );
        // Docblock @param vs native signature containment (Psalm's
        // MismatchingDocblockParamType) — same check the class analyzer runs
        // for methods.
        crate::stmt::class_analyzer::check_functionlike_docblock_param_type_mismatches(
            &func_analyzer,
            info,
            None,
            &fqn,
            crate::expr::call::function_call_analyzer::get_template_defaults(info),
            analysis_data,
        );

        maybe_emit_missing_return_type_issue(
            &func_analyzer,
            info,
            analysis_data,
            return_types_start,
            &fqn,
        );

        maybe_emit_missing_param_type_issues(&func_analyzer, info, analysis_data, &fqn);
    }

    // Drop this function's recorded return/yield types so an enclosing
    // function-like (nested named functions are legal PHP) only sees its own
    // returns in the shared vec.
    analysis_data.inferred_return_types.truncate(return_types_start);
    analysis_data.inferred_yield_types.truncate(yield_types_start);

    // Hakana's end-of-functionlike pass: reconcile the type-variable bounds
    // accumulated during this body (closures included — pzoom's shared
    // analysis data is Hakana's bounds merge). Only the outermost
    // function-like reconciles; a nested named function defers to it.
    if analyzer.function_info.is_none() {
        let span = func.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        crate::expr::call_analyzer::check_type_variable_bounds_at_function_end(
            &func_analyzer,
            analysis_data,
            pzoom_code_info::CodeLocation::new(
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ),
        );
    }

    Ok(())
}

/// Psalm's MissingParamType for plain functions (the method version lives in
/// class_analyzer): an untyped param with neither signature nor docblock type.
fn maybe_emit_missing_param_type_issues(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    function_name: &str,
) {
    let has_assertions = !function_info.assertions.is_empty()
        || !function_info.if_true_assertions.is_empty()
        || !function_info.if_false_assertions.is_empty();
    if has_assertions {
        return;
    }
    // Psalm types a param that a conditional `@return ($p is X ? ...)` or
    // `properties-of<$p>` references (it becomes a function template), so
    // MissingParamType does not fire for it; pzoom keeps the param untyped
    // and resolves the conditional at the call, so mirror the skip here.
    fn return_type_references_param(union: &TUnion, param_names: &[pzoom_str::StrId]) -> bool {
        union.types.iter().any(|atomic| match atomic {
            TAtomic::TConditional(conditional) => {
                param_names.contains(&conditional.param_name)
                    || return_type_references_param(&conditional.if_true_type, param_names)
                    || return_type_references_param(&conditional.if_false_type, param_names)
            }
            TAtomic::TTemplateParam { name, .. } => param_names.contains(name),
            TAtomic::TTemplatePropertiesOf {
                param_name: subject,
                ..
            } => param_names.contains(subject),
            // `properties-of<$a>` parses the `$a` subject as a named object
            // before the resolution pass rewrites it to the template form.
            TAtomic::TPropertiesOf { classlike_name, .. } => {
                param_names.contains(classlike_name)
            }
            _ => false,
        })
    }
    for param in &function_info.params {
        // Docblock references spell the param either bare or as `$name`.
        let mut param_names = vec![param.name];
        let bare = analyzer.interner.lookup(param.name);
        let with_sigil = format!("${}", bare.trim_start_matches('$'));
        if let Some(id) = analyzer.interner.find(&with_sigil) {
            param_names.push(id);
        }
        if let Some(id) = analyzer.interner.find(bare.trim_start_matches('$')) {
            param_names.push(id);
        }
        let docblock_types_param = function_info
            .return_type
            .as_ref()
            .is_some_and(|return_type| return_type_references_param(return_type, &param_names));
        if param.signature_type.is_none() && param.param_type.is_none() && !docblock_types_param {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingParamType,
                format!(
                    "Argument {} of {} does not have a type",
                    analyzer.interner.lookup(param.name),
                    function_name
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn maybe_emit_missing_return_type_issue(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    return_types_start: usize,
    function_name: &str,
) {
    if function_info.signature_return_type.is_some() || function_info.return_type.is_some() {
        return;
    }

    let has_invalid_or_missing_return_docblock =
        function_info.docblock_issues.iter().any(|issue| {
            issue
                .message
                .eq_ignore_ascii_case("Missing return docblock type")
                || issue
                    .message
                    .eq_ignore_ascii_case("Invalid return docblock type")
        });

    // With no return type node, the issue points at the function name
    // (Psalm's name location).
    let (issue_start, issue_end) = function_info
        .name_location
        .unwrap_or((function_info.start_offset, function_info.end_offset));

    if has_invalid_or_missing_return_docblock {
        let (line, col) = analyzer.get_line_column(issue_start);
        analysis_data.add_issue(Issue::new(
            IssueKind::MissingReturnType,
            format!("Function {} does not have a return type", function_name),
            analyzer.file_path,
            issue_start,
            issue_end,
            line,
            col,
        ));
        return;
    }

    // Empty inferred set yields `void`, which the guard below treats as "no
    // missing-return-type issue", matching the previous early return.
    let inferred_return_type = analysis_data.combine_inferred_return_types(return_types_start);

    if inferred_return_type.is_void()
        || inferred_return_type.is_nothing()
        || inferred_return_type.is_mixed()
        || inferred_return_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TNonEmptyMixed))
    {
        return;
    }

    let (line, col) = analyzer.get_line_column(issue_start);
    analysis_data.add_issue(Issue::new(
        IssueKind::MissingReturnType,
        format!("Function {} does not have a return type", function_name),
        analyzer.file_path,
        issue_start,
        issue_end,
        line,
        col,
    ));
}

/// Psalm `ReturnTypeAnalyzer::verifyReturnType`'s no-return / never checks
/// (the inferred-vs-declared comparison part is handled per return statement).
///
/// Sequence (each emits at most one issue, mirroring Psalm's early returns):
/// 1. implicit fall-through with a native (or signature-identical docblock /
///    native-nullable) declared type — "Not all code paths ...";
/// 2. declared `never` but the body can return;
/// 3. no `return` statements at all but a return type was declared;
/// 4. the body always exits but the declared type is not `never`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_missing_return_checks(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    issue_offset: u32,
    cased_name: &str,
    has_yield: bool,
    has_returned: bool,
    has_return_statement: bool,
    control_actions: &rustc_hash::FxHashSet<crate::stmt::scope_analyzer::ControlAction>,
    exit_control_actions: &rustc_hash::FxHashSet<crate::stmt::scope_analyzer::ControlAction>,
    only_throws: bool,
    only_throws_or_exits: bool,
    return_types_start: usize,
    yield_types_start: usize,
    class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    check_less_specific_type: bool,
) {
    use crate::stmt::scope_analyzer::ControlAction;

    let parent_class_exists = class_info.is_some_and(|info| info.parent_class.is_some());

    if analyzer
        .codebase
        .files
        .get(&analyzer.file_path)
        .is_some_and(|file_info| file_info.is_stub)
    {
        return;
    }

    if function_info.is_abstract {
        return;
    }

    let Some(declared) = function_info.get_return_type() else {
        return;
    };
    // A docblock @return is stored in `return_type` (signature-only types in
    // `signature_return_type`); the union's own flag is unreliable after
    // conditional-type resolution.
    let declared_from_docblock = function_info.return_type.is_some();
    // Expand any conditional return type to the union of its branches before deciding
    // whether a return value is required (a void branch makes it optional).
    let mut declared = declared.clone();
    crate::type_expander::bind_properties_of_self_names(
        &mut declared,
        class_info.map(|info| info.name),
        class_info.and_then(|info| info.parent_class),
    );
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut declared,
        &crate::type_expander::TypeExpansionOptions { evaluate_conditional_types: true, ..Default::default() },
    );
    let declared = &declared;

    // Psalm drops a docblock type identical to the signature type at scan time
    // ("dontOverrideSameType"), so such a type behaves as native here.
    let treat_as_native = !declared_from_docblock
        || function_info.signature_return_type.as_ref().is_some_and(|signature_type| {
            signature_type.get_id(Some(analyzer.interner)) == declared.get_id(Some(analyzer.interner))
        });

    let is_nullable = declared.is_nullable()
        || declared.types.iter().any(|atomic| matches!(atomic, TAtomic::TNull));
    let null_from_docblock = if declared.docblock_bits_valid() {
        declared
            .types
            .iter()
            .enumerate()
            .any(|(index, atomic)| {
                matches!(atomic, TAtomic::TNull) && declared.atomic_from_docblock(index)
            })
    } else {
        declared.from_docblock
    };
    let declared_void = declared.is_void()
        || declared
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TVoid));

    let function_always_exits =
        control_actions.len() == 1 && control_actions.contains(&ControlAction::End);
    // Fall-through detection uses the return-as-exit scan (pzoom's switch /
    // continue handling is calibrated for it); the return-distinct scan above
    // only feeds the always-exits / has-return distinctions.
    // The statement walk's has_returned (switch exhaustiveness, loop
    // analysis) is more precise than the syntactic scan; trust it as a veto.
    let function_returns_implicitly =
        exit_control_actions.contains(&ControlAction::None) && !has_returned;
    // `has_return_statement` mirrors Psalm's $inferred_return_type_parts (the
    // collected return-statement types), which sees through constructs the
    // syntactic action scan handles conservatively (switch fallthrough, deep
    // loops). The scan's Return action still helps as a fallback.
    let has_return_statement =
        has_return_statement || control_actions.contains(&ControlAction::Return);

    // Issues about the declared return type point at the return type node
    // (signature hint or docblock @return) when one exists, like Psalm's
    // `$storage->return_type_location`; otherwise at the function name.
    let (issue_start, issue_end) = function_info
        .return_type_location
        .unwrap_or((issue_offset, issue_offset + 1));
    let (line, col) = analyzer.get_line_column(issue_start);
    let emit = |kind: IssueKind, message: String, analysis_data: &mut FunctionAnalysisData| {
        analysis_data.add_issue(Issue::new(
            kind,
            message,
            analyzer.file_path,
            issue_start,
            issue_end,
            line,
            col,
        ));
    };

    // 1. Implicit fall-through (Psalm: "Not all code paths ... end in a
    //    return statement, return type X expected").
    let has_template = declared
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }));
    let signature_nullable = function_info
        .signature_return_type
        .as_ref()
        .is_some_and(|signature_type| {
            signature_type.is_nullable()
                || signature_type
                    .types
                    .iter()
                    .any(|atomic| matches!(atomic, TAtomic::TNull))
        });
    let _ = null_from_docblock;
    // Psalm's check fires for native declared types, and for docblock types
    // whose nullability comes from the signature. The third arm proxies
    // Psalm's inferred-comparison path (implicit fall-through adds `null` to
    // the inferred union, failing containment in a non-nullable docblock
    // type → InvalidNullableReturnType).
    let psalm_check_one = treat_as_native || (is_nullable && !has_template && signature_nullable);
    let implicit_null_mismatch =
        has_return_statement && !is_nullable && !declared.is_mixed() && declared_from_docblock;
    if (psalm_check_one || implicit_null_mismatch)
        && !declared_void
        && !declared.is_nothing()
        && function_returns_implicitly
        && !has_yield
    {
        let issue_kind = if function_info.signature_return_type.is_some() {
            IssueKind::InvalidReturnType
        } else {
            IssueKind::InvalidNullableReturnType
        };
        emit(
            issue_kind,
            format!(
                "Not all code paths of {} end in a return statement, return type {} expected",
                cased_name,
                declared.get_id(Some(analyzer.interner))
            ),
            analysis_data,
        );
        return;
    }

    // 2. Declared never, but the body can return.
    if declared.is_nothing() && !function_always_exits && !has_yield {
        emit(
            IssueKind::InvalidReturnType,
            format!(
                "{} is not expected to return, but it does, either implicitly or explicitly",
                cased_name
            ),
            analysis_data,
        );
        return;
    }

    // 3. No return statements at all (and the body does not always exit —
    //    Psalm's inferred type is `never` then, handled by check 4).
    if !has_return_statement
        && !function_always_exits
        && !declared_void
        && !declared.is_nothing()
        && !has_yield
    {
        if only_throws_or_exits {
            // A lone `throw` presumably documents the method as unusable.
            return;
        }
        if treat_as_native || !is_nullable {
            emit(
                IssueKind::InvalidReturnType,
                format!(
                    "No return statements were found for method {} but return type '{}' was expected",
                    cased_name,
                    declared.get_id(Some(analyzer.interner))
                ),
                analysis_data,
            );
        }
        return;
    }

    // 4. Body always exits but the declared type is not never.
    // In practice Psalm only flags types declared *solely* via docblock here:
    // any native return hint stays quiet even when a docblock accompanies it
    // (`function foo(): array { exit; }` with `@return string[]` passes —
    // Psalm's allowImplicitNever; verified against the reference checkout).
    if !declared.is_nothing()
        && function_always_exits
        && declared_from_docblock
        && function_info.signature_return_type.is_none()
        && !only_throws
        && !has_yield
    {
        emit(
            IssueKind::InvalidReturnType,
            format!(
                "The declared return type '{}' for {} is incorrect, got 'never'",
                declared.get_id(Some(analyzer.interner)),
                cased_name
            ),
            analysis_data,
        );
        return;
    }

    // Psalm ReturnTypeAnalyzer::verifyReturnType — the function-end inferred
    // vs declared containment check (InvalidReturnType / MoreSpecificReturnType
    // / MixedReturnTypeCoercion / LessSpecificReturnType). The per-statement
    // InvalidReturnStatement check sees individual returns; this one sees the
    // combined inferred type, which is the only check that covers generators
    // (yield-aggregated Generator types never pass through a return statement).
    let mut inferred_parts: Vec<TUnion> =
        analysis_data.inferred_return_types[return_types_start..].to_vec();
    if function_returns_implicitly {
        inferred_parts.push(TUnion::void());
    }
    // Psalm filters TNever parts that have no bearing on the combined type.
    if inferred_parts.len() > 1 {
        inferred_parts.retain(|part| !part.is_nothing());
    }
    let mut inferred = inferred_parts
        .iter()
        .skip(1)
        .fold(
            inferred_parts
                .first()
                .cloned()
                .unwrap_or_else(TUnion::void),
            |combined, part| {
                pzoom_code_info::ttype::type_combiner::combine_union_types(&combined, part, false)
            },
        );
    if function_always_exits {
        inferred = TUnion::nothing();
    }

    let yield_parts = &analysis_data.inferred_yield_types[yield_types_start..];
    if !yield_parts.is_empty() {
        let mut key_type: Option<TUnion> = None;
        let mut value_type: Option<TUnion> = None;
        for (yield_key, yield_value) in yield_parts {
            let yield_key = yield_key.clone().unwrap_or_else(TUnion::int);
            key_type = Some(match key_type {
                Some(existing) => pzoom_code_info::ttype::type_combiner::combine_union_types(
                    &existing, &yield_key, false,
                ),
                None => yield_key,
            });
            value_type = Some(match value_type {
                Some(existing) => pzoom_code_info::ttype::type_combiner::combine_union_types(
                    &existing,
                    yield_value,
                    false,
                ),
                None => yield_value.clone(),
            });
        }
        // The generator's return slot is the combined `return` statement type.
        let generator_return = inferred;
        inferred = TUnion::new(TAtomic::TNamedObject {
            name: StrId::GENERATOR,
            type_params: Some(vec![
                key_type.unwrap_or_else(TUnion::mixed),
                value_type.unwrap_or_else(TUnion::mixed),
                TUnion::mixed(),
                generator_return,
            ]),
            is_static: false,
            remapped_params: false,
        });
    }

    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut inferred,
        &crate::type_expander::TypeExpansionOptions {
            evaluate_conditional_types: true,
            ..Default::default()
        },
    );

    // Psalm's TypeExpander binds `static`/`self` to the concrete class when the
    // class (or method) is final — `: static` on a final class accepts the
    // class itself.
    let localized_declared;
    let declared = if let Some(class_info) = class_info
        && (class_info.is_final || function_info.is_final)
    {
        localized_declared = crate::stmt::class_analyzer::localize_special_class_names_for_final_class(
            declared,
            class_info.name,
            class_info.parent_class,
        );
        &localized_declared
    } else {
        declared
    };

    // Psalm's hasMixed covers the whole mixed family (non-empty-mixed etc.).
    let union_has_mixed = |union: &TUnion| {
        union
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
    };
    if union_has_mixed(declared) || union_has_mixed(&inferred) {
        return;
    }
    if inferred.is_void() && (declared_void || has_yield) {
        return;
    }

    let mut comparison_result = crate::type_comparator::TypeComparisonResult::new();
    if !crate::type_comparator::union_type_comparator::is_contained_by(
        analyzer.codebase,
        &inferred,
        declared,
        true,
        true,
        &mut comparison_result,
    ) {
        if comparison_result.type_coerced.unwrap_or(false) {
            if comparison_result.type_coerced_from_mixed.unwrap_or(false) {
                if !comparison_result
                    .type_coerced_from_as_mixed
                    .unwrap_or(false)
                {
                    emit(
                        IssueKind::MixedReturnTypeCoercion,
                        format!(
                            "The declared return type '{}' for {} is more specific than the inferred return type '{}'",
                            declared.get_id(Some(analyzer.interner)),
                            cased_name,
                            inferred.get_id(Some(analyzer.interner))
                        ),
                        analysis_data,
                    );
                }
            } else {
                emit(
                    IssueKind::MoreSpecificReturnType,
                    format!(
                        "The declared return type '{}' for {} is more specific than the inferred return type '{}'",
                        declared.get_id(Some(analyzer.interner)),
                        cased_name,
                        inferred.get_id(Some(analyzer.interner))
                    ),
                    analysis_data,
                );
            }
        } else if !is_nullable || !parent_class_exists {
            emit(
                IssueKind::InvalidReturnType,
                format!(
                    "The declared return type '{}' for {} is incorrect, got '{}'",
                    declared.get_id(Some(analyzer.interner)),
                    cased_name,
                    inferred.get_id(Some(analyzer.interner))
                ),
                analysis_data,
            );
        }
    } else if check_less_specific_type
        && !crate::type_comparator::union_type_comparator::is_contained_by(
            analyzer.codebase,
            declared,
            &inferred,
            false,
            false,
            &mut crate::type_comparator::TypeComparisonResult::new(),
        )
    {
        let inferred_nullable = inferred.is_nullable()
            || inferred
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TNull));
        let inferred_falsable = inferred
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TFalse));
        let declared_falsable = declared
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TFalse));
        // Psalm only reports redundant nullability/falsability by default
        // (Config::restrict_return_types widens this to any mismatch).
        if (!inferred_nullable && is_nullable) || (!inferred_falsable && declared_falsable) {
            emit(
                IssueKind::LessSpecificReturnType,
                format!(
                    "The inferred return type '{}' for {} is more specific than the declared return type '{}'",
                    inferred.get_id(Some(analyzer.interner)),
                    cased_name,
                    declared.get_id(Some(analyzer.interner))
                ),
                analysis_data,
            );
        }
    }

    // Psalm ReturnTypeAnalyzer's independent declaration checks, run
    // regardless of the containment outcome: a nullable/falsable inferred
    // return against a declaration that allows neither.
    if !inferred.ignore_nullable_issues
        && inferred.is_nullable()
        && !declared.is_nullable()
        && !declared
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
        && !declared.is_void()
    {
        emit(
            IssueKind::InvalidNullableReturnType,
            format!(
                "The declared return type '{}' for {} is not nullable, but '{}' contains null",
                declared.get_id(Some(analyzer.interner)),
                cased_name,
                inferred.get_id(Some(analyzer.interner))
            ),
            analysis_data,
        );
    }

    if !inferred.ignore_falsable_issues
        && inferred.is_falsable()
        && !declared.is_falsable()
        && !declared
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
        && !declared
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TScalar))
    {
        emit(
            IssueKind::InvalidFalsableReturnType,
            format!(
                "The declared return type '{}' for {} does not allow false, but '{}' contains false",
                declared.get_id(Some(analyzer.interner)),
                cased_name,
                inferred.get_id(Some(analyzer.interner))
            ),
            analysis_data,
        );
    }
}

/// Psalm's FunctionLikeAnalyzer ReservedWord checks: `void`/`never` cannot be
/// parameter types, and a `@template` name must not shadow an existing
/// class/interface.
fn check_reserved_signature_words(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &function_info.params {
        let Some(param_type) = param.get_type() else {
            continue;
        };

        let reserved = if param_type.is_void() {
            Some("void")
        } else if param_type.is_nothing() {
            Some("never")
        } else {
            None
        };

        if let Some(reserved) = reserved {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ReservedWord,
                format!("Parameter cannot be {}", reserved),
                analyzer.file_path,
                param.start_offset,
                param.start_offset,
                line,
                col,
            ));
        }
    }

    // Version-gated native signature types (Psalm gates these on
    // analysis_php_version_id at scan time; pzoom checks the collected
    // signature types): `void` needs 7.1, `object` 7.2, `never` 8.1;
    // native unions need 8.0; native intersections 8.1 (with class-like
    // members only); DNF (intersections inside unions) needs 8.2.
    let php_version_id = analyzer.config.php_version_id();
    let mut signature_types: Vec<(&TUnion, u32)> = Vec::new();
    for param in &function_info.params {
        if let Some(signature_type) = &param.signature_type {
            signature_types.push((signature_type, param.start_offset));
        }
    }
    if let Some(signature_return_type) = &function_info.signature_return_type {
        signature_types.push((signature_return_type, function_info.start_offset));
    }

    for (signature_type, offset) in signature_types {
        let reserved = if php_version_id < 70100 && signature_type.is_void() {
            Some("void")
        } else if php_version_id < 70200
            && signature_type
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TObject))
        {
            Some("object")
        } else if php_version_id < 80100 && signature_type.is_nothing() {
            Some("never")
        } else {
            None
        };

        if let Some(reserved) = reserved {
            let (line, col) = analyzer.get_line_column(offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ReservedWord,
                format!(
                    "{} is only supported in newer PHP versions",
                    reserved
                ),
                analyzer.file_path,
                offset,
                offset,
                line,
                col,
            ));
            continue;
        }

        let non_null_members = signature_type
            .types
            .iter()
            .filter(|atomic| !matches!(atomic, TAtomic::TNull))
            .count();
        let has_intersection = signature_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TObjectIntersection { .. }));
        let intersection_has_non_class = signature_type.types.iter().any(|atomic| {
            matches!(atomic, TAtomic::TObjectIntersection { types }
                if types.iter().any(|member| !matches!(
                    member,
                    TAtomic::TNamedObject { .. } | TAtomic::TTemplateParam { .. }
                )))
        });

        let parse_error = if has_intersection && php_version_id < 80100 {
            Some("intersection types are only supported in PHP 8.1 and newer")
        } else if has_intersection && intersection_has_non_class {
            Some("intersection types can only be composed of class and interface types")
        } else if has_intersection && signature_type.types.len() > 1 && php_version_id < 80200 {
            // `(A&B)|C` and `(A&B)|null` are DNF syntax (PHP 8.2); plain
            // nullable intersections don't exist before that either.
            Some("DNF types are only supported in PHP 8.2 and newer")
        } else if non_null_members > 1 && php_version_id < 80000 {
            Some("union types are only supported in PHP 8.0 and newer")
        } else {
            None
        };

        if let Some(parse_error) = parse_error {
            let (line, col) = analyzer.get_line_column(offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ParseError,
                parse_error,
                analyzer.file_path,
                offset,
                offset,
                line,
                col,
            ));
        }
    }

    for template_type in &function_info.template_types {
        // Psalm resolves the template name like a class reference
        // (`getFQCLNFromString` with the file's aliases): unqualified names
        // resolve inside the current namespace, falling back to the global
        // scope for built-ins.
        let shadows_class = analyzer.codebase.class_exists(template_type.name)
            || namespace
                .and_then(|ns| {
                    analyzer.interner.find(&format!(
                        "{}\\{}",
                        ns,
                        analyzer.interner.lookup(template_type.name)
                    ))
                })
                .is_some_and(|fq_id| analyzer.codebase.class_exists(fq_id));

        if shadows_class {
            let (line, col) = analyzer.get_line_column(function_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ReservedWord,
                format!(
                    "Cannot use {} as template name since the class already exists",
                    analyzer.interner.lookup(template_type.name)
                ),
                analyzer.file_path,
                function_info.start_offset,
                function_info.end_offset,
                line,
                col,
            ));
        }
    }
}

/// Validates that the type arguments of a generic type used in the signature
/// satisfy their template parameters' bounds, including dependent bounds such as
/// `@template B of AType<T>` where an earlier argument is substituted into the
/// bound before comparison. Mirrors Psalm's `InvalidTemplateParam`.
fn check_template_param_bounds(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &function_info.params {
        if let Some(param_type) = param.get_type() {
            for atomic in &param_type.types {
                check_atomic_template_bounds(analyzer, atomic, param.start_offset, analysis_data);
            }
        }
    }

    if let Some(return_type) = &function_info.return_type {
        let offset = function_info.start_offset;
        for atomic in &return_type.types {
            check_atomic_template_bounds(analyzer, atomic, offset, analysis_data);
        }
    }
}

fn check_atomic_template_bounds(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    offset: u32,
    analysis_data: &mut FunctionAnalysisData,
) {
    let TAtomic::TNamedObject {
        name,
        type_params: Some(type_params),
        ..
    } = atomic
    else {
        return;
    };

    if let Some(class_info) = analyzer.codebase.get_class(*name) {
        // Map each template parameter name to the supplied argument so dependent
        // bounds (`B of AType<T>`) can be resolved.
        let mut substitutions = pzoom_code_info::TemplateResult::default();
        for (index, template_type) in class_info.template_types.iter().enumerate() {
            if let Some(type_param) = type_params.get(index) {
                crate::template::lower_bounds_insert(
                    &mut substitutions,
                    template_type.name,
                    template_type.defining_entity,
                    type_param.clone(),
                );
            }
        }
        for (index, template_type) in class_info.template_types.iter().enumerate() {
            let Some(type_param) = type_params.get(index) else {
                continue;
            };
            if template_type.as_type.is_mixed() {
                continue;
            }

            let effective_bound = crate::expr::call::function_call_analyzer::replace_templates_in_union(
                &template_type.as_type,
                &substitutions,
            );
            if effective_bound.is_mixed() {
                continue;
            }

            let mut comparison_result = TypeComparisonResult::new();
            let is_contained = union_type_comparator::is_contained_by(
                analyzer.codebase,
                type_param,
                &effective_bound,
                false,
                false,
                &mut comparison_result,
            );

            // Psalm's TypeChecker::checkGenericParams reports on a plain
            // `!isContainedBy` — a coerced match is still outside the bound
            // (`Generic<T>` with `T as mixed` against `T as object` is an
            // InvalidTemplateParam). pzoom keeps the coercion exemption only
            // for non-template args, where its comparator marks benign
            // literal/parent matches as coercions that Psalm accepts.
            let arg_mentions_template = crate::type_comparator::generic_type_comparator::union_has_template(type_param);
            if !is_contained
                && (arg_mentions_template || !comparison_result.type_coerced.unwrap_or(false))
            {
                let (line, col) = analyzer.get_line_column(offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidTemplateParam,
                    format!(
                        "Type {} is not within the bound {} of template param {} of {}",
                        type_param.get_id(Some(analyzer.interner)),
                        effective_bound.get_id(Some(analyzer.interner)),
                        analyzer.interner.lookup(template_type.name),
                        analyzer.interner.lookup(*name),
                    ),
                    analyzer.file_path,
                    offset,
                    offset.saturating_add(1),
                    line,
                    col,
                ));
            }
        }
    }

    // Recurse into the type arguments, which may themselves be generic.
    for type_param in type_params {
        for inner in &type_param.types {
            check_atomic_template_bounds(analyzer, inner, offset, analysis_data);
        }
    }
}

pub(crate) fn check_param_class_casing(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted: FxHashSet<(u32, StrId)> = FxHashSet::default();

    let mut signature_unions: Vec<(&TUnion, u32, bool, Option<&TUnion>)> = Vec::new();
    for param in &function_info.params {
        if let Some(param_type) = param.signature_type.as_ref().or(param.param_type.as_ref()) {
            let docblock_union = if param.has_docblock_type {
                param.param_type.as_ref()
            } else {
                None
            };
            signature_unions.push((
                param_type,
                param.start_offset,
                param.signature_type.is_some(),
                docblock_union,
            ));
        }
    }
    if let Some(signature_return_type) = function_info.signature_return_type.as_ref() {
        signature_unions.push((
            signature_return_type,
            function_info.start_offset,
            true,
            function_info.return_type.as_ref(),
        ));
    }

    for (param_type, start_offset, is_native_signature, docblock_union) in signature_unions {
        for atomic in &param_type.types {
            // A native-signature `resource` is a reserved word (Psalm's
            // TypeChecker::checkResource — only docblocks may use the
            // pseudo-type).
            if is_native_signature
                && matches!(atomic, TAtomic::TResource)
                && !param_type.from_docblock
            {
                let resource_id = analyzer.interner.intern("resource");
                if emitted.insert((start_offset, resource_id)) {
                    let (line, col) = analyzer.get_line_column(start_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::ReservedWord,
                        "resource is a reserved word",
                        analyzer.file_path,
                        start_offset,
                        start_offset + 1,
                        line,
                        col,
                    ));
                }
                continue;
            }

            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };

            if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                continue;
            }

            if let Some(class_info) = analyzer.codebase.get_class(*name) {
                if class_info.is_deprecated && emitted.insert((start_offset, *name)) {
                    let class_name = analyzer.interner.lookup(*name);
                    let (line, col) = analyzer.get_line_column(start_offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedClass,
                        format!("{} is marked deprecated", class_name),
                        analyzer.file_path,
                        start_offset,
                        start_offset + 1,
                        line,
                        col,
                    ));
                }
                continue;
            }

            // Names are resolved case-sensitively; a wrong-cased hint is an
            // undefined class, with the declared casing named in the message.
            // With no casing match, a native signature naming a missing class
            // is Psalm's "Class, interface or enum named X does not exist".
            let message = match crate::class_casing::class_casing_hint(analyzer, *name) {
                Some(_) => crate::class_casing::undefined_class_message(
                    analyzer,
                    analyzer.interner.lookup(*name),
                ),
                None => {
                    if !is_native_signature {
                        continue;
                    }
                    // Psalm checks the effective (docblock-first) type, so a
                    // docblock type naming the same missing class reports
                    // UndefinedDocblockClass instead of flagging the native
                    // hint separately.
                    if docblock_union.is_some_and(|docblock| {
                        docblock.types.iter().any(|docblock_atomic| {
                            matches!(docblock_atomic, TAtomic::TNamedObject { name: docblock_name, .. } if docblock_name == name)
                        })
                    }) {
                        continue;
                    }
                    format!(
                        "Class, interface or enum named {} does not exist",
                        analyzer.interner.lookup(*name)
                    )
                }
            };

            if !emitted.insert((start_offset, *name)) {
                continue;
            }

            let (line, col) = analyzer.get_line_column(start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                message,
                analyzer.file_path,
                start_offset,
                start_offset + 1,
                line,
                col,
            ));
        }
    }

    // Docblock @return types naming a wrong-cased class: Psalm's TypeChecker
    // reports InvalidClass "has wrong casing" (resolution itself stays
    // case-sensitive — a pzoom departure — so mismatch issues may follow).
    if let Some(return_type) = function_info.return_type.as_ref() {
        let (start_offset, end_offset) = function_info
            .return_type_location
            .unwrap_or((function_info.start_offset, function_info.start_offset + 1));
        let mut wrong_cased: Vec<StrId> = Vec::new();
        for atomic in &return_type.types {
            collect_wrong_cased_classes(analyzer, atomic, &mut wrong_cased);
        }
        for name in wrong_cased {
            if !emitted.insert((start_offset, name)) {
                continue;
            }
            let (line, col) = analyzer.get_line_column(start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidClass,
                format!(
                    "Class, interface or enum {} has wrong casing",
                    analyzer.interner.lookup(name)
                ),
                analyzer.file_path,
                start_offset,
                end_offset,
                line,
                col,
            ));
        }
    }

    // A `@deprecated` class/interface named anywhere in the docblock `@return`
    // type — nested inside `Foo[]`, generics or shapes included — reports
    // Deprecated{Class,Interface}, mirroring Psalm's TypeChecker type-visitor
    // (`checkNamedObject`) which the native-signature walk above (top-level
    // atomics only) can't reach for a docblock-only `@return Foo[]`. Psalm skips
    // this for an *inherited* return type (ReturnTypeAnalyzer passes
    // `$storage->inherited_return_type`), so the notice fires on the declaring
    // method, not again on every override (the noNoticeOnInheritance case).
    // A `@deprecated` class/interface named anywhere in the docblock `@return`
    // type — nested in `Foo[]`, generics or shapes included — reports
    // Deprecated{Class,Interface}, mirroring Psalm's ReturnTypeAnalyzer running
    // the TypeChecker visitor on the declared return type (for abstract and
    // interface methods too). The native-signature walk above only inspects
    // top-level atomics, so the docblock-only nested case lands here. Skipped
    // for an inherited return type (Psalm's `inherited_return_type`) so the
    // notice fires on the declaring method, not again on every override.
    if let Some(return_type) = function_info.return_type.as_ref() {
        if !function_info.inherited_return_type {
            let mut named_classes = Vec::new();
            pzoom_code_info::ttype::visit_type_tree(
                &pzoom_code_info::ttype::TypeNode::Union(return_type),
                &mut |node| {
                    if let pzoom_code_info::ttype::TypeNode::Atomic(TAtomic::TNamedObject {
                        name,
                        ..
                    }) = node
                    {
                        named_classes.push(*name);
                    }
                    true
                },
            );
            for name in named_classes {
                emit_docblock_type_deprecation(
                    analyzer,
                    analysis_data,
                    &mut emitted,
                    name,
                    function_info.declaring_class,
                    function_info.start_offset,
                );
            }
        }
    }

    // Return hints get the same treatment as param hints.
    if let Some(return_type) = function_info.signature_return_type.as_ref() {
        for atomic in &return_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };

            if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                continue;
            }

            if analyzer.codebase.get_class(*name).is_some() {
                continue;
            }

            let Some(actual_id) = crate::class_casing::class_casing_hint(analyzer, *name) else {
                continue;
            };

            if !emitted.insert((function_info.start_offset, actual_id)) {
                continue;
            }

            let (line, col) = analyzer.get_line_column(function_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                crate::class_casing::undefined_class_message(
                    analyzer,
                    analyzer.interner.lookup(*name),
                ),
                analyzer.file_path,
                function_info.start_offset,
                function_info.start_offset + 1,
                line,
                col,
            ));
        }
    }
}

/// Collects class names referenced anywhere in `atomic`'s type tree — generic
/// params, array element/key types, shape fields, callable params/returns,
/// template bounds, class-strings — that fail exact lookup but match a declared
/// classlike case-insensitively (Psalm's wrong-casing check). Walks the shared
/// [`pzoom_code_info::ttype::TypeNode`] recursion.
fn collect_wrong_cased_classes(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    wrong_cased: &mut Vec<StrId>,
) {
    pzoom_code_info::ttype::visit_type_tree(
        &pzoom_code_info::ttype::TypeNode::Atomic(atomic),
        &mut |node| {
            if let pzoom_code_info::ttype::TypeNode::Atomic(TAtomic::TNamedObject {
                name, ..
            }) = node
                && !matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT)
                && analyzer.codebase.get_class(*name).is_none()
                && crate::class_casing::class_casing_hint(analyzer, *name).is_some()
                && !wrong_cased.contains(name)
            {
                wrong_cased.push(*name);
            }
            true
        },
    );
}

/// Reports Deprecated{Class,Interface} for a class named in a docblock type,
/// mirroring Psalm's `TypeChecker::checkNamedObject`: `self`/`static`/`parent`
/// and a self-referential name (Psalm's `getFQCLN() !== value`) are skipped,
/// and only a known, `@deprecated` class fires. Interfaces report
/// DeprecatedInterface, everything else DeprecatedClass (Psalm draws no trait
/// distinction in the type visitor). `emitted` dedups against the
/// native-signature walk so a class named by both hints reports once.
fn emit_docblock_type_deprecation(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    emitted: &mut FxHashSet<(u32, StrId)>,
    name: StrId,
    self_class: Option<StrId>,
    start_offset: u32,
) {
    if matches!(name, StrId::SELF | StrId::STATIC | StrId::PARENT) || self_class == Some(name) {
        return;
    }
    let Some(referenced_info) = analyzer.codebase.get_class(name) else {
        return;
    };
    if !referenced_info.is_deprecated || !emitted.insert((start_offset, name)) {
        return;
    }
    let issue_kind =
        if referenced_info.kind == pzoom_code_info::class_like_info::ClassLikeKind::Interface {
            IssueKind::DeprecatedInterface
        } else {
            IssueKind::DeprecatedClass
        };
    let (line, col) = analyzer.get_line_column(start_offset);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!("{} is marked deprecated", analyzer.interner.lookup(name)),
        analyzer.file_path,
        start_offset,
        start_offset + 1,
        line,
        col,
    ));
}

fn emit_docblock_issues(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for issue in &function_info.docblock_issues {
        let (line, col) = analyzer.get_line_column(issue.start_offset);
        let issue_kind = if issue.message.eq_ignore_ascii_case("Missing docblock type") {
            IssueKind::MissingDocblockType
        } else if issue
            .message
            .eq_ignore_ascii_case("Possibly invalid docblock tag")
        {
            IssueKind::PossiblyInvalidDocblockTag
        } else if issue
            .message
            .eq_ignore_ascii_case("Undefined docblock class")
        {
            IssueKind::UndefinedDocblockClass
        } else if issue.message.eq_ignore_ascii_case("Undefined constant") {
            IssueKind::UndefinedConstant
        } else if issue.message.starts_with("Incorrect param name") {
            IssueKind::InvalidDocblockParamName
        } else {
            IssueKind::InvalidDocblock
        };
        analysis_data.add_issue(Issue::new(
            issue_kind,
            issue.message.clone(),
            analyzer.file_path,
            issue.start_offset,
            issue.end_offset,
            line,
            col,
        ));
    }
}

/// Report UnresolvableConstant for deferred `key-of<Class::CONST>` /
/// `value-of<Class::CONST>` sentinels in a function-like's declared types
/// (Psalm's UnresolvableConstantException caught in FunctionLikeAnalyzer).
/// Methods run only this narrow check; full docblock-type inspection for
/// methods matches Psalm elsewhere.
pub(crate) fn check_key_value_of_sentinels(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted = FxHashSet::default();
    for param in &function_info.params {
        if let Some(param_type) = param.get_type() {
            for atomic in &param_type.types {
                report_key_value_of_sentinel(analyzer, function_info, atomic, &mut emitted, analysis_data);
            }
        }
    }
    if let Some(return_type) = function_info.get_return_type() {
        for atomic in &return_type.types {
            report_key_value_of_sentinel(analyzer, function_info, atomic, &mut emitted, analysis_data);
        }
    }
}

fn report_key_value_of_sentinel(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    atomic: &TAtomic,
    emitted: &mut FxHashSet<(StrId, StrId)>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let TAtomic::TNamedObject {
        name,
        type_params: None,
        ..
    } = atomic
    else {
        return;
    };
    let raw_name = analyzer.interner.lookup(*name);
    let Some((_, inner)) = crate::type_expander::split_key_value_of_sentinel(&raw_name) else {
        return;
    };
    let Some((class_part, constant_part)) = inner.split_once("::") else {
        return;
    };
    let class_part = class_part.trim();
    let constant_part = constant_part.trim();
    let static_class =
        class_part.eq_ignore_ascii_case("static") || class_part.eq_ignore_ascii_case("$this");
    let resolved_class = if static_class {
        None
    } else {
        resolve_docblock_class_id(
            analyzer,
            function_info,
            analyzer.interner.intern(class_part.trim_start_matches('\\')),
        )
    };
    let unresolvable = match resolved_class {
        None => true,
        Some(class_id) => !docblock_class_constant_exists(analyzer, class_id, constant_part),
    };
    if unresolvable {
        let constant_id = analyzer.interner.intern(constant_part);
        let class_key = analyzer.interner.intern(class_part);
        if emitted.insert((class_key, constant_id)) {
            let (line, col) = analyzer.get_line_column(function_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnresolvableConstant,
                format!("Could not resolve constant {}::{}", class_part, constant_part),
                analyzer.file_path,
                function_info.start_offset,
                function_info.start_offset,
                line,
                col,
            ));
        }
    }
}

fn check_undefined_docblock_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted_classes = FxHashSet::default();
    let mut emitted_constants = FxHashSet::default();

    for param in &function_info.params {
        if !param.has_docblock_type {
            continue;
        }

        let Some(param_type) = param.get_type() else {
            continue;
        };

        inspect_union_for_docblock_refs(
            analyzer,
            function_info,
            param_type,
            &mut emitted_classes,
            &mut emitted_constants,
            analysis_data,
        );
    }

    if let Some(return_type) = function_info.get_return_type() {
        let matches_signature = function_info
            .signature_return_type
            .as_ref()
            .is_some_and(|signature_return_type| signature_return_type == return_type);

        if return_type.from_docblock || !matches_signature {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                return_type,
                &mut emitted_classes,
                &mut emitted_constants,
                analysis_data,
            );
        }
    }
}

fn inspect_union_for_docblock_refs(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    union: &TUnion,
    emitted_classes: &mut FxHashSet<StrId>,
    emitted_constants: &mut FxHashSet<(StrId, StrId)>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for atomic in &union.types {
        inspect_atomic_for_docblock_refs(
            analyzer,
            function_info,
            atomic,
            emitted_classes,
            emitted_constants,
            analysis_data,
        );
    }
}

fn inspect_atomic_for_docblock_refs(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    atomic: &TAtomic,
    emitted_classes: &mut FxHashSet<StrId>,
    emitted_constants: &mut FxHashSet<(StrId, StrId)>,
    analysis_data: &mut FunctionAnalysisData,
) {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
            if *name == StrId::PZOOM_INDEXED_ACCESS {
                if let Some(type_params) = type_params {
                    for type_param in type_params {
                        inspect_union_for_docblock_refs(
                            analyzer,
                            function_info,
                            type_param,
                            emitted_classes,
                            emitted_constants,
                            analysis_data,
                        );
                    }
                }
                return;
            }

            let raw_name = analyzer.interner.lookup(*name);
            let raw_name = raw_name.as_ref().trim();
            // A deferred `key-of<Class::CONST>` / `value-of<Class::CONST>`
            // sentinel is checked separately.
            if crate::type_expander::split_key_value_of_sentinel(raw_name).is_some() {
                report_key_value_of_sentinel(
                    analyzer,
                    function_info,
                    atomic,
                    emitted_constants,
                    analysis_data,
                );
                return;
            }

            if let Some((class_part, constant_part)) = raw_name.split_once("::") {
                let class_name = class_part.trim().trim_start_matches('\\');
                let class_id = analyzer.interner.intern(class_name);
                let Some(actual_class_id) =
                    resolve_docblock_class_id(analyzer, function_info, class_id)
                else {
                    emit_undefined_docblock_class_issue(
                        analyzer,
                        function_info,
                        class_id,
                        emitted_classes,
                        analysis_data,
                    );
                    return;
                };

                let constant_name = constant_part.trim();
                if !constant_name.eq_ignore_ascii_case("class")
                    && !docblock_class_constant_exists(analyzer, actual_class_id, constant_name)
                {
                    let constant_id = analyzer.interner.intern(constant_name);
                    if emitted_constants.insert((actual_class_id, constant_id)) {
                        let (line, col) = analyzer.get_line_column(function_info.start_offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::UndefinedConstant,
                            format!(
                                "Constant {}::{} is not defined",
                                analyzer.interner.lookup(actual_class_id),
                                constant_name
                            ),
                            analyzer.file_path,
                            function_info.start_offset,
                            function_info.end_offset,
                            line,
                            col,
                        ));
                    }
                }
            } else {
                let class_name = raw_name.trim_start_matches('\\');
                let class_id = analyzer.interner.intern(class_name);
                if resolve_docblock_class_id(analyzer, function_info, class_id).is_none() {
                    emit_undefined_docblock_class_issue(
                        analyzer,
                        function_info,
                        class_id,
                        emitted_classes,
                        analysis_data,
                    );
                }
            }

            if let Some(type_params) = type_params {
                for type_param in type_params {
                    inspect_union_for_docblock_refs(
                        analyzer,
                        function_info,
                        type_param,
                        emitted_classes,
                        emitted_constants,
                        analysis_data,
                    );
                }
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => inspect_union_for_docblock_refs(
            analyzer,
            function_info,
            as_type,
            emitted_classes,
            emitted_constants,
            analysis_data,
        ),
        TAtomic::TTemplateParamClass { as_type, .. } => inspect_atomic_for_docblock_refs(
            analyzer,
            function_info,
            as_type,
            emitted_classes,
            emitted_constants,
            analysis_data,
        ),
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                inspect_atomic_for_docblock_refs(
                    analyzer,
                    function_info,
                    nested_atomic,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        TAtomic::TClassString { as_type } => {
            if let Some(as_type) = as_type {
                inspect_atomic_for_docblock_refs(
                    analyzer,
                    function_info,
                    as_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                key_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                value_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            inspect_union_for_docblock_refs(
                analyzer,
                function_info,
                value_type,
                emitted_classes,
                emitted_constants,
                analysis_data,
            );
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for property_type in properties.values() {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    property_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }

            if let Some(fallback_key_type) = fallback_key_type {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    fallback_key_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }

            if let Some(fallback_value_type) = fallback_value_type {
                inspect_union_for_docblock_refs(
                    analyzer,
                    function_info,
                    fallback_value_type,
                    emitted_classes,
                    emitted_constants,
                    analysis_data,
                );
            }
        }
        _ => {}
    }
}

fn emit_undefined_docblock_class_issue(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    class_id: StrId,
    emitted_classes: &mut FxHashSet<StrId>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !emitted_classes.insert(class_id) {
        return;
    }

    // Psalm anchors docblock-class issues on the docblock type; the
    // return-type location is the closest tracked anchor (falling back to
    // the function name rather than the whole span).
    let (issue_start, issue_end) = function_info
        .return_type_location
        .or(function_info.name_location)
        .unwrap_or((function_info.start_offset, function_info.end_offset));
    let (line, col) = analyzer.get_line_column(issue_start);
    analysis_data.add_issue(Issue::new(
        IssueKind::UndefinedDocblockClass,
        crate::class_casing::undefined_docblock_class_message(analyzer, class_id),
        analyzer.file_path,
        issue_start,
        issue_end,
        line,
        col,
    ));
}

fn resolve_docblock_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    class_id: StrId,
) -> Option<StrId> {
    if analyzer.codebase.get_class(class_id).is_some() {
        return Some(class_id);
    }

    let class_name = analyzer.interner.lookup(class_id);
    let class_name = class_name.as_ref();
    if class_name.contains('\\') {
        return None;
    }

    let function_name = analyzer.interner.lookup(function_info.name);
    let namespace = function_name.rsplit_once('\\').map(|(ns, _)| ns)?;
    let namespaced_candidate = analyzer
        .interner
        .intern(&format!("{namespace}\\{class_name}"));

    analyzer
        .codebase
        .get_class(namespaced_candidate)
        .map(|_| namespaced_candidate)
}

fn docblock_class_constant_exists(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    constant_name: &str,
) -> bool {
    let mut to_visit = vec![class_id];
    let mut visited = FxHashSet::default();

    while let Some(current_class_id) = to_visit.pop() {
        if !visited.insert(current_class_id) {
            continue;
        }

        let Some(class_info) = analyzer.codebase.get_class(current_class_id) else {
            continue;
        };

        if constant_name.ends_with('*') {
            let prefix = constant_name.trim_end_matches('*');
            if class_info.constants.keys().any(|const_id| {
                analyzer
                    .interner
                    .lookup(*const_id)
                    .as_ref()
                    .starts_with(prefix)
            }) {
                return true;
            }
        } else {
            let constant_id = analyzer.interner.intern(constant_name);
            if class_info.constants.contains_key(&constant_id) {
                return true;
            }
        }

        if let Some(parent_class_id) = class_info.parent_class {
            to_visit.push(parent_class_id);
        }

        to_visit.extend(class_info.interfaces.iter().copied());
        to_visit.extend(class_info.all_parent_interfaces.iter().copied());
    }

    false
}

/// A scan-time default like `D::class` is stored as the literal string "D";
/// against a class-string param the comparator flags it as a coercion. Like
/// the argument analyzer, tolerate literals that name existing classes.
fn default_is_class_string_literal_standin(
    analyzer: &StatementsAnalyzer<'_>,
    default_type: &pzoom_code_info::TUnion,
    param_type: &pzoom_code_info::TUnion,
) -> bool {
    crate::expr::call::callable_validation::expects_class_string_union(param_type)
        && !default_type.types.is_empty()
        && default_type.types.iter().all(|atomic| match atomic {
            pzoom_code_info::TAtomic::TLiteralString { value } => {
                analyzer.codebase.resolve_classlike_name(value).is_some()
            }
            pzoom_code_info::TAtomic::TLiteralClassString { .. }
            | pzoom_code_info::TAtomic::TClassString { .. } => true,
            _ => false,
        })
}

fn check_invalid_param_defaults(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    function_name: &str,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (idx, param) in function_info.params.iter().enumerate() {
        let Some(default_type) = param.default_type.as_ref() else {
            continue;
        };
        let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) else {
            continue;
        };
        let default_check_param_type = if default_type.is_null() {
            param
                .signature_type
                .as_ref()
                .filter(|signature_type| signature_type.is_nullable() || signature_type.is_null())
                .unwrap_or(param_type)
        } else {
            param_type
        };

        if union_has_callable_like(default_check_param_type) && !default_type.is_null() {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type for callable argument {} of {} can only be null, {} specified",
                    idx + 1,
                    function_name,
                    default_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset.saturating_add(1),
                line,
                col,
            ));
            continue;
        }

        if default_type.is_mixed() {
            continue;
        }

        if is_empty_array_default_for_array_like_param(default_type, default_check_param_type) {
            continue;
        }

        let mut comparison_result = TypeComparisonResult::new();
        // Psalm checks param defaults with allow_interface_equality=true, so a
        // default fitting a template param's bound is accepted.
        let default_is_valid = union_type_comparator::is_contained_by_in_context(
            analyzer.codebase,
            default_type,
            default_check_param_type,
            false,
            false,
            true,
            &mut comparison_result,
        );

        if !default_is_valid
            && default_is_class_string_literal_standin(
                analyzer,
                default_type,
                default_check_param_type,
            )
        {
            continue;
        }

        if !default_is_valid {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type {} for argument {} of {} does not match the given type {}",
                    default_type.get_id(Some(analyzer.interner)),
                    idx + 1,
                    function_name,
                    default_check_param_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                param.start_offset,
                param.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn emit_invalid_by_ref_param_out_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &function_info.params {
        if !param.by_ref || param.is_variadic {
            continue;
        }

        let Some(actual_type) = context.get_var_type(&analyzer.interner.lookup(param.name)) else {
            continue;
        };

        let expected_type = param
            .param_out_type
            .as_ref()
            .or(param.get_type())
            .or(param.signature_type.as_ref());
        let Some(expected_type) = expected_type else {
            continue;
        };

        if actual_type.is_mixed() {
            continue;
        }

        let mut comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            actual_type,
            expected_type,
            false,
            false,
            &mut comparison,
        ) {
            continue;
        }

        // Avoid false positives when analysis widened the observed by-ref local
        // type but it still includes all values allowed by the declared out type.
        let mut reverse_comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            expected_type,
            actual_type,
            false,
            false,
            &mut reverse_comparison,
        ) {
            if !(union_is_list_only(expected_type) && union_has_non_list_array_like(actual_type)) {
                continue;
            }
        }

        let (line, col) = analyzer.get_line_column(param.start_offset);
        let param_name = analyzer.interner.lookup(param.name);
        analysis_data.add_issue(Issue::new(
            IssueKind::ReferenceConstraintViolation,
            format!(
                "Variable {} is limited to values of type {} because it is passed by reference, {} type found. Use @param-out to specify a different output type",
                param_name,
                expected_type.get_id(Some(analyzer.interner)),
                actual_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            param.start_offset,
            param.start_offset.saturating_add(1),
            line,
            col,
        ));
    }
}

fn union_has_callable_like(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
}

fn union_is_list_only(union: &TUnion) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TList { .. }
                    | TAtomic::TNonEmptyList { .. }
                    | TAtomic::TKeyedArray { is_list: true, .. }
            )
        })
}

fn union_has_non_list_array_like(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TKeyedArray { is_list: false, .. }
        )
    })
}

fn is_empty_array_default_for_array_like_param(default_type: &TUnion, param_type: &TUnion) -> bool {
    if !is_empty_array_type(default_type) {
        return false;
    }

    param_type.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { .. }
                | TAtomic::TList { .. }
                | TAtomic::TKeyedArray { .. }
                | TAtomic::TIterable { .. }
        )
    })
}

fn is_empty_array_type(union: &TUnion) -> bool {
    let Some(single) = union.get_single() else {
        return false;
    };

    match single {
        TAtomic::TArray {
            key_type,
            value_type,
        } => key_type.is_nothing() && value_type.is_nothing(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => properties.is_empty() && fallback_key_type.is_none() && fallback_value_type.is_none(),
        _ => false,
    }
}

fn get_alternate_param_var_id(
    _analyzer: &StatementsAnalyzer<'_>,
    var_name: &str,
) -> Option<VarName> {
    if var_name.is_empty() {
        return None;
    }

    if let Some(stripped) = var_name.strip_prefix('$') {
        Some(VarName::new(stripped))
    } else {
        Some(VarName::from(format!("${}", var_name)))
    }
}



/// Psalm's UnusedDocblockParam (FunctionLikeAnalyzer): a docblock `@param`
/// with no counterpart signature parameter, reported under find_unused_code.
pub(crate) fn emit_unused_docblock_params(
    analyzer: &StatementsAnalyzer<'_>,
    info: &pzoom_code_info::FunctionLikeInfo,
    cased_id: &str,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !analyzer.config.find_unused_code {
        return;
    }
    for (param_name, tag_offset) in &info.unused_docblock_params {
        let (line, col) = analyzer.get_line_column(*tag_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::UnusedDocblockParam,
            format!(
                "Docblock parameter ${} in docblock for {} does not have a counterpart in signature parameter list",
                param_name, cased_id
            ),
            analyzer.file_path,
            *tag_offset,
            tag_offset.saturating_add(1),
            line,
            col,
        ));
    }
}
