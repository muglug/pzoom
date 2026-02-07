//! Instance property fetch analyzer.

use mago_syntax::ast::ast::access::{NullSafePropertyAccess, PropertyAccess};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expr::call::function_call_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

/// Analyze an instance property access expression ($obj->prop).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    _in_assignment: bool,
) {
    // Analyze the object expression
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        ClassLikeMemberSelector::Variable(var) => {
            let _var_pos = expression_analyzer::analyze(
                analyzer,
                &Expression::Variable(var.clone()),
                analysis_data,
                context,
            );
            None
        }
        ClassLikeMemberSelector::Expression(expr) => {
            let _expr_pos =
                expression_analyzer::analyze(analyzer, expr.expression, analysis_data, context);
            None
        }
    };

    // Check if this is $this->prop
    let is_this_fetch = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    if let Some(prop_name) = prop_name {
        if let Some(keyed_type) =
            get_reconciled_property_type(analyzer, context, access.object, prop_name)
        {
            analysis_data.set_expr_type(pos, keyed_type);
            return;
        }
    }

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(prop_type) = get_property_type(
            analyzer,
            &obj_t,
            prop_name,
            pos,
            analysis_data,
            is_this_fetch,
            context.inside_isset,
            context.has_this,
            context,
        ) {
            analysis_data.set_expr_type(pos, prop_type);
            return;
        }
    }

    // Fall back to mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

/// Analyze a null-safe property access expression ($obj?->prop).
pub fn analyze_nullsafe(
    analyzer: &StatementsAnalyzer<'_>,
    access: &NullSafePropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the object expression
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    let obj_type = analysis_data.get_expr_type(obj_pos);

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    };

    if let Some(prop_name) = prop_name {
        if let Some(mut keyed_type) =
            get_reconciled_property_type(analyzer, context, access.object, prop_name)
        {
            if obj_type.is_some_and(|obj_t| obj_t.is_nullable) {
                keyed_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, keyed_type);
            return;
        }
    }

    // Try to look up property type
    if let (Some(obj_t), Some(prop_name)) = (obj_type, prop_name) {
        if let Some(mut prop_type) = get_property_type(
            analyzer,
            &obj_t,
            prop_name,
            pos,
            analysis_data,
            false,
            true,
            context.has_this,
            context,
        ) {
            // If the object could be null, the result could be null
            if obj_t.is_nullable {
                prop_type.add_type(TAtomic::TNull);
            }
            analysis_data.set_expr_type(pos, prop_type);
            return;
        }
    }

    // Fall back to mixed|null
    let mut result = TUnion::mixed();
    result.add_type(TAtomic::TNull);
    analysis_data.set_expr_type(pos, result);
}

fn get_reconciled_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    object_expr: &Expression<'_>,
    prop_name: &str,
) -> Option<TUnion> {
    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let property_key = format!("{}->{}", object_key, prop_name);
    let property_id = analyzer.interner.find(&property_key)?;
    context.locals.get(&property_id).cloned()
}

