//! Class declaration analyzer.
//!
//! Analyzes method bodies with proper context.

use bumpalo::Bump;
use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::enum_case::EnumCaseItem;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::{Method, MethodBody};
use mago_syntax::ast::ast::class_like::{Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::namespace::NamespaceBody;
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::type_hint::Hint;

use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{DataFlowNode, Issue, IssueKind, TAtomic, TUnion, VarId, VariableSourceKind};
use pzoom_str::StrId;
use pzoom_syntax::{FileId, parse_file_content, resolve_names};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expr::call::function_call_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::internal_access::{can_class_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::attribute_analyzer;
use crate::stmt_analyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a class declaration.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_with_namespace(analyzer, class, None, analysis_data, context)
}

/// Analyze a trait declaration.
pub fn analyze_trait(
    analyzer: &StatementsAnalyzer<'_>,
    trait_stmt: &Trait<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_trait_with_namespace(analyzer, trait_stmt, None, analysis_data, context)
}

/// Analyze an interface declaration.
pub fn analyze_interface(
    analyzer: &StatementsAnalyzer<'_>,
    interface_stmt: &Interface<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_interface_with_namespace(analyzer, interface_stmt, None, analysis_data, context)
}

/// Analyze an enum declaration.
pub fn analyze_enum(
    analyzer: &StatementsAnalyzer<'_>,
    enum_stmt: &Enum<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_enum_with_namespace(analyzer, enum_stmt, None, analysis_data, context)
}

/// Analyze a class declaration with a namespace context.
pub fn analyze_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the class name - use FQN if in a namespace
    let class_name = class.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, class_name)
    } else {
        class_name.to_string()
    };
    let class_name_id = analyzer.interner.intern(&fqn);

    if analysis_data
        .declared_classlike_names
        .insert(class_name_id, class.span().start.offset)
        .is_some()
    {
        let span = class.span();
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::DuplicateClass,
            format!("Class {} has already been defined", fqn),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    // Look up the class info from the codebase
    let class_info = analyzer.codebase.get_class(class_name_id);

    attribute_analyzer::analyze_class_attributes(
        analyzer,
        class,
        class_name_id,
        class_info,
        context,
        analysis_data,
    );

    // Check for unimplemented abstract methods (only for non-abstract classes)
    if let Some(info) = class_info {
        if context.inside_conditional {
            return Ok(());
        }

        check_class_relationships(analyzer, info, context, analysis_data);
        check_trait_requirements(analyzer, info, context, analysis_data);
        check_missing_dependencies(analyzer, info, context, analysis_data);
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_reserved_class_constant_names(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_method_override_issues(analyzer, info, analysis_data);
        check_property_override_visibility(analyzer, info, analysis_data);
        check_property_type_invariance(analyzer, info, analysis_data);
        check_invalid_traversable_implementation(analyzer, info, analysis_data);
        check_missing_constructor_for_typed_properties(analyzer, info, analysis_data);

        if !info.is_abstract {
            check_unimplemented_abstract_methods(analyzer, class, info, analysis_data);
        }
        // Check for missing property types
        check_missing_property_types(analyzer, &fqn, info, analysis_data);
        check_immutable_relationships(analyzer, class, info, analysis_data);
    }

    // Analyze each method in the class
    for member in class.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            analyze_method(
                analyzer,
                method,
                class_name_id,
                class_info,
                context.namespace,
                analysis_data,
            )?;
        }
    }

    if let Some(info) = class_info {
        analyze_methods_from_used_traits(analyzer, info, class_name_id, analysis_data)?;
    }

    Ok(())
}

/// Analyze an interface declaration with a namespace context.
pub fn analyze_interface_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    interface_stmt: &Interface<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let interface_name = interface_stmt.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, interface_name)
    } else {
        interface_name.to_string()
    };
    let interface_name_id = analyzer.interner.intern(&fqn);

    let interface_info = analyzer.codebase.get_class(interface_name_id);
    attribute_analyzer::analyze_interface_or_trait_attributes(
        analyzer,
        interface_stmt.attribute_lists.as_slice(),
        interface_stmt.members.as_slice(),
        interface_info,
        interface_name_id,
        context,
        analysis_data,
    );

    if let Some(info) = interface_info {
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_missing_interface_method_typehints(analyzer, info, analysis_data);
    }

    let _ = context;

    Ok(())
}

/// Analyze an enum declaration with a namespace context.
pub fn analyze_enum_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    enum_stmt: &Enum<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let enum_name = enum_stmt.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, enum_name)
    } else {
        enum_name.to_string()
    };
    let enum_name_id = analyzer.interner.intern(&fqn);

    let enum_info = analyzer.codebase.get_class(enum_name_id);

    attribute_analyzer::analyze_interface_or_trait_attributes(
        analyzer,
        enum_stmt.attribute_lists.as_slice(),
        enum_stmt.members.as_slice(),
        enum_info,
        enum_name_id,
        context,
        analysis_data,
    );

    if let Some(info) = enum_info {
        if context.inside_conditional {
            return Ok(());
        }

        check_class_relationships(analyzer, info, context, analysis_data);
        check_missing_dependencies(analyzer, info, context, analysis_data);
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_reserved_class_constant_names(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_invalid_traversable_implementation(analyzer, info, analysis_data);
        check_missing_constructor_for_typed_properties(analyzer, info, analysis_data);
        check_enum_declaration_issues(analyzer, enum_stmt, analysis_data);
    }

    for member in enum_stmt.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            analyze_method(
                analyzer,
                method,
                enum_name_id,
                enum_info,
                context.namespace,
                analysis_data,
            )?;
        }
    }

    if let Some(info) = enum_info {
        analyze_methods_from_used_traits(analyzer, info, enum_name_id, analysis_data)?;
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnumBackingType {
    Int,
    String,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum EnumCaseLiteralValue {
    Int(i64),
    String(String),
}

fn check_enum_declaration_issues(
    analyzer: &StatementsAnalyzer<'_>,
    enum_stmt: &Enum<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let backing_type = match enum_stmt.backing_type_hint.as_ref() {
        Some(backing_hint) => {
            if hint_is_int(&backing_hint.hint) {
                Some(EnumBackingType::Int)
            } else if hint_is_string(&backing_hint.hint) {
                Some(EnumBackingType::String)
            } else {
                let span = backing_hint.hint.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidEnumBackingType,
                    "Enums cannot be backed by this type, string or int expected",
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
                None
            }
        }
        None => None,
    };

    let mut seen_case_names = FxHashSet::default();
    let mut seen_case_values = FxHashSet::default();

    for member in enum_stmt.members.iter() {
        match member {
            ClassLikeMember::Property(property) => {
                let span = property.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NoEnumProperties,
                    "Enums cannot have properties",
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
            ClassLikeMember::Method(method) => {
                if !is_invalid_enum_method_name(method.name.value, backing_type.is_some()) {
                    continue;
                }

                let span = method.name.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidEnumMethod,
                    format!("Enums cannot define {}", method.name.value),
                    analyzer.file_path,
                    span.start.offset,
                    span.end.offset,
                    line,
                    col,
                ));
            }
            ClassLikeMember::EnumCase(enum_case) => {
                let case_name = enum_case.item.name().value;
                let case_name_id = analyzer.interner.intern(case_name);
                let case_span = enum_case.item.name().span();

                if !seen_case_names.insert(case_name_id) {
                    let (line, col) = analyzer.get_line_column(case_span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DuplicateEnumCase,
                        "Enum case names should be unique",
                        analyzer.file_path,
                        case_span.start.offset,
                        case_span.end.offset,
                        line,
                        col,
                    ));
                }

                match (&enum_case.item, backing_type) {
                    (EnumCaseItem::Unit(_), Some(_)) => {
                        let (line, col) = analyzer.get_line_column(case_span.start.offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidEnumCaseValue,
                            "Case of a backed enum should have a value",
                            analyzer.file_path,
                            case_span.start.offset,
                            case_span.end.offset,
                            line,
                            col,
                        ));
                    }
                    (EnumCaseItem::Backed(_), None) => {
                        let (line, col) = analyzer.get_line_column(case_span.start.offset);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidEnumCaseValue,
                            "Case of a non-backed enum should not have a value",
                            analyzer.file_path,
                            case_span.start.offset,
                            case_span.end.offset,
                            line,
                            col,
                        ));
                    }
                    (EnumCaseItem::Backed(backed_case), Some(expected_backing_type)) => {
                        let literal_value = get_enum_case_literal_value(&backed_case.value);
                        let is_invalid = match (&literal_value, expected_backing_type) {
                            (Some(EnumCaseLiteralValue::Int(_)), EnumBackingType::Int) => false,
                            (Some(EnumCaseLiteralValue::String(_)), EnumBackingType::String) => {
                                false
                            }
                            _ => true,
                        };

                        if is_invalid {
                            let value_span = backed_case.value.span();
                            let (line, col) = analyzer.get_line_column(value_span.start.offset);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidEnumCaseValue,
                                "Enum case value type does not match enum backing type",
                                analyzer.file_path,
                                value_span.start.offset,
                                value_span.end.offset,
                                line,
                                col,
                            ));
                        } else if let Some(literal_value) = literal_value
                            && !seen_case_values.insert(literal_value)
                        {
                            let value_span = backed_case.value.span();
                            let (line, col) = analyzer.get_line_column(value_span.start.offset);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::DuplicateEnumCaseValue,
                                "Enum case values should be unique",
                                analyzer.file_path,
                                value_span.start.offset,
                                value_span.end.offset,
                                line,
                                col,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn hint_is_int(hint: &Hint<'_>) -> bool {
    match hint {
        Hint::Integer(_) => true,
        Hint::Parenthesized(parenthesized) => hint_is_int(parenthesized.hint),
        _ => false,
    }
}

fn hint_is_string(hint: &Hint<'_>) -> bool {
    match hint {
        Hint::String(_) => true,
        Hint::Parenthesized(parenthesized) => hint_is_string(parenthesized.hint),
        _ => false,
    }
}

fn is_invalid_enum_method_name(method_name: &str, is_backed_enum: bool) -> bool {
    let lowered = method_name.to_ascii_lowercase();

    matches!(
        lowered.as_str(),
        "__construct"
            | "__destruct"
            | "__clone"
            | "__get"
            | "__set"
            | "__unset"
            | "__isset"
            | "__tostring"
            | "__debuginfo"
            | "__serialize"
            | "__unserialize"
            | "__sleep"
            | "__wakeup"
            | "__set_state"
            | "cases"
    ) || (is_backed_enum && (lowered == "from" || lowered == "tryfrom"))
}

fn get_enum_case_literal_value(expr: &Expression<'_>) -> Option<EnumCaseLiteralValue> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(integer_literal)) => integer_literal
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(EnumCaseLiteralValue::Int),
        Expression::Literal(Literal::String(string_literal)) => string_literal
            .value
            .map(|value| EnumCaseLiteralValue::String(value.to_string())),
        _ => None,
    }
}

fn check_missing_interface_method_typehints(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.kind != ClassLikeKind::Interface {
        return;
    }

    for method_info in class_info.methods.values() {
        if method_info.file_path != analyzer.file_path {
            continue;
        }

        let mut inherited_methods = Vec::new();
        let mut seen_interfaces = rustc_hash::FxHashSet::default();
        for parent_interface in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
        {
            if !seen_interfaces.insert(*parent_interface) {
                continue;
            }

            let Some(parent_info) = analyzer.codebase.get_class(*parent_interface) else {
                continue;
            };

            if let Some(parent_method) = parent_info
                .methods
                .get(&method_info.name)
                .or_else(|| get_method_case_insensitive(analyzer, parent_info, &method_info.name))
            {
                inherited_methods.push(parent_method);
            }
        }

        let inherited_return_type_available = inherited_methods.iter().any(|parent_method| {
            parent_method.signature_return_type.is_some() || parent_method.return_type.is_some()
        });

        let method_requires_omitted_return = matches!(
            method_info.name,
            StrId::CONSTRUCT | StrId::CLONE | StrId::DESTRUCT
        );
        let has_assertions = !method_info.assertions.is_empty()
            || !method_info.if_true_assertions.is_empty()
            || !method_info.if_false_assertions.is_empty();

        if method_info.signature_return_type.is_none()
            && method_info.return_type.is_none()
            && !method_requires_omitted_return
            && method_info.name != StrId::TO_STRING
            && !has_assertions
            && !inherited_return_type_available
        {
            let (line, col) = analyzer.get_line_column(method_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingReturnType,
                format!(
                    "Method {}::{} does not have a return type",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(method_info.name)
                ),
                analyzer.file_path,
                method_info.start_offset,
                method_info.end_offset,
                line,
                col,
            ));
        }

        check_invalid_to_string_return_type(analyzer, class_info, method_info, analysis_data);

        for param in &method_info.params {
            let param_index = method_info
                .params
                .iter()
                .position(|candidate| candidate.name == param.name)
                .unwrap_or(usize::MAX);
            let inherited_param_type_available = param_index != usize::MAX
                && inherited_methods.iter().any(|parent_method| {
                    parent_method
                        .params
                        .get(param_index)
                        .is_some_and(|parent_param| {
                            parent_param.signature_type.is_some()
                                || parent_param.param_type.is_some()
                        })
                });

            if param.signature_type.is_none()
                && param.param_type.is_none()
                && !has_assertions
                && !inherited_param_type_available
            {
                let (line, col) = analyzer.get_line_column(param.start_offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MissingParamType,
                    format!(
                        "Argument {} of method {}::{} does not have a type",
                        analyzer.interner.lookup(param.name),
                        analyzer.interner.lookup(class_info.name),
                        analyzer.interner.lookup(method_info.name)
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
}

fn check_invalid_to_string_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if method_info.name != StrId::TO_STRING || class_info.kind == ClassLikeKind::Interface {
        return;
    }

    let return_type = method_info
        .signature_return_type
        .as_ref()
        .or(method_info.return_type.as_ref());

    let Some(return_type) = return_type else {
        let (line, col) = analyzer.get_line_column(method_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidToString,
            "Method __toString must return a string",
            analyzer.file_path,
            method_info.start_offset,
            method_info.end_offset,
            line,
            col,
        ));
        return;
    };

    if !union_is_string_return_type(return_type) {
        let (line, col) = analyzer.get_line_column(method_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidToString,
            format!(
                "Method __toString has invalid return type {}, expected string",
                return_type.get_id(Some(analyzer.interner))
            ),
            analyzer.file_path,
            method_info.start_offset,
            method_info.end_offset,
            line,
            col,
        ));
    }
}

fn union_is_string_return_type(return_type: &TUnion) -> bool {
    return_type.types.iter().all(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TTruthyString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TNumericString
        )
    })
}

