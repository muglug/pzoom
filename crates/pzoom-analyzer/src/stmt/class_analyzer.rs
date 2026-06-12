//! Class declaration analyzer.
//!
//! Analyzes method bodies with proper context.

use bumpalo::Bump;
use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::enum_case::EnumCaseItem;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::{Method, MethodBody};
use mago_syntax::ast::ast::class_like::{AnonymousClass, Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::namespace::NamespaceBody;
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::type_hint::Hint;

use pzoom_code_info::class_like_info::{ClassLikeKind, TemplateVariance, Visibility};
use pzoom_code_info::VarName;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, VarId, VariableSourceKind};
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
use pzoom_code_info::TemplateResult;
use indexmap::IndexMap;

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

    // A declaration guarded by `if (class_exists(Unknown::class))` would never
    // have been registered by Psalm's scanner (enterConditional resolves the
    // guard to false once the codebase is known) — skip its analysis entirely.
    if analyzer
        .codebase
        .get_class(class_name_id)
        .is_some_and(|class_info| {
            class_info
                .conditional_guard_classes
                .iter()
                .any(|guard_class| analyzer.codebase.get_class(*guard_class).is_none())
        })
    {
        return Ok(());
    }

    if analysis_data
        .declared_classlike_names
        .insert(class_name_id, class.span().start.offset)
        .is_some()
    {
        let span = class.name.span();
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

    // Psalm's ClassAnalyzer: a class named after a reserved word
    // (int|float|bool|string|void|null|false|true|object|mixed, or the bare
    // name `resource`) reports ReservedWord at the class name.
    {
        let reserved = matches!(
            class_name.to_ascii_lowercase().as_str(),
            "int" | "float" | "bool" | "string" | "void" | "null" | "false" | "true" | "object"
                | "mixed"
        ) || fqn.eq_ignore_ascii_case("resource");
        if reserved {
            let span = class.name.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ReservedWord,
                format!("{} is a reserved word", class_name),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
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

        let name_span = class.name.span();
        let dependency_fallback = (name_span.start.offset, name_span.end.offset);
        let dependency_spans = collect_dependency_name_spans(
            analyzer,
            class.extends.as_ref(),
            class.implements.as_ref(),
            class.members.as_slice(),
            context,
        );
        check_class_relationships(analyzer, info, context, analysis_data);
        check_inheritor_violations(analyzer, info, analysis_data);
        check_private_final_methods(analyzer, info, analysis_data);
        check_trait_requirements(
            analyzer, info, context, analysis_data, &dependency_spans, dependency_fallback,
        );
        check_missing_dependencies(
            analyzer, info, context, analysis_data, &dependency_spans, dependency_fallback,
        );
        // Psalm's ClassAnalyzer skips the rest of a class whose dependencies
        // are unresolved (`if ($storage->invalid_dependencies) return;`) —
        // the body would only produce noise on top of the UndefinedClass
        // already reported.
        if class_has_unresolved_dependency(analyzer, info, context) {
            return Ok(());
        }
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_undefined_docblock_template_extends_classes(analyzer, info, analysis_data);
        check_template_variance(analyzer, info, analysis_data);
        check_reserved_class_constant_names(analyzer, info, analysis_data);
        check_undefined_classes_in_constant_initializers(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
        check_missing_interface_method_typehints(analyzer, info, analysis_data);
        check_method_override_issues(analyzer, info, analysis_data);
        check_invalid_override_attributes(analyzer, info, analysis_data);
        check_property_override_visibility(analyzer, info, analysis_data);
        check_property_type_invariance(analyzer, info, analysis_data);
        check_invalid_traversable_implementation(analyzer, info, analysis_data);
        check_property_initialization(analyzer, class, info, analysis_data);
        check_property_defaults(analyzer, class.members.as_slice(), info, analysis_data);

        if !info.is_abstract {
            check_unimplemented_abstract_methods(analyzer, class, info, analysis_data);
        }
        // Check for missing property types
        check_missing_property_types(analyzer, &fqn, info, analysis_data);
        check_immutable_relationships(analyzer, class, info, analysis_data);
    }

    if class_info
        .is_some_and(|info| class_has_unresolved_dependency(analyzer, info, context))
    {
        return Ok(());
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

/// Analyze the members of an anonymous class registered in the codebase
/// under its synthetic `@anonymous-class:{file}:{offset}` name. Methods get
/// the full method analysis (declaring class, `$this`, visibility, return
/// checks), exactly as Psalm analyzes its registered `{parent}@anonymous`
/// storages.
pub fn analyze_anonymous_class(
    analyzer: &StatementsAnalyzer<'_>,
    anonymous_class: &AnonymousClass<'_>,
    class_name_id: pzoom_str::StrId,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let class_info = analyzer.codebase.get_class(class_name_id);

    if let Some(info) = class_info {
        check_class_relationships(analyzer, info, context, analysis_data);
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
        check_property_override_visibility(analyzer, info, analysis_data);
        check_property_type_invariance(analyzer, info, analysis_data);
        check_method_override_issues(analyzer, info, analysis_data);
    }

    for member in anonymous_class.members.iter() {
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

    check_interface_property_declarations(analyzer, interface_stmt, analysis_data);

    if let Some(info) = interface_info {
        check_inheritor_violations(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_undefined_docblock_template_extends_classes(analyzer, info, analysis_data);
        check_template_variance(analyzer, info, analysis_data);
        check_missing_interface_method_typehints(analyzer, info, analysis_data);
        check_invalid_override_attributes(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
    }

    let _ = context;

    Ok(())
}

/// PHP 8.4 interface property rules: an interface property must be hooked,
/// explicitly public, and non-static; hooks require PHP >= 8.4. These are
/// parse errors in PHP (Psalm's parser reports them; mago accepts them, so
/// re-check here with the configured version).
fn check_interface_property_declarations(
    analyzer: &StatementsAnalyzer<'_>,
    interface_stmt: &Interface<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_span::HasSpan;
    use mago_syntax::ast::ast::class_like::property::Property;
    use mago_syntax::ast::ast::modifier::Modifier;

    for member in interface_stmt.members.iter() {
        let ClassLikeMember::Property(property) = member else {
            continue;
        };

        let span = property.span();
        let mut emit = |message: &str, analysis_data: &mut FunctionAnalysisData| {
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ParseError,
                message.to_string(),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        };

        match property {
            Property::Plain(_) => {
                emit("Interfaces may only include hooked properties", analysis_data);
            }
            Property::Hooked(hooked) => {
                if analyzer.config.php_version_id() < 80400 {
                    emit(
                        "Property hooks are not available before PHP 8.4",
                        analysis_data,
                    );
                    continue;
                }

                let mut has_public = false;
                let mut bad_modifier = None;
                for modifier in hooked.modifiers.iter() {
                    match modifier {
                        Modifier::Public(_) => has_public = true,
                        Modifier::Private(_) => {
                            bad_modifier = Some("Interface properties cannot be private")
                        }
                        Modifier::Protected(_) => {
                            bad_modifier = Some("Interface properties cannot be protected")
                        }
                        Modifier::Static(_) => {
                            bad_modifier = Some("Interface properties cannot be static")
                        }
                        _ => {}
                    }
                }
                if let Some(message) = bad_modifier {
                    emit(message, analysis_data);
                } else if !has_public {
                    emit(
                        "Interface properties must be declared public",
                        analysis_data,
                    );
                }
            }
        }
    }
}

/// Emit InvalidOverride for any method carrying `#[\Override]` that does not actually
/// override (or implement) an inherited method.
fn check_invalid_override_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (method_name, method_info) in &class_info.methods {
        if !method_info.has_override_attribute
            || method_info.declaring_class != Some(class_info.name)
        {
            continue;
        }

        // Mirrors Psalm: an `#[Override]` is valid iff the method overrides at
        // least one ancestor method, as recorded in `overridden_method_ids`
        // during population (parent classes, interfaces, and abstract trait
        // requirements all count).
        if class_info
            .overridden_method_ids
            .get(method_name)
            .is_some_and(|ancestors| !ancestors.is_empty())
        {
            continue;
        }

        let (line, col) = analyzer.get_line_column(method_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidOverride,
            format!(
                "Method {}::{} does not match any inherited method, but has the Override attribute",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(*method_name),
            ),
            analyzer.file_path,
            method_info.start_offset,
            method_info.end_offset,
            line,
            col,
        ));
    }

    check_missing_override_attributes(analyzer, class_info, analysis_data);
}

/// Emit MissingOverrideAttribute for methods that override an inherited method
/// without carrying `#[\Override]` — Psalm's `FunctionLikeAnalyzer` check,
/// gated on `ensure_override_attribute`. Constructors are exempt; `__toString`
/// only counts when the class directly implements `Stringable`.
fn check_missing_override_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !analyzer.config.ensure_override_attribute {
        return;
    }

    let stringable_id = analyzer.interner.find("Stringable");

    for (method_name, method_info) in &class_info.methods {
        if method_info.has_override_attribute
            || method_info.declaring_class != Some(class_info.name)
        {
            continue;
        }

        if !class_info
            .overridden_method_ids
            .get(method_name)
            .is_some_and(|ancestors| !ancestors.is_empty())
        {
            continue;
        }

        if *method_name == StrId::CONSTRUCT {
            continue;
        }

        if *method_name == StrId::TO_STRING
            && !stringable_id.is_some_and(|id| class_info.interfaces.contains(&id))
        {
            continue;
        }

        let (line, col) = analyzer.get_line_column(method_info.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::MissingOverrideAttribute,
            format!(
                "Method {}::{} should have the \"Override\" attribute",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(*method_name),
            ),
            analyzer.file_path,
            method_info.start_offset,
            method_info.end_offset,
            line,
            col,
        ));
    }
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

        let name_span = enum_stmt.name.span();
        let dependency_fallback = (name_span.start.offset, name_span.end.offset);
        let dependency_spans = collect_dependency_name_spans(
            analyzer,
            None,
            enum_stmt.implements.as_ref(),
            enum_stmt.members.as_slice(),
            context,
        );
        check_class_relationships(analyzer, info, context, analysis_data);
        check_missing_dependencies(
            analyzer, info, context, analysis_data, &dependency_spans, dependency_fallback,
        );
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
        check_missing_template_params(analyzer, info, context, analysis_data);
        check_undefined_docblock_template_extends_classes(analyzer, info, analysis_data);
        check_template_variance(analyzer, info, analysis_data);
        check_reserved_class_constant_names(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
        check_invalid_traversable_implementation(analyzer, info, analysis_data);
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

/// Property defaults must satisfy the declared property type (Psalm routes
/// these through InstancePropertyAssignmentAnalyzer when analyzing the
/// declaration, reporting InvalidPropertyAssignmentValue).
fn check_property_defaults(
    analyzer: &StatementsAnalyzer<'_>,
    members: &[ClassLikeMember<'_>],
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_syntax::ast::ast::class_like::property::{Property, PropertyItem};

    let class_fqn = analyzer.interner.lookup(class_info.name);

    for member in members {
        let ClassLikeMember::Property(property) = member else {
            continue;
        };
        let items: Vec<&PropertyItem<'_>> = match property {
            Property::Plain(plain) => plain.items.iter().collect(),
            Property::Hooked(hooked) => vec![&hooked.item],
        };
        for item in items {
            let PropertyItem::Concrete(concrete) = item else {
                continue;
            };
            let prop_name_str = concrete.variable.name.trim_start_matches('$');
            let prop_name = analyzer.interner.intern(prop_name_str);
            let Some(prop_info) = class_info.properties.get(&prop_name) else {
                continue;
            };
            if prop_info.declaring_class != class_info.name {
                continue;
            }
            let Some(declared_type) = prop_info.get_type() else {
                continue;
            };
            if declared_type.is_mixed() {
                continue;
            }
            let Some(default_type) =
                pzoom_syntax::declaration_collector::simple_type_inferer::infer_in_class(
                    &concrete.value,
                    Some(class_fqn.as_ref()),
                )
            else {
                continue;
            };
            // A bare null default on a docblock-typed property is the legacy
            // "uninitialized" idiom; the initialization checks own that. An
            // empty-array default on a non-empty/shaped array type is the
            // "starts empty, filled later" idiom Psalm likewise accepts.
            if default_type.is_null() {
                continue;
            }
            let default_is_empty_array = matches!(
                default_type.get_single(),
                Some(TAtomic::TArray { key_type, value_type })
                    if key_type.is_nothing() && value_type.is_nothing()
            );
            if default_is_empty_array {
                continue;
            }
            if default_type.is_mixed() {
                continue;
            }
            let mut comparison_result =
                crate::type_comparator::TypeComparisonResult::new();
            if !crate::type_comparator::union_type_comparator::is_contained_by(
                analyzer.codebase,
                &default_type,
                declared_type,
                false,
                false,
                &mut comparison_result,
            ) && !comparison_result.type_coerced.unwrap_or(false)
            {
                let span = concrete.value.span();
                let (line, col) = analyzer.get_line_column(span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidPropertyAssignmentValue,
                    format!(
                        "{}::${} with declared type '{}' cannot be assigned type '{}'",
                        class_fqn,
                        prop_name_str,
                        declared_type.get_id(Some(analyzer.interner)),
                        default_type.get_id(Some(analyzer.interner))
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
                        // Resolve the initializer's value kind: a syntactic
                        // literal, a known constant (global or class), or the
                        // scan-time inferred value; an unknown kind stays
                        // silent (Psalm defers unresolvable const exprs).
                        let literal_value = get_enum_case_literal_value(&backed_case.value);
                        let value_kind = literal_value
                            .as_ref()
                            .map(|literal| match literal {
                                EnumCaseLiteralValue::Int(_) => EnumValueKind::Int,
                                EnumCaseLiteralValue::String(_) => EnumValueKind::String,
                            })
                            .or_else(|| {
                                resolve_enum_case_value_kind(
                                    analyzer,
                                    enum_stmt,
                                    case_name_id,
                                    &backed_case.value,
                                )
                            });
                        let is_invalid = match (&value_kind, expected_backing_type) {
                            (Some(EnumValueKind::Int), EnumBackingType::Int) => false,
                            (Some(EnumValueKind::String), EnumBackingType::String) => false,
                            (None, _) => false,
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

#[derive(PartialEq)]
enum EnumValueKind {
    Int,
    String,
    Other,
}

fn union_enum_value_kind(union: &TUnion) -> Option<EnumValueKind> {
    // Psalm requires a backed case's resolved value to be a LITERAL of the
    // backing kind — a constant typed plain `int`/`string` (PHP_VERSION_ID,
    // PHP_BINARY) is InvalidEnumCaseValue. Unresolvable (mixed) stays silent.
    let mut kind: Option<EnumValueKind> = None;
    for atomic in &union.types {
        let atomic_kind = match atomic {
            TAtomic::TLiteralInt { .. } => EnumValueKind::Int,
            TAtomic::TLiteralString { .. } | TAtomic::TLiteralClassString { .. } => {
                EnumValueKind::String
            }
            TAtomic::TMixed | TAtomic::TNonEmptyMixed => return None,
            _ => EnumValueKind::Other,
        };
        match &kind {
            None => kind = Some(atomic_kind),
            Some(existing) if *existing == atomic_kind => {}
            Some(_) => return None,
        }
    }
    kind
}

/// The value kind of a non-literal enum case initializer: a known global or
/// class constant's type, else the scan-time inferred case value.
fn resolve_enum_case_value_kind(
    analyzer: &StatementsAnalyzer<'_>,
    enum_stmt: &Enum<'_>,
    case_name_id: StrId,
    value_expr: &Expression<'_>,
) -> Option<EnumValueKind> {
        use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;

    match value_expr.unparenthesized() {
        // A global constant (`\PHP_BINARY`): its collected type decides
        // (runtime constants are typed non-literal at collection).
        Expression::ConstantAccess(const_access) => {
            let name = const_access.name.value();
            let trimmed = name.trim_start_matches('\\');
            let const_id = analyzer.interner.intern(trimmed);
            let const_info = analyzer.codebase.constants.get(&const_id)?;
            union_enum_value_kind(&const_info.constant_type)
        }
        // A class constant (`Foo::FOO`).
        Expression::Access(Access::ClassConstant(const_access)) => {
            let ClassLikeConstantSelector::Identifier(const_name) = &const_access.constant else {
                return None;
            };
            let class_span = const_access.class.span();
            let class_id = analyzer
                .get_resolved_name(class_span.start.offset)
                .or_else(|| match const_access.class.unparenthesized() {
                    Expression::Identifier(class_identifier) => Some(
                        analyzer
                            .interner
                            .intern(class_identifier.value().trim_start_matches('\\')),
                    ),
                    _ => None,
                })?;
            let class_info = analyzer.codebase.get_class(class_id)?;
            let const_name_id = analyzer.interner.intern(const_name.value);
            let const_info = class_info.constants.get(&const_name_id)?;
            union_enum_value_kind(&const_info.constant_type)
        }
        // Anything else: the scan-time inferred case value (covers literal
        // arithmetic like `1 << 0`).
        _ => {
            let enum_id = analyzer.interner.intern(enum_stmt.name.value);
            let scanned = analyzer
                .codebase
                .get_class(enum_id)
                .filter(|class_info| class_info.kind == ClassLikeKind::Enum)
                .or_else(|| {
                    analyzer.codebase.classlike_infos.values().find(|class_info| {
                        class_info.kind == ClassLikeKind::Enum
                            && analyzer
                                .interner
                                .lookup(class_info.name)
                                .rsplit('\\')
                                .next()
                                == Some(enum_stmt.name.value)
                    })
                })?;
            let case_value = scanned
                .constants
                .get(&case_name_id)
                .and_then(|const_info| const_info.enum_case_value.as_ref())?;
            union_enum_value_kind(case_value)
        }
    }
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
    if !matches!(
        class_info.kind,
        ClassLikeKind::Interface | ClassLikeKind::Class
    ) {
        return;
    }

    for method_info in class_info.methods.values() {
        if method_info.file_path != analyzer.file_path {
            continue;
        }

        // Magic methods have well-known signatures; Psalm does not require
        // them to declare types.
        if analyzer
            .interner
            .lookup(method_info.name)
            .as_ref()
            .starts_with("__")
        {
            continue;
        }

        let mut inherited_methods = Vec::new();
        let mut seen_ancestors = rustc_hash::FxHashSet::default();
        for ancestor in class_info
            .interfaces
            .iter()
            .chain(class_info.all_parent_interfaces.iter())
            .chain(class_info.parent_class.iter())
            .chain(class_info.all_parent_classes.iter())
            .chain(class_info.used_traits.iter())
        {
            if !seen_ancestors.insert(*ancestor) {
                continue;
            }

            let Some(parent_info) = analyzer.codebase.get_class(*ancestor) else {
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
            // No return type node by definition: point at the method name
            // (Psalm's name location).
            let (issue_start, issue_end) = method_info
                .name_location
                .unwrap_or((method_info.start_offset, method_info.end_offset));
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingReturnType,
                format!(
                    "Method {}::{} does not have a return type",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(method_info.name)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
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
            // Only an inherited *docblock* type suppresses the report: Psalm
            // inherits docblock params (implicit inheritdoc), but a parent's
            // native-only hint leaves the child param untyped
            // (intParamTypeDefinedInParent still reports MissingParamType).
            let inherited_param_type_available = param_index != usize::MAX
                && inherited_methods.iter().any(|parent_method| {
                    parent_method
                        .params
                        .get(param_index)
                        .is_some_and(|parent_param| parent_param.has_docblock_type)
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

/// Resolved (class-id, span) pairs for every name written in `extends`,
/// `implements` and `use Trait;` clauses — Psalm points MissingDependency /
/// trait-requirement issues at the specific name node, not the class body.
fn collect_dependency_name_spans(
    analyzer: &StatementsAnalyzer<'_>,
    extends: Option<&mago_syntax::ast::ast::class_like::inheritance::Extends<'_>>,
    implements: Option<&mago_syntax::ast::ast::class_like::inheritance::Implements<'_>>,
    members: &[mago_syntax::ast::ast::class_like::member::ClassLikeMember<'_>],
    context: &BlockContext,
) -> Vec<(StrId, (u32, u32))> {
    let mut spans = Vec::new();
    let mut add = |identifier: &mago_syntax::ast::ast::identifier::Identifier<'_>| {
        let id = analyzer
            .interner
            .intern(identifier.value().trim_start_matches('\\'));
        let span = identifier.span();
        spans.push((
            resolve_alias_in_context(id, context),
            (span.start.offset, span.end.offset),
        ));
    };
    if let Some(extends) = extends {
        for identifier in extends.types.iter() {
            add(identifier);
        }
    }
    if let Some(implements) = implements {
        for identifier in implements.types.iter() {
            add(identifier);
        }
    }
    for member in members {
        if let mago_syntax::ast::ast::class_like::member::ClassLikeMember::TraitUse(trait_use) =
            member
        {
            for identifier in trait_use.trait_names.iter() {
                add(identifier);
            }
        }
    }
    spans
}

/// The recorded span for a dependency name, defaulting to the given fallback
/// (the class-name node).
fn dependency_name_pos(
    spans: &[(StrId, (u32, u32))],
    dependency: StrId,
    fallback: (u32, u32),
) -> (u32, u32) {
    spans
        .iter()
        .find(|(id, _)| *id == dependency)
        .map(|(_, span)| *span)
        .unwrap_or(fallback)
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

/// Psalm's `@psalm-inheritors` enforcement (ClassAnalyzer / InterfaceAnalyzer):
/// a class-like inheriting from a parent that declares a closed inheritor set
/// must be contained by that set.
fn check_inheritor_violations(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let class_union = pzoom_code_info::TUnion::new(TAtomic::TNamedObject {
        name: class_info.name,
        type_params: None,
        is_static: false,
        remapped_params: false,
    });

    for parent_id in class_info
        .all_parent_classes
        .iter()
        .chain(class_info.interfaces.iter())
        .chain(class_info.all_parent_interfaces.iter())
    {
        // The declared name may be miscased; resolve case-insensitively like
        // Psalm's storage lookup.
        let parent_info = analyzer.codebase.get_class(*parent_id).or_else(|| {
            analyzer
                .codebase
                .classlike_name_lookup
                .get(
                    &analyzer
                        .interner
                        .lookup(*parent_id)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase(),
                )
                .and_then(|resolved_id| analyzer.codebase.get_class(*resolved_id))
        });
        let Some(parent_info) = parent_info else {
            continue;
        };
        if parent_info.inheritors.is_empty() {
            continue;
        }

        let inheritors_union =
            pzoom_code_info::TUnion::from_types(parent_info.inheritors.clone());
        let mut comparison_result =
            crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &class_union,
            &inheritors_union,
            false,
            false,
            &mut comparison_result,
        ) {
            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::InheritorViolation,
                format!(
                    "{} {} is not an allowed inheritor of parent {}",
                    if class_info.kind == ClassLikeKind::Interface {
                        "Interface"
                    } else {
                        "Class"
                    },
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(parent_info.name)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        }
    }
}

/// PHP 8.0+: `final private function` is meaningless (private methods are
/// invisible to children) — Psalm's PrivateFinalMethod. Constructors exempt.
/// Whether the class-like has a parent/interface dependency that never
/// resolved (no storage, and no registered class alias covering it).
fn class_has_unresolved_dependency(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
) -> bool {
    class_info.invalid_dependencies.iter().any(|dependency| {
        let resolved_dependency = resolve_alias_in_context(*dependency, context);
        analyzer.codebase.get_class(resolved_dependency).is_none()
            && resolved_dependency == *dependency
    })
}

fn check_private_final_methods(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (method_name, method_info) in &class_info.methods {
        if method_info.declaring_class != Some(class_info.name)
            || *method_name == StrId::CONSTRUCT
        {
            continue;
        }
        if method_info.is_final && method_info.visibility == Visibility::Private {
            let (line, col) = analyzer.get_line_column(method_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::PrivateFinalMethod,
                "Private methods cannot be final",
                analyzer.file_path,
                method_info.start_offset,
                method_info.end_offset,
                line,
                col,
            ));
        }
    }
}

fn check_class_relationships(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    // `use B;` where B is a class or interface (Psalm's UndefinedTrait
    // "B is not a trait").
    for used_trait in &class_info.used_traits {
        let resolved_trait = resolve_alias_in_context(*used_trait, context);
        if let Some(trait_info) = analyzer.codebase.get_class(resolved_trait)
            && trait_info.kind != ClassLikeKind::Trait
        {
            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedTrait,
                format!(
                    "{} is not a trait",
                    analyzer.interner.lookup(resolved_trait)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        }
    }

    if let Some(parent_class) = class_info.parent_class {
        let resolved_parent = resolve_alias_in_context(parent_class, context);
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);

        if resolved_parent == class_info.name {
            analysis_data.add_issue(Issue::new(
                IssueKind::CircularReference,
                format!(
                    "Circular reference discovered when resolving {}",
                    analyzer.interner.lookup(class_info.name)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
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
                issue_start,
                issue_end,
                line,
                col,
            ));
        } else if let Some(parent_info) = analyzer.codebase.get_class(resolved_parent) {
            if parent_info.kind != ClassLikeKind::Class {
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedClass,
                    crate::class_casing::undefined_class_message(analyzer, &analyzer.interner.lookup(parent_class)
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
            } else if parent_info.is_readonly && !class_info.is_readonly {
                // PHP 8.2: a readonly class can only be extended by readonly
                // classes (Psalm's InvalidExtendClass).
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidExtendClass,
                    format!(
                        "Non-readonly class {} may not inherit from readonly class {}",
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
        } else if resolved_parent == parent_class {
            // An alias-resolved parent whose target is missing was already
            // reported (or suppressed) at the class_alias() call site.
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                crate::class_casing::undefined_class_message(analyzer, &analyzer.interner.lookup(parent_class)
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
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);

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
                issue_start,
                issue_end,
                line,
                col,
            ));
            continue;
        }

        // Psalm: a concrete class implementing Throwable must extend
        // Exception or Error (InvalidInterfaceImplementation).
        if class_info.kind == ClassLikeKind::Class
            && !class_info.is_abstract
            && analyzer
                .interner
                .lookup(resolved_interface)
                .eq_ignore_ascii_case("Throwable")
            && !class_info.all_parent_classes.iter().any(|ancestor_id| {
                matches!(
                    &*analyzer.interner.lookup(*ancestor_id),
                    "Exception" | "Error"
                )
            })
        {
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidInterfaceImplementation,
                "Classes implementing Throwable should extend Exception or Error",
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
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
                crate::class_casing::undefined_class_message(analyzer, &analyzer.interner.lookup(*interface_id)
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
    dependency_spans: &[(StrId, (u32, u32))],
    dependency_fallback: (u32, u32),
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

            // Psalm points at the trait name in the `use` clause.
            let (start, end) =
                dependency_name_pos(dependency_spans, resolved_trait, dependency_fallback);
            let (line, col) = analyzer.get_line_column(start);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingDependency,
                format!(
                    "Trait {} requires using class to extend {}",
                    analyzer.interner.lookup(resolved_trait),
                    analyzer.interner.lookup(required_parent)
                ),
                analyzer.file_path,
                start,
                end,
                line,
                col,
            ));
        }

        for required_interface in &trait_info.required_implements {
            let required_interface = resolve_alias_in_context(*required_interface, context);
            if implemented_interfaces.contains(&required_interface) {
                continue;
            }

            let (start, end) =
                dependency_name_pos(dependency_spans, resolved_trait, dependency_fallback);
            let (line, col) = analyzer.get_line_column(start);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingDependency,
                format!(
                    "Trait {} requires using class to implement {}",
                    analyzer.interner.lookup(resolved_trait),
                    analyzer.interner.lookup(required_interface)
                ),
                analyzer.file_path,
                start,
                end,
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
    dependency_spans: &[(StrId, (u32, u32))],
    dependency_fallback: (u32, u32),
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

        // Psalm reports MissingDependency at USE sites (instantiation etc.,
        // via checkFullyQualifiedClassLikeName), not at the declaration —
        // the declaration gets UndefinedClass from the relationship checks.
        if !is_missing_trait {
            continue;
        }

        let (issue_kind, message) = (
            IssueKind::UndefinedTrait,
            format!(
                "Trait {} does not exist",
                analyzer.interner.lookup(*dependency)
            ),
        );

        // Psalm points at the extends/implements/use name node.
        let (start, end) = dependency_name_pos(
            dependency_spans,
            resolve_alias_in_context(*dependency, context),
            dependency_fallback,
        );
        let (line, col) = analyzer.get_line_column(start);
        analysis_data.add_issue(Issue::new(
            issue_kind,
            message,
            analyzer.file_path,
            start,
            end,
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
        for template_type in &method_info.template_types {
            crate::template::template_types_insert(
                &mut template_defaults,
                template_type.name,
                template_type.defining_entity,
                template_type.as_type.clone(),
            );
        }

        let callable_name = format!(
            "{}::{}",
            analyzer.interner.lookup(class_info.name),
            analyzer.interner.lookup(method_info.name),
        );
        check_functionlike_docblock_param_type_mismatches(
            analyzer,
            method_info,
            Some(class_info),
            &callable_name,
            template_defaults,
            analysis_data,
        );
    }
}

/// The per-parameter docblock-vs-signature containment check shared by
/// methods and plain functions (Psalm's FunctionLikeAnalyzer parameter
/// check emitting `MismatchingDocblockParamType`).
pub(crate) fn check_functionlike_docblock_param_type_mismatches(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &pzoom_code_info::FunctionLikeInfo,
    class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    callable_name: &str,
    template_defaults: TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
) {
    {
        for param in &method_info.params {
            if !param.has_docblock_type {
                continue;
            }

            let (Some(docblock_type), Some(signature_type)) =
                (param.param_type.as_ref(), param.signature_type.as_ref())
            else {
                continue;
            };

            let mut localized_docblock_type = if template_defaults.template_types.is_empty() {
                docblock_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    docblock_type,
                    &template_defaults,
                )
            };
            let mut localized_signature_type = if template_defaults.template_types.is_empty() {
                signature_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(
                    signature_type,
                    &template_defaults,
                )
            };

            if let Some(class_info) = class_info {
                localized_docblock_type = localize_special_class_names_for_final_class(
                    &localized_docblock_type,
                    class_info.name,
                    class_info.parent_class,
                );
            }
            // Resolve class-constant references/wildcards (`Foo::BAR_*`) now that
            // the codebase is populated — the same analysis-time expansion Psalm
            // performs via TypeExpander before comparing a docblock param type to
            // the native signature (pzoom's call-site checker uses the same
            // helper).
            localized_docblock_type =
                crate::expr::call::callable_validation::normalize_class_constant_param_type(
                    analyzer,
                    &localized_docblock_type,
                    callable_name,
                );
            if let Some(class_info) = class_info {
                localized_signature_type = localize_special_class_names_for_final_class(
                    &localized_signature_type,
                    class_info.name,
                    class_info.parent_class,
                );
            }

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

            // A deferred `key-of<T>` / `value-of<T>` is template-dependent; the native
            // signature is a reasonable widening of it, so don't flag a mismatch (Psalm
            // does not emit MismatchingDocblockParamType for these).
            if localized_docblock_type.types.iter().any(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TTemplateKeyOf { .. } | TAtomic::TTemplateValueOf { .. }
                )
            }) {
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

            // An empty docblock type (e.g. value-of over a unit enum) is
            // trivially contained in anything but documents an impossible
            // parameter — Psalm flags it against the native signature.
            let docblock_is_empty =
                localized_docblock_type.is_nothing() && !localized_signature_type.is_nothing();

            if (is_compatible && !docblock_is_empty)
                || comparison_result
                    .type_coerced_from_mixed
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

    // The same containment for the return type (Psalm's
    // ReturnTypeAnalyzer::checkReturnType signature comparison):
    // "Docblock has incorrect return type 'X', should be 'Y'".
    if let (Some(docblock_return), Some(signature_return)) = (
        method_info.return_type.as_ref(),
        method_info.signature_return_type.as_ref(),
    ) && !method_info.inherited_return_type
    {
        let mut localized_docblock = if template_defaults.template_types.is_empty() {
            docblock_return.clone()
        } else {
            function_call_analyzer::replace_templates_in_union(docblock_return, &template_defaults)
        };
        if let Some(class_info) = class_info {
            localized_docblock = localize_special_class_names_for_final_class(
                &localized_docblock,
                class_info.name,
                class_info.parent_class,
            );
        }
        localized_docblock =
            crate::expr::call::callable_validation::normalize_class_constant_param_type(
                analyzer,
                &localized_docblock,
                callable_name,
            );
        let mut localized_signature = signature_return.clone();
        if let Some(class_info) = class_info {
            localized_signature = localize_special_class_names_for_final_class(
                &localized_signature,
                class_info.name,
                class_info.parent_class,
            );
        }

        // key-of<T>/value-of<T> are NOT deferred here: the comparator resolves
        // them against the template's bound (Psalm checks them in
        // keyOf/valueOfUnresolvedTemplateParamIsStillChecked).
        let docblock_has_deferred = localized_docblock.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TTemplateParam { .. } | TAtomic::TConditional(_)
            )
        }) || docblock_return
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TConditional(_)));
        let mut comparison_result = TypeComparisonResult::new();
        if !docblock_has_deferred
            && !localized_docblock.is_mixed()
            && !union_type_comparator::is_contained_by(
                analyzer.codebase,
                &localized_docblock,
                &localized_signature,
                false,
                false,
                &mut comparison_result,
            )
            && !comparison_result.type_coerced_from_mixed.unwrap_or(false)
        {
            let (line, col) = analyzer.get_line_column(method_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MismatchingDocblockReturnType,
                format!(
                    "Docblock has incorrect return type '{}', should be '{}'",
                    localized_docblock.get_id(Some(analyzer.interner)),
                    localized_signature.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                method_info.start_offset,
                method_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }
}

/// Validate `@template-extends Base<...>` / `@template-implements` args
/// against the parent templates' bounds (Psalm ClassLikeAnalyzer:
/// "Extended template param T expects type X, type Y given").
fn check_extended_template_param_bounds(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (parent_name, extended_params) in &class_info.template_extended_params {
        // Psalm's checkTemplateParams only runs against direct relationships
        // (parent class, implemented interfaces, used traits); inherited
        // extended-params entries hold intermediate template links that are
        // validated at the class that declares them.
        if class_info.parent_class != Some(*parent_name)
            && !class_info.interfaces.contains(parent_name)
            && !class_info.used_traits.contains(parent_name)
        {
            continue;
        }

        let Some(parent_info) = analyzer.codebase.get_class(*parent_name) else {
            continue;
        };

        let mut substitutions = TemplateResult::default();
        for template_type in &parent_info.template_types {
            if let Some(extended_type) = extended_params.get(&template_type.name) {
                crate::template::lower_bounds_insert(
                    &mut substitutions,
                    template_type.name,
                    template_type.defining_entity,
                    extended_type.clone(),
                );
            }
        }

        for template_type in &parent_info.template_types {
            let Some(extended_type) = extended_params.get(&template_type.name) else {
                continue;
            };

            // Psalm: a strictly-enforced (@psalm-consistent-templates) parent
            // template param must be extended with a child template param
            // sharing the same constraint.
            if parent_info.enforce_template_inheritance {
                for extended_atomic in &extended_type.types {
                    let child_constraint =
                        if let TAtomic::TTemplateParam { name: child_name, .. } = extended_atomic {
                            class_info
                                .template_types
                                .iter()
                                .find(|child_template| child_template.name == *child_name)
                                .map(|child_template| (*child_name, &child_template.as_type))
                        } else {
                            None
                        };

                    match child_constraint {
                        None => {
                            let (issue_start, issue_end) = class_issue_pos(class_info);
                            let (line, col) = analyzer.get_line_column(issue_start);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidTemplateParam,
                                format!(
                                    "Cannot extend a strictly-enforced parent template param {} with a non-template type",
                                    analyzer.interner.lookup(template_type.name)
                                ),
                                analyzer.file_path,
                                issue_start,
                                issue_end,
                                line,
                                col,
                            ));
                        }
                        Some((child_name, child_bound))
                            if child_bound.get_id(Some(analyzer.interner))
                                != template_type.as_type.get_id(Some(analyzer.interner)) =>
                        {
                            let (issue_start, issue_end) = class_issue_pos(class_info);
                            let (line, col) = analyzer.get_line_column(issue_start);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InvalidTemplateParam,
                                format!(
                                    "Cannot extend a strictly-enforced parent template param {} with constraint {} with a child template param {} with different constraint {}",
                                    analyzer.interner.lookup(template_type.name),
                                    template_type.as_type.get_id(Some(analyzer.interner)),
                                    analyzer.interner.lookup(child_name),
                                    child_bound.get_id(Some(analyzer.interner))
                                ),
                                analyzer.file_path,
                                issue_start,
                                issue_end,
                                line,
                                col,
                            ));
                        }
                        _ => {}
                    }
                }
            }

            if template_type.as_type.is_mixed() {
                continue;
            }
            // A child template forwarded into the parent slot is checked at
            // the child's own declaration; an unspecified slot defaults to
            // mixed (Psalm reports MissingTemplateParam separately, not a
            // bound violation).
            if extended_type.is_mixed()
                || extended_type
                    .types
                    .iter()
                    .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
            {
                continue;
            }

            let effective_bound =
                crate::expr::call::function_call_analyzer::replace_templates_in_union(
                    &template_type.as_type,
                    &substitutions,
                );
            if effective_bound.is_mixed() {
                continue;
            }

            let mut comparison_result = TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                analyzer.codebase,
                extended_type,
                &effective_bound,
                false,
                false,
                &mut comparison_result,
            ) {
                let (issue_start, issue_end) = class_issue_pos(class_info);
                let (line, col) = analyzer.get_line_column(issue_start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidTemplateParam,
                    format!(
                        "Extended template param {} expects type {}, type {} given",
                        analyzer.interner.lookup(template_type.name),
                        effective_bound.get_id(Some(analyzer.interner)),
                        extended_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    issue_start,
                    issue_end,
                    line,
                    col,
                ));
            }
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
            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingTemplateParam,
                format!(
                    "{} has missing template params when extending {}",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(related_name)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        };

    let emit_too_many_template_params =
        |analysis_data: &mut FunctionAnalysisData,
         related_name: StrId,
         class_info: &pzoom_code_info::ClassLikeInfo| {
            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooManyTemplateParams,
                format!(
                    "{} has too many template params when extending {}",
                    analyzer.interner.lookup(class_info.name),
                    analyzer.interner.lookup(related_name)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        };

    // The number of type arguments supplied via `@extends`/`@implements`/`@use`
    // must match the parent's template parameter count: too few is a
    // MissingTemplateParam, too many a TooManyTemplateParams (Psalm).
    let provided_param_count = |class_info: &pzoom_code_info::ClassLikeInfo,
                                resolved_id: StrId,
                                raw_id: StrId| {
        class_info
            .template_extended_offsets
            .get(&resolved_id)
            .or_else(|| class_info.template_extended_offsets.get(&raw_id))
            .map(|params| params.len())
    };

    // Psalm: a parent with @psalm-consistent-templates requires children to
    // redeclare the same number of template params, so `static<T>` types stay
    // sound ("X requires the same number of template params as Y but saw N").
    let check_enforced_count = |analysis_data: &mut FunctionAnalysisData,
                                parent_info: &pzoom_code_info::ClassLikeInfo| {
        if !parent_info.enforce_template_inheritance {
            return;
        }
        let expected = parent_info.template_types.len();
        let own = class_info.template_types.len();
        if expected == own {
            return;
        }
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);
        analysis_data.add_issue(Issue::new(
            if expected > own {
                IssueKind::MissingTemplateParam
            } else {
                IssueKind::TooManyTemplateParams
            },
            format!(
                "{} requires the same number of template params as {} but saw {}",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(parent_info.name),
                own
            ),
            analyzer.file_path,
            issue_start,
            issue_end,
            line,
            col,
        ));
    };

    if let Some(parent_id) = class_info.parent_class {
        let resolved_parent_id = resolve_alias_in_context(parent_id, context);
        if let Some(parent_info) = analyzer.codebase.get_class(resolved_parent_id) {
            let expected = parent_info.template_types.len();
            match provided_param_count(class_info, resolved_parent_id, parent_id) {
                None => {
                    if expected > 0 {
                        emit_missing_template_param(analysis_data, resolved_parent_id, class_info);
                    }
                }
                Some(provided) if provided < expected => {
                    emit_missing_template_param(analysis_data, resolved_parent_id, class_info);
                }
                Some(provided) if provided > expected => {
                    emit_too_many_template_params(analysis_data, resolved_parent_id, class_info);
                }
                _ => {}
            }
            check_enforced_count(analysis_data, parent_info);
        }
    }

    for interface_id in &class_info.interfaces {
        let resolved_interface_id = resolve_alias_in_context(*interface_id, context);
        if let Some(interface_info) = analyzer.codebase.get_class(resolved_interface_id) {
            let expected = interface_info.template_types.len();
            match provided_param_count(class_info, resolved_interface_id, *interface_id) {
                None => {
                    if expected > 0 {
                        emit_missing_template_param(
                            analysis_data,
                            resolved_interface_id,
                            class_info,
                        );
                        break;
                    }
                }
                Some(provided) if provided < expected => {
                    emit_missing_template_param(analysis_data, resolved_interface_id, class_info);
                    break;
                }
                Some(provided) if provided > expected => {
                    emit_too_many_template_params(
                        analysis_data,
                        resolved_interface_id,
                        class_info,
                    );
                    break;
                }
                _ => {}
            }
            check_enforced_count(analysis_data, interface_info);
        }
    }

    // used_traits is flattened through the parent chain by the populator; a
    // trait the parent already used (and bound via its own `@use`) is not
    // re-declared by the child, so only directly-used traits are checked
    // (Psalm checks the `use` statement's declaring class).
    let parent_used_traits = class_info
        .parent_class
        .map(|parent_id| resolve_alias_in_context(parent_id, context))
        .and_then(|resolved_parent_id| analyzer.codebase.get_class(resolved_parent_id))
        .map(|parent_info| &parent_info.used_traits);
    for trait_id in &class_info.used_traits {
        let resolved_trait_id = resolve_alias_in_context(*trait_id, context);
        if parent_used_traits.is_some_and(|parent_traits| {
            parent_traits.contains(&resolved_trait_id) || parent_traits.contains(trait_id)
        }) {
            continue;
        }
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

/// `X::class` in a constant initializer referencing an unknown class is an
/// UndefinedClass (Psalm's const-expression analysis reports it).
fn check_undefined_classes_in_constant_initializers(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for const_info in class_info.constants.values() {
        if const_info.declaring_class != class_info.name {
            continue;
        }

        let mut stack: Vec<&TUnion> = vec![&const_info.constant_type];
        let mut emitted: FxHashSet<StrId> = FxHashSet::default();
        while let Some(union) = stack.pop() {
            for atomic in &union.types {
                match atomic {
                    TAtomic::TLiteralClassString { name } => {
                        let class_id = analyzer.interner.intern(name.trim_start_matches('\\'));
                        if analyzer.codebase.get_class(class_id).is_none()
                            && emitted.insert(class_id)
                        {
                            let (line, col) = analyzer.get_line_column(const_info.start_offset);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedClass,
                                format!("Class, interface or enum named {} does not exist", name),
                                analyzer.file_path,
                                const_info.start_offset,
                                const_info.start_offset.saturating_add(1),
                                line,
                                col,
                            ));
                        }
                    }
                    TAtomic::TKeyedArray { properties, .. } => {
                        stack.extend(properties.values());
                    }
                    TAtomic::TArray { key_type, value_type }
                    | TAtomic::TNonEmptyArray { key_type, value_type } => {
                        stack.push(key_type);
                        stack.push(value_type);
                    }
                    TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                        stack.push(value_type);
                    }
                    _ => {}
                }
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
                    false,
                    analysis_data,
                );
            }
        } else if *method_name == StrId::CONSTRUCT
            && method_info
                .params
                .iter()
                .any(|param| !param.is_optional && !param.is_variadic)
        {
            // A @psalm-consistent-constructor ancestor with no explicit
            // constructor implicitly defines a zero-arg one; a child
            // constructor with required params breaks `new static()` (Psalm
            // compares against the implicit parent constructor).
            let consistent_ancestor = class_info
                .parent_class
                .iter()
                .chain(class_info.all_parent_classes.iter())
                .find(|ancestor| {
                    analyzer
                        .codebase
                        .get_class(**ancestor)
                        .is_some_and(|ancestor_info| ancestor_info.is_consistent_constructor)
                });
            if let Some(ancestor) = consistent_ancestor {
                let (line, col) = analyzer.get_line_column(method_info.start_offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ConstructorSignatureMismatch,
                    format!(
                        "Method {}::__construct has more required parameters than parent method {}::__construct",
                        analyzer.interner.lookup(class_info.name),
                        analyzer.interner.lookup(*ancestor)
                    ),
                    analyzer.file_path,
                    method_info.start_offset,
                    method_info.end_offset,
                    line,
                    col,
                ));
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
                false,
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
                    false,
                    analysis_data,
                );
            }
        }
    }
}

fn check_method_signature_must_omit_return_type(
    analyzer: &StatementsAnalyzer<'_>,
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

/// Psalm MethodComparator::comparePseudoMethods: a `@method` annotation that
/// shadows a real (declared or inherited) method is compared against it, with
/// the native-signature checks disabled (prevent_method_signature_mismatch).
fn check_pseudo_method_annotations(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (method_name, pseudo_method) in class_info
        .pseudo_methods
        .iter()
        .chain(class_info.pseudo_static_methods.iter())
    {
        if *method_name == StrId::CONSTRUCT {
            continue;
        }
        let Some(real_method) = class_info.methods.get(method_name) else {
            continue;
        };
        let guide_class_id = real_method.declaring_class.unwrap_or(class_info.name);
        compare_method_to_guide(
            analyzer,
            class_info,
            *method_name,
            pseudo_method,
            guide_class_id,
            real_method,
            false,
            true,
            analysis_data,
        );
    }
}

fn compare_method_to_guide(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method_name: StrId,
    implementer_method: &pzoom_code_info::FunctionLikeInfo,
    guide_class_id: StrId,
    guide_method: &pzoom_code_info::FunctionLikeInfo,
    guide_is_trait: bool,
    // A pseudo (@method annotation) implementer: Psalm passes
    // prevent_method_signature_mismatch=false, skipping the obvious-mismatch
    // and native-signature checks, and reports docblock conflicts against the
    // same class as MismatchingDocblockReturnType.
    implementer_is_pseudo: bool,
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

    // Psalm's FunctionLikeAnalyzer skips MethodComparator::compare entirely
    // for __construct when the parent lacks @psalm-consistent-constructor —
    // including the visibility and final checks, since PHP lets constructors
    // change signature and access freely.
    if !enforce_constructor_signature {
        return;
    }

    let base_mismatch_kind =
        if guide_is_trait && implementer_method.declaring_class == Some(class_info.name) {
            IssueKind::TraitMethodSignatureMismatch
        } else {
            IssueKind::MethodSignatureMismatch
        };

    if guide_method.is_final
        && !implementer_is_pseudo
        // An inherited copy of the very same method (ForkContext gets the
        // final __clone flattened from AbstractContext's ForbidCloning trait)
        // is not an override — Psalm only flags a re-declaration.
        && implementer_method.declaring_class != guide_method.declaring_class
    {
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

    // Psalm: an override cannot opt out of named arguments the guide accepts.
    if implementer_method.no_named_arguments
        && !guide_method.no_named_arguments
        && !implementer_is_pseudo
    {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMismatch,
            format!(
                "Method {} should accept named arguments as {} does",
                implementer_method_id, guide_method_id
            ),
        );
    }

    if is_visibility_more_restrictive(implementer_method.visibility, guide_method.visibility)
        && !implementer_is_pseudo
    {
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

    // A trait's abstract method is a REQUIREMENT on the using class, not an
    // override: an inherited concrete method satisfies it (Psalm).
    let implementer_is_trait_requirement = implementer_method
        .declaring_class
        .and_then(|declaring_class| analyzer.codebase.get_class(declaring_class))
        .is_some_and(|declaring_info| declaring_info.kind == ClassLikeKind::Trait);

    if implementer_method.is_abstract
        && !guide_method.is_abstract
        && guide_class_info.kind == ClassLikeKind::Class
        && !guide_class_info.is_abstract
        && !implementer_is_pseudo
        && !implementer_is_trait_requirement
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

    if guide_method.returns_by_ref && !implementer_method.returns_by_ref && !implementer_is_pseudo
    {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMismatch,
            format!("Method {} must return by-reference", implementer_method_id),
        );
    }

    // Psalm only reports a static-ness mismatch in one direction: when the guide
    // (parent/interface) method is static and the implementer is non-static (see
    // ClassAnalyzer's interface-method check, "should be static like ..."). It does
    // NOT report overriding a non-static method with a static one. Mirror that
    // direction here.
    if guide_method.is_static && !implementer_method.is_static && !implementer_is_pseudo {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            base_mismatch_kind,
            format!(
                "Method {} should be static like {}",
                implementer_method_id, guide_method_id
            ),
        );
    }

    let specialize_for_comparison = |union: &TUnion| {
        let specialized =
            replace_extended_templates_in_union(union, &class_info.template_extended_params);

        // Psalm's MethodComparator dissolves *function-level* template params
        // (`fn-` defining entities) into their bounds on both sides before
        // comparing, so a method template `TBool as bool` matches a plain
        // `bool` in the parent signature.
        let mut dissolved_types = Vec::with_capacity(specialized.types.len());
        let mut dissolved_any = false;
        for atomic in &specialized.types {
            match atomic {
                pzoom_code_info::TAtomic::TTemplateParam {
                    defining_entity: pzoom_code_info::GenericParent::FunctionLike(_),
                    as_type,
                    ..
                } => {
                    dissolved_any = true;
                    for bound_atomic in &as_type.types {
                        if !dissolved_types.contains(bound_atomic) {
                            dissolved_types.push(bound_atomic.clone());
                        }
                    }
                }
                _ => {
                    if !dissolved_types.contains(atomic) {
                        dissolved_types.push(atomic.clone());
                    }
                }
            }
        }
        let specialized = if dissolved_any {
            let mut dissolved = TUnion::from_types(dissolved_types);
            dissolved.from_docblock = specialized.from_docblock;
            dissolved
        } else {
            specialized
        };

        localize_special_class_names_union(
            &specialized,
            class_info.name,
            class_info.parent_class,
            !class_info.is_final,
        )
    };

    let guide_class_name = analyzer.interner.lookup(guide_class_id);
    let guide_is_array_access = guide_class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("ArrayAccess");
    let offset_get = StrId::OFFSET_GET;
    let offset_set = StrId::OFFSET_SET;
    let offset_exists = StrId::OFFSET_EXISTS;
    let offset_unset = StrId::OFFSET_UNSET;

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
            // Psalm skips the name check when the parent method opts out of
            // named arguments (@no-named-arguments, e.g. ArrayObject's
            // offsetSet whose param name conflicts with ArrayAccess's).
            && !guide_method.no_named_arguments
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

        // A docblock-only implementer param compares against the guide's
        // docblock-preferred type (Psalm's MethodComparator docblock check):
        // an override re-declaring the parent's `@param non-empty-list<string>`
        // over a native `array` hint is not a mismatch.
        let implementer_is_docblock_only =
            implementer_param.signature_type.is_none() && implementer_param.has_docblock_type;
        let guide_param_signature = if implementer_is_docblock_only {
            guide_param
                .get_type()
                .or(guide_param.signature_type.as_ref())
        } else {
            guide_param
                .signature_type
                .as_ref()
                .or_else(|| guide_param.get_type())
        };
        let implementer_param_signature = implementer_param
            .signature_type
            .as_ref()
            .or_else(|| implementer_param.get_type());

        // Psalm's MethodComparator gates narrowing complaints on the guide
        // class being user-defined: narrowing a stub interface's mixed param
        // (ArrayAccess::offsetSet) via docblock stays silent.
        let guide_param_is_stubbed = analyzer
            .codebase
            .files
            .get(&guide_class_info.file_path)
            .is_some_and(|file_info| file_info.is_stub);

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
            ) && !(guide_param_is_stubbed
                && comparison_result.type_coerced.unwrap_or(false))
            {
                // Kind selection (Psalm MethodComparator): a pseudo
                // (@method) implementer conflicts with the real method's
                // docblock — MismatchingDocblockParamType; a docblock-only
                // implementer param conflicting with the guide is an
                // ImplementedParamTypeMismatch; native signatures keep the
                // signature-mismatch kinds.
                let issue_kind = if implementer_is_pseudo {
                    IssueKind::MismatchingDocblockParamType
                } else if implementer_param.signature_type.is_none()
                    && implementer_param.has_docblock_type
                {
                    IssueKind::ImplementedParamTypeMismatch
                } else if method_name == StrId::CONSTRUCT
                    && (guide_class_info.kind == ClassLikeKind::Interface
                        || guide_class_info.is_consistent_constructor)
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

                    // Psalm only reports the coerced (narrowing) case for
                    // user-defined guide classes.
                    let coerced_reportable = comparison_result.type_coerced.unwrap_or(false)
                        && !guide_param_is_stubbed;
                    if coerced_reportable
                        || (implementer_is_subset
                            && !comparison_result.type_coerced.unwrap_or(false))
                    {
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
            && (guide_class_info.kind == ClassLikeKind::Interface
                || guide_class_info.is_consistent_constructor)
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

    let specialize_and_expand = |union: &TUnion| {
        let mut specialized = specialize_for_comparison(union);
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            &mut specialized,
            &crate::type_expander::TypeExpansionOptions {
                evaluate_conditional_types: true,
                ..Default::default()
            },
        );
        specialized
    };

    // Stub-ness follows the guide method's *declaring* class: an inherited
    // method storage shared into a user-defined interface still carries the
    // stub's tentative signature semantics.
    let guide_declaring_file_path = guide_method
        .declaring_class
        .and_then(|declaring_class| analyzer.codebase.get_class(declaring_class))
        .map_or(guide_class_info.file_path, |declaring_info| {
            declaring_info.file_path
        });
    let guide_is_stubbed = analyzer
        .codebase
        .files
        .get(&guide_declaring_file_path)
        .is_some_and(|file_info| file_info.is_stub);

    // Psalm MethodComparator::compareMethodSignatureReturnTypes — native
    // signature vs native signature, run whenever the guide declares one.
    if let Some(guide_signature_return_type) = guide_method.signature_return_type.as_ref()
        && !implementer_is_pseudo
    {
        let guide_signature = specialize_and_expand(guide_signature_return_type);

        match implementer_method.signature_return_type.as_ref() {
            Some(implementer_signature_return_type) => {
                let implementer_signature = specialize_and_expand(implementer_signature_return_type);
                let mut comparison_result = TypeComparisonResult::new();
                // Psalm: signature return covariance is only allowed from PHP
                // 7.4 (analysis_php_version_id >= 7_04_00 → isContainedBy;
                // earlier versions require the PHP-level types to match).
                let signatures_compatible = if analyzer.config.php_version_id() >= 70400 {
                    union_type_comparator::is_contained_by(
                        analyzer.codebase,
                        &implementer_signature,
                        &guide_signature,
                        false,
                        false,
                        &mut comparison_result,
                    )
                } else {
                    implementer_signature.get_id(Some(analyzer.interner))
                        == guide_signature.get_id(Some(analyzer.interner))
                };
                if !signatures_compatible {
                    emit_method_issue(
                        analyzer,
                        analysis_data,
                        implementer_method,
                        base_mismatch_kind,
                        format!(
                            "Method {} with return type '{}' is different to return type '{}' of inherited method {}",
                            implementer_method_id,
                            implementer_signature.get_id(Some(analyzer.interner)),
                            guide_signature.get_id(Some(analyzer.interner)),
                            guide_method_id
                        ),
                    );
                }
            }
            None => {
                // Psalm reports a missing implementer signature against a
                // user-defined guide signature. Stub signatures are PHP's
                // *tentative* return types (Psalm's own stubs omit them), so
                // omitting them is legal — that case is covered by
                // MethodSignatureMustProvideReturnType instead. A
                // #[ReturnTypeWillChange] attribute also waives the check.
                if !guide_is_stubbed
                    && !guide_signature.is_mixed()
                    && !implementer_method.has_return_type_will_change_attribute
                {
                    emit_method_issue(
                        analyzer,
                        analysis_data,
                        implementer_method,
                        base_mismatch_kind,
                        format!(
                            "Method {} with return type '' is different to return type '{}' of inherited method {}",
                            implementer_method_id,
                            guide_signature.get_id(Some(analyzer.interner)),
                            guide_method_id
                        ),
                    );
                }
            }
        }
    }

    // Psalm MethodComparator: PHP 8.1 deprecates omitting the return type
    // signature when overriding a native method that declares one (tentative
    // return types). #[ReturnTypeWillChange] waives the notice.
    if guide_is_stubbed
        && !implementer_is_pseudo
        && analyzer.config.php_version_id() >= 80100
        && (guide_method.return_type.is_some() || guide_method.signature_return_type.is_some())
        && implementer_method.signature_return_type.is_none()
        && !implementer_method.has_return_type_will_change_attribute
    {
        emit_method_issue(
            analyzer,
            analysis_data,
            implementer_method,
            IssueKind::MethodSignatureMustProvideReturnType,
            format!(
                "Method {} must have a return type signature",
                implementer_method_id
            ),
        );
    }

    let inherited_return_fallback = if implementer_method.signature_return_type.is_none()
        && implementer_method.return_type.is_none()
        && implementer_method.declaring_class == Some(class_info.name)
    {
        get_specialized_inherited_return_type(analyzer, class_info, method_name)
    } else {
        None
    };

    // An implementer with no declared types at all checks its inherited
    // (documenting) docblock return against the guide; a mismatch there means
    // the method body cannot satisfy the inherited declaration.
    if let (Some(inherited_return_type), Some(guide_return_type)) = (
        inherited_return_fallback.as_ref(),
        guide_method
            .return_type
            .as_ref()
            .or(guide_method.signature_return_type.as_ref()),
    ) {
        let guide_return_type = specialize_and_expand(guide_return_type);
        let inherited_return_type = specialize_and_expand(inherited_return_type);
        let mut comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &inherited_return_type,
            &guide_return_type,
            false,
            false,
            &mut comparison_result,
        ) {
            emit_method_issue(
                analyzer,
                analysis_data,
                implementer_method,
                IssueKind::InvalidReturnType,
                format!(
                    "Method {} with return type '{}' is different to return type '{}' of inherited method {}",
                    implementer_method_id,
                    inherited_return_type.get_id(Some(analyzer.interner)),
                    guide_return_type.get_id(Some(analyzer.interner)),
                    guide_method_id
                ),
            );
        }
        return;
    }

    // When the implementer declares no docblock return type, it inherits the guide's
    // docblock return type. If the guide's (more specific) type fits within the
    // implementer's native return type, treat it as inherited and skip the mismatch
    // rather than comparing the widened native type against the guide. Matches Psalm.
    if implementer_method.return_type.is_none()
        && let (Some(guide_return_type), Some(native_return_type)) = (
            guide_method
                .return_type
                .as_ref()
                .or(guide_method.signature_return_type.as_ref()),
            implementer_method.signature_return_type.as_ref(),
        )
    {
        let guide_specialized = specialize_and_expand(guide_return_type);
        let native_specialized = specialize_and_expand(native_return_type);
        let mut inherit_result = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            &guide_specialized,
            &native_specialized,
            false,
            false,
            &mut inherit_result,
        ) {
            return;
        }
    }

    // Psalm MethodComparator: an inherited docblock return type is not the
    // method's own declaration — comparing it against another ancestor's
    // docblock would manufacture conflicts the user never wrote.
    if implementer_method.inherited_return_type {
        return;
    }

    // Psalm MethodComparator::compareMethodDocblockReturnTypes — docblock vs
    // docblock (either side falling back to its signature type), gated on at
    // least one side declaring a real docblock type, and on stub guides
    // carrying class templates (un-templated stub docblocks are not enforced).
    if std::env::var("PZOOM_DBG_PSEUDO").is_ok() {
        eprintln!(
            "DBCMP {}: guide_rt={:?} impl_rt={:?} inherited={} stubbed={}",
            guide_method_id,
            guide_method.return_type.as_ref().map(|t| t.get_id(Some(analyzer.interner))),
            implementer_method.return_type.as_ref().map(|t| t.get_id(Some(analyzer.interner))),
            implementer_method.inherited_return_type,
            guide_is_stubbed,
        );
    }
    if let (Some(guide_return_type), Some(implementer_return_type)) = (
        guide_method
            .return_type
            .as_ref()
            .or(guide_method.signature_return_type.as_ref()),
        implementer_method
            .return_type
            .as_ref()
            .or(implementer_method.signature_return_type.as_ref()),
    ) && (guide_method.return_type.is_some() || implementer_method.return_type.is_some())
        && (!guide_is_stubbed || !guide_class_info.template_types.is_empty())
    {
        let mut guide_return_type = specialize_and_expand(guide_return_type);
        let mut implementer_return_type = specialize_and_expand(implementer_return_type);

        // Psalm: treat void as null when comparing docblock return types.
        if implementer_return_type.is_void() {
            implementer_return_type = TUnion::null();
        }
        if guide_return_type.is_void() {
            guide_return_type = TUnion::null();
        }

        let mut comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &implementer_return_type,
            &guide_return_type,
            false,
            false,
            &mut comparison_result,
        ) {
            let implementer_declaring_method_id = format!(
                "{}::{}",
                analyzer.interner.lookup(class_info.name),
                analyzer.interner.lookup(method_name).to_lowercase()
            );
            if comparison_result.type_coerced.unwrap_or(false) {
                emit_method_issue(
                    analyzer,
                    analysis_data,
                    implementer_method,
                    IssueKind::LessSpecificImplementedReturnType,
                    format!(
                        "The inherited return type '{}' for {} is more specific than the implemented return type for {} '{}'",
                        guide_return_type.get_id(Some(analyzer.interner)),
                        guide_method_id,
                        implementer_declaring_method_id,
                        implementer_return_type.get_id(Some(analyzer.interner))
                    ),
                );
            } else if guide_class_id == class_info.name {
                // Psalm: a conflict against the SAME class's real method is a
                // bad @method annotation.
                emit_method_issue(
                    analyzer,
                    analysis_data,
                    implementer_method,
                    IssueKind::MismatchingDocblockReturnType,
                    format!(
                        "The inherited return type '{}' for {} is different to the corresponding @method annotation '{}'",
                        guide_return_type.get_id(Some(analyzer.interner)),
                        guide_method_id,
                        implementer_return_type.get_id(Some(analyzer.interner))
                    ),
                );
            } else {
                emit_method_issue(
                    analyzer,
                    analysis_data,
                    implementer_method,
                    IssueKind::ImplementedReturnTypeMismatch,
                    format!(
                        "The inherited return type '{}' for {} is different to the implemented return type for {} '{}'",
                        guide_return_type.get_id(Some(analyzer.interner)),
                        guide_method_id,
                        implementer_declaring_method_id,
                        implementer_return_type.get_id(Some(analyzer.interner))
                    ),
                );
            }
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

        // Detect whether the parent property type is fully driven by
        // `@template-covariant` template parameters. If so, a covariant (narrower)
        // child type is allowed (mirrors Psalm's ClassAnalyzer covariant-template
        // upper-bound substitution).
        let parent_type_allows_covariance =
            union_template_params_all_covariant(analyzer, &parent_type);

        if let Some(parent_template_replacements) = class_info
            .template_extended_params
            .get(&parent_property.declaring_class)
            && let Some(parent_declaring_info) =
                analyzer.codebase.get_class(parent_property.declaring_class)
        {
            let mut parent_template_result =
                function_call_analyzer::get_class_template_defaults(parent_declaring_info);
            for (template_name, replacement) in parent_template_replacements {
                crate::template::lower_bounds_insert(
                    &mut parent_template_result,
                    *template_name,
                    pzoom_code_info::GenericParent::ClassLike(parent_property.declaring_class),
                    replacement.clone(),
                );
            }
            parent_type = function_call_analyzer::replace_templates_in_union(
                &parent_type,
                &parent_template_result,
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

        // Mirror Psalm's ClassAnalyzer: a `@readonly`/readonly parent property
        // cannot be written, so a covariant (narrower) child type is allowed as
        // long as the child type is contained by the parent type.
        if parent_property.is_readonly && child_contained_by_parent {
            continue;
        }

        // `@template-covariant` parent property: a covariant (narrower) child type
        // is permitted, so only require the child type to be contained by the parent.
        if parent_type_allows_covariance && child_contained_by_parent {
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

    let (issue_start, issue_end) = class_issue_pos(class_info);
    let (line, col) = analyzer.get_line_column(issue_start);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidTraversableImplementation,
        format!(
            "Class {} cannot implement Traversable directly",
            analyzer.interner.lookup(class_info.name)
        ),
        analyzer.file_path,
        issue_start,
        issue_end,
        line,
        col,
    ));
}


/// Determine whether a (parent) property type's variance is fully driven by
/// `@template-covariant` template parameters. Returns true only when the type
/// references at least one template parameter and every referenced template
/// parameter is declared covariant in its defining class. Used to permit a
/// covariant child property type when overriding a covariant-templated parent.
fn union_template_params_all_covariant(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> bool {
    let mut saw_template = false;
    let mut all_covariant = true;

    for atomic in &union.types {
        collect_template_covariance(analyzer, atomic, &mut saw_template, &mut all_covariant);
    }

    saw_template && all_covariant
}

fn collect_template_covariance(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
    saw_template: &mut bool,
    all_covariant: &mut bool,
) {
    match atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity,
            as_type,
        } => {
            *saw_template = true;
            let is_covariant = match defining_entity {
                pzoom_code_info::GenericParent::ClassLike(defining_class) => {
                    analyzer.codebase.get_class(*defining_class)
                }
                _ => None,
            }
                .and_then(|class_info| {
                    class_info
                        .template_types
                        .iter()
                        .find(|template| template.name == *name)
                })
                .map(|template| {
                    matches!(
                        template.variance,
                        pzoom_code_info::class_like_info::TemplateVariance::Covariant
                    )
                })
                .unwrap_or(false);
            if !is_covariant {
                *all_covariant = false;
            }
            for nested in &as_type.types {
                collect_template_covariance(analyzer, nested, saw_template, all_covariant);
            }
        }
        _ => {}
    }
}

/// Report `PropertyNotSetInConstructor` for typed, non-nullable, default-less,
/// non-promoted instance properties declared by this class that are never
/// initialized in its constructor — or in any same-class method the constructor
/// calls (Psalm's `CallAnalyzer::collectSpecialInformation`). Mirrors Psalm's
/// `ClassAnalyzer::checkPropertyInitialization`.
/// Psalm's `ClassAnalyzer::checkPropertyInitialization`: report
/// `MissingConstructor` for typed default-less properties of a concrete class
/// with no constructor, `PropertyNotSetInConstructor` for properties the
/// constructor (and the `$this`-bound methods it definitely calls) fails to
/// assign on every path, and `UninitializedProperty` for constructor reads of
/// a property before anything could have initialized it. Where Psalm
/// re-simulates the constructor with a `collect_initializations` context,
/// pzoom expands the scan-time per-method initialization summaries across the
/// class hierarchy.
fn check_property_initialization(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    if class_info.kind != ClassLikeKind::Class || class_info.is_abstract {
        return;
    }

    let constructor = class_info.methods.get(&StrId::CONSTRUCT);
    let constructor_declared_here = constructor
        .map(|method| method.declaring_class == Some(class_info.name))
        .unwrap_or(false);

    let declared_in_stub = |declaring_class: StrId| {
        analyzer
            .codebase
            .get_class(declaring_class)
            .and_then(|declaring_info| analyzer.codebase.files.get(&declaring_info.file_path))
            .is_some_and(|file_info| file_info.is_stub)
    };

    // Psalm's $uninitialized_properties: non-static, default-less properties
    // appearing on this class that nothing has initialized yet.
    let mut candidates: Vec<StrId> = Vec::new();
    for property_name in class_info.appearing_property_ids.keys().copied() {
        let Some(property) = class_info.properties.get(&property_name) else {
            continue;
        };
        if property.is_static || property.has_default || property.is_hooked {
            continue;
        }
        // Location-less properties (PropertyMap entries) are skipped — Psalm
        // gates both reports on `$property_storage->location`.
        if property.location_free {
            continue;
        }
        // `@psalm-suppress PropertyNotSetInConstructor` on the property
        // docblock marks it initialized at scan time (Psalm's
        // ClassLikeNodeScanner), for inheritors too.
        if property.marked_initialized {
            continue;
        }
        // A promoted property is initialized by its declaring constructor —
        // unless this class declares its own constructor while the property
        // was promoted in a parent's (Psalm unsets property_is_initialized).
        if property.is_promoted
            && !(property.declaring_class != class_info.name && constructor_declared_here)
        {
            continue;
        }
        // A docblock-only nullable type is implicitly null (Psalm skips
        // `from_docblock && isNullable`); a native nullable type still
        // starts uninitialized.
        if property.signature_type.is_none()
            && property.get_type().is_some_and(TUnion::is_nullable)
        {
            continue;
        }
        // Stub classes' uninitialized properties are signature artifacts —
        // their constructors are opaque to us.
        if declared_in_stub(property.declaring_class) {
            continue;
        }
        candidates.push(property_name);
    }

    if candidates.is_empty() {
        return;
    }

    // Psalm's $uninitialized_typed_properties: natively typed, or docblock
    // typed with something more specific than mixed.
    let property_is_typed = |property: &pzoom_code_info::property_info::PropertyInfo| {
        property.signature_type.is_some()
            || property.get_type().is_some_and(|union| !union.is_mixed())
    };

    let class_name = analyzer.interner.lookup(class_info.name);

    // A constructor declared in a stub has no followable body; Psalm's
    // simulation requires `user_defined && !stubbed` and otherwise falls
    // through to MissingConstructor.
    let constructor_is_opaque = constructor.is_some_and(|method| {
        analyzer
            .codebase
            .files
            .get(&method.file_path)
            .is_some_and(|file_info| file_info.is_stub)
            || method.declaring_class.is_some_and(|declaring_class| {
                analyzer
                    .codebase
                    .get_class(declaring_class)
                    .is_some_and(|declaring_info| declaring_info.is_stubbed)
            })
    });

    let Some(constructor) = constructor.filter(|_| !constructor_is_opaque) else {
        // The phpunit plugin (loaded via psalm.xml pluginClass) suppresses
        // MissingConstructor for TestCase descendants that declare an
        // initializer like setUp() (TestCaseHandler::afterCodebasePopulated).
        if analyzer
            .config
            .plugin_stubs
            .iter()
            .any(|stub| stub.contains("plugin-phpunit"))
            && class_info.all_parent_classes.iter().any(|parent| {
                &*analyzer.interner.lookup(*parent) == "PHPUnit\\Framework\\TestCase"
            })
            && class_info.methods.keys().any(|method_name| {
                analyzer
                    .interner
                    .lookup(*method_name)
                    .eq_ignore_ascii_case("setup")
            })
        {
            return;
        }

        // No (followable) constructor anywhere in the hierarchy: every typed
        // candidate is a MissingConstructor (Psalm reports one per property).
        if analyzer.config.is_issue_suppressed("MissingConstructor")
            || docblock_before_offset_suppresses(
                analyzer.source,
                class.span().start.offset,
                "MissingConstructor",
            )
        {
            return;
        }
        for property_name in candidates {
            let Some(property) = class_info.properties.get(&property_name) else {
                continue;
            };
            if !property_is_typed(property.as_ref()) {
                continue;
            }
            // Psalm locates MissingConstructor at the property itself; for a
            // property declared in a vendor/dependency file the issue lands
            // outside the project and is dropped (hide_external_errors /
            // ignoreFiles).
            let declared_in_project = analyzer
                .codebase
                .get_class(property.declaring_class)
                .and_then(|declaring_info| analyzer.codebase.files.get(&declaring_info.file_path))
                .is_none_or(|file_info| file_info.is_in_project_dirs);
            if !declared_in_project {
                continue;
            }
            let own_property = property.declaring_class == class_info.name;
            if own_property
                && docblock_before_offset_suppresses(
                    analyzer.source,
                    property.start_offset,
                    "MissingConstructor",
                )
            {
                continue;
            }
            // Own property: point at the property; inherited: at the class
            // (the property's offset belongs to another file).
            let error_offset = if own_property {
                property.start_offset
            } else {
                class.span().start.offset
            };
            let prop_name = analyzer.interner.lookup(property_name);
            let (line, col) = analyzer.get_line_column(error_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::MissingConstructor,
                format!(
                    "{} has an uninitialized property {}::${}, but no constructor",
                    class_name, class_name, prop_name
                ),
                analyzer.file_path,
                error_offset,
                error_offset,
                line,
                col,
            ));
        }
        return;
    };

    // Abstract constructors have no code; stubbed ones are opaque.
    if constructor.is_abstract {
        return;
    }
    if constructor.declaring_class.is_some_and(declared_in_stub) {
        return;
    }

    let any_private = candidates.iter().any(|name| {
        class_info
            .properties
            .get(name)
            .is_some_and(|property| matches!(property.visibility, Visibility::Private))
    });
    // Psalm's collect_nonprivate_initializations: when every uninitialized
    // property is non-private, overridable methods may initialize them too.
    let collect_nonprivate = !any_private;

    // Does an assignment made by a method declared in `from_class` initialize
    // the checked class's property? A private property only counts when both
    // sides resolve to the same declaration (Psalm's initialized_class check:
    // assigning `$this->b` in a parent constructor doesn't set a child's own
    // private `$b`).
    let assignment_initializes = |from_class: StrId, property_name: StrId| -> bool {
        let Some(property) = class_info.properties.get(&property_name) else {
            return false;
        };
        if !matches!(property.visibility, Visibility::Private) || from_class == class_info.name {
            return true;
        }
        let from_class_info = analyzer.codebase.get_class(from_class);
        if from_class_info.is_some_and(|info| info.kind == ClassLikeKind::Trait) {
            // Trait methods run in the using class's context.
            return true;
        }
        let from_declaring = from_class_info
            .and_then(|info| info.declaring_property_ids.get(&property_name).copied());
        from_declaring.is_some()
            && from_declaring == class_info.declaring_property_ids.get(&property_name).copied()
    };

    // Expand the constructor's events, following `$this`-bound calls the way
    // Psalm's collectSpecialInformation does: instance calls only when the
    // resolved method is private or final (or nothing private is at stake),
    // `parent::`/ancestor static-dispatch calls unconditionally. An exhaustive
    // alternation initializes what every alternative (fully expanded)
    // initializes.
    let mut initialized: FxHashSet<StrId> = FxHashSet::default();
    let mut visited: FxHashSet<(StrId, StrId)> = FxHashSet::default();
    expand_initializer_method(
        analyzer,
        class_info,
        constructor,
        collect_nonprivate,
        &assignment_initializes,
        &mut initialized,
        &mut visited,
    );

    // UninitializedProperty: the constructor body read `$this->prop` before
    // anything could have initialized it. Only this class's own constructor
    // has positions in this file.
    if constructor_declared_here && !analyzer.config.is_issue_suppressed("UninitializedProperty")
    {
        for (property_name, offset) in &constructor.initializer_uninit_reads {
            if !candidates.contains(property_name) {
                continue;
            }
            let prop_name = analyzer.interner.lookup(*property_name);
            let (line, col) = analyzer.get_line_column(*offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::UninitializedProperty,
                format!("Cannot use uninitialized property $this->{}", prop_name),
                analyzer.file_path,
                *offset,
                *offset,
                line,
                col,
            ));
        }
    }

    // Class-level suppression covers every property.
    if analyzer
        .config
        .is_issue_suppressed("PropertyNotSetInConstructor")
        || docblock_before_offset_suppresses(
            analyzer.source,
            class.span().start.offset,
            "PropertyNotSetInConstructor",
        )
    {
        return;
    }

    // "private or final " when any uninitialized property is private (Psalm).
    let visibility_phrase = if any_private { "private or final " } else { "" };

    for property_name in candidates {
        if initialized.contains(&property_name) {
            continue;
        }
        let Some(property) = class_info.properties.get(&property_name) else {
            continue;
        };
        // Only typed properties are reported (Psalm widens its inferred type
        // with null for the rest instead).
        if !property_is_typed(property.as_ref()) {
            continue;
        }

        // Property-level suppression.
        if docblock_before_offset_suppresses(
            analyzer.source,
            property.start_offset,
            "PropertyNotSetInConstructor",
        ) {
            continue;
        }

        // psalm.xml `<referencedProperty name="Class::$prop"/>` suppression.
        let property_id = format!(
            "{}::${}",
            analyzer.interner.lookup(property.declaring_class),
            analyzer.interner.lookup(property_name)
        );
        if analyzer
            .config
            .is_issue_suppressed_for_property("PropertyNotSetInConstructor", &property_id)
        {
            continue;
        }

        // Own property: point at the property. Inherited property: point at
        // the class (Psalm uses the class location when the declaring class
        // differs).
        let error_offset = if property.declaring_class == class_info.name {
            property.start_offset
        } else {
            class.span().start.offset
        };

        let prop_name = analyzer.interner.lookup(property_name);
        let (line, col) = analyzer.get_line_column(error_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::PropertyNotSetInConstructor,
            format!(
                "Property {}::${} is not defined in constructor of {} or in any {}methods called in the constructor",
                class_name, prop_name, class_name, visibility_phrase
            ),
            analyzer.file_path,
            error_offset,
            error_offset,
            line,
            col,
        ));
    }
}


/// Expand one method's initialization events into `initialized`, following
/// `$this`-bound calls (pzoom's stand-in for Psalm's `getMethodMutations`
/// during a `collect_initializations` pass).
#[allow(clippy::too_many_arguments)]
fn expand_initializer_method(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    method: &std::sync::Arc<pzoom_code_info::FunctionLikeInfo>,
    collect_nonprivate: bool,
    assignment_initializes: &dyn Fn(StrId, StrId) -> bool,
    initialized: &mut FxHashSet<StrId>,
    visited: &mut FxHashSet<(StrId, StrId)>,
) {
    let method_class = method.declaring_class.unwrap_or(class_info.name);
    if !visited.insert((method_class, method.name)) {
        return;
    }

    // A constructor's promoted parameters assign their properties.
    if method.name == StrId::CONSTRUCT
        && let Some(method_class_info) = analyzer.codebase.get_class(method_class)
    {
        for (property_name, property) in &method_class_info.properties {
            if property.is_promoted
                && property.declaring_class == method_class
                && assignment_initializes(method_class, *property_name)
            {
                initialized.insert(*property_name);
            }
        }
    }

    expand_initializer_events(
        analyzer,
        class_info,
        &method.initializer_events,
        method_class,
        collect_nonprivate,
        assignment_initializes,
        initialized,
        visited,
    );
}

#[allow(clippy::too_many_arguments)]
fn expand_initializer_events(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    events: &[pzoom_code_info::functionlike_info::InitializerEvent],
    method_class: StrId,
    collect_nonprivate: bool,
    assignment_initializes: &dyn Fn(StrId, StrId) -> bool,
    initialized: &mut FxHashSet<StrId>,
    visited: &mut FxHashSet<(StrId, StrId)>,
) {
    use pzoom_code_info::functionlike_info::InitializerEvent;

    for event in events {
        match event {
            InitializerEvent::Assign(property_name) => {
                if assignment_initializes(method_class, *property_name) {
                    initialized.insert(*property_name);
                }
            }
            InitializerEvent::ThisCall(callee_name) => {
                // Resolve against the checked class so overrides win; magic
                // __call resolutions just aren't followed (Psalm).
                let Some(callee) = class_info.methods.get(callee_name) else {
                    continue;
                };
                if callee.is_static {
                    continue;
                }
                if collect_nonprivate
                    || matches!(callee.visibility, Visibility::Private)
                    || callee.is_final
                    || class_info.is_final
                {
                    expand_initializer_method(
                        analyzer,
                        class_info,
                        callee,
                        collect_nonprivate,
                        assignment_initializes,
                        initialized,
                        visited,
                    );
                }
            }
            InitializerEvent::ParentCall(callee_name) => {
                // A trait method's `parent::` refers to the using class's
                // parent.
                let parent_base = match analyzer.codebase.get_class(method_class) {
                    Some(info) if info.kind == ClassLikeKind::Trait => class_info.name,
                    _ => method_class,
                };
                let Some(callee) = analyzer
                    .codebase
                    .get_class(parent_base)
                    .and_then(|info| info.parent_class)
                    .and_then(|parent| analyzer.codebase.get_class(parent))
                    .and_then(|parent_info| parent_info.methods.get(callee_name))
                else {
                    continue;
                };
                expand_initializer_method(
                    analyzer,
                    class_info,
                    callee,
                    collect_nonprivate,
                    assignment_initializes,
                    initialized,
                    visited,
                );
            }
            InitializerEvent::NamedCall(raw_class_name, callee_name) => {
                // Psalm follows `AncestorClass::m()` static-dispatch calls
                // when the checked class extends the named class.
                let Some(target_class) =
                    resolve_initializer_named_class(analyzer, class_info, *raw_class_name)
                else {
                    continue;
                };
                let Some(callee) = analyzer
                    .codebase
                    .get_class(target_class)
                    .and_then(|target_info| target_info.methods.get(callee_name))
                else {
                    continue;
                };
                expand_initializer_method(
                    analyzer,
                    class_info,
                    callee,
                    collect_nonprivate,
                    assignment_initializes,
                    initialized,
                    visited,
                );
            }
            InitializerEvent::Branch(branches) => {
                // An exhaustive alternation establishes what every alternative
                // (fully expanded) establishes.
                let mut merged: Option<FxHashSet<StrId>> = None;
                for branch in branches {
                    let mut branch_initialized = initialized.clone();
                    let mut branch_visited = visited.clone();
                    expand_initializer_events(
                        analyzer,
                        class_info,
                        branch,
                        method_class,
                        collect_nonprivate,
                        assignment_initializes,
                        &mut branch_initialized,
                        &mut branch_visited,
                    );
                    merged = Some(match merged {
                        None => branch_initialized,
                        Some(accumulated) => accumulated
                            .intersection(&branch_initialized)
                            .copied()
                            .collect(),
                    });
                }
                if let Some(merged) = merged {
                    *initialized = merged;
                }
            }
        }
    }
}

/// Resolve a `SomeClass::m()` class name as written in a method body against
/// the checked class's ancestry (Psalm follows such calls only when
/// `classExtends($context->self, X)` holds).
fn resolve_initializer_named_class(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    raw_name: StrId,
) -> Option<StrId> {
    let raw = analyzer.interner.lookup(raw_name);
    let trimmed = raw.trim_start_matches('\\');

    let mut candidates: Vec<StrId> = vec![analyzer.interner.intern(trimmed)];
    // A relative name resolves against the checked class's namespace.
    let class_name = analyzer.interner.lookup(class_info.name);
    if let Some((namespace, _)) = class_name.rsplit_once('\\') {
        candidates.push(
            analyzer
                .interner
                .intern(&format!("{}\\{}", namespace, trimmed)),
        );
    }

    candidates.into_iter().find(|candidate| {
        *candidate == class_info.name || class_info.all_parent_classes.contains(candidate)
    })
}

/// Whether the `/** ... */` docblock immediately preceding `offset` carries a
/// `@psalm-suppress <issue_name>` tag.
fn docblock_before_offset_suppresses(source: &str, offset: u32, issue_name: &str) -> bool {
    let offset = (offset as usize).min(source.len());
    let head = &source[..offset];
    let Some(end) = head.rfind("*/") else {
        return false;
    };
    // The docblock must be adjacent (only whitespace between it and the target).
    if head[end + 2..].chars().any(|c| !c.is_whitespace()) {
        return false;
    }
    let Some(start) = head[..end].rfind("/**") else {
        return false;
    };
    let docblock = &head[start..end + 2];

    docblock.lines().any(|line| {
        line.find("@psalm-suppress").is_some_and(|idx| {
            line[idx + "@psalm-suppress".len()..]
                .split(|c: char| c.is_whitespace() || c == ',')
                .map(|token| {
                    token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\\')
                })
                .any(|token| token == issue_name)
        })
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

    // PHP < 8.2: traits cannot declare constants (Psalm's
    // ConstantDeclarationInTrait).
    if analyzer.config.php_version_id() < 80200
        && let Some(info) = trait_info
    {
        for const_info in info.constants.values() {
            if const_info.declaring_class != info.name {
                continue;
            }
            let (line, col) = analyzer.get_line_column(const_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::ConstantDeclarationInTrait,
                "Traits cannot declare constants until PHP 8.2",
                analyzer.file_path,
                const_info.start_offset,
                const_info.start_offset.saturating_add(1),
                line,
                col,
            ));
        }
    }

    // Check for missing property types in trait-declared properties
    if let Some(info) = trait_info {
        let name_span = trait_stmt.name.span();
        let dependency_fallback = (name_span.start.offset, name_span.end.offset);
        let dependency_spans = collect_dependency_name_spans(
            analyzer,
            None,
            None,
            trait_stmt.members.as_slice(),
            context,
        );
        check_missing_dependencies(
            analyzer, info, context, analysis_data, &dependency_spans, dependency_fallback,
        );
        check_duplicate_property_declarations(analyzer, info, analysis_data);
        check_duplicate_constant_declarations(analyzer, info, analysis_data);
        check_class_constant_overrides(analyzer, info, analysis_data);
        check_docblock_issues(analyzer, info, analysis_data);
        check_undefined_docblock_mixins(analyzer, info, analysis_data);
        check_undefined_docblock_property_types(analyzer, info, analysis_data);
        check_pseudo_method_compatibility(analyzer, info, analysis_data);
        check_pseudo_method_annotations(analyzer, info, analysis_data);
        check_deprecated_and_internal_relationships(analyzer, info, analysis_data);
        check_method_docblock_param_type_mismatches(analyzer, info, analysis_data);
        check_extended_template_param_bounds(analyzer, info, analysis_data);
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

        // Psalm's InstancePropertyAssignmentAnalyzer::analyzeStatement: a
        // readonly property cannot have a default value.
        if prop_info.is_readonly_native && prop_info.has_default && !prop_info.is_promoted {
            let prop_name_str = analyzer.interner.lookup(prop_info.name);
            let (line, col) = analyzer.get_line_column(prop_info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidPropertyAssignment,
                format!(
                    "Readonly property {}::${} cannot have a default",
                    class_name, prop_name_str
                ),
                analyzer.file_path,
                prop_info.start_offset,
                prop_info.start_offset + 1,
                line,
                col,
            ));
        }

        // If the property overrides a parent declaration, Psalm doesn't re-report
        // missing-type issues on the overriding declaration.
        if find_parent_property(analyzer, class_info.parent_class, prop_info.name).is_some() {
            continue;
        }

        // A readonly class requires NATIVE property types: a docblock alone
        // does not satisfy PHP, so MissingPropertyType still reports (Psalm).
        if class_info.is_readonly
            && prop_info.signature_type.is_none()
            && !prop_info.is_promoted
        {
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
        } else if issue
            .message
            .starts_with("Cannot add an item with an offset beyond")
        {
            IssueKind::InvalidArrayOffset
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
            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);

            if parent_info.is_deprecated {
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedClass,
                    format!("{} is marked deprecated", parent_name),
                    analyzer.file_path,
                    issue_start,
                    issue_end,
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
                    issue_start,
                    issue_end,
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
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);

        if interface_info.is_deprecated {
            analysis_data.add_issue(Issue::new(
                IssueKind::DeprecatedInterface,
                format!("{} is marked deprecated", interface_name),
                analyzer.file_path,
                issue_start,
                issue_end,
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
                issue_start,
                issue_end,
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
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);
        analysis_data.add_issue(Issue::new(
            IssueKind::DeprecatedTrait,
            format!("Trait {} is deprecated", trait_name),
            analyzer.file_path,
            issue_start,
            issue_end,
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

                let (issue_start, issue_end) = class_issue_pos(class_info);
                let (line, col) = analyzer.get_line_column(issue_start);
                analysis_data.add_issue(Issue::new(
                    issue_kind,
                    format!("{} is marked deprecated", referenced_name),
                    analyzer.file_path,
                    issue_start,
                    issue_end,
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

            let (issue_start, issue_end) = class_issue_pos(class_info);
            let (line, col) = analyzer.get_line_column(issue_start);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedDocblockClass,
                format!(
                    "Docblock-defined class {} does not exist",
                    analyzer.interner.lookup(normalized_class)
                ),
                analyzer.file_path,
                issue_start,
                issue_end,
                line,
                col,
            ));
        }
    }
}

/// Psalm's `ClassConstAnalyzer::analyze` + `getOverriddenConstant`: per-class
/// constant override checks — covariance against the inherited declared type
/// (InvalidClassConstantType / LessSpecificClassConstantType), final
/// overrides (OverriddenFinalConstant), interface overrides before PHP 8.1
/// (OverriddenInterfaceConstant), ambiguous multiple inheritance
/// (AmbiguousConstantInheritance), and `final const` before PHP 8.1
/// (ParseError).
fn check_class_constant_overrides(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let php_lt_81 = analyzer.config.php_version_id() < 80100;
    let class_name = analyzer.interner.lookup(class_info.name);

    let implemented_interfaces: Vec<StrId> = {
        let mut seen = FxHashSet::default();
        class_info
            .interfaces
            .iter()
            .copied()
            .chain(class_info.all_parent_interfaces.iter().copied())
            .filter(|iface| seen.insert(*iface))
            .collect()
    };

    for (const_name, const_info) in &class_info.constants {
        // Enum cases have their own checks.
        if matches!(
            const_info.constant_type.get_single(),
            Some(TAtomic::TEnumCase { .. })
        ) {
            continue;
        }

        let const_name_str = analyzer.interner.lookup(*const_name);
        let is_own = const_info.declaring_class == class_info.name;
        let emit = |analysis_data: &mut FunctionAnalysisData, kind, message: String| {
            // Own constants point at themselves; inherited ones at the class.
            let offset = if is_own {
                const_info.start_offset
            } else {
                class_issue_pos(class_info).0
            };
            let (line, col) = analyzer.get_line_column(offset);
            analysis_data.add_issue(Issue::new(
                kind,
                message,
                analyzer.file_path,
                offset,
                offset,
                line,
                col,
            ));
        };

        // --- Psalm's getOverriddenConstant ---
        let mut parent_classlike: Option<StrId> = None;
        let mut parent_const: Option<&pzoom_code_info::class_constant_info::ClassConstantInfo> =
            None;
        let mut interface_const_class: Option<StrId> = None;
        let mut interface_const_declaring: Option<StrId> = None;
        let mut interface_overrides: Vec<StrId> = Vec::new();

        for iface in &implemented_interfaces {
            let Some(iface_info) = analyzer.codebase.get_class(*iface) else {
                continue;
            };
            let Some(iface_const) = iface_info.constants.get(const_name) else {
                continue;
            };
            // Psalm compares storage identity; distinct declaring classes
            // mean distinct declarations.
            let same_storage = iface_const.declaring_class == const_info.declaring_class;
            if !same_storage && php_lt_81 {
                interface_overrides.push(*iface);
            }
            if let (Some(prev_iface), Some(prev_declaring)) =
                (parent_classlike, interface_const_declaring)
                && interface_const_class.is_some()
            {
                let prev_info = analyzer.codebase.get_class(prev_iface);
                let related = prev_info.is_some_and(|info| {
                    info.interfaces.contains(iface)
                        || info.all_parent_interfaces.contains(iface)
                }) || iface_info.interfaces.contains(&prev_iface)
                    || iface_info.all_parent_interfaces.contains(&prev_iface);
                if !related && prev_declaring != iface_const.declaring_class {
                    emit(
                        analysis_data,
                        IssueKind::AmbiguousConstantInheritance,
                        format!(
                            "Ambiguous inheritance of {}::{} from {} and {}",
                            class_name,
                            const_name_str,
                            analyzer.interner.lookup(*iface),
                            analyzer.interner.lookup(prev_iface),
                        ),
                    );
                }
            }
            interface_const_class = Some(*iface);
            interface_const_declaring = Some(iface_const.declaring_class);
            parent_classlike = Some(*iface);
            parent_const = Some(iface_const);
        }

        let mut found_in_parent = false;
        for parent in &class_info.all_parent_classes {
            let Some(parent_info) = analyzer.codebase.get_class(*parent) else {
                continue;
            };
            let Some(parent_const_info) = parent_info.constants.get(const_name) else {
                continue;
            };
            if let Some(prev_iface) = interface_const_class {
                let parent_implements = parent_info.interfaces.contains(&prev_iface)
                    || parent_info.all_parent_interfaces.contains(&prev_iface);
                if !parent_implements {
                    emit(
                        analysis_data,
                        IssueKind::AmbiguousConstantInheritance,
                        format!(
                            "Ambiguous inheritance of {}::{} from {} and {}",
                            class_name,
                            const_name_str,
                            analyzer.interner.lookup(prev_iface),
                            analyzer.interner.lookup(*parent),
                        ),
                    );
                }
            }
            // If the parent holds this very declaration and doesn't implement
            // the overridden interface, it's ambiguity, not an override.
            if parent_const_info.declaring_class == const_info.declaring_class {
                interface_overrides.retain(|iface| {
                    parent_info.interfaces.contains(iface)
                        || parent_info.all_parent_interfaces.contains(iface)
                });
            }
            parent_classlike = Some(*parent);
            parent_const = Some(parent_const_info);
            found_in_parent = true;
            break;
        }
        let _ = found_in_parent;

        for iface in &interface_overrides {
            emit(
                analysis_data,
                IssueKind::OverriddenInterfaceConstant,
                format!(
                    "{}::{} cannot override constant from {}",
                    class_name,
                    const_name_str,
                    analyzer.interner.lookup(*iface),
                ),
            );
        }

        if let (Some(parent_classlike), Some(parent_const)) = (parent_classlike, parent_const) {
            let parent_classlike_name = analyzer.interner.lookup(parent_classlike);
            let same_storage = parent_const.declaring_class == const_info.declaring_class;

            // Covariance of the DECLARED types.
            if let (Some(child_type), Some(parent_type)) =
                (&const_info.declared_type, &parent_const.declared_type)
                && !same_storage
                && !union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    child_type,
                    parent_type,
                    false,
                    false,
                    &mut TypeComparisonResult::new(),
                )
            {
                if union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    parent_type,
                    child_type,
                    false,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    emit(
                        analysis_data,
                        IssueKind::LessSpecificClassConstantType,
                        format!(
                            "The type \"{}\" for {}::{} is more general than the type \"{}\" inherited from {}::{}",
                            child_type.get_id(Some(analyzer.interner)),
                            class_name,
                            const_name_str,
                            parent_type.get_id(Some(analyzer.interner)),
                            parent_classlike_name,
                            const_name_str,
                        ),
                    );
                } else {
                    emit(
                        analysis_data,
                        IssueKind::InvalidClassConstantType,
                        format!(
                            "The type \"{}\" for {}::{} does not satisfy the type \"{}\" inherited from {}::{}",
                            child_type.get_id(Some(analyzer.interner)),
                            class_name,
                            const_name_str,
                            parent_type.get_id(Some(analyzer.interner)),
                            parent_classlike_name,
                            const_name_str,
                        ),
                    );
                }
            }

            if parent_const.is_final && !same_storage {
                emit(
                    analysis_data,
                    IssueKind::OverriddenFinalConstant,
                    format!(
                        "{} cannot be overridden because it is marked as final in {}",
                        const_name_str, parent_classlike_name,
                    ),
                );
            }
        }

        // Declared type vs assigned value (Psalm's analyzeAssignment:
        // InvalidConstantAssignmentValue when the value doesn't satisfy a
        // docblock/hinted declared type).
        if is_own
            && let Some(declared_type) = &const_info.declared_type
            && *declared_type != const_info.constant_type
            && !const_info.constant_type.is_mixed()
            && !union_type_comparator::is_contained_by(
                analyzer.codebase,
                &const_info.constant_type,
                declared_type,
                false,
                false,
                &mut TypeComparisonResult::new(),
            )
        {
            emit(
                analysis_data,
                IssueKind::InvalidConstantAssignmentValue,
                format!(
                    "{}::{} with declared type {} cannot be assigned type {}",
                    class_name,
                    const_name_str,
                    declared_type.get_id(Some(analyzer.interner)),
                    const_info.constant_type.get_id(Some(analyzer.interner)),
                ),
            );
        }

        // References the initializer couldn't resolve (Psalm reports these
        // when analyzing the constant's assignment expression).
        if is_own {
            for failure in &const_info.resolution_failures {
                use pzoom_code_info::class_constant_info::ConstResolutionFailure;
                match failure {
                    ConstResolutionFailure::MissingClass(missing_class) => emit(
                        analysis_data,
                        IssueKind::UndefinedClass,
                        format!(
                            "Class, interface or enum named {} does not exist",
                            analyzer.interner.lookup(*missing_class)
                        ),
                    ),
                    ConstResolutionFailure::MissingClassConstant(
                        constant_class,
                        missing_constant,
                    ) => emit(
                        analysis_data,
                        IssueKind::UndefinedConstant,
                        format!(
                            "Constant {}::{} is not defined",
                            analyzer.interner.lookup(*constant_class),
                            analyzer.interner.lookup(*missing_constant)
                        ),
                    ),
                    ConstResolutionFailure::MissingGlobalConstant(missing_constant) => emit(
                        analysis_data,
                        IssueKind::UndefinedConstant,
                        format!(
                            "Const {} is not defined",
                            analyzer.interner.lookup(*missing_constant)
                        ),
                    ),
                }
            }
        }

        // A cyclic initializer (Psalm's CircularReferenceException at the
        // initializer's analysis).
        if is_own && const_info.circular {
            emit(
                analysis_data,
                IssueKind::CircularReference,
                format!(
                    "Constant {}::{} contains a circular reference",
                    class_name, const_name_str,
                ),
            );
        }

        // `final const` requires PHP >= 8.1.
        if is_own && const_info.is_final && php_lt_81 {
            emit(
                analysis_data,
                IssueKind::ParseError,
                "Class constants cannot be marked final before PHP 8.1".to_string(),
            );
        }
    }
}

fn check_duplicate_constant_declarations(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for duplicate in &class_info.duplicate_constant_issues {
        let (line, col) = analyzer.get_line_column(duplicate.start_offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::DuplicateConstant,
            "Constant names should be unique".to_string(),
            analyzer.file_path,
            duplicate.start_offset,
            duplicate.end_offset,
            line,
            col,
        ));
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

        // Psalm's ClassAnalyzer: a docblock property type incompatible with
        // the native signature hint is MismatchingDocblockPropertyType. The
        // docblock type is expanded first (Psalm's fleshed_out_type via
        // TypeExpander) so constant wildcards like `Foo::BAR_*` resolve.
        if let Some(signature_type) = property_info.signature_type.as_ref() {
            let mut expanded_property_type = property_type.clone();
            crate::type_expander::expand_union(
                analyzer.codebase,
                analyzer.interner,
                &mut expanded_property_type,
                &crate::type_expander::TypeExpansionOptions {
                    self_class: Some(class_info.name),
                    static_class_type: crate::type_expander::StaticClassType::Name(
                        class_info.name,
                    ),
                    ..Default::default()
                },
            );
            let mut union_comparison_result =
                crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                analyzer.codebase,
                &expanded_property_type,
                signature_type,
                false,
                false,
                &mut union_comparison_result,
            ) && union_comparison_result.type_coerced_from_mixed != Some(true)
            {
                let (line, col) = analyzer.get_line_column(property_info.start_offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MismatchingDocblockPropertyType,
                    format!(
                        "Parameter {}::${} has wrong type '{}', should be '{}'",
                        analyzer.interner.lookup(class_info.name),
                        analyzer.interner.lookup(property_info.name),
                        property_type.get_id(Some(analyzer.interner)),
                        signature_type.get_id(Some(analyzer.interner)),
                    ),
                    analyzer.file_path,
                    property_info.start_offset,
                    property_info.start_offset.saturating_add(1),
                    line,
                    col,
                ));
            }
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

/// Composes an outer variance position with an inner one, following the usual
/// rule: an invariant position anywhere forces invariance, two equal variances
/// compose to covariance, and opposing variances compose to contravariance.
fn compose_variance(outer: TemplateVariance, inner: TemplateVariance) -> TemplateVariance {
    use TemplateVariance::*;
    match (outer, inner) {
        (Invariant, _) | (_, Invariant) => Invariant,
        (Covariant, Covariant) | (Contravariant, Contravariant) => Covariant,
        _ => Contravariant,
    }
}

/// Walk a type at a given variance position, recording any covariant template
/// parameter (defined by `defining_entity`) that appears in a non-covariant
/// position.
fn collect_covariant_misuse(
    codebase: &pzoom_code_info::CodebaseInfo,
    atomic: &TAtomic,
    position: TemplateVariance,
    covariant_names: &[StrId],
    defining_entity: StrId,
    found: &mut FxHashSet<StrId>,
) {
    match atomic {
        TAtomic::TTemplateParam {
            name,
            defining_entity: entity,
            ..
        } => {
            if *entity == pzoom_code_info::GenericParent::ClassLike(defining_entity)
                && covariant_names.contains(name)
                && position != TemplateVariance::Covariant
            {
                found.insert(*name);
            }
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } => {
            let target = codebase.get_class(*name);
            for (index, type_param) in type_params.iter().enumerate() {
                let inner_variance = target
                    .and_then(|info| info.template_types.get(index))
                    .map(|template| template.variance)
                    .unwrap_or(TemplateVariance::Invariant);
                let inner_position = compose_variance(position, inner_variance);
                for inner in &type_param.types {
                    collect_covariant_misuse(
                        codebase,
                        inner,
                        inner_position,
                        covariant_names,
                        defining_entity,
                        found,
                    );
                }
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for inner in types {
                collect_covariant_misuse(
                    codebase,
                    inner,
                    position,
                    covariant_names,
                    defining_entity,
                    found,
                );
            }
        }
        _ => {}
    }
}

/// Reports `InvalidTemplateParam` when a `@template-covariant` parameter is used
/// in a non-covariant position (a method parameter, or an extends/implements
/// type argument whose parent slot is invariant or contravariant), mirroring
/// Psalm's template variance validation.
fn check_template_variance(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let covariant_names: Vec<StrId> = class_info
        .template_types
        .iter()
        .filter(|template| template.variance == TemplateVariance::Covariant)
        .map(|template| template.name)
        .collect();

    if covariant_names.is_empty() {
        return;
    }

    let mut found = FxHashSet::default();

    // In an immutable class covariance is always sound, so Psalm skips the
    // contravariant-parameter check entirely.
    let check_params = !class_info.is_immutable;

    for method in class_info.methods.values() {
        if method.declaring_class != Some(class_info.name) {
            continue;
        }

        // Constructor parameters are how a covariant container is populated and
        // are exempt from the contravariant-position check.
        if check_params && method.name != StrId::CONSTRUCT {
            for param in &method.params {
                if let Some(param_type) = &param.param_type {
                    for atomic in &param_type.types {
                        collect_covariant_misuse(
                            analyzer.codebase,
                            atomic,
                            TemplateVariance::Contravariant,
                            &covariant_names,
                            class_info.name,
                            &mut found,
                        );
                    }
                }
            }
        }

        // Return types are covariant (output) positions; a covariant template
        // nested inside an invariant generic there is still a misuse.
        if let Some(return_type) = &method.return_type {
            for atomic in &return_type.types {
                collect_covariant_misuse(
                    analyzer.codebase,
                    atomic,
                    TemplateVariance::Covariant,
                    &covariant_names,
                    class_info.name,
                    &mut found,
                );
            }
        }
    }

    // Extends/implements type arguments take the variance of the parent's
    // corresponding template slot.
    for (parent_id, type_params) in &class_info.template_extended_offsets {
        let parent_info = analyzer.codebase.get_class(*parent_id);
        for (index, type_param) in type_params.iter().enumerate() {
            let slot_variance = parent_info
                .and_then(|info| info.template_types.get(index))
                .map(|template| template.variance)
                .unwrap_or(TemplateVariance::Invariant);
            for atomic in &type_param.types {
                collect_covariant_misuse(
                    analyzer.codebase,
                    atomic,
                    slot_variance,
                    &covariant_names,
                    class_info.name,
                    &mut found,
                );
            }
        }
    }

    for template_name in found {
        let (issue_start, issue_end) = class_issue_pos(class_info);
        let (line, col) = analyzer.get_line_column(issue_start);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidTemplateParam,
            format!(
                "Template param {} of {} is marked covariant but is used in an \
                 invariant or contravariant position",
                analyzer.interner.lookup(template_name),
                analyzer.interner.lookup(class_info.name)
            ),
            analyzer.file_path,
            issue_start,
            issue_end,
            line,
            col,
        ));
    }
}

/// Validate that classes referenced in `@template-extends`/`@template-implements`/
/// `@template-use` type parameters exist, mirroring Psalm's `UndefinedDocblockClass`
/// reporting for e.g. `@template-extends A<Z>` where `Z` is undefined.
fn check_undefined_docblock_template_extends_classes(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut emitted = FxHashSet::default();

    for (parent_id, type_params) in &class_info.template_extended_offsets {
        // Psalm only materializes an extends/implements/use type argument as a
        // class reference when the corresponding template parameter is actually
        // used by a member of the parent (e.g. `@var T` / `@param T`). When the
        // parent never uses the template (an empty trait, say) an undefined
        // argument like `bar` is harmless and is not reported.
        let Some(parent_info) = analyzer.codebase.get_class(*parent_id) else {
            continue;
        };

        for (index, type_param) in type_params.iter().enumerate() {
            let Some(template) = parent_info.template_types.get(index) else {
                continue;
            };



            let mut referenced_classes = Vec::new();
            for atomic in &type_param.types {
                collect_named_docblock_classes(atomic, &mut referenced_classes);
            }

            for referenced_class in referenced_classes {
                let normalized_class =
                    normalize_docblock_class_reference(analyzer, referenced_class);

                if matches!(
                    normalized_class,
                    StrId::SELF | StrId::STATIC | StrId::PARENT
                ) {
                    continue;
                }

                // Template parameters of the extending class are not classes.
                if class_info
                    .template_types
                    .iter()
                    .any(|template| template.name == normalized_class)
                {
                    continue;
                }

                if analyzer.codebase.get_class(normalized_class).is_some() {
                    continue;
                }

                // Psalm resolves classlikes case-insensitively, so a
                // wrong-cased argument names a real class and is not
                // undefined (pzoom's casing strictness reports elsewhere).
                if crate::class_casing::class_casing_hint(analyzer, normalized_class).is_some() {
                    continue;
                }

                if !emitted.insert(normalized_class) {
                    continue;
                }

                let (issue_start, issue_end) = class_issue_pos(class_info);
                let (line, col) = analyzer.get_line_column(issue_start);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedDocblockClass,
                    format!(
                        "Docblock class {} does not exist",
                        analyzer.interner.lookup(normalized_class)
                    ),
                    analyzer.file_path,
                    issue_start,
                    issue_end,
                    line,
                    col,
                ));
            }
        }
    }
}

fn collect_named_docblock_classes(atomic: &TAtomic, classes: &mut Vec<StrId>) {
    match atomic {
        TAtomic::TNamedObject { name, type_params , .. } => {
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

            if has_return_type_mismatch(analyzer, class_info.name, pseudo_method, parent_method) {
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
) -> Option<&'a std::sync::Arc<pzoom_code_info::FunctionLikeInfo>> {
    // PHP matches method declarations (overrides) case-insensitively even
    // though pzoom resolves method *references* case-sensitively.
    let cased = class_info.cased_method_for(analyzer.interner, *method_name)?;
    class_info.methods.get(&cased)
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
    implementing_class: pzoom_str::StrId,
    pseudo_method: &pzoom_code_info::FunctionLikeInfo,
    parent_method: &pzoom_code_info::FunctionLikeInfo,
) -> bool {
    let Some(parent_return_type) = parent_method.get_return_type() else {
        return false;
    };
    let Some(pseudo_return_type) = pseudo_method.get_return_type() else {
        return false;
    };

    // The parent's `static`/`self` bind to the annotated class at the
    // comparison site (Psalm expands the guide type before comparing).
    let mut expanded_parent_return_type = parent_return_type.clone();
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut expanded_parent_return_type,
        &crate::type_expander::TypeExpansionOptions {
            self_class: parent_method.declaring_class,
            static_class_type: crate::type_expander::StaticClassType::Name(implementing_class),
            ..Default::default()
        },
    );

    let mut comparison_result = TypeComparisonResult::new();
    !union_type_comparator::is_contained_by(
        analyzer.codebase,
        pseudo_return_type,
        &expanded_parent_return_type,
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
                    let span = class.name.span();
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
                // Psalm does not require an explicit `__construct` implementation for
                // an interface that declares one; constructors are exempt from the
                // unimplemented-interface-method check.
                if *method_name == StrId::CONSTRUCT {
                    continue;
                }
                if !implemented_methods.contains(method_name) {
                    let method_name_str = analyzer.interner.lookup(*method_name);
                    let iface_name_str = analyzer.interner.lookup(*iface_name);
                    let span = class.name.span();
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
    )
    .with_arena(&arena);

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
        crate::stmt::function_analyzer::check_param_class_casing(analyzer, info, analysis_data);
        crate::stmt::function_analyzer::check_key_value_of_sentinels(analyzer, info, analysis_data);
        crate::stmt::function_analyzer::emit_unused_docblock_params(
            analyzer,
            info,
            &format!(
                "{}::{}",
                analyzer.interner.lookup(class_name_id),
                method_name
            ),
            analysis_data,
        );

        // `: parent` on a class with no parent (Psalm's InvalidParent).
        if class_info.is_some_and(|ci| ci.parent_class.is_none())
            && info.signature_return_type.as_ref().is_some_and(|ret| {
                ret.types.iter().any(|atomic| {
                    matches!(atomic, TAtomic::TNamedObject { name, .. } if *name == pzoom_str::StrId::PARENT)
                })
            })
        {
            let (line, col) = analyzer.get_line_column(info.start_offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidParent,
                "Cannot use parent as a return type when class has no parent",
                analyzer.file_path,
                info.start_offset,
                info.end_offset,
                line,
                col,
            ));
        }
    }

    // Create a function-like info wrapper for the method
    let func_info = method_info.map(|mi| (**mi).clone()).map(|mut mi| {
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
    let method_analyzer = analyzer.for_nested_function(func_info.as_ref());

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
                                defining_entity: pzoom_code_info::GenericParent::ClassLike(
                                    ci.name,
                                ),
                                as_type: Box::new(template_type.as_type.clone()),
                            })
                        })
                        .collect(),
                )
            });

            // `$this` is the late-static-bound type: the concrete class in `name`
            // with is_static set, so it re-resolves to the runtime class.
            TUnion::new(pzoom_code_info::TAtomic::TNamedObject {
                name: class_name_id,
                type_params: this_type_params,
                is_static: true,
                remapped_params: false,
            })
        };

        let mut this_type = method_info
            .and_then(|info| info.if_this_is_type.clone())
            .unwrap_or(default_this_type);

        // Inside an external-mutation-free method `$this` is reference-free
        // (Psalm FunctionLikeAnalyzer): calling further external-mutation-free
        // methods on it is pure-compatible. Outside the constructor its
        // properties also may not be mutated.
        if method_info.is_some_and(|info| {
            info.is_external_mutation_free && !info.mutation_free_inferred
        }) {
            this_type.reference_free = true;
            if method_name_id != StrId::CONSTRUCT {
                this_type.allow_mutations = false;
            }
        }

        // Hakana `functionlike_analyzer`: in whole-program (taint) mode the
        // method's `$this` starts from a `ThisBeforeMethod` node, so receiver
        // state from call sites flows into the body.
        if let pzoom_code_info::GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind {
            let this_before_node = pzoom_code_info::DataFlowNode::get_for_this_before_method(
                &pzoom_code_info::method_identifier::MethodIdentifier(
                    class_name_id,
                    method_name_id,
                ),
                None,
                None,
            );
            analysis_data
                .data_flow_graph
                .add_node(this_before_node.clone());
            this_type.parent_nodes.push(this_before_node);
        }

        method_context.set_var_type(VarName::new_static("$this"), this_type);
    }

    // Add parameters to context
    let no_named_arguments = method_info.is_some_and(|info| info.no_named_arguments);
    for (param_index, param) in method.parameter_list.parameters.iter().enumerate() {
        let param_name = param.variable.name;
        let param_name_id = VarName::new(param_name);

        // Get parameter info from method info
        let param_info =
            method_info.and_then(|mi| {
                mi.params.iter().find(|p| {
                    analyzer.interner.lookup(p.name).as_ref() == param_name_id.as_str()
                })
            });

        // Get parameter type - for variadic params, wrap in array type
        let mut param_type = if let Some(info) = param_info {
            let mut base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);

            // Psalm only registers __construct in overridden_method_ids under
            // preserve_constructor_signature, so constructors don't borrow
            // ancestor docblock param types (Dispatcher's `@param object
            // \$target` must not retype a child's promoted params).
            let constructor_can_inherit = method_name_id != pzoom_str::StrId::CONSTRUCT
                || class_info.is_some_and(|info| {
                    info.parent_class
                        .and_then(|parent| analyzer.codebase.get_class(parent))
                        .is_some_and(|parent_info| parent_info.is_consistent_constructor)
                });
            if !info.has_docblock_type && constructor_can_inherit {
                if let Some(current_class_info) = class_info {
                    if let Some((inherited_param_type, inherited_has_docblock_type)) =
                        get_specialized_inherited_param_type(
                            analyzer,
                            current_class_info,
                            method_name_id,
                            param_index,
                        )
                    {
                        // Psalm's Methods::getMethodParams borrows the
                        // documenting ancestor's param type when that param
                        // has a docblock type; an own native type is otherwise
                        // kept (a child widening `string` to `?string`).
                        if info.signature_type.is_none() || inherited_has_docblock_type {
                            base_type = inherited_param_type;
                        }
                    }
                }
            }

            // Resolve class-constant references/wildcards (`Foo::BAR_*`) against the
            // populated codebase before the type becomes the parameter variable's
            // type — mirroring Psalm's processParams, which runs the full
            // TypeExpander at function entry so nested positions
            // (`list<self::ACTION_*>`, callable params) expand too. Runs after
            // documenting-ancestor inheritance so an inherited `static`/`self`
            // localizes to this class (Psalm's Methods::localizeType).
            crate::type_expander::expand_union(
                analyzer.codebase,
                analyzer.interner,
                &mut base_type,
                &crate::type_expander::TypeExpansionOptions {
                    self_class: Some(class_name_id),
                    static_class_type: crate::type_expander::StaticClassType::Name(class_name_id),
                    parent_class: class_info.and_then(|ci| ci.parent_class),
                    ..Default::default()
                },
            );
            // The stored param type's from_docblock provenance was decided at
            // scan time (FunctionLikeDocblockScanner's typehint-matching rule),
            // matching Psalm's processParams seeding the storage type as-is.

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

        // Hakana `functionlike_analyzer::get_param_source_kind`: inout (≈ PHP
        // by-ref) → InoutParam; private methods are `is_simple_fn` →
        // PrivateParam; public/protected methods are simple only when
        // final-and-unextended (pzoom conservatively uses NonPrivateParam).
        let param_storage_info = method_info
            .map(|info| &info.params)
            .and_then(|params| {
                params.iter().find(|p| {
                    analyzer.interner.lookup(p.name).as_ref() == param_name_id.as_str()
                })
            });
        let source_kind = if param_storage_info.is_some_and(|info| info.by_ref) {
            VariableSourceKind::InoutParam
        } else if method_info.is_some_and(|info| matches!(info.visibility, Visibility::Private)) {
            VariableSourceKind::PrivateParam
        } else {
            VariableSourceKind::NonPrivateParam
        };

        let param_span = param.variable.span();
        let method_functionlike_id = class_info.map(|ci| {
            pzoom_code_info::data_flow::node::FunctionLikeIdentifier::Method(
                ci.name,
                method_name_id,
            )
        });
        let parent_node = crate::data_flow::add_param_dataflow_node(
            &mut analysis_data.data_flow_graph,
            source_kind,
            VarId(analyzer.interner.intern(&param_name_id)),
            make_data_flow_node_position(
                analyzer,
                (param_span.start.offset, param_span.end.offset),
            ),
            method_functionlike_id.as_ref(),
            param_index,
            param_storage_info.and_then(|info| info.signature_type.as_ref()),
        );
        analysis_data
            .param_sources
            .push(crate::function_analysis_data::ParamSourceInfo {
                node_id: parent_node.id.clone(),
                function_key: method.span().start.offset,
                param_index,
                is_closure: false,
                // Psalm's checkParamReferences only reports params of plain
                // functions, closures and PRIVATE methods.
                reportable: method_info
                    .is_some_and(|info| matches!(info.visibility, Visibility::Private)),
                is_promoted: param_storage_info.is_some_and(|info| info.is_promoted),
                by_ref: param_storage_info.is_some_and(|info| info.by_ref),
                function_end: method.span().end.offset,
                name: param_name.to_string(),
                span: (param_span.start.offset, param_span.end.offset),
                method_param_meta: Some((
                    method_info.is_some_and(|info| info.is_final)
                        || class_info.is_some_and(|ci| ci.is_final),
                    class_info.is_some_and(|ci| {
                        ci.kind == pzoom_code_info::class_like_info::ClassLikeKind::Interface
                    }),
                    class_info.is_some_and(|ci| {
                        ci.overridden_method_ids
                            .get(&method_name_id)
                            .is_some_and(|ids| !ids.is_empty())
                    }),
                )),
            });
        param_type.parent_nodes.push(parent_node);

        if param_storage_info.is_some_and(|info| info.by_ref) {
            // Writes to a by-ref param are visible to the caller.
            method_context.mark_external_reference(param_name_id.clone());
        }
        method_context.set_var_type(param_name_id.clone(), param_type.clone());
        if let Some(alt_param_name_id) = get_alternate_param_var_id(analyzer, param_name)
            && alt_param_name_id.as_str() != param_name_id.as_str()
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
        let prev_is_generator = analysis_data.current_function_is_generator;
        let body_has_yield = stmt_analyzer::body_contains_yield(block.statements.as_slice());
        analysis_data.current_function_is_generator = body_has_yield;
        let saved_var_appearances = std::mem::take(&mut analysis_data.first_var_appearances);
        stmt_analyzer::analyze_stmts(
            &method_analyzer,
            block.statements.as_slice(),
            analysis_data,
            &mut method_context,
        )?;
        analysis_data.first_var_appearances = saved_var_appearances;
        analysis_data.current_function_is_generator = prev_is_generator;
        // Syntactic, like Psalm's storage->has_yield: a value-less `yield;`
        // makes a generator without recording an inferred yield type.
        let has_yield =
            body_has_yield || analysis_data.inferred_yield_types.len() > yield_types_start;

        // Hakana `functionlike_analyzer`: a body that falls through without
        // returning still flows by-ref (inout) param values out of the method.
        if !method_context.has_returned {
            crate::stmt::return_analyzer::handle_byref_at_return(
                &method_analyzer,
                analysis_data,
                &method_context,
            );
        }

        // Hakana `functionlike_analyzer`: at the end of a non-static method
        // `$this`'s dataflow exits through a `ThisAfterMethod` node, carrying
        // instance state (e.g. property assignments under
        // `@psalm-taint-specialize`) back to the call site.
        if let pzoom_code_info::GraphKind::WholeProgram(_) = analysis_data.data_flow_graph.kind
            && !method_info.is_some_and(|mi| mi.is_static)
            && let Some(this_type) = method_context.locals.get("$this")
        {
            let this_after_node = pzoom_code_info::DataFlowNode::get_for_this_after_method(
                &pzoom_code_info::method_identifier::MethodIdentifier(
                    class_name_id,
                    method_name_id,
                ),
                None,
                None,
            );
            for parent_node in &this_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &this_after_node.id,
                    pzoom_code_info::PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            analysis_data.data_flow_graph.add_node(this_after_node);
        }

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
            method_info.map(|method| &**method),
            method_name_id,
            &method_context,
            method.span().start.offset as u32,
            return_types_start,
            analysis_data,
        );

        if let Some(info) = method_info {
            let control_actions = crate::stmt::scope_analyzer::get_control_actions(
                block.statements.as_slice(),
                analysis_data,
                &[],
                false,
            );
            let cased_name = format!(
                "{}::{}",
                analyzer.interner.lookup(class_name_id),
                analyzer.interner.lookup(method_name_id)
            );

            let exit_control_actions = crate::stmt::scope_analyzer::get_control_actions(
                block.statements.as_slice(),
                analysis_data,
                &[],
                true,
            );

            crate::stmt::function_analyzer::verify_missing_return_checks(
                &method_analyzer,
                info,
                analysis_data,
                method.name.span().start.offset as u32,
                &cased_name,
                has_yield,
                method_context.has_returned,
                analysis_data.inferred_return_types.len() > return_types_start,
                &control_actions,
                &exit_control_actions,
                crate::stmt::scope_analyzer::only_throws(block.statements.as_slice()),
                crate::stmt::scope_analyzer::only_throws_or_exits(
                    block.statements.as_slice(),
                    analysis_data,
                ),
                return_types_start,
                yield_types_start,
                class_info,
                // Psalm checks LessSpecificReturnType for private methods and
                // for methods not overridden anywhere; pzoom does not track
                // overridden_somewhere, so only the private case is checked.
                info.visibility == pzoom_code_info::class_like_info::Visibility::Private,
            );
        }

        // Drop this method's recorded return/yield types so an enclosing
        // function-like (anonymous classes nest inside function bodies) only
        // sees its own returns in the shared vec.
        analysis_data.inferred_return_types.truncate(return_types_start);
        analysis_data.inferred_yield_types.truncate(yield_types_start);

        // Hakana's end-of-functionlike pass: reconcile the type-variable
        // bounds accumulated during this method body (closures included —
        // pzoom's shared analysis data is Hakana's bounds merge). A method on
        // an anonymous class nested inside a function defers to that function.
        if analyzer.function_info.is_none() {
            let span = method.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            crate::expr::call_analyzer::check_type_variable_bounds_at_function_end(
                &method_analyzer,
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

        // Resolve class-constant references/wildcards (`Foo::BAR_*`) in the by-ref
        // constraint against the populated codebase, matching where Psalm's
        // TypeExpander resolves them, so a `RECONCILIATION_*` out type accepts the
        // concrete `0|1|2` values assigned in the body.
        let callable_name = method_info
            .declaring_class
            .map(|class_id| {
                format!(
                    "{}::{}",
                    analyzer.interner.lookup(class_id),
                    analyzer.interner.lookup(method_info.name),
                )
            })
            .unwrap_or_else(|| analyzer.interner.lookup(method_info.name).to_string());
        let expected_type = crate::expr::call::callable_validation::normalize_class_constant_param_type(
            analyzer,
            expected_type,
            &callable_name,
        );

        let mut comparison = TypeComparisonResult::new();
        if union_type_comparator::is_contained_by(
            analyzer.codebase,
            actual_type,
            &expected_type,
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
            &expected_type,
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
) -> Option<(TUnion, bool)> {
    // Trait methods are flattened into the class, so a used trait is the
    // closest documenting source for an override (resolved via `@use T<...>`).
    for trait_name in &class_info.used_traits {
        if let Some((inherited_type, has_docblock_type)) =
            get_param_type_from_classlike(analyzer, *trait_name, method_name, param_index)
        {
            return Some((
                replace_extended_templates_in_union(
                    &inherited_type,
                    &class_info.template_extended_params,
                ),
                has_docblock_type,
            ));
        }
    }

    if let Some(parent_class) = class_info.parent_class {
        if let Some((inherited_type, has_docblock_type)) =
            get_param_type_from_classlike(analyzer, parent_class, method_name, param_index)
        {
            return Some((
                replace_extended_templates_in_union(
                    &inherited_type,
                    &class_info.template_extended_params,
                ),
                has_docblock_type,
            ));
        }
    }

    for interface_name in &class_info.interfaces {
        if let Some((inherited_type, has_docblock_type)) =
            get_param_type_from_classlike(analyzer, *interface_name, method_name, param_index)
        {
            return Some((
                replace_extended_templates_in_union(
                    &inherited_type,
                    &class_info.template_extended_params,
                ),
                has_docblock_type,
            ));
        }
    }

    for interface_name in &class_info.all_parent_interfaces {
        if let Some((inherited_type, has_docblock_type)) =
            get_param_type_from_classlike(analyzer, *interface_name, method_name, param_index)
        {
            return Some((
                replace_extended_templates_in_union(
                    &inherited_type,
                    &class_info.template_extended_params,
                ),
                has_docblock_type,
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
    // Trait methods are flattened into the class, so a used trait is the
    // closest documenting source for an override (resolved via `@use T<...>`).
    for trait_name in &class_info.used_traits {
        if let Some(inherited_type) =
            get_return_type_from_classlike(analyzer, *trait_name, method_name)
        {
            return Some(replace_extended_templates_in_union(
                &inherited_type,
                &class_info.template_extended_params,
            ));
        }
    }

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
) -> Option<(TUnion, bool)> {
    let class_storage = analyzer.codebase.get_class(classlike_name)?;
    let method_storage = class_storage.methods.get(&method_name)?;
    let param = method_storage.params.get(param_index)?;
    param
        .get_type()
        .cloned()
        .map(|param_type| (param_type, param.has_docblock_type))
}

fn get_return_type_from_classlike(
    analyzer: &StatementsAnalyzer<'_>,
    classlike_name: StrId,
    method_name: StrId,
) -> Option<TUnion> {
    let class_storage = analyzer.codebase.get_class(classlike_name)?;
    let method_storage = class_storage.methods.get(&method_name)?;

    // A `static::CONST` return resolved against the ancestor: the
    // inheritor's own constant may differ (Psalm resolves these late).
    if method_storage.return_type_mentions_static_const {
        return method_storage.signature_return_type.clone();
    }

    method_storage
        .return_type
        .clone()
        .or_else(|| method_storage.signature_return_type.clone())
}

pub fn replace_extended_templates_in_union(
    union: &TUnion,
    template_extended_params: &IndexMap<StrId, IndexMap<StrId, TUnion>>,
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
                if let Some(referenced_type) = defining_entity
                    .classlike_name()
                    .and_then(|entity_class| template_extended_params.get(&entity_class))
                    .and_then(|map| map.get(name))
                {
                    changed = true;
                    // The extends map keeps template chains linked one level
                    // at a time (Psalm's Populator::extendType), so resolve
                    // the referenced type transitively — Psalm's
                    // MethodComparator::transformTemplates recursion. Only
                    // recurse on templates of *other* entities to keep
                    // identity entries from looping.
                    let resolved = if referenced_type.types.iter().any(|atomic| {
                        matches!(
                            atomic,
                            TAtomic::TTemplateParam { defining_entity: referenced_entity, .. }
                                if referenced_entity != defining_entity
                        )
                    }) {
                        replace_extended_templates_in_union(
                            referenced_type,
                            template_extended_params,
                        )
                    } else {
                        referenced_type.clone()
                    };
                    for referenced_atomic in &resolved.types {
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
                if let Some(referenced_type) = defining_entity
                    .classlike_name()
                    .and_then(|entity_class| template_extended_params.get(&entity_class))
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
    template_extended_params: &IndexMap<StrId, IndexMap<StrId, TUnion>>,
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
            for (key, value) in properties.iter() {
                new_properties.insert(
                    key.clone(),
                    replace_extended_templates_in_union(value, template_extended_params),
                );
            }

            TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(new_properties),
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
        TAtomic::TNamedObject { name, type_params , .. } => TAtomic::TNamedObject {
            name: *name,
            type_params: type_params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| {
                        replace_extended_templates_in_union(param, template_extended_params)
                    })
                    .collect()
            }),
        is_static: false, remapped_params: false },
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
        } => {
            // Substitute the template itself when the extends map binds it
            // to a single atomic (e.g. `class-string<T:I>` becoming
            // `class-string<T2:C>` through @template-implements); multi-atomic
            // bindings cannot fit an atomic slot and keep the param.
            if let Some(referenced_type) = defining_entity
                .classlike_name()
                .and_then(|entity_class| template_extended_params.get(&entity_class))
                .and_then(|map| map.get(name))
                && let [single] = referenced_type.types.as_slice()
            {
                single.clone()
            } else {
                TAtomic::TTemplateParam {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(replace_extended_templates_in_union(
                        as_type,
                        template_extended_params,
                    )),
                }
            }
        }
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
                        is_static: false, remapped_params: false })),
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
            TAtomic::TNamedObject { name, type_params , .. } => {
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
        TAtomic::TNamedObject { name, type_params , .. } => {
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

pub(crate) fn localize_special_class_names_for_final_class(
    union: &TUnion,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    localize_special_class_names_union(union, self_class_id, parent_class_id, false)
}

/// Like [`localize_special_class_names_for_final_class`], but `keep_static`
/// keeps late-static-bound atomics abstract (name bound to the class, static
/// flag retained) — a NON-final class's `static` stays distinct from the
/// concrete class in comparisons.
fn localize_special_class_names_union(
    union: &TUnion,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
    keep_static: bool,
) -> TUnion {
    let mut localized = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let localized_atomic =
            localize_special_class_names_in_atomic(atomic, self_class_id, parent_class_id, keep_static);
        push_unique_atomic(&mut localized, localized_atomic);
    }

    TUnion::from_types(localized)
}

fn localize_special_class_names_in_atomic(
    atomic: &TAtomic,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
    keep_static: bool,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params, is_static, .. } => {
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
                            localize_special_class_names_union(
                                param,
                                self_class_id,
                                parent_class_id,
                                keep_static,
                            )
                        })
                        .collect()
                }),
            is_static: keep_static && (*is_static || *name == StrId::STATIC), remapped_params: false }
        }
        TAtomic::TObjectIntersection { types } => TAtomic::TObjectIntersection {
            types: types
                .iter()
                .map(|nested| {
                    localize_special_class_names_in_atomic(nested, self_class_id, parent_class_id, keep_static)
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
            as_type: Box::new(localize_special_class_names_union(
                as_type,
                self_class_id,
                parent_class_id,
                keep_static,
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
                keep_static,
            )),
        },
        _ => atomic.clone(),
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

/// Class-wide issues anchor on the class NAME when it exists (Psalm's
/// behavior); anonymous classes fall back to the declaration start.
fn class_issue_pos(class_info: &pzoom_code_info::ClassLikeInfo) -> (u32, u32) {
    class_info.name_location.unwrap_or((
        class_info.start_offset,
        class_info.start_offset.saturating_add(1),
    ))
}