/// Look up the type of a property on a type.
fn get_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    is_this_fetch: bool,
    suppress_null_issues: bool,
    has_this: bool,
    context: &BlockContext,
) -> Option<TUnion> {
    let prop_id = analyzer.interner.intern(prop_name);
    let expanded_obj_type = expand_template_object_union(obj_type);
    let mut lookup_types = expand_intersection_lookup_types(&expanded_obj_type);

    for atomic in &mut lookup_types {
        if let TAtomic::TEnumCase { enum_name, .. } = atomic {
            *atomic = TAtomic::TNamedObject {
                name: *enum_name,
                type_params: None,
            };
        }
    }

    // Check for null/invalid types in the union
    let has_object_type = lookup_types
        .iter()
        .any(|t| matches!(t, TAtomic::TNamedObject { .. } | TAtomic::TObject));
    let has_null = lookup_types.iter().any(|t| matches!(t, TAtomic::TNull));
    let has_invalid_type = lookup_types.iter().any(|t| {
        !matches!(
            t,
            TAtomic::TNamedObject { .. } | TAtomic::TObject | TAtomic::TNull | TAtomic::TMixed
        )
    });

    // Check for purely null type (NullPropertyFetch)
    if obj_type.is_null() {
        if !suppress_null_issues {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::NullPropertyFetch,
                format!("Cannot access property ${} on null", prop_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
        return None;
    }

    // Check for nullable type (PossiblyNullPropertyFetch)
    if has_null && has_object_type {
        if !suppress_null_issues {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyNullPropertyFetch,
                format!(
                    "Cannot access property ${} on possibly null type",
                    prop_name
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Check for invalid (non-object) types
    if has_invalid_type {
        if !has_object_type {
            // Purely invalid type
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidPropertyFetch,
                format!(
                    "Cannot access property ${} on {}",
                    prop_name,
                    expanded_obj_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            return None;
        } else {
            // Mixed valid/invalid types
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::PossiblyInvalidPropertyFetch,
                format!(
                    "Cannot access property ${} on possibly non-object type {}",
                    prop_name,
                    expanded_obj_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // If any object type in the union has this property, prefer that successful lookup.
    // This avoids false positives when a union includes mixed/other object branches.
    let mut resolved_property: Option<(pzoom_str::StrId, Visibility, bool, Option<TUnion>)> = None;

    for atomic in &lookup_types {
        if let TAtomic::TNamedObject { name, type_params } = atomic {
            if let Some(class_info) = analyzer.codebase.get_class(*name) {
                if let Some(prop_info) = class_info.properties.get(&prop_id) {
                    let property_type = get_pseudo_property_get_type(
                        analyzer,
                        class_info,
                        type_params.as_deref(),
                        prop_id,
                    )
                    .or_else(|| {
                        prop_info.get_type().map(|property_type| {
                            substitute_class_template_params(
                                analyzer,
                                class_info,
                                type_params.as_deref(),
                                property_type,
                            )
                        })
                    });
                    resolved_property = Some((
                        *name,
                        prop_info.visibility,
                        prop_info.is_deprecated,
                        property_type,
                    ));
                    break;
                }
            }
        }
    }

    if let Some((class_id, visibility, is_deprecated, property_type)) = resolved_property {
        let visibility_scope_class_id = analyzer
            .codebase
            .get_class(class_id)
            .map(|class_info| get_property_visibility_scope_class_id(class_info, prop_id))
            .unwrap_or(class_id);

        match visibility {
            Visibility::Public => {}
            Visibility::Private => {
                let is_same_class = analyzer
                    .get_declaring_class()
                    .is_some_and(|calling_class| calling_class == visibility_scope_class_id);

                if !is_same_class
                    && !receiver_allows_property_visibility_override(
                        analyzer,
                        &expanded_obj_type,
                        visibility_scope_class_id,
                    )
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleProperty,
                        format!(
                            "Cannot access private property {}::${}",
                            analyzer.interner.lookup(visibility_scope_class_id),
                            prop_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
            Visibility::Protected => {
                let can_access = analyzer.get_declaring_class().is_some_and(|calling_class| {
                    can_access_protected_member_visibility(
                        analyzer,
                        calling_class,
                        visibility_scope_class_id,
                    )
                });

                if !can_access
                    && !receiver_allows_property_visibility_override(
                        analyzer,
                        &expanded_obj_type,
                        visibility_scope_class_id,
                    )
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InaccessibleProperty,
                        format!(
                            "Cannot access protected property {}::${}",
                            analyzer.interner.lookup(visibility_scope_class_id),
                            prop_name
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }

        if is_deprecated {
            let class_name = analyzer.interner.lookup(class_id);
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::DeprecatedProperty,
                format!("Property {}::${} is deprecated", class_name, prop_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                let scope_phrase = format_internal_scope_phrase(analyzer, &class_info.internal);
                let class_name = analyzer.interner.lookup(class_id);
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InternalProperty,
                    format!(
                        "{}::${} is internal to {}",
                        class_name, prop_name, scope_phrase
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                if !can_access_internal(analyzer, &prop_info.internal, Some(context)) {
                    let scope_phrase = format_internal_scope_phrase(analyzer, &prop_info.internal);
                    let class_name = analyzer.interner.lookup(class_id);
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InternalProperty,
                        format!(
                            "{}::${} is internal to {}",
                            class_name, prop_name, scope_phrase
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            }
        }

        let mut final_property_type = property_type.unwrap_or_else(TUnion::mixed);
        if has_null && has_object_type {
            final_property_type.add_type(TAtomic::TNull);
        }

        return Some(final_property_type);
    }

    for atomic in &lookup_types {
        match atomic {
            TAtomic::TNamedObject { name, type_params } => {
                if *name == pzoom_str::StrId::SIMPLE_XML_ELEMENT {
                    return Some(TUnion::mixed());
                }

                // Look up the class
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    // Look up the property
                    if let Some(prop_info) = class_info.properties.get(&prop_id) {
                        let visibility_scope_class_id =
                            get_property_visibility_scope_class_id(class_info, prop_id);

                        match prop_info.visibility {
                            Visibility::Public => {}
                            Visibility::Private => {
                                let is_same_class =
                                    analyzer.get_declaring_class().is_some_and(|calling_class| {
                                        calling_class == visibility_scope_class_id
                                    });

                                if !is_same_class
                                    && !receiver_allows_property_visibility_override(
                                        analyzer,
                                        &expanded_obj_type,
                                        visibility_scope_class_id,
                                    )
                                {
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::InaccessibleProperty,
                                        format!(
                                            "Cannot access private property {}::${}",
                                            analyzer.interner.lookup(visibility_scope_class_id),
                                            prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }
                            }
                            Visibility::Protected => {
                                let can_access =
                                    analyzer.get_declaring_class().is_some_and(|calling_class| {
                                        can_access_protected_member_visibility(
                                            analyzer,
                                            calling_class,
                                            visibility_scope_class_id,
                                        )
                                    });

                                if !can_access
                                    && !receiver_allows_property_visibility_override(
                                        analyzer,
                                        &expanded_obj_type,
                                        visibility_scope_class_id,
                                    )
                                {
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::InaccessibleProperty,
                                        format!(
                                            "Cannot access protected property {}::${}",
                                            analyzer.interner.lookup(visibility_scope_class_id),
                                            prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }
                            }
                        }

                        // Check for deprecated properties
                        if prop_info.is_deprecated {
                            let class_name = analyzer.interner.lookup(*name);
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::DeprecatedProperty,
                                format!("Property {}::${} is deprecated", class_name, prop_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                            let class_name = analyzer.interner.lookup(*name);
                            let scope_phrase =
                                format_internal_scope_phrase(analyzer, &class_info.internal);
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InternalProperty,
                                format!(
                                    "{}::${} is internal to {}",
                                    class_name, prop_name, scope_phrase
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        if !can_access_internal(analyzer, &prop_info.internal, Some(context)) {
                            let class_name = analyzer.interner.lookup(*name);
                            let scope_phrase =
                                format_internal_scope_phrase(analyzer, &prop_info.internal);
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::InternalProperty,
                                format!(
                                    "{}::${} is internal to {}",
                                    class_name, prop_name, scope_phrase
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        // Return the property's type
                        return prop_info.get_type().map(|property_type| {
                            substitute_class_template_params(
                                analyzer,
                                class_info,
                                type_params.as_deref(),
                                property_type,
                            )
                        });
                    } else {
                        if class_info.kind == ClassLikeKind::Interface {
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(Issue::new(
                                IssueKind::NoInterfaceProperties,
                                "Interfaces cannot have properties",
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }

                        if class_has_magic_getter(class_info) {
                            if let Some(pseudo_property_type) = get_pseudo_property_get_type(
                                analyzer,
                                class_info,
                                type_params.as_deref(),
                                prop_id,
                            ) {
                                return Some(pseudo_property_type);
                            }

                            if class_has_sealed_properties(class_info)
                                || !class_info.pseudo_property_get_types.is_empty()
                            {
                                let class_name = analyzer.interner.lookup(*name);
                                let (line, col) = analyzer.get_line_column(pos.0);

                                if is_this_fetch {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::UndefinedThisPropertyFetch,
                                        format!(
                                            "Property {}::${} does not exist",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                } else {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::UndefinedMagicPropertyFetch,
                                        format!(
                                            "Magic property {}::${} does not exist",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }
                            } else {
                                if let Some(magic_get_return_type) = get_magic_get_return_type(
                                    analyzer,
                                    class_info,
                                    type_params.as_deref(),
                                ) {
                                    return Some(magic_get_return_type);
                                }

                                if let Some(simplexml_magic_type) =
                                    get_simplexml_magic_property_type(analyzer, *name)
                                {
                                    return Some(simplexml_magic_type);
                                }

                                return Some(TUnion::mixed());
                            }

                            continue;
                        }

                        if let Some(simplexml_magic_type) =
                            get_simplexml_magic_property_type(analyzer, *name)
                        {
                            return Some(simplexml_magic_type);
                        }

                        if class_info.no_seal_properties {
                            return Some(TUnion::mixed());
                        }

                        // Property not found - emit appropriate issue
                        let class_name = analyzer.interner.lookup(*name);
                        let (line, col) = analyzer.get_line_column(pos.0);

                        if is_this_fetch {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedThisPropertyFetch,
                                format!("Property {}::${} does not exist", class_name, prop_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        } else {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::UndefinedPropertyFetch,
                                format!("Property {}::${} does not exist", class_name, prop_name),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                        }
                    }
                }
            }
            TAtomic::TObject => {
                // Generic object - can't look up property
            }
            TAtomic::TMixed => {
                if is_this_fetch && !has_this {
                    continue;
                }
                if context.inside_general_use {
                    continue;
                }
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedPropertyFetch,
                    format!("Cannot access property ${} on mixed type", prop_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            TAtomic::TNull => {
                // Already handled above
            }
            _ => {
                // Already handled in has_invalid_type check above
            }
        }
    }

    None
}

fn get_pseudo_property_get_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    prop_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let pseudo_type = class_info.pseudo_property_get_types.get(&prop_id)?;
    Some(substitute_class_template_params(
        analyzer,
        class_info,
        type_params,
        pseudo_type,
    ))
}

fn get_magic_get_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
) -> Option<TUnion> {
    let method_info = class_info.methods.get(&pzoom_str::StrId::GET)?;
    let return_type = method_info.get_return_type()?;

    let mut template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    template_defaults.extend(function_call_analyzer::get_template_defaults(method_info));

    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            type_params,
        ),
    );

    if template_defaults.is_empty() && template_replacements.is_empty() {
        Some(return_type.clone())
    } else {
        Some(function_call_analyzer::replace_templates_in_union(
            return_type,
            &template_replacements,
            &template_defaults,
        ))
    }
}

fn class_has_magic_getter(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::GET)
}

fn class_has_sealed_properties(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.sealed_properties.unwrap_or(false) && !class_info.no_seal_properties
}

fn get_simplexml_magic_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> Option<TUnion> {
    let class_name = analyzer.interner.lookup(class_id);
    let normalized = class_name.trim_start_matches('\\');
    if !normalized.eq_ignore_ascii_case("SimpleXMLElement")
        && !normalized.eq_ignore_ascii_case("SimpleXMLIterator")
    {
        return None;
    }

    let mut types = vec![TAtomic::TNamedObject {
        name: class_id,
        type_params: None,
    }];

    if let Some(iterator_id) = analyzer.interner.find("SimpleXMLIterator")
        && analyzer.codebase.get_class(iterator_id).is_some()
        && iterator_id != class_id
    {
        types.push(TAtomic::TNamedObject {
            name: iterator_id,
            type_params: None,
        });
    }

    Some(TUnion::from_types(types))
}

fn substitute_class_template_params(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    property_type: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return property_type.clone();
    }

    let template_defaults = function_call_analyzer::get_class_template_defaults(class_info);
    let mut template_replacements =
        function_call_analyzer::infer_class_template_replacements_from_extended_params(class_info);
    function_call_analyzer::overlay_template_replacements(
        &mut template_replacements,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            type_params,
        ),
    );

    if template_defaults.is_empty() && template_replacements.is_empty() {
        return property_type.clone();
    }

    function_call_analyzer::replace_templates_in_union(
        property_type,
        &template_replacements,
        &template_defaults,
    )
}

fn expand_template_object_union(obj_type: &TUnion) -> TUnion {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TTemplateParam { as_type, .. } => {
                if as_type.is_mixed() {
                    expanded_types.push(TAtomic::TMixed);
                } else {
                    expanded_types.extend(as_type.types.iter().cloned());
                }
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                expanded_types.push((**as_type).clone());
            }
            TAtomic::TObjectIntersection { types } => {
                let mut expanded_intersection = Vec::new();
                for nested in types {
                    match nested {
                        TAtomic::TTemplateParam { as_type, .. } => {
                            if as_type.is_mixed() {
                                expanded_intersection.push(TAtomic::TMixed);
                            } else {
                                expanded_intersection.extend(as_type.types.iter().cloned());
                            }
                        }
                        TAtomic::TTemplateParamClass { as_type, .. } => {
                            expanded_intersection.push((**as_type).clone());
                        }
                        _ => expanded_intersection.push(nested.clone()),
                    }
                }
                expanded_types.push(TAtomic::TObjectIntersection {
                    types: expanded_intersection,
                });
            }
            _ => expanded_types.push(atomic.clone()),
        }
    }

    TUnion::from_types(expanded_types)
}

fn expand_intersection_lookup_types(obj_type: &TUnion) -> Vec<TAtomic> {
    let mut expanded_types = Vec::new();

    for atomic in &obj_type.types {
        match atomic {
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if !expanded_types.contains(nested) {
                        expanded_types.push(nested.clone());
                    }
                }
            }
            _ => {
                if !expanded_types.contains(atomic) {
                    expanded_types.push(atomic.clone());
                }
            }
        }
    }

    expanded_types
}

fn receiver_allows_property_visibility_override(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_type: &TUnion,
    target_class: StrId,
) -> bool {
    let mut has_target_class = false;
    let mut has_override_interface = false;

    let mut track_named = |name: StrId| {
        if name == target_class {
            has_target_class = true;
        }

        if analyzer.codebase.get_class(name).is_some_and(|info| {
            info.kind == ClassLikeKind::Interface && info.override_property_visibility
        }) {
            has_override_interface = true;
        }
    };

    for atomic in &receiver_type.types {
        match atomic {
            TAtomic::TNamedObject { name, .. } => track_named(*name),
            TAtomic::TObjectIntersection { types } => {
                for nested in types {
                    if let TAtomic::TNamedObject { name, .. } = nested {
                        track_named(*name);
                    }
                }
            }
            _ => {}
        }
    }

    has_target_class && has_override_interface
}

fn get_property_visibility_scope_class_id(
    class_info: &pzoom_code_info::ClassLikeInfo,
    prop_id: StrId,
) -> StrId {
    class_info
        .appearing_property_ids
        .get(&prop_id)
        .copied()
        .unwrap_or(class_info.name)
}

fn can_access_protected_member_visibility(
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