fn resolve_alias_in_context(class_id: StrId, context: &BlockContext) -> StrId {
    context
        .class_aliases
        .get(&class_id)
        .copied()
        .unwrap_or(class_id)
}

fn has_parent_cycle(
    codebase: &pzoom_code_info::CodebaseInfo,
    class_name: StrId,
    mut current_parent: StrId,
) -> bool {
    let mut visited = FxHashSet::default();

    loop {
        if current_parent == class_name || !visited.insert(current_parent) {
            return true;
        }

        let Some(parent_info) = codebase.get_class(current_parent) else {
            return false;
        };

        let Some(next_parent) = parent_info.parent_class else {
            return false;
        };

        current_parent = next_parent;
    }
}

fn check_class_relationships(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    if let Some(parent_class) = class_info.parent_class {
        let resolved_parent = resolve_alias_in_context(parent_class, context);
        let (line, col) = analyzer.get_line_column(class_info.start_offset);

        if resolved_parent == class_info.name {
            analysis_data.add_issue(Issue::new(
                IssueKind::CircularReference,
                format!(
                    "Circular reference discovered when resolving {}",
                    analyzer.interner.lookup(class_info.name)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        } else if has_parent_cycle(analyzer.codebase, class_info.name, resolved_parent) {
            analysis_data.add_issue(Issue::new(
                IssueKind::CircularReference,
                format!(
                    "Circular reference discovered when resolving {}",
                    analyzer.interner.lookup(class_info.name)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        } else if let Some(parent_info) = analyzer.codebase.get_class(resolved_parent) {
            if parent_info.kind != ClassLikeKind::Class {
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    format!(
                        "Class {} does not exist",
                        analyzer.interner.lookup(parent_class)
                    ),
                    analyzer.file_path,
                    class_info.start_offset,
                    class_info.end_offset,
                    line,
                    col,
                ));
            } else if parent_info.is_final {
                if !should_suppress_class_issue(
                    analyzer,
                    class_info.start_offset,
                    &["InvalidExtends", "InvalidExtendClass"],
                ) {
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidExtendClass,
                        format!(
                            "Class {} may not inherit from final class {}",
                            analyzer.interner.lookup(class_info.name),
                            analyzer.interner.lookup(resolved_parent)
                        ),
                        analyzer.file_path,
                        class_info.start_offset,
                        class_info.end_offset,
                        line,
                        col,
                    ));
                }
            }
        } else {
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!(
                    "Class {} does not exist",
                    analyzer.interner.lookup(parent_class)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        }
    }

    for interface_id in &class_info.interfaces {
        let resolved_interface = resolve_alias_in_context(*interface_id, context);
        let (line, col) = analyzer.get_line_column(class_info.start_offset);

        if class_info.kind == ClassLikeKind::Class
            && is_enum_builtin_interface(analyzer, resolved_interface)
        {
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidInterfaceImplementation,
                format!(
                    "Class {} cannot implement {}",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(*interface_id)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
            continue;
        }

        if let Some(interface_info) = analyzer.codebase.get_class(resolved_interface) {
            if interface_info.kind != ClassLikeKind::Interface {
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedInterface,
                    format!(
                        "{} is not an interface",
                        analyzer.interner.lookup(*interface_id)
                    ),
                    analyzer.file_path,
                    class_info.start_offset,
                    class_info.end_offset,
                    line,
                    col,
                ));
            }
        } else {
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                format!(
                    "Class {} does not exist",
                    analyzer.interner.lookup(*interface_id)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        }
    }
}

fn is_enum_builtin_interface(analyzer: &StatementsAnalyzer<'_>, interface_id: StrId) -> bool {
    let interface_name = analyzer.interner.lookup(interface_id);
    let short_name = interface_name
        .as_ref()
        .rsplit('\\')
        .next()
        .unwrap_or(interface_name.as_ref());

    short_name.eq_ignore_ascii_case("UnitEnum")
        || short_name.eq_ignore_ascii_case("BackedEnum")
        || short_name.eq_ignore_ascii_case("IntBackedEnum")
        || short_name.eq_ignore_ascii_case("StringBackedEnum")
}

fn check_trait_requirements(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.kind != ClassLikeKind::Class {
        return;
    }

    let mut extended_classes: FxHashSet<StrId> = class_info
        .all_parent_classes
        .iter()
        .copied()
        .map(|class_id| resolve_alias_in_context(class_id, context))
        .collect();
    if let Some(parent_class) = class_info.parent_class {
        extended_classes.insert(resolve_alias_in_context(parent_class, context));
    }

    let implemented_interfaces: FxHashSet<StrId> = class_info
        .interfaces
        .iter()
        .copied()
        .chain(class_info.all_parent_interfaces.iter().copied())
        .map(|interface_id| resolve_alias_in_context(interface_id, context))
        .collect();

    for used_trait in &class_info.used_traits {
        let resolved_trait = resolve_alias_in_context(*used_trait, context);
        let Some(trait_info) = analyzer.codebase.get_class(resolved_trait) else {
            continue;
        };

        for required_parent in &trait_info.required_extends {
            let required_parent = resolve_alias_in_context(*required_parent, context);
            if extended_classes.contains(&required_parent) {
                continue;
            }

            let (line, col) = analyzer.get_line_column(class_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingDependency,
                format!(
                    "Trait {} requires using class to extend {}",
                    analyzer.interner.lookup(resolved_trait),
                    analyzer.interner.lookup(required_parent)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        }

        for required_interface in &trait_info.required_implements {
            let required_interface = resolve_alias_in_context(*required_interface, context);
            if implemented_interfaces.contains(&required_interface) {
                continue;
            }

            let (line, col) = analyzer.get_line_column(class_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingDependency,
                format!(
                    "Trait {} requires using class to implement {}",
                    analyzer.interner.lookup(resolved_trait),
                    analyzer.interner.lookup(required_interface)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        }
    }
}

fn check_missing_dependencies(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut seen = FxHashSet::default();

    for dependency in &class_info.invalid_dependencies {
        if !seen.insert(*dependency) {
            continue;
        }

        let resolved_dependency = resolve_alias_in_context(*dependency, context);
        if analyzer.codebase.get_class(resolved_dependency).is_some() {
            continue;
        }

        let dependency_name = analyzer.interner.lookup(*dependency);
        let alias_target = context
            .class_aliases
            .iter()
            .find_map(|(alias_id, target_id)| {
                let alias_name = analyzer.interner.lookup(*alias_id);
                if alias_name
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(dependency_name.trim_start_matches('\\'))
                {
                    Some(*target_id)
                } else {
                    None
                }
            });

        if alias_target.is_some_and(|target_id| analyzer.codebase.get_class(target_id).is_some()) {
            continue;
        }

        let is_missing_trait = class_info.used_traits.contains(dependency);

        let (issue_kind, message) = if is_missing_trait {
            (
                IssueKind::UndefinedTrait,
                format!(
                    "Trait {} does not exist",
                    analyzer.interner.lookup(*dependency)
                ),
            )
        } else {
            (
                IssueKind::MissingDependency,
                format!(
                    "Class {} has unresolved dependency {}",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(*dependency)
                ),
            )
        };

        let (line, col) = analyzer.get_line_column(class_info.start_offset);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            message,
            analyzer.file_path,
            class_info.start_offset,
            class_info.end_offset,
            line,
            col,
        ));
    }
}

fn check_method_docblock_param_type_mismatches(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for method_info in class_info.methods.values() {
        if method_info.declaring_class != Some(class_info.name) {
            continue;
        }

        let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
        template_defaults.extend(function_call_analyzer::get_template_defaults(method_info));
        let template_replacements: FxHashMap<StrId, TUnion> = FxHashMap::default();

        for param in &method_info.params {
            if !param.has_docblock_type {
                continue;
            }

            let (Some(docblock_type), Some(signature_type)) =
                (param.param_type.as_ref(), param.signature_type.as_ref())
            else {
                continue;
            };

            let mut localized_docblock_type = if template_defaults.is_empty() {
                docblock_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    docblock_type,
                    &template_replacements,
                    &template_defaults,
                )
            };
            let mut localized_signature_type = if template_defaults.is_empty() {
                signature_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    signature_type,
                    &template_replacements,
                    &template_defaults,
                )
            };

            localized_docblock_type = localize_special_class_names_for_final_class(
                &localized_docblock_type,
                class_info.name,
                class_info.parent_class,
            );
            localized_signature_type = localize_special_class_names_for_final_class(
                &localized_signature_type,
                class_info.name,
                class_info.parent_class,
            );

            let is_magic_property_method = matches!(method_info.name, StrId::GET | StrId::SET);
            let docblock_is_array_key = localized_docblock_type.types.len() == 1
                && matches!(localized_docblock_type.types[0], TAtomic::TArrayKey);
            let signature_is_int_only =
                localized_signature_type.has_int() && !localized_signature_type.has_string();

            if is_magic_property_method
                && docblock_is_array_key
                && localized_signature_type.has_string()
            {
                continue;
            }

            // `key-of<...>` docblocks can be conservatively parsed as `array-key`
            // during scan-time; avoid false positives when the signature expects int.
            if docblock_is_array_key && signature_is_int_only {
                continue;
            }

            if union_is_class_constant_reference(&localized_docblock_type, analyzer)
                && localized_signature_type.has_string()
            {
                continue;
            }

            let mut comparison_result = TypeComparisonResult::new();
            let is_compatible = union_type_comparator::is_contained_by(
                analyzer.codebase,
                &localized_docblock_type,
                &localized_signature_type,
                false,
                false,
                &mut comparison_result,
            );

            if is_compatible
                || comparison_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
            {
                continue;
            }

            let (line, col) = analyzer.get_line_column(param.start_offset);
            let param_name = analyzer.interner.lookup(param.name);
            analysis_data.add_issue(Issue::new(
                IssueKind::MismatchingDocblockParamType,
                format!(
                    "Parameter {} has wrong type '{}', should be '{}'",
                    param_name,
                    localized_docblock_type.get_id(Some(analyzer.interner)),
                    localized_signature_type.get_id(Some(analyzer.interner))
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

fn check_missing_template_params(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let emit_missing_template_param =
        |analysis_data: &mut FunctionAnalysisData,
         related_name: StrId,
         class_info: &pzoom_code_info::ClassLikeInfo| {
            let (line, col) = analyzer.get_line_column(class_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingTemplateParam,
                format!(
                    "{} has missing template params when extending {}",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(related_name)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.end_offset,
                line,
                col,
            ));
        };

    if let Some(parent_id) = class_info.parent_class {
        let resolved_parent_id = resolve_alias_in_context(parent_id, context);
        if let Some(parent_info) = analyzer.codebase.get_class(resolved_parent_id) {
            if !parent_info.template_types.is_empty()
                && !class_info
                    .template_extended_offsets
                    .contains_key(&resolved_parent_id)
                && !class_info
                    .template_extended_offsets
                    .contains_key(&parent_id)
            {
                emit_missing_template_param(analysis_data, resolved_parent_id, class_info);
            }
        }
    }

    for interface_id in &class_info.interfaces {
        let resolved_interface_id = resolve_alias_in_context(*interface_id, context);
        if let Some(interface_info) = analyzer.codebase.get_class(resolved_interface_id) {
            if !interface_info.template_types.is_empty()
                && !class_info
                    .template_extended_offsets
                    .contains_key(&resolved_interface_id)
                && !class_info
                    .template_extended_offsets
                    .contains_key(interface_id)
            {
                emit_missing_template_param(analysis_data, resolved_interface_id, class_info);
                break;
            }
        }
    }

    for trait_id in &class_info.used_traits {
        let resolved_trait_id = resolve_alias_in_context(*trait_id, context);
        if let Some(trait_info) = analyzer.codebase.get_class(resolved_trait_id) {
            if !trait_info.template_types.is_empty()
                && !class_info
                    .template_extended_offsets
                    .contains_key(&resolved_trait_id)
                && !class_info.template_extended_offsets.contains_key(trait_id)
            {
                emit_missing_template_param(analysis_data, resolved_trait_id, class_info);
                break;
            }
        }
    }
}

fn check_reserved_class_constant_names(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for const_info in class_info.constants.values() {
        if const_info.declaring_class != class_info.name {
            continue;
        }

        let const_name = analyzer.interner.lookup(const_info.name);
        if !const_name.eq_ignore_ascii_case("class") {
            continue;
        }

        let (line, col) = analyzer.get_line_column(const_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::ReservedWord,
            format!("'{}' is a reserved word", const_name),
            analyzer.file_path,
            const_info.start_offset,
            const_info.start_offset.saturating_add(1),
            line,
            col,
        ));
    }
}

fn visibility_rank(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 3,
        Visibility::Protected => 2,
        Visibility::Private => 1,
    }
}

fn is_visibility_more_restrictive(child: Visibility, parent: Visibility) -> bool {
    visibility_rank(child) < visibility_rank(parent)
}

fn find_parent_method<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
) -> Option<(StrId, &'a pzoom_code_info::FunctionLikeInfo)> {
    let mut current_parent = class_info.parent_class;

    while let Some(parent_id) = current_parent {
        let parent_info = analyzer.codebase.get_class(parent_id)?;
        if let Some(parent_method) = parent_info
            .methods
            .get(&method_name)
            .or_else(|| get_method_case_insensitive(analyzer, parent_info, &method_name))
        {
            if parent_method.visibility == Visibility::Private {
                current_parent = parent_info.parent_class;
                continue;
            }

            return Some((parent_id, parent_method));
        }

        current_parent = parent_info.parent_class;
    }

    None
}

fn check_method_override_issues(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut checked_guides = FxHashSet::default();

    for (method_name, method_info) in &class_info.methods {
        let declared_here_or_used_trait =
            method_info.declaring_class.is_some_and(|declaring_class| {
                declaring_class == class_info.name
                    || class_info.used_traits.contains(&declaring_class)
            });

        if !declared_here_or_used_trait {
            continue;
        }

        check_method_signature_must_omit_return_type(
            analyzer,
            class_info,
            *method_name,
            method_info,
            analysis_data,
        );

        if let Some((parent_class_id, parent_method)) =
            find_parent_method(analyzer, class_info, *method_name)
        {
            if checked_guides.insert((parent_class_id, *method_name)) {
                compare_method_to_guide(
                    analyzer,
                    class_info,
                    *method_name,
                    method_info,
                    parent_class_id,
                    parent_method,
                    false,
                    analysis_data,
                );
            }
        }

        for interface_id in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
        {
            let Some(interface_info) = analyzer.codebase.get_class(*interface_id) else {
                continue;
            };

            let Some(interface_method) = interface_info
                .methods
                .get(method_name)
                .or_else(|| get_method_case_insensitive(analyzer, interface_info, method_name))
            else {
                continue;
            };

            if checked_guides.insert((*interface_id, *method_name)) {
                compare_method_to_guide(
                    analyzer,
                    class_info,
                    *method_name,
                    method_info,
                    *interface_id,
                    interface_method,
                    false,
                    analysis_data,
                );
            }
        }

        for trait_id in &class_info.used_traits {
            let Some(trait_info) = analyzer.codebase.get_class(*trait_id) else {
                continue;
            };

            let Some(trait_method) = trait_info
                .methods
                .get(method_name)
                .or_else(|| get_method_case_insensitive(analyzer, trait_info, method_name))
            else {
                continue;
            };

            if !trait_method.is_abstract || !checked_guides.insert((*trait_id, *method_name)) {
                continue;
            }

            compare_method_to_guide(
                analyzer,
                class_info,
                *method_name,
                method_info,
                *trait_id,
                trait_method,
                true,
                analysis_data,
            );
        }
    }

    // Trait abstract methods can be implemented by an inherited parent method.
    // Validate those even when the current class does not redeclare the method.
    for trait_id in &class_info.used_traits {
        let Some(trait_info) = analyzer.codebase.get_class(*trait_id) else {
            continue;
        };

        for (method_name, trait_method) in &trait_info.methods {
            if !trait_method.is_abstract {
                continue;
            }

            if let Some(implementer_method) = class_info
                .methods
                .get(method_name)
                .or_else(|| get_method_case_insensitive(analyzer, class_info, method_name))
            {
                if implementer_method.declaring_class == Some(class_info.name) {
                    continue;
                }

                if implementer_method.declaring_class == Some(*trait_id) {
                    if let Some((_parent_class_id, parent_method)) =
                        find_parent_method(analyzer, class_info, *method_name)
                    {
                        compare_method_to_guide(
                            analyzer,
                            class_info,
                            *method_name,
                            parent_method,
                            *trait_id,
                            trait_method,
                            false,
                            analysis_data,
                        );
                    }

                    continue;
                }

                compare_method_to_guide(
                    analyzer,
                    class_info,
                    *method_name,
                    implementer_method,
                    *trait_id,
                    trait_method,
                    false,
                    analysis_data,
                );

                continue;
            }

            if let Some((_parent_class_id, parent_method)) =
                find_parent_method(analyzer, class_info, *method_name)
            {
                compare_method_to_guide(
                    analyzer,
                    class_info,
                    *method_name,
                    parent_method,
                    *trait_id,
                    trait_method,
                    false,
                    analysis_data,
                );
            }
        }
    }
}

fn check_method_signature_must_omit_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if method_info.signature_return_type.is_none() {
        return;
    }

    if !matches!(
        method_name,
        StrId::CONSTRUCT | StrId::CLONE | StrId::DESTRUCT
    ) {
        return;
    }

    let (line, col) = analyzer.get_line_column(method_info.start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::MethodSignatureMustOmitReturnType,
        format!(
            "Method {} must not declare a return type",
            analyzer.interner.lookup(method_name)
        ),
        analyzer.file_path,
        method_info.start_offset,
        method_info.end_offset,
        line,
        col,
    ));
}

fn compare_method_to_guide(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
    implementer_method: &pzoom_code_info::FunctionLikeInfo,
    guide_class_id: StrId,
    guide_method: &pzoom_code_info::FunctionLikeInfo,
    guide_is_trait: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(guide_class_info) = analyzer.codebase.get_class(guide_class_id) else {
        return;
    };

    let implementer_method_id = format_method_id(analyzer, class_info.name, method_name);
    let guide_method_id = format_method_id(analyzer, guide_class_id, method_name);
    let enforce_constructor_signature = method_name != StrId::CONSTRUCT
        || guide_class_info.kind == ClassLikeKind::Interface
        || guide_class_info.is_consistent_constructor;

    let base_mismatch_kind =
        if guide_is_trait && implementer_method.declaring_class == Some(class_info.name) {
            IssueKind::TraitMethodSignatureMismatch
        } else {
            IssueKind::MethodSignatureMismatch
        };

    if guide_method.is_final {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMismatch,
            format!(
                "Method {} overrides final method {}",
                implementer_method_id, guide_method_id
            ),
        );
    }

    if is_visibility_more_restrictive(implementer_method.visibility, guide_method.visibility) {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::OverriddenMethodAccess,
            format!(
                "Overridden method {} has incorrect access level",
                implementer_method_id
            ),
        );
    }

    if implementer_method.is_abstract
        && !guide_method.is_abstract
        && guide_class_info.kind == ClassLikeKind::Class
        && !guide_class_info.is_abstract
    {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMismatch,
            format!(
                "Method {} cannot be abstract when inherited method {} is non-abstract",
                implementer_method_id, guide_method_id
            ),
        );
    }

    if guide_method.returns_by_ref && !implementer_method.returns_by_ref {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMismatch,
            format!("Method {} must return by-reference", implementer_method_id),
        );
    }

    if implementer_method.is_static != guide_method.is_static {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            base_mismatch_kind,
            format!(
                "Method {} is{} static while inherited method {} is{}",
                implementer_method_id,
                if implementer_method.is_static {
                    ""
                } else {
                    " not"
                },
                guide_method_id,
                if guide_method.is_static { "" } else { " not" }
            ),
        );
    }

    if !enforce_constructor_signature {
        return;
    }

    let specialize_for_comparison = |union: &TUnion| {
        let specialized =
            replace_extended_templates_in_union(union, &class_info.template_extended_params);

        localize_special_class_names_for_final_class(
            &specialized,
            class_info.name,
            class_info.parent_class,
        )
    };

    let guide_class_name = analyzer.interner.lookup(guide_class_id);
    let guide_is_array_access = guide_class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("ArrayAccess");
    let offset_get = analyzer.interner.intern("offsetGet");
    let offset_set = analyzer.interner.intern("offsetSet");
    let offset_exists = analyzer.interner.intern("offsetExists");
    let offset_unset = analyzer.interner.intern("offsetUnset");

    for (param_index, guide_param) in guide_method.params.iter().enumerate() {
        let Some(implementer_param) = implementer_method.params.get(param_index) else {
            if guide_param.is_optional || guide_param.is_variadic {
                continue;
            }

            emit_method_issue(
                analyzer,
                analysis_data,
                implementer_method,
                base_mismatch_kind,
                format!(
                    "Method {} has fewer parameters than parent method {}",
                    implementer_method_id, guide_method_id
                ),
            );
            return;
        };

        let relax_array_access_offset_param = guide_is_array_access
            && param_index == 0
            && (method_name == offset_get
                || method_name == offset_set
                || method_name == offset_exists
                || method_name == offset_unset);

        let should_compare_names = should_compare_param_names(method_name)
            || (method_name == StrId::CONSTRUCT && guide_class_info.is_consistent_constructor);

        if !relax_array_access_offset_param
            && should_compare_names
            && guide_param.name != implementer_param.name
        {
            let guide_param_name =
                normalize_param_name(analyzer.interner.lookup(guide_param.name).as_ref());
            let implementer_param_name =
                normalize_param_name(analyzer.interner.lookup(implementer_param.name).as_ref());

            emit_param_issue(
                analyzer,
                analysis_data,
                implementer_param.start_offset,
                IssueKind::ParamNameMismatch,
                format!(
                    "Argument {} of {} has wrong name {}, expecting {} as defined by {}",
                    param_index + 1,
                    implementer_method_id,
                    implementer_param_name,
                    guide_param_name,
                    guide_method_id
                ),
            );
        }

        let guide_param_signature = guide_param
            .signature_type
            .as_ref()
            .or_else(|| guide_param.get_type());
        let implementer_param_signature = implementer_param
            .signature_type
            .as_ref()
            .or_else(|| implementer_param.get_type());

        if !relax_array_access_offset_param
            && let (Some(guide_signature), Some(implementer_signature)) =
                (guide_param_signature, implementer_param_signature)
        {
            let guide_signature = specialize_for_comparison(guide_signature);
            let implementer_signature = specialize_for_comparison(implementer_signature);
            let mut comparison_result = TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                analyzer.codebase,
                &guide_signature,
                &implementer_signature,
                false,
                false,
                &mut comparison_result,
            ) {
                let issue_kind = if method_name == StrId::CONSTRUCT
                    && guide_class_info.kind == ClassLikeKind::Interface
                {
                    IssueKind::ConstructorSignatureMismatch
                } else {
                    base_mismatch_kind
                };

                emit_param_issue(
                    analyzer,
                    analysis_data,
                    implementer_param.start_offset,
                    issue_kind,
                    format!(
                        "Argument {} of {} has wrong type \\'{}\\', expecting \\'{}\\' as defined by {}",
                        param_index + 1,
                        implementer_method_id,
                        implementer_signature.get_id(Some(analyzer.interner)),
                        guide_signature.get_id(Some(analyzer.interner)),
                        guide_method_id
                    ),
                );
            }
        }

        if !relax_array_access_offset_param
            && let (Some(guide_param_type), Some(implementer_param_type)) =
                (guide_param.get_type(), implementer_param.get_type())
        {
            let guide_param_type = specialize_for_comparison(guide_param_type);
            let implementer_param_type = specialize_for_comparison(implementer_param_type);

            if guide_param_type != implementer_param_type {
                let mut comparison_result = TypeComparisonResult::new();
                let is_compatible = union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &guide_param_type,
                    &implementer_param_type,
                    false,
                    false,
                    &mut comparison_result,
                );

                if !is_compatible {
                    let mut reverse_comparison = TypeComparisonResult::new();
                    let implementer_is_subset = union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &implementer_param_type,
                        &guide_param_type,
                        false,
                        false,
                        &mut reverse_comparison,
                    );

                    if comparison_result.type_coerced.unwrap_or(false) || implementer_is_subset {
                        emit_param_issue(
                            analyzer,
                            analysis_data,
                            implementer_param.start_offset,
                            IssueKind::MoreSpecificImplementedParamType,
                            format!(
                                "Argument {} of {} has the more specific type '{}', expecting '{}' as defined by {}",
                                param_index + 1,
                                implementer_method_id,
                                implementer_param_type.get_id(Some(analyzer.interner)),
                                guide_param_type.get_id(Some(analyzer.interner)),
                                guide_method_id
                            ),
                        );
                    }
                }
            }
        }

        if implementer_param.by_ref != guide_param.by_ref {
            emit_param_issue(
                analyzer,
                analysis_data,
                implementer_param.start_offset,
                base_mismatch_kind,
                format!(
                    "Argument {} of {} is{} passed by reference, but argument {} of {} is{}",
                    param_index + 1,
                    implementer_method_id,
                    if implementer_param.by_ref { "" } else { " not" },
                    param_index + 1,
                    guide_method_id,
                    if guide_param.by_ref { "" } else { " not" }
                ),
            );
        }
    }

    let required_guide_params = guide_method
        .params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();
    let required_implementer_params = implementer_method
        .params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    if required_implementer_params > required_guide_params {
        let issue_kind = if method_name == StrId::CONSTRUCT
            && guide_class_info.kind == ClassLikeKind::Interface
        {
            IssueKind::ConstructorSignatureMismatch
        } else {
            base_mismatch_kind
        };

        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            issue_kind,
            format!(
                "Method {} has more required parameters than parent method {}",
                implementer_method_id, guide_method_id
            ),
        );
    }

    let guide_return_type = guide_method
        .signature_return_type
        .as_ref()
        .or_else(|| guide_method.return_type.as_ref());

    let inherited_return_fallback = if implementer_method.signature_return_type.is_none()
        && implementer_method.return_type.is_none()
        && implementer_method.declaring_class == Some(class_info.name)
    {
        get_specialized_inherited_return_type(analyzer, class_info, method_name)
    } else {
        None
    };

    let implementer_return_type = implementer_method
        .signature_return_type
        .as_ref()
        .or_else(|| implementer_method.return_type.as_ref())
        .or(inherited_return_fallback.as_ref());

    let return_mismatch_issue_kind = if inherited_return_fallback.is_some() {
        IssueKind::InvalidReturnType
    } else if guide_class_info.kind == ClassLikeKind::Interface
        && implementer_method.signature_return_type.is_none()
        && implementer_method.return_type.is_some()
    {
        IssueKind::ImplementedReturnTypeMismatch
    } else {
        base_mismatch_kind
    };

    if let (Some(guide_return_type), Some(implementer_return_type)) =
        (guide_return_type, implementer_return_type)
    {
        let guide_return_type = specialize_for_comparison(guide_return_type);
        let implementer_return_type = specialize_for_comparison(implementer_return_type);
        let mut comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &implementer_return_type,
            &guide_return_type,
            false,
            false,
            &mut comparison_result,
        ) {
            emit_method_issue(
                analyzer,
                analysis_data,
                implementer_method,
                return_mismatch_issue_kind,
                format!(
                    "Method {} with return type '{}' is different to return type '{}' of inherited method {}",
                    implementer_method_id,
                    implementer_return_type.get_id(Some(analyzer.interner)),
                    guide_return_type.get_id(Some(analyzer.interner)),
                    guide_method_id
                ),
            );
        }
    }
}

fn should_compare_param_names(method_name: StrId) -> bool {
    !matches!(
        method_name,
        StrId::CONSTRUCT
            | StrId::DESTRUCT
            | StrId::CLONE
            | StrId::CALL
            | StrId::CALL_STATIC
            | StrId::GET
            | StrId::SET
            | StrId::ISSET
            | StrId::UNSET
            | StrId::SLEEP
            | StrId::WAKEUP
            | StrId::TO_STRING
            | StrId::INVOKE
            | StrId::SET_STATE
            | StrId::DEBUG_INFO
            | StrId::MAGIC_SERIALIZE
            | StrId::SERIALIZE
            | StrId::UNSERIALIZE
    )
}

fn normalize_param_name(name: &str) -> String {
    if name.starts_with('$') {
        name.to_string()
    } else {
        format!("${}", name)
    }
}

fn format_method_id(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    method_name: StrId,
) -> String {
    format!(
        "{}::{}",
        analyzer.interner.lookup(class_id),
        analyzer.interner.lookup(method_name)
    )
}

fn emit_method_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    kind: IssueKind,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(method_info.start_offset);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        method_info.start_offset,
        method_info.end_offset,
        line,
        col,
    ));
}

fn emit_param_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start_offset: u32,
    kind: IssueKind,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(start_offset);
    analysis_data.add_issue(Issue::new(
        kind,
        message,
        analyzer.file_path,
        start_offset,
        start_offset.saturating_add(1),
        line,
        col,
    ));
}

fn find_parent_property<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    mut parent_class: Option<StrId>,
    property_name: StrId,
) -> Option<&'a pzoom_code_info::class_like_info::PropertyInfo> {
    while let Some(parent_id) = parent_class {
        let parent_info = analyzer.codebase.get_class(parent_id)?;
        if let Some(property_info) = parent_info.properties.get(&property_name) {
            return Some(property_info);
        }
        parent_class = parent_info.parent_class;
    }

    None
}

fn check_property_override_visibility(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (property_name, property_info) in &class_info.properties {
        if property_info.declaring_class != class_info.name {
            continue;
        }

        let Some(parent_property) =
            find_parent_property(analyzer, class_info.parent_class, *property_name)
        else {
            continue;
        };

        if parent_property.visibility == Visibility::Private {
            continue;
        }

        if is_visibility_more_restrictive(property_info.visibility, parent_property.visibility) {
            let (line, col) = analyzer.get_line_column(property_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::OverriddenPropertyAccess,
                format!(
                    "Overridden property {}::${} has incorrect access level",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(*property_name)
                ),
                analyzer.file_path,
                property_info.start_offset,
                property_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn check_property_type_invariance(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (property_name, property_info) in &class_info.properties {
        if property_info.declaring_class != class_info.name {
            continue;
        }

        let Some(parent_property) =
            find_parent_property(analyzer, class_info.parent_class, *property_name)
        else {
            continue;
        };

        if parent_property.visibility == Visibility::Private {
            continue;
        }

        let child_signature = property_info.signature_type.as_ref();
        let parent_signature = parent_property.signature_type.as_ref();
        let has_signature_variance = child_signature != parent_signature;

        if has_signature_variance {
            let (line, col) = analyzer.get_line_column(property_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::NonInvariantPropertyType,
                format!(
                    "Property {}::${} has non-invariant signature type",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(*property_name)
                ),
                analyzer.file_path,
                property_info.start_offset,
                property_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }

        let Some(child_type) = property_info.get_type() else {
            continue;
        };

        let mut parent_type = parent_property
            .get_type()
            .cloned()
            .unwrap_or_else(TUnion::mixed);
        if let Some(parent_template_replacements) = class_info
            .template_extended_params
            .get(&parent_property.declaring_class)
            && let Some(parent_declaring_info) =
                analyzer.codebase.get_class(parent_property.declaring_class)
        {
            let parent_template_defaults =
                function_call_analyzer::get_class_template_defaults(parent_declaring_info);
            parent_type = function_call_analyzer::replace_templates_in_union(
                &parent_type,
                parent_template_replacements,
                &parent_template_defaults,
            );
        }
        if has_signature_variance && !child_type.from_docblock && !parent_type.from_docblock {
            continue;
        }

        let mut child_to_parent = TypeComparisonResult::new();
        let child_contained_by_parent = union_type_comparator::is_contained_by(
            analyzer.codebase,
            child_type,
            &parent_type,
            false,
            false,
            &mut child_to_parent,
        );

        let mut parent_to_child = TypeComparisonResult::new();
        let parent_contained_by_child = union_type_comparator::is_contained_by(
            analyzer.codebase,
            &parent_type,
            child_type,
            false,
            false,
            &mut parent_to_child,
        );

        if child_contained_by_parent && parent_contained_by_child {
            continue;
        }

        let issue_kind = if child_type.from_docblock || parent_type.from_docblock {
            IssueKind::NonInvariantDocblockPropertyType
        } else {
            IssueKind::NonInvariantPropertyType
        };

        let (line, col) = analyzer.get_line_column(property_info.start_offset);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            format!(
                "Property {}::${} has non-invariant type",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(*property_name)
            ),
            analyzer.file_path,
            property_info.start_offset,
            property_info.start_offset.saturating_add(1),
            line,
            col,
        ));
    }
}

fn check_invalid_traversable_implementation(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.kind != ClassLikeKind::Class {
        return;
    }

    if class_info.is_abstract {
        return;
    }

    let implements_traversable = class_info.interfaces.contains(&StrId::TRAVERSABLE)
        || class_info
            .all_parent_interfaces
            .iter()
            .any(|interface| *interface == StrId::TRAVERSABLE);

    if !implements_traversable {
        return;
    }

    let implements_iterator_family = class_info.interfaces.contains(&StrId::ITERATOR)
        || class_info.interfaces.contains(&StrId::ITERATOR_AGGREGATE)
        || class_info
            .all_parent_interfaces
            .iter()
            .any(|interface| matches!(*interface, StrId::ITERATOR | StrId::ITERATOR_AGGREGATE));

    if implements_iterator_family {
        return;
    }

    let (line, col) = analyzer.get_line_column(class_info.start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidTraversableImplementation,
        format!(
            "Class {} cannot implement Traversable directly",
            analyzer.interner.lookup(class_info.name)
        ),
        analyzer.file_path,
        class_info.start_offset,
        class_info.end_offset,
        line,
        col,
    ));
}

fn check_missing_constructor_for_typed_properties(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.kind != ClassLikeKind::Class || class_info.is_abstract {
        return;
    }

    if class_info.methods.contains_key(&StrId::CONSTRUCT) {
        return;
    }

    let has_relevant_property = class_info.properties.values().any(|property| {
        if property.declaring_class != class_info.name
            || property.is_static
            || property.has_default
            || property.is_promoted
        {
            return false;
        }

        property
            .get_type()
            .is_some_and(union_contains_class_string_like_type)
    });

    if !has_relevant_property {
        return;
    }

    let (line, col) = analyzer.get_line_column(class_info.start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::MissingConstructor,
        format!(
            "Class {} has typed properties that require initialization in a constructor",
            analyzer.interner.lookup(class_info.name)
        ),
        analyzer.file_path,
        class_info.start_offset,
        class_info.end_offset,
        line,
        col,
    ));
}

fn union_contains_class_string_like_type(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. }
        )
    })
}

fn check_immutable_relationships(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let class_name = analyzer.interner.lookup(class_info.name);
    let class_span = class.span();

    if let Some(parent_name) = class_info.parent_class {
        if let Some(parent_info) = analyzer.codebase.get_class(parent_name) {
            let parent_name_str = analyzer.interner.lookup(parent_name);
            let (line, col) = analyzer.get_line_column(class_span.start.offset);

            if parent_info.is_immutable && !class_info.is_immutable {
                analysis_data.add_issue(Issue::new(
                    IssueKind::MissingImmutableAnnotation,
                    format!(
                        "{} is marked @psalm-immutable, but {} is not marked @psalm-immutable",
                        parent_name_str, class_name
                    ),
                    analyzer.file_path,
                    class_span.start.offset,
                    class_span.end.offset,
                    line,
                    col,
                ));
            }

            if class_info.is_immutable && !parent_info.is_immutable {
                analysis_data.add_issue(Issue::new(
                    IssueKind::MutableDependency,
                    format!(
                        "{} is marked @psalm-immutable but {} is not",
                        class_name, parent_name_str
                    ),
                    analyzer.file_path,
                    class_span.start.offset,
                    class_span.end.offset,
                    line,
                    col,
                ));
            }
        }
    }

    if !class_info.is_immutable {
        for iface_name in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
        {
            if let Some(iface_info) = analyzer.codebase.get_class(*iface_name) {
                if iface_info.is_immutable {
                    let iface_name_str = analyzer.interner.lookup(*iface_name);
                    let (line, col) = analyzer.get_line_column(class_span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MissingImmutableAnnotation,
                        format!(
                            "{} is marked @psalm-immutable, but {} is not marked @psalm-immutable",
                            iface_name_str, class_name
                        ),
                        analyzer.file_path,
                        class_span.start.offset,
                        class_span.end.offset,
                        line,
                        col,
                    ));
                    break;
                }
            }
        }
    }

    if class_info.is_immutable {
        for trait_name in &class_info.used_traits {
            if let Some(trait_info) = analyzer.codebase.get_class(*trait_name) {
                if !trait_info.is_immutable {
                    let trait_name_str = analyzer.interner.lookup(*trait_name);
                    let (line, col) = analyzer.get_line_column(class_span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MutableDependency,
                        format!(
                            "{} is marked @psalm-immutable but {} is not",
                            class_name, trait_name_str
                        ),
                        analyzer.file_path,
                        class_span.start.offset,
                        class_span.end.offset,
                        line,
                        col,
                    ));
                    break;
                }
            }
        }
    }
}

/// Analyze a trait declaration with a namespace context.
pub fn analyze_trait_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    trait_stmt: &Trait<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the trait name - use FQN if in a namespace
    let trait_name = trait_stmt.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, trait_name)
    } else {
        trait_name.to_string()
    };
    let trait_name_id = analyzer.interner.intern(&fqn);

    // Look up the trait info from the codebase
    let trait_info = analyzer.codebase.get_class(trait_name_id);

    attribute_analyzer::analyze_interface_or_trait_attributes(
        analyzer,
        trait_stmt.attribute_lists.as_slice(),
        trait_stmt.members.as_slice(),
        trait_info,
        trait_name_id,
        context,
        analysis_data,
    );

    // Check for missing property types in trait-declared properties
    if let Some(info) = trait_info {
        check_missing_dependencies(analyzer, info, context, analysis_data);
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_missing_property_types(analyzer, &fqn, info, analysis_data);
    }

    for member in trait_stmt.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            let issue_count_before = analysis_data.issues.len();
            analyze_method(
                analyzer,
                method,
                trait_name_id,
                trait_info,
                context.namespace,
                analysis_data,
            )?;

            let method_name_id = analyzer.interner.intern(method.name.value);
            let should_emit_return_mismatch = trait_info
                .and_then(|info| info.methods.get(&method_name_id))
                .and_then(|method_info| method_info.get_return_type())
                .is_some_and(|return_type| !union_contains_special_class_names(return_type));

            let new_issues = analysis_data.issues.split_off(issue_count_before);
            let filtered_issues: Vec<_> = new_issues
                .into_iter()
                .filter_map(|mut issue| {
                    if !should_emit_return_mismatch {
                        return None;
                    }

                    if !matches!(
                        issue.kind,
                        IssueKind::InvalidReturnStatement | IssueKind::InvalidReturnType
                    ) {
                        return None;
                    }

                    issue.kind = IssueKind::InvalidReturnType;
                    Some(issue)
                })
                .collect();

            analysis_data.issues.extend(filtered_issues);
        }
    }

    let _ = trait_stmt;

    Ok(())
}

/// Check for properties without type declarations.
fn check_missing_property_types(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.is_consistent_constructor {
        return;
    }

    for (_prop_name, prop_info) in &class_info.properties {
        // Only report properties declared directly on this class.
        // Inherited properties (including from traits/parents) should not be
        // re-reported at each subclass.
        if prop_info.declaring_class != class_info.name {
            continue;
        }

        // If the property overrides a parent declaration, Psalm doesn't re-report
        // missing-type issues on the overriding declaration.
        if find_parent_property(analyzer, class_info.parent_class, prop_info.name).is_some() {
            continue;
        }

        // Skip properties with explicit type declarations (native PHP types or docblocks)
        if prop_info.has_type() {
            continue;
        }

        // Psalm does not emit MissingPropertyType for private properties.
        if prop_info.visibility == Visibility::Private {
            continue;
        }

        // Skip promoted properties (they get their type from constructor param)
        if prop_info.is_promoted {
            continue;
        }

        let prop_name_str = analyzer.interner.lookup(prop_info.name);
        let property_id = format!("{}::${}", class_name, prop_name_str);
        let (line, col) = analyzer.get_line_column(prop_info.start_offset);

        analysis_data.add_issue(Issue::new(
            IssueKind::MissingPropertyType,
            format!("Property {} does not have a declared type", property_id),
            analyzer.file_path,
            prop_info.start_offset,
            prop_info.start_offset + 1,
            line,
            col,
        ));
    }
}

fn check_docblock_issues(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for issue in &class_info.docblock_issues {
        let (line, col) = analyzer.get_line_column(issue.start_offset);
        let issue_kind = if issue.message.eq_ignore_ascii_case("Missing docblock type") {
            IssueKind::MissingDocblockType
        } else if issue
            .message
            .eq_ignore_ascii_case("Possibly invalid docblock tag")
        {
            IssueKind::PossiblyInvalidDocblockTag
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

    for method_info in class_info.methods.values() {
        if method_info.declaring_class != Some(class_info.name) {
            continue;
        }

        for issue in &method_info.docblock_issues {
            let (line, col) = analyzer.get_line_column(issue.start_offset);
            let issue_kind = if issue.message.eq_ignore_ascii_case("Missing docblock type") {
                IssueKind::MissingDocblockType
            } else if issue
                .message
                .eq_ignore_ascii_case("Possibly invalid docblock tag")
            {
                IssueKind::PossiblyInvalidDocblockTag
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

        for assertion in method_info
            .assertions
            .iter()
            .chain(method_info.if_true_assertions.iter())
            .chain(method_info.if_false_assertions.iter())
        {
            let union = match &assertion.assertion_type {
                pzoom_code_info::functionlike_info::AssertionType::IsType(union)
                | pzoom_code_info::functionlike_info::AssertionType::IsEqual(union)
                | pzoom_code_info::functionlike_info::AssertionType::IsLooselyEqual(union)
                | pzoom_code_info::functionlike_info::AssertionType::IsNotType(union)
                | pzoom_code_info::functionlike_info::AssertionType::IsNotEqual(union)
                | pzoom_code_info::functionlike_info::AssertionType::IsNotLooselyEqual(union) => {
                    union
                }
                _ => continue,
            };

            if !assertion_union_has_invalid_negation(union, analyzer) {
                continue;
            }

            let (line, col) = analyzer.get_line_column(method_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidDocblock,
                "Invalid assertion type in docblock".to_string(),
                analyzer.file_path,
                method_info.start_offset,
                method_info.end_offset,
                line,
                col,
            ));
        }
    }
}

fn assertion_union_has_invalid_negation(
    union: &pzoom_code_info::TUnion,
    analyzer: &StatementsAnalyzer<'_>,
) -> bool {
    union.types.iter().any(|atomic| match atomic {
        pzoom_code_info::TAtomic::TNamedObject { name, .. } => {
            analyzer.interner.lookup(*name).contains('!')
        }
        _ => false,
    })
}

fn check_deprecated_and_internal_relationships(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if let Some(parent_id) = class_info.parent_class {
        if let Some(parent_info) = analyzer.codebase.get_class(parent_id) {
            let parent_name = analyzer.interner.lookup(parent_id);
            let (line, col) = analyzer.get_line_column(class_info.start_offset);

            if parent_info.is_deprecated {
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedClass,
                    format!("{} is marked deprecated", parent_name),
                    analyzer.file_path,
                    class_info.start_offset,
                    class_info.start_offset.saturating_add(1),
                    line,
                    col,
                ));
            }

            if !can_class_access_internal(analyzer, class_info.name, &parent_info.internal) {
                let scope_phrase = format_internal_scope_phrase(analyzer, &parent_info.internal);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InternalClass,
                    format!("{} is internal to {}", parent_name, scope_phrase),
                    analyzer.file_path,
                    class_info.start_offset,
                    class_info.start_offset.saturating_add(1),
                    line,
                    col,
                ));
            }
        }
    }

    for interface_id in &class_info.interfaces {
        let Some(interface_info) = analyzer.codebase.get_class(*interface_id) else {
            continue;
        };

        let interface_name = analyzer.interner.lookup(*interface_id);
        let (line, col) = analyzer.get_line_column(class_info.start_offset);

        if interface_info.is_deprecated {
            analysis_data.add_issue(Issue::new(
                IssueKind::DeprecatedInterface,
                format!("{} is marked deprecated", interface_name),
                analyzer.file_path,
                class_info.start_offset,
                class_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }

        if !can_class_access_internal(analyzer, class_info.name, &interface_info.internal) {
            let scope_phrase = format_internal_scope_phrase(analyzer, &interface_info.internal);
            analysis_data.add_issue(Issue::new(
                IssueKind::InternalClass,
                format!("{} is internal to {}", interface_name, scope_phrase),
                analyzer.file_path,
                class_info.start_offset,
                class_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }

    for trait_id in &class_info.used_traits {
        let Some(trait_info) = analyzer.codebase.get_class(*trait_id) else {
            continue;
        };

        if !trait_info.is_deprecated {
            continue;
        }

        let trait_name = analyzer.interner.lookup(*trait_id);
        let (line, col) = analyzer.get_line_column(class_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::DeprecatedTrait,
            format!("Trait {} is deprecated", trait_name),
            analyzer.file_path,
            class_info.start_offset,
            class_info.start_offset.saturating_add(1),
            line,
            col,
        ));
    }

    let mut emitted_template_deprecations: FxHashMap<StrId, ()> = FxHashMap::default();
    for template_args in class_info.template_extended_offsets.values() {
        for template_arg in template_args {
            let mut referenced_classes = Vec::new();
            for atomic in &template_arg.types {
                collect_named_docblock_classes(atomic, &mut referenced_classes);
            }

            for referenced_class in referenced_classes {
                if emitted_template_deprecations
                    .insert(referenced_class, ())
                    .is_some()
                {
                    continue;
                }

                let Some(referenced_info) = analyzer.codebase.get_class(referenced_class) else {
                    continue;
                };

                if !referenced_info.is_deprecated {
                    continue;
                }

                let referenced_name = analyzer.interner.lookup(referenced_class);
                let issue_kind = match referenced_info.kind {
                    pzoom_code_info::class_like_info::ClassLikeKind::Interface => {
                        IssueKind::DeprecatedInterface
                    }
                    pzoom_code_info::class_like_info::ClassLikeKind::Trait => {
                        IssueKind::DeprecatedTrait
                    }
                    _ => IssueKind::DeprecatedClass,
                };

                let (line, col) = analyzer.get_line_column(class_info.start_offset);
                analysis_data.add_issue(Issue::new(
                    issue_kind,
                    format!("{} is marked deprecated", referenced_name),
                    analyzer.file_path,
                    class_info.start_offset,
                    class_info.start_offset.saturating_add(1),
                    line,
                    col,
                ));
            }
        }
    }
}

fn check_undefined_docblock_mixins(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted = FxHashMap::default();

    for mixin_atomic in &class_info.named_mixins {
        let mut referenced_classes = Vec::new();
        collect_named_docblock_classes(mixin_atomic, &mut referenced_classes);

        for mixin_class in referenced_classes {
            let normalized_class = normalize_docblock_class_reference(analyzer, mixin_class);

            if matches!(
                normalized_class,
                StrId::SELF | StrId::STATIC | StrId::PARENT
            ) {
                continue;
            }

            if analyzer.codebase.get_class(normalized_class).is_some() {
                continue;
            }

            if emitted.insert(normalized_class, ()).is_some() {
                continue;
            }

            let (line, col) = analyzer.get_line_column(class_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedDocblockClass,
                format!(
                    "Docblock-defined class {} does not exist",
                    analyzer.interner.lookup(normalized_class)
                ),
                analyzer.file_path,
                class_info.start_offset,
                class_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn check_duplicate_property_declarations(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for duplicate in &class_info.duplicate_property_issues {
        let (line, col) = analyzer.get_line_column(duplicate.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::DuplicateProperty,
            format!(
                "Property {}::${} has already been defined",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(duplicate.property_name)
            ),
            analyzer.file_path,
            duplicate.start_offset,
            duplicate.end_offset,
            line,
            col,
        ));
    }
}

fn check_undefined_docblock_property_types(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted = FxHashSet::default();

    for property_info in class_info.properties.values() {
        if property_info.declaring_class != class_info.name {
            continue;
        }

        let Some(property_type) = property_info.get_type() else {
            continue;
        };

        if !property_type.from_docblock {
            continue;
        }

        let mut referenced_classes = Vec::new();
        for atomic in &property_type.types {
            collect_named_docblock_classes(atomic, &mut referenced_classes);
        }

        for referenced_class in referenced_classes {
            let normalized_class = normalize_docblock_class_reference(analyzer, referenced_class);

            if matches!(
                normalized_class,
                StrId::SELF | StrId::STATIC | StrId::PARENT
            ) {
                continue;
            }

            if !emitted.insert((property_info.name, normalized_class)) {
                continue;
            }

            let issue_message = match analyzer.codebase.get_class(normalized_class) {
                Some(referenced_info) if referenced_info.kind == ClassLikeKind::Trait => {
                    Some(format!(
                        "Docblock class {} cannot be a trait",
                        analyzer.interner.lookup(normalized_class)
                    ))
                }
                Some(_) => None,
                None => Some(format!(
                    "Docblock class {} does not exist",
                    analyzer.interner.lookup(normalized_class)
                )),
            };

            let Some(message) = issue_message else {
                continue;
            };

            let (line, col) = analyzer.get_line_column(property_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedDocblockClass,
                message,
                analyzer.file_path,
                property_info.start_offset,
                property_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

fn collect_named_docblock_classes(atomic: &TAtomic, classes: &mut Vec<StrId>) {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            classes.push(*name);

            if let Some(type_params) = type_params {
                for type_param in type_params {
                    for nested_atomic in &type_param.types {
                        collect_named_docblock_classes(nested_atomic, classes);
                    }
                }
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            for nested_atomic in &as_type.types {
                collect_named_docblock_classes(nested_atomic, classes);
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => {
            collect_named_docblock_classes(as_type, classes);
        }
        TAtomic::TObjectIntersection { types } => {
            for nested_atomic in types {
                collect_named_docblock_classes(nested_atomic, classes);
            }
        }
        _ => {}
    }
}

fn normalize_docblock_class_reference(analyzer: &StatementsAnalyzer<'_>, class_id: StrId) -> StrId {
    let raw_name = analyzer.interner.lookup(class_id);
    let trimmed_name = raw_name.trim();
    let class_name = trimmed_name
        .split_once("::")
        .map_or(trimmed_name, |(class_name, _)| class_name.trim());

    if class_name.eq_ignore_ascii_case("self") {
        return StrId::SELF;
    }
    if class_name.eq_ignore_ascii_case("static") {
        return StrId::STATIC;
    }
    if class_name.eq_ignore_ascii_case("parent") {
        return StrId::PARENT;
    }

    analyzer
        .interner
        .intern(class_name.trim_start_matches('\\'))
}

fn should_suppress_class_issue(
    analyzer: &StatementsAnalyzer<'_>,
    issue_offset: u32,
    issue_names: &[&str],
) -> bool {
    if issue_names
        .iter()
        .any(|issue_name| analyzer.config.is_issue_suppressed(issue_name))
    {
        return true;
    }

    let source = analyzer.source;
    let offset = issue_offset as usize;
    if offset == 0 || offset > source.len() {
        return false;
    }

    let bytes = source.as_bytes();
    let mut cursor = offset;
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }

    if cursor < 2 || &source[cursor - 2..cursor] != "*/" {
        return false;
    }

    let doc_end = cursor;
    let Some(doc_start) = source[..doc_end - 2].rfind("/**") else {
        return false;
    };

    let docblock = &source[doc_start..doc_end];
    docblock
        .split('\n')
        .filter(|line| line.contains("@psalm-suppress"))
        .flat_map(|line| {
            line.split_whitespace()
                .skip_while(|part| *part != "@psalm-suppress")
                .skip(1)
                .flat_map(|part| part.split(','))
                .map(|part| part.trim().trim_end_matches(','))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .any(|suppressed| {
            issue_names
                .iter()
                .any(|issue_name| suppressed == *issue_name)
        })
}

fn check_pseudo_method_compatibility(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (method_name, pseudo_method) in class_info
        .pseudo_methods
        .iter()
        .chain(class_info.pseudo_static_methods.iter())
    {
        let mut parent_candidates = Vec::new();

        if let Some(parent_name) = class_info.parent_class {
            if let Some(parent_info) = analyzer.codebase.get_class(parent_name) {
                if let Some(parent_method) = parent_info
                    .methods
                    .get(method_name)
                    .or_else(|| get_method_case_insensitive(analyzer, parent_info, method_name))
                {
                    parent_candidates.push(parent_method);
                }
            }
        }

        for interface_name in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
        {
            let Some(interface_info) = analyzer.codebase.get_class(*interface_name) else {
                continue;
            };

            if let Some(parent_method) = interface_info
                .methods
                .get(method_name)
                .or_else(|| get_method_case_insensitive(analyzer, interface_info, method_name))
            {
                parent_candidates.push(parent_method);
            }
        }

        for parent_method in parent_candidates {
            if has_param_type_mismatch(analyzer, pseudo_method, parent_method) {
                let (line, col) = analyzer.get_line_column(pseudo_method.start_offset);
                let method_name = analyzer.interner.lookup(*method_name);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImplementedParamTypeMismatch,
                    format!(
                        "Pseudo method {} has incompatible parameter types",
                        method_name
                    ),
                    analyzer.file_path,
                    pseudo_method.start_offset,
                    pseudo_method.end_offset,
                    line,
                    col,
                ));
                break;
            }

            if has_return_type_mismatch(analyzer, pseudo_method, parent_method) {
                let (line, col) = analyzer.get_line_column(pseudo_method.start_offset);
                let method_name = analyzer.interner.lookup(*method_name);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImplementedReturnTypeMismatch,
                    format!("Pseudo method {} has incompatible return type", method_name),
                    analyzer.file_path,
                    pseudo_method.start_offset,
                    pseudo_method.end_offset,
                    line,
                    col,
                ));
                break;
            }
        }
    }
}

fn get_method_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a pzoom_code_info::ClassLikeInfo,
    method_name: &StrId,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let target = analyzer.interner.lookup(*method_name);

    class_info.methods.iter().find_map(|(stored, method_info)| {
        analyzer
            .interner
            .lookup(*stored)
            .as_ref()
            .eq_ignore_ascii_case(target.as_ref())
            .then_some(method_info)
    })
}

fn has_param_type_mismatch(
    analyzer: &StatementsAnalyzer<'_>,
    pseudo_method: &pzoom_code_info::FunctionLikeInfo,
    parent_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let shared_len = usize::min(pseudo_method.params.len(), parent_method.params.len());

    for idx in 0..shared_len {
        let Some(parent_param_type) = parent_method.params[idx].get_type() else {
            continue;
        };
        let Some(pseudo_param_type) = pseudo_method.params[idx].get_type() else {
            continue;
        };

        let mut comparison_result = TypeComparisonResult::new();
        let is_compatible = union_type_comparator::is_contained_by(
            analyzer.codebase,
            parent_param_type,
            pseudo_param_type,
            false,
            false,
            &mut comparison_result,
        );

        if !is_compatible {
            return true;
        }
    }

    false
}

fn has_return_type_mismatch(
    analyzer: &StatementsAnalyzer<'_>,
    pseudo_method: &pzoom_code_info::FunctionLikeInfo,
    parent_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let Some(parent_return_type) = parent_method.get_return_type() else {
        return false;
    };
    let Some(pseudo_return_type) = pseudo_method.get_return_type() else {
        return false;
    };

    let mut comparison_result = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        pseudo_return_type,
        parent_return_type,
        false,
        false,
        &mut comparison_result,
    )
}

/// Check for unimplemented abstract methods.
fn check_unimplemented_abstract_methods(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    // Collect all implemented methods in this class
    let mut implemented_methods = rustc_hash::FxHashSet::default();
    for (method_name, method_info) in &class_info.methods {
        if method_info.is_abstract {
            continue;
        }

        let declared_on_interface = method_info.declaring_class.is_some_and(|declaring_class| {
            analyzer
                .codebase
                .get_class(declaring_class)
                .is_some_and(|declaring_info| declaring_info.kind == ClassLikeKind::Interface)
        });

        if !declared_on_interface {
            implemented_methods.insert(*method_name);
        }
    }

    // Check parent class for abstract methods
    if let Some(parent_name) = class_info.parent_class {
        if let Some(parent_info) = analyzer.codebase.get_class(parent_name) {
            for (method_name, method_info) in &parent_info.methods {
                if method_info.is_abstract && !implemented_methods.contains(method_name) {
                    let method_name_str = analyzer.interner.lookup(*method_name);
                    let parent_name_str = analyzer.interner.lookup(parent_name);
                    let span = class.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UnimplementedAbstractMethod,
                        format!(
                            "Class {} does not implement abstract method {}::{}",
                            class.name.value, parent_name_str, method_name_str
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    // Check interfaces for unimplemented methods
    let mut seen_ifaces = rustc_hash::FxHashSet::default();
    for iface_name in class_info
        .interfaces
        .iter()
        .chain(class_info.all_parent_interfaces.iter())
    {
        if !seen_ifaces.insert(*iface_name) {
            continue;
        }

        if let Some(iface_info) = analyzer.codebase.get_class(*iface_name) {
            for (method_name, _method_info) in &iface_info.methods {
                if !implemented_methods.contains(method_name) {
                    let method_name_str = analyzer.interner.lookup(*method_name);
                    let iface_name_str = analyzer.interner.lookup(*iface_name);
                    let span = class.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UnimplementedInterfaceMethod,
                        format!(
                            "Class {} does not implement interface method {}::{}",
                            class.name.value, iface_name_str, method_name_str
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }
        }
    }
}

/// Analyze a method declaration.
fn analyze_methods_from_used_traits(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    class_name_id: StrId,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    let mut analyzed_traits = FxHashSet::default();

    for trait_id in &class_info.used_traits {
        analyze_methods_from_trait(
            analyzer,
            class_info,
            class_name_id,
            *trait_id,
            analysis_data,
            &mut analyzed_traits,
        )?;
    }

    Ok(())
}

fn analyze_methods_from_trait(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    class_name_id: StrId,
    trait_id: StrId,
    analysis_data: &mut FunctionAnalysisData,
    analyzed_traits: &mut FxHashSet<StrId>,
) -> Result<(), AnalysisError> {
    if !analyzed_traits.insert(trait_id) {
        return Ok(());
    }

    let Some(trait_info) = analyzer.codebase.get_class(trait_id) else {
        return Ok(());
    };

    if trait_info.kind != ClassLikeKind::Trait {
        return Ok(());
    }

    let class_name = analyzer.interner.lookup(class_name_id);
    let mut stricter_override_methods = FxHashSet::default();
    let mut too_few_argument_needles = Vec::new();

    for (method_name, trait_method_info) in &trait_info.methods {
        let Some(class_method_info) = class_info.methods.get(method_name) else {
            continue;
        };

        if class_method_info.declaring_class != Some(class_name_id) {
            continue;
        }

        if required_param_count(class_method_info) <= required_param_count(trait_method_info) {
            continue;
        }

        stricter_override_methods.insert(*method_name);
        too_few_argument_needles.push(format!(
            "{}::{}",
            class_name,
            analyzer.interner.lookup(*method_name)
        ));
    }

    for nested_trait_id in &trait_info.used_traits {
        analyze_methods_from_trait(
            analyzer,
            class_info,
            class_name_id,
            *nested_trait_id,
            analysis_data,
            analyzed_traits,
        )?;
    }

    if stricter_override_methods.is_empty() {
        return Ok(());
    }

    let Some(trait_file_info) = analyzer.codebase.files.get(&trait_info.file_path) else {
        return Ok(());
    };

    let trait_path = analyzer.interner.lookup(trait_info.file_path);
    let arena = Bump::new();
    let file_id = FileId::new(&*trait_path);
    let (program, _parse_error) = parse_file_content(&arena, file_id, &trait_file_info.contents);
    let resolved_names = resolve_names(&program, analyzer.interner);

    let Some((trait_stmt, trait_namespace)) = find_trait_statement_by_offset(
        program.statements.as_slice(),
        trait_info.start_offset,
        analyzer.interner,
        None,
    ) else {
        return Ok(());
    };

    let trait_analyzer = StatementsAnalyzer::new(
        analyzer.codebase,
        analyzer.interner,
        trait_info.file_path,
        &trait_file_info.contents,
        &resolved_names,
        analyzer.config,
    );

    for member in trait_stmt.members.iter() {
        let ClassLikeMember::Method(method) = member else {
            continue;
        };

        let method_name_id = analyzer.interner.intern(method.name.value);
        if class_info
            .methods
            .get(&method_name_id)
            .is_some_and(|method_info| method_info.declaring_class == Some(class_name_id))
        {
            continue;
        }

        let issue_count_before = analysis_data.issues.len();
        analyze_method(
            &trait_analyzer,
            method,
            class_name_id,
            Some(class_info),
            trait_namespace,
            analysis_data,
        )?;

        if analysis_data.issues.len() == issue_count_before {
            continue;
        }

        let new_issues = analysis_data.issues.split_off(issue_count_before);
        let filtered_issues: Vec<_> = new_issues
            .into_iter()
            .filter(|issue| {
                issue.kind == IssueKind::TooFewArguments
                    && too_few_argument_needles
                        .iter()
                        .any(|needle| issue.message.contains(needle))
            })
            .collect();

        analysis_data.issues.extend(filtered_issues);
    }

    Ok(())
}

fn find_trait_statement_by_offset<'a>(
    statements: &'a [Statement<'a>],
    trait_start_offset: u32,
    interner: &pzoom_str::Interner,
    namespace: Option<StrId>,
) -> Option<(&'a Trait<'a>, Option<StrId>)> {
    for statement in statements {
        match statement {
            Statement::Trait(trait_stmt) => {
                if trait_stmt.span().start.offset == trait_start_offset {
                    return Some((trait_stmt, namespace));
                }
            }
            Statement::Namespace(namespace_stmt) => {
                let next_namespace = namespace_stmt
                    .name
                    .as_ref()
                    .map(|name| interner.intern(name.value()));
                let nested_statements = match &namespace_stmt.body {
                    NamespaceBody::Implicit(body) => body.statements.as_slice(),
                    NamespaceBody::BraceDelimited(body) => body.statements.as_slice(),
                };

                if let Some(found) = find_trait_statement_by_offset(
                    nested_statements,
                    trait_start_offset,
                    interner,
                    next_namespace,
                ) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }

    None
}

fn analyze_method(
    analyzer: &StatementsAnalyzer<'_>,
    method: &Method<'_>,
    class_name_id: pzoom_str::StrId,
    class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    namespace: Option<pzoom_str::StrId>,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    // Get the method name
    let method_name = method.name.value;
    let method_name_id = analyzer.interner.intern(method_name);

    // Look up the method info from the class
    let method_info = class_info.and_then(|ci| ci.methods.get(&method_name_id));

    if let Some(info) = method_info {
        check_invalid_param_defaults_for_method(
            analyzer,
            class_name_id,
            method_name_id,
            info,
            analysis_data,
        );
    }

    // Create a function-like info wrapper for the method
    let func_info = method_info.cloned().map(|mut mi| {
        if mi.return_type.is_none() && mi.signature_return_type.is_none() {
            if let Some(current_class_info) = class_info {
                if let Some(inherited_return_type) = get_specialized_inherited_return_type(
                    analyzer,
                    current_class_info,
                    method_name_id,
                ) {
                    let inherited_return_type = if current_class_info.is_final {
                        localize_special_class_names_for_final_class(
                            &inherited_return_type,
                            current_class_info.name,
                            current_class_info.parent_class,
                        )
                    } else {
                        inherited_return_type
                    };

                    mi.return_type = Some(inherited_return_type);
                }
            }
        }

        mi.name = method_name_id;
        mi.declaring_class = Some(class_name_id);
        mi
    });

    // Create a new analyzer with the method context
    let method_analyzer = StatementsAnalyzer {
        codebase: analyzer.codebase,
        interner: analyzer.interner,
        function_info: func_info.as_ref(),
        file_path: analyzer.file_path,
        source: analyzer.source,
        resolved_names: analyzer.resolved_names,
        config: analyzer.config,
    };

    // Create a new context for the method body with namespace preserved
    let mut method_context = BlockContext::new();
    method_context.namespace = namespace;
    method_context.self_class = Some(class_name_id);
    method_context.parent_class = class_info.and_then(|ci| ci.parent_class);

    // Add $this if not static
    if !method_info.is_some_and(|mi| mi.is_static) {
        let default_this_type = {
            let this_type_params = class_info.and_then(|ci| {
                if ci.template_types.is_empty() {
                    return None;
                }

                Some(
                    ci.template_types
                        .iter()
                        .map(|template_type| {
                            TUnion::new(TAtomic::TTemplateParam {
                                name: template_type.name,
                                defining_entity: ci.name,
                                as_type: Box::new(template_type.as_type.clone()),
                            })
                        })
                        .collect(),
                )
            });

            TUnion::new(pzoom_code_info::TAtomic::TNamedObject {
                name: class_name_id,
                type_params: this_type_params,
            })
        };

        let this_type = method_info
            .and_then(|info| info.if_this_is_type.clone())
            .unwrap_or(default_this_type);

        let this_id = pzoom_str::StrId::THIS_VAR;
        method_context.set_var_type(this_id, this_type);
    }

    // Add parameters to context
    let no_named_arguments = method_info.is_some_and(|info| info.no_named_arguments);
    for (param_index, param) in method.parameter_list.parameters.iter().enumerate() {
        let param_name = param.variable.name;
        let param_name_id = analyzer.interner.intern(param_name);

        // Get parameter info from method info
        let param_info =
            method_info.and_then(|mi| mi.params.iter().find(|p| p.name == param_name_id));

        // Get parameter type - for variadic params, wrap in array type
        let mut param_type = if let Some(info) = param_info {
            let mut base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);
            if let Some(signature_type) = &info.signature_type {
                if !info.has_docblock_type {
                    base_type.from_docblock = false;
                } else {
                    base_type.from_docblock =
                        should_preserve_docblock_param_origin(signature_type, &base_type);
                }
            }

            if !info.has_docblock_type {
                if let Some(current_class_info) = class_info {
                    if let Some(inherited_param_type) = get_specialized_inherited_param_type(
                        analyzer,
                        current_class_info,
                        method_name_id,
                        param_index,
                    ) {
                        base_type = inherited_param_type;
                    }
                }
            }

            if info.is_variadic {
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

        let source_kind =
            if method_info.is_some_and(|info| matches!(info.visibility, Visibility::Private)) {
                VariableSourceKind::PrivateParam
            } else {
                VariableSourceKind::NonPrivateParam
            };

        let param_span = param.variable.span();
        let parent_node = DataFlowNode::get_for_variable_source(
            source_kind,
            VarId(param_name_id),
            make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            method_info.is_some_and(|info| info.is_pure),
            !param_type.parent_nodes.is_empty(),
            false,
            false,
            false,
        );
        analysis_data.data_flow_graph.add_node(parent_node.clone());
        param_type.parent_nodes = vec![parent_node];

        method_context.set_var_type(param_name_id, param_type.clone());
        if let Some(alt_param_name_id) = get_alternate_param_var_id(analyzer, param_name)
            && alt_param_name_id != param_name_id
        {
            method_context.set_var_type(alt_param_name_id, param_type.clone());
        }
    }

    // Analyze parameter default expressions in method scope.
    for param in method.parameter_list.parameters.iter() {
        let Some(default_value) = &param.default_value else {
            continue;
        };
        let _ = expression_analyzer::analyze(
            &method_analyzer,
            &default_value.value,
            analysis_data,
            &mut method_context,
        );
    }

    // Analyze the method body (only if it has a concrete body)
    if let MethodBody::Concrete(block) = &method.body {
        let yield_types_start = analysis_data.inferred_yield_types.len();
        let return_types_start = analysis_data.inferred_return_types.len();
        stmt_analyzer::analyze_stmts(
            &method_analyzer,
            block.statements.as_slice(),
            analysis_data,
            &mut method_context,
        )?;
        let has_yield = analysis_data.inferred_yield_types.len() > yield_types_start;

        if let Some(info) = method_info {
            emit_invalid_by_ref_param_out_types_for_method(
                &method_analyzer,
                info,
                &method_context,
                analysis_data,
            );
        }

        maybe_emit_invalid_to_string_issue_for_method(
            analyzer,
            class_info,
            method_info,
            method_name_id,
            &method_context,
            method.span().start.offset as u32,
            return_types_start,
            analysis_data,
        );

        if let Some(info) = method_info {
            maybe_emit_missing_method_return_issue(
                &method_analyzer,
                info,
                &method_context,
                analysis_data,
                method.span().start.offset as u32,
                class_name_id,
                method_name_id,
                has_yield,
                !block.statements.is_empty(),
            );
        }
    }

    Ok(())
}

fn maybe_emit_invalid_to_string_issue_for_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    method_info: Option<&pzoom_code_info::FunctionLikeInfo>,
    method_name_id: StrId,
    method_context: &BlockContext,
    issue_offset: u32,
    return_types_start: usize,
    analysis_data: &mut FunctionAnalysisData,
) {
    if method_name_id != StrId::TO_STRING
        || class_info.is_some_and(|info| info.kind == ClassLikeKind::Interface)
    {
        return;
    }

    let declared_return = method_info.and_then(|info| {
        info.signature_return_type
            .as_ref()
            .or(info.return_type.as_ref())
            .cloned()
    });

    let effective_return = if let Some(declared_return) = declared_return {
        declared_return
    } else {
        let inferred = &analysis_data.inferred_return_types[return_types_start..];
        if inferred.is_empty() {
            if method_context.has_returned {
                return;
            }
            emit_invalid_to_string_issue(
                analyzer,
                issue_offset,
                analysis_data,
                "missing return type",
            );
            return;
        }

        let mut combined = inferred[0].clone();
        for inferred_return in &inferred[1..] {
            combined = pzoom_code_info::combine_union_types(&combined, inferred_return, false);
        }
        combined
    };

    if !union_is_string_return_type(&effective_return) {
        emit_invalid_to_string_issue(
            analyzer,
            issue_offset,
            analysis_data,
            &format!(
                "invalid return type {}",
                effective_return.get_id(Some(analyzer.interner))
            ),
        );
    }
}

fn emit_invalid_by_ref_param_out_types_for_method(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    for param in &method_info.params {
        if !param.by_ref || param.is_variadic {
            continue;
        }

        let Some(actual_type) = context.get_var_type(param.name) else {
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

        // Avoid false positives when analysis widens the tracked local type
        // but it still covers all values permitted by the by-ref out type.
        let mut reverse_comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            expected_type,
            actual_type,
            false,
            false,
            &mut reverse_comparison,
        ) {
            continue;
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

fn emit_invalid_to_string_issue(
    analyzer: &StatementsAnalyzer<'_>,
    issue_offset: u32,
    analysis_data: &mut FunctionAnalysisData,
    detail: &str,
) {
    let (line, col) = analyzer.get_line_column(issue_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidToString,
        format!("Method __toString has {}", detail),
        analyzer.file_path,
        issue_offset,
        issue_offset + 1,
        line,
        col,
    ));
}

fn required_param_count(function_like_info: &pzoom_code_info::FunctionLikeInfo) -> usize {
    function_like_info
        .params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count()
}

fn maybe_emit_missing_method_return_issue(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    issue_offset: u32,
    class_name_id: StrId,
    method_name_id: StrId,
    has_yield: bool,
    has_statements: bool,
) {
    if analyzer
        .codebase
        .files
        .get(&analyzer.file_path)
        .is_some_and(|file_info| file_info.is_stub)
    {
        return;
    }

    let Some(expected_return_type) = method_info.get_return_type() else {
        return;
    };

    if context.has_returned
        || method_info.is_abstract
        || has_yield
        || !has_statements
        || expected_return_type.is_nullable
        || expected_return_type.is_void()
        || expected_return_type.is_mixed()
        || expected_return_type.is_nothing()
    {
        return;
    }

    let issue_kind = if method_info.signature_return_type.is_some() {
        IssueKind::InvalidReturnType
    } else {
        IssueKind::InvalidNullableReturnType
    };

    let class_name = analyzer.interner.lookup(class_name_id);
    let method_name = analyzer.interner.lookup(method_name_id);
    let (line, col) = analyzer.get_line_column(issue_offset);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        format!(
            "Not all code paths of {}::{} end in a return statement, expected {}",
            class_name,
            method_name,
            expected_return_type.get_id(Some(analyzer.interner))
        ),
        analyzer.file_path,
        issue_offset,
        issue_offset + 1,
        line,
        col,
    ));
}

fn check_invalid_param_defaults_for_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_name_id: StrId,
    method_name_id: StrId,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (idx, param) in method_info.params.iter().enumerate() {
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
                .filter(|signature_type| signature_type.is_nullable || signature_type.is_null())
                .unwrap_or(param_type)
        } else {
            param_type
        };

        if union_has_callable_like(default_check_param_type) && !default_type.is_null() {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type for callable argument {} of method {}::{} can only be null, {} specified",
                    idx + 1,
                    analyzer.interner.lookup(class_name_id),
                    analyzer.interner.lookup(method_name_id),
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
        let default_is_valid = union_type_comparator::is_contained_by(
            analyzer.codebase,
            default_type,
            default_check_param_type,
            false,
            false,
            &mut comparison_result,
        );

        if !default_is_valid {
            let (line, col) = analyzer.get_line_column(param.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParamDefault,
                format!(
                    "Default value type {} for argument {} of method {}::{} does not match the given type {}",
                    default_type.get_id(Some(analyzer.interner)),
                    idx + 1,
                    analyzer.interner.lookup(class_name_id),
                    analyzer.interner.lookup(method_name_id),
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

fn union_has_callable_like(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }))
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

fn get_specialized_inherited_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
    param_index: usize,
) -> Option<TUnion> {
    if let Some(parent_class) = class_info.parent_class {
        if let Some(inherited_type) =
            get_param_type_from_classlike(analyzer, parent_class, method_name, param_index)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    for interface_name in &class_info.interfaces {
        if let Some(inherited_type) =
            get_param_type_from_classlike(analyzer, *interface_name, method_name, param_index)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    for interface_name in &class_info.all_parent_interfaces {
        if let Some(inherited_type) =
            get_param_type_from_classlike(analyzer, *interface_name, method_name, param_index)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    None
}

fn get_specialized_inherited_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
) -> Option<TUnion> {
    if let Some(parent_class) = class_info.parent_class {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, parent_class, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    for interface_name in &class_info.interfaces {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *interface_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    for interface_name in &class_info.all_parent_interfaces {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *interface_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

    None
}

fn get_param_type_from_classlike(
    analyzer: &StatementsAnalyzer<'_>,
    classlike_name: StrId,
    method_name: StrId,
    param_index: usize,
) -> Option<TUnion> {
    let class_storage = analyzer.codebase.get_class(classlike_name)?;
    let method_storage = class_storage.methods.get(&method_name)?;
    let param = method_storage.params.get(param_index)?;
    param.get_type().cloned()
}

fn get_return_type_from_classlike(
    analyzer: &StatementsAnalyzer<'_>,
    classlike_name: StrId,
    method_name: StrId,
) -> Option<TUnion> {
    let class_storage = analyzer.codebase.get_class(classlike_name)?;
    let method_storage = class_storage.methods.get(&method_name)?;

    method_storage
        .return_type
        .clone()
        .or_else(|| method_storage.signature_return_type.clone())
}

fn replace_extended_templates_in_union(
    union: &TUnion,
    template_extended_params: &FxHashMap<StrId, FxHashMap<StrId, TUnion>>,
) -> TUnion {
    if template_extended_params.is_empty() {
        return union.clone();
    }

    let mut changed = false;
    let mut replaced_types = Vec::new();

    for atomic_type in &union.types {
        match atomic_type {
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => {
                if let Some(referenced_type) = template_extended_params
                    .get(defining_entity)
                    .and_then(|map| map.get(name))
                {
                    changed = true;
                    for referenced_atomic in &referenced_type.types {
                        push_unique_atomic(&mut replaced_types, referenced_atomic.clone());
                    }
                } else {
                    let replaced_as_type =
                        replace_extended_templates_in_union(as_type, template_extended_params);
                    if replaced_as_type != **as_type {
                        changed = true;
                    }

                    push_unique_atomic(
                        &mut replaced_types,
                        TAtomic::TTemplateParam {
                            name: *name,
                            defining_entity: *defining_entity,
                            as_type: Box::new(replaced_as_type),
                        },
                    );
                }
            }
            TAtomic::TTemplateParamClass {
                name,
                defining_entity,
                as_type,
            } => {
                if let Some(referenced_type) = template_extended_params
                    .get(defining_entity)
                    .and_then(|map| map.get(name))
                {
                    changed = true;
                    for referenced_atomic in
                        replace_template_param_class_union(referenced_type).types
                    {
                        push_unique_atomic(&mut replaced_types, referenced_atomic);
                    }
                } else {
                    let replaced_as_type =
                        replace_extended_templates_in_atomic(as_type, template_extended_params);
                    if replaced_as_type != **as_type {
                        changed = true;
                    }

                    push_unique_atomic(
                        &mut replaced_types,
                        TAtomic::TTemplateParamClass {
                            name: *name,
                            defining_entity: *defining_entity,
                            as_type: Box::new(replaced_as_type),
                        },
                    );
                }
            }
            _ => {
                let replaced =
                    replace_extended_templates_in_atomic(atomic_type, template_extended_params);
                if replaced != *atomic_type {
                    changed = true;
                }
                push_unique_atomic(&mut replaced_types, replaced);
            }
        }
    }

    if !changed {
        return union.clone();
    }

    let mut replaced_union = TUnion::from_types(replaced_types);
    replaced_union.from_docblock = union.from_docblock;
    replaced_union.is_resolved = union.is_resolved;
    replaced_union.parent_nodes = union.parent_nodes.clone();
    replaced_union.ignore_nullable_issues = union.ignore_nullable_issues;
    replaced_union.ignore_falsable_issues = union.ignore_falsable_issues;
    replaced_union
}

fn replace_extended_templates_in_atomic(
    atomic_type: &TAtomic,
    template_extended_params: &FxHashMap<StrId, FxHashMap<StrId, TUnion>>,
) -> TAtomic {
    match atomic_type {
        TAtomic::TArray {
            key_type,
            value_type,
        } => TAtomic::TArray {
            key_type: Box::new(replace_extended_templates_in_union(
                key_type,
                template_extended_params,
            )),
            value_type: Box::new(replace_extended_templates_in_union(
                value_type,
                template_extended_params,
            )),
        },
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => TAtomic::TNonEmptyArray {
            key_type: Box::new(replace_extended_templates_in_union(
                key_type,
                template_extended_params,
            )),
            value_type: Box::new(replace_extended_templates_in_union(
                value_type,
                template_extended_params,
            )),
        },
        TAtomic::TList { value_type } => TAtomic::TList {
            value_type: Box::new(replace_extended_templates_in_union(
                value_type,
                template_extended_params,
            )),
        },
        TAtomic::TNonEmptyList { value_type } => TAtomic::TNonEmptyList {
            value_type: Box::new(replace_extended_templates_in_union(
                value_type,
                template_extended_params,
            )),
        },
        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            let mut new_properties = rustc_hash::FxHashMap::default();
            for (key, value) in properties {
                new_properties.insert(
                    key.clone(),
                    replace_extended_templates_in_union(value, template_extended_params),
                );
            }

            TAtomic::TKeyedArray {
                properties: new_properties,
                is_list: *is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.as_ref().map(|fallback_key| {
                    Box::new(replace_extended_templates_in_union(
                        fallback_key,
                        template_extended_params,
                    ))
                }),
                fallback_value_type: fallback_value_type.as_ref().map(|fallback_value| {
                    Box::new(replace_extended_templates_in_union(
                        fallback_value,
                        template_extended_params,
                    ))
                }),
            }
        }
        TAtomic::TNamedObject { name, type_params } => TAtomic::TNamedObject {
            name: *name,
            type_params: type_params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| {
                        replace_extended_templates_in_union(param, template_extended_params)
                    })
                    .collect()
            }),
        },
        TAtomic::TObjectIntersection { types } => TAtomic::TObjectIntersection {
            types: types
                .iter()
                .map(|nested| {
                    replace_extended_templates_in_atomic(nested, template_extended_params)
                })
                .collect(),
        },
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: replace_extended_templates_in_union(
                            &param.param_type,
                            template_extended_params,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|ret| {
                Box::new(replace_extended_templates_in_union(
                    ret,
                    template_extended_params,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: replace_extended_templates_in_union(
                            &param.param_type,
                            template_extended_params,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|ret| {
                Box::new(replace_extended_templates_in_union(
                    ret,
                    template_extended_params,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|class_atomic| {
                Box::new(replace_extended_templates_in_atomic(
                    class_atomic,
                    template_extended_params,
                ))
            }),
        },
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(replace_extended_templates_in_union(
                as_type,
                template_extended_params,
            )),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParamClass {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(replace_extended_templates_in_atomic(
                as_type,
                template_extended_params,
            )),
        },
        _ => atomic_type.clone(),
    }
}

fn replace_template_param_class_union(union: &TUnion) -> TUnion {
    let mut class_template_types = Vec::new();

    for atomic in &union.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                push_unique_atomic(
                    &mut class_template_types,
                    TAtomic::TClassString {
                        as_type: Some(Box::new(TAtomic::TNamedObject {
                            name: *name,
                            type_params: None,
                        })),
                    },
                );
            }
            TAtomic::TClassString { .. } | TAtomic::TLiteralClassString { .. } => {
                push_unique_atomic(&mut class_template_types, atomic.clone());
            }
            _ => {}
        }
    }

    if class_template_types.is_empty() {
        TUnion::new(TAtomic::TClassString { as_type: None })
    } else {
        TUnion::from_types(class_template_types)
    }
}

fn push_unique_atomic(types: &mut Vec<TAtomic>, atomic: TAtomic) {
    if !types.contains(&atomic) {
        types.push(atomic);
    }
}

fn union_is_class_constant_reference(union: &TUnion, analyzer: &StatementsAnalyzer<'_>) -> bool {
    !union.types.is_empty()
        && union.types.iter().all(|atomic| match atomic {
            TAtomic::TNamedObject { name, type_params } => {
                type_params.is_none() && analyzer.interner.lookup(*name).contains("::")
            }
            _ => false,
        })
}

fn union_contains_special_class_names(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains_special_class_names)
}

fn atomic_contains_special_class_names(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            if matches!(*name, StrId::SELF | StrId::STATIC | StrId::PARENT) {
                return true;
            }

            type_params.as_ref().is_some_and(|params| {
                params
                    .iter()
                    .any(|param| union_contains_special_class_names(param))
            })
        }
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_contains_special_class_names)
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_special_class_names(as_type),
        TAtomic::TTemplateParamClass { as_type, .. } => {
            atomic_contains_special_class_names(as_type)
        }
        _ => false,
    }
}

fn localize_special_class_names_for_final_class(
    union: &TUnion,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let localized_atomic =
            localize_special_class_names_in_atomic(atomic, self_class_id, parent_class_id);
        push_unique_atomic(&mut localized, localized_atomic);
    }

    TUnion::from_types(localized)
}

fn localize_special_class_names_in_atomic(
    atomic: &TAtomic,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            let localized_name = if matches!(*name, StrId::SELF | StrId::STATIC) {
                self_class_id
            } else if *name == StrId::PARENT {
                parent_class_id.unwrap_or(StrId::PARENT)
            } else {
                *name
            };

            TAtomic::TNamedObject {
                name: localized_name,
                type_params: type_params.as_ref().map(|params| {
                    params
                        .iter()
                        .map(|param| {
                            localize_special_class_names_for_final_class(
                                param,
                                self_class_id,
                                parent_class_id,
                            )
                        })
                        .collect()
                }),
            }
        }
        TAtomic::TObjectIntersection { types } => TAtomic::TObjectIntersection {
            types: types
                .iter()
                .map(|nested| {
                    localize_special_class_names_in_atomic(nested, self_class_id, parent_class_id)
                })
                .collect(),
        },
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParam {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_names_for_final_class(
                as_type,
                self_class_id,
                parent_class_id,
            )),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => TAtomic::TTemplateParamClass {
            name: *name,
            defining_entity: *defining_entity,
            as_type: Box::new(localize_special_class_names_in_atomic(
                as_type,
                self_class_id,
                parent_class_id,
            )),
        },
        _ => atomic.clone(),
    }
}

fn should_preserve_docblock_param_origin(signature_type: &TUnion, effective_type: &TUnion) -> bool {
    if signature_type == effective_type {
        return false;
    }

    if signature_type.is_nullable && !effective_type.is_nullable {
        return true;
    }

    if signature_type.is_falsable && !effective_type.is_falsable {
        return true;
    }

    let signature_maybe_truthy_and_falsy =
        !signature_type.is_always_truthy() && !signature_type.is_always_falsy();
    let effective_constant_truthiness =
        effective_type.is_always_truthy() || effective_type.is_always_falsy();

    signature_maybe_truthy_and_falsy && effective_constant_truthiness
}

fn get_alternate_param_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    if var_name.is_empty() {
        return None;
    }

    if let Some(stripped) = var_name.strip_prefix('$') {
        Some(analyzer.interner.intern(stripped))
    } else {
        Some(analyzer.interner.intern(&format!("${}", var_name)))
    }
}
