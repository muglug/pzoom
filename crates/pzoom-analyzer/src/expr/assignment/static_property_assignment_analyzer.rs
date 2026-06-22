//! Static property assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::StaticPropertyAccess;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

/// Analyze a static property assignment (Foo::$bar = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &StaticPropertyAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_compound: bool,
) {
    // Analyze the class expression to get the class name
    let _class_pos = expression_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Psalm: writing a static property is global mutable state, so it is impure from a
    // `@psalm-pure` context.
    crate::expr::fetch::static_property_fetch_analyzer::emit_impure_static_property(
        analyzer,
        pos,
        analysis_data,
    );

    // Try to get the class name
    let class_name = get_class_name(analyzer, access.class, context);

    // Get the property name from the Variable
    let prop_name = match &access.property {
        mago_syntax::ast::ast::variable::Variable::Direct(direct) => Some(direct.name),
        mago_syntax::ast::ast::variable::Variable::Indirect(indirect) => {
            // Dynamic property names (`static::${$var} = …`) consume their
            // inner expression (general use).
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;
            let _ =
                expression_analyzer::analyze(analyzer, indirect.expression, analysis_data, context);
            context.inside_general_use = was_inside_general_use;
            None
        }
        _ => None,
    };

    // Analyze the value expression
    // Hakana: a value assigned to a static property is general use.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let value_pos = expression_analyzer::analyze(analyzer, value_expr, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let mut value_type = analysis_data
        .expr_types
        .get(&value_pos)
        .cloned()
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // A statement-level `/** @var T */` overrides the assigned type (Psalm's
    // AssignmentAnalyzer applies var comments to any assignment target,
    // including static properties — e.g. `self::$map = require(...)`).
    if let Some(annotation_type) = analysis_data.current_stmt_start.and_then(|stmt_start| {
        let annotations = analyzer.get_inline_var_annotations(stmt_start)?;
        let prop_key = prop_name.as_ref().map(|prop_name| {
            format!(
                "{}::${}",
                static_class_key(access),
                prop_name.trim_start_matches('$')
            )
        });
        let mut unnamed_match = None;
        for annotation in annotations {
            match annotation.var_name {
                Some(name)
                    if prop_key.as_deref().is_some_and(|prop_key| {
                        analyzer.interner.lookup(name).as_ref() == prop_key
                    }) =>
                {
                    return Some(annotation.var_type.clone());
                }
                None if unnamed_match.is_none() => {
                    unnamed_match = Some(annotation.var_type.clone())
                }
                _ => {}
            }
        }
        unnamed_match
    }) {
        value_type = annotation_type;
        analysis_data
            .expr_types
            .insert(value_pos, Rc::new(value_type.clone()));
    }

    // Verify property type if we can resolve it
    if let (Some(class_name), Some(prop_name)) = (class_name, prop_name) {
        let class_id = analyzer.interner.intern(&class_name);
        // Strip the leading $ from property name
        let prop_name_str = prop_name.trim_start_matches('$');
        let prop_id = analyzer.interner.intern(prop_name_str);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            if let Some(prop_info) = class_info.properties.get(&prop_id) {
                // Any write to a static property counts as a reference in
                // Psalm (a static-property assignment goes through a property
                // fetch), so a static property that is only ever written is not
                // reported as unused. (Instance-property writes, by contrast,
                // do not count -- only reads do.) A compound assignment also
                // reads the previous value, but plain writes count too.
                let _ = is_compound;
                if analyzer.config.find_unused_code {
                    analysis_data
                        .referenced_properties
                        .insert((prop_info.declaring_class, prop_id));
                    analysis_data.add_class_member_reference(
                        &context.function_context,
                        (prop_info.declaring_class, prop_id),
                        false,
                    );
                }

                // A non-static property is invisible to static access:
                // Psalm's StaticPropertyFetchAnalyzer reports
                // UndefinedPropertyAssignment when writing.
                if !prop_info.is_static {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedPropertyAssignment,
                        format!(
                            "Static property {}::${} is not defined",
                            class_name, prop_name_str
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Check property visibility - private properties are only accessible within the same class
                if prop_info.visibility == Visibility::Private {
                    let is_same_class = analyzer
                        .get_declaring_class()
                        .is_some_and(|calling_class| calling_class == class_id);

                    if !is_same_class {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InaccessibleProperty,
                            format!(
                                "Cannot access private property {}::${}",
                                class_name, prop_name_str
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }

                // Check if property is readonly
                if prop_info.is_readonly {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::InvalidPropertyAssignmentValue,
                        format!(
                            "Cannot assign to readonly property {}::${}",
                            class_name, prop_name_str
                        ),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if prop_info.is_deprecated {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedProperty,
                        format!("Property {}::${} is deprecated", class_name, prop_name_str),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Psalm `StaticPropertyAssignmentAnalyzer` →
                // `taintUnspecializedProperty`: the assigned value flows into
                // the global `A::$prop` property node (static properties are
                // shared state, so taints cross call sites through them).
                if let pzoom_code_info::GraphKind::WholeProgram(_) =
                    analysis_data.data_flow_graph.kind
                {
                    crate::expr::assignment::instance_property_assignment_analyzer::
                        add_unspecialized_property_assignment_dataflow(
                        analyzer,
                        (class_id, prop_id),
                        pos,
                        analysis_data,
                        &value_type,
                        Some(prop_info.declaring_class),
                    );
                }

                // Verify type compatibility using proper type comparator
                // Only check if property has a declared type. Psalm expands
                // the declared type at the use site (class-constant
                // wildcards like TaintKind::*, self/static). An untyped
                // redeclaration inherits the overridden ancestor property's
                // type (Psalm's Properties::getPropertyType fallback).
                let inherited_prop_type = if prop_info.get_type().is_none() {
                    crate::expr::fetch::atomic_property_fetch_analyzer::get_overridden_property_type(
                        analyzer.codebase,
                        class_id,
                        prop_id,
                    )
                } else {
                    None
                };
                if let Some(prop_type) = prop_info.get_type().or(inherited_prop_type.as_ref()) {
                    let mut expanded_prop_type = prop_type.clone();
                    crate::type_expander::expand_union(
                        analyzer.codebase,
                        analyzer.interner,
                        &mut expanded_prop_type,
                        &crate::type_expander::TypeExpansionOptions {
                            self_class: Some(prop_info.declaring_class),
                            static_class_type: crate::type_expander::StaticClassType::Name(
                                prop_info.declaring_class,
                            ),
                            ..Default::default()
                        },
                    );
                    check_assigned_value_against_property_type(
                        analyzer,
                        &class_name,
                        prop_name_str,
                        &expanded_prop_type,
                        &value_type,
                        pos,
                        analysis_data,
                    );
                }
            } else {
                // Property not found
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedPropertyAssignment,
                    format!("Property {}::${} does not exist", class_name, prop_name_str),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        } else {
            // Class not found
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UndefinedClass,
                crate::class_casing::undefined_class_message(analyzer, &class_name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Psalm's StaticPropertyAssignmentAnalyzer records the assigned type
    // under the property's var id (`$context->vars_in_scope[$var_id]`), so
    // subsequent reads in this scope see the narrowed type. The key matches
    // expression_identifier::get_expression_var_key.
    let class_key_part = match access.class.unparenthesized() {
        Expression::Identifier(identifier) => Some(identifier.value().to_string()),
        Expression::Self_(_) => Some("self".to_string()),
        Expression::Static(_) => Some("static".to_string()),
        Expression::Parent(_) => Some("parent".to_string()),
        _ => None,
    };
    if let (Some(class_key_part), Some(prop_name)) = (class_key_part, prop_name) {
        let var_id = VarName::new(&format!(
            "{}::${}",
            class_key_part,
            prop_name.trim_start_matches('$')
        ));
        context.set_var_type(var_id, value_type.clone());
    }

    // The assignment expression returns the assigned value
    analysis_data.expr_types.insert(pos, Rc::new(value_type));
}

/// The class part of a static property assignment's var key, matching
/// `expression_identifier::get_expression_var_key` ("self", "static",
/// "parent", or the literal class name).
fn static_class_key(access: &StaticPropertyAccess<'_>) -> String {
    match access.class.unparenthesized() {
        Expression::Identifier(identifier) => identifier.value().to_string(),
        Expression::Self_(_) => "self".to_string(),
        Expression::Static(_) => "static".to_string(),
        Expression::Parent(_) => "parent".to_string(),
        _ => String::new(),
    }
}

/// Like [`analyze`], but for an assignment whose value type is already known —
/// used by the array assignment analyzer when the assignment root is a static
/// property (`self::$map[$k] = ...`): Psalm re-checks the updated root type
/// against the declared property type (which is what surfaces
/// `InvalidPropertyAssignmentValue` for a bad `class-string-map` value). The
/// property fetch that started the chain already reported
/// existence/visibility/static-ness issues, so only the type check runs here.
pub fn analyze_with_known_type(
    analyzer: &StatementsAnalyzer<'_>,
    access: &StaticPropertyAccess<'_>,
    value_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let Some(class_name) = get_class_name(analyzer, access.class, context) else {
        return;
    };

    let prop_name = match &access.property {
        mago_syntax::ast::ast::variable::Variable::Direct(direct) => direct.name,
        _ => return,
    };
    let prop_name_str = prop_name.trim_start_matches('$');

    let class_id = analyzer.interner.intern(&class_name);
    let prop_id = analyzer.interner.intern(prop_name_str);

    let Some((prop_type, declaring_class)) = analyzer
        .codebase
        .get_class(class_id)
        .and_then(|class_info| class_info.properties.get(&prop_id))
        .and_then(|prop_info| {
            prop_info
                .get_type()
                .map(|prop_type| (prop_type.clone(), prop_info.declaring_class))
        })
    else {
        return;
    };

    let mut expanded_prop_type = prop_type;
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut expanded_prop_type,
        &crate::type_expander::TypeExpansionOptions {
            self_class: Some(declaring_class),
            static_class_type: crate::type_expander::StaticClassType::Name(declaring_class),
            ..Default::default()
        },
    );
    check_assigned_value_against_property_type(
        analyzer,
        &class_name,
        prop_name_str,
        &expanded_prop_type,
        value_type,
        pos,
        analysis_data,
    );
}

/// The value-vs-declared-property-type check shared by direct
/// (`Foo::$bar = ...`) and array-offset (`Foo::$bar[...] = ...`) static
/// property assignments.
fn check_assigned_value_against_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    prop_name_str: &str,
    prop_type: &TUnion,
    value_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    // The assigned value may itself carry unexpanded class-constant tokens
    // (a local @var with TaintKind::* feeding the property) — expand both
    // sides like Psalm's use-site TypeExpander before comparing.
    let mut expanded_value_type = value_type.clone();
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut expanded_value_type,
        &crate::type_expander::TypeExpansionOptions::default(),
    );
    // Psalm's static property assignment runs its containment with
    // ignore_null/ignore_false and (unlike the instance path) has no
    // follow-up Possibly{Null,False}PropertyAssignmentValue checks —
    // `self::\$tmpDir = tempnam(...)` into a string property stays silent.
    if !prop_type.is_nullable() || !prop_type.types.iter().any(|a| matches!(a, TAtomic::TFalse)) {
        let kept: Vec<TAtomic> = expanded_value_type
            .types
            .iter()
            .filter(|atomic| {
                !(matches!(atomic, TAtomic::TNull) && !prop_type.is_nullable())
                    && !(matches!(atomic, TAtomic::TFalse)
                        && !prop_type.types.iter().any(|a| {
                            matches!(a, TAtomic::TFalse | TAtomic::TBool | TAtomic::TScalar)
                        }))
            })
            .cloned()
            .collect();
        if !kept.is_empty() && kept.len() != expanded_value_type.types.len() {
            expanded_value_type.types = kept;
        }
    }
    let value_type = &expanded_value_type;

    let mut comparison_result = TypeComparisonResult::new();
    let is_contained = union_type_comparator::is_contained_by(
        analyzer.codebase,
        value_type,
        prop_type,
        false,
        false,
        &mut comparison_result,
    );

    if is_contained {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);

    // Check for type coercion
    if comparison_result.type_coerced.unwrap_or(false) {
        if comparison_result.type_coerced_from_mixed.unwrap_or(false) {
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedPropertyTypeCoercion,
                format!(
                    "Property {}::${} expects {}, parent type {} provided",
                    class_name,
                    prop_name_str,
                    prop_type.get_id(Some(analyzer.interner)),
                    value_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        } else {
            analysis_data.add_issue(Issue::new(
                IssueKind::PropertyTypeCoercion,
                format!(
                    "Property {}::${} expects {}, parent type {} provided",
                    class_name,
                    prop_name_str,
                    prop_type.get_id(Some(analyzer.interner)),
                    value_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    } else {
        // Check if there's a partial match (possibly invalid)
        if has_mixed_array_key_property_coercion(analyzer, value_type, prop_type) {
            analysis_data.add_issue(Issue::new(
                IssueKind::MixedPropertyTypeCoercion,
                format!(
                    "Property {}::${} expects {}, parent type {} provided",
                    class_name,
                    prop_name_str,
                    prop_type.get_id(Some(analyzer.interner)),
                    value_type.get_id(Some(analyzer.interner))
                ),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        } else {
            let can_be_contained = union_type_comparator::can_be_contained_by(
                analyzer.codebase,
                value_type,
                prop_type,
            );

            if can_be_contained {
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyInvalidPropertyAssignmentValue,
                    format!(
                        "Property {}::${} expects {}, possibly different type {} provided",
                        class_name,
                        prop_name_str,
                        prop_type.get_id(Some(analyzer.interner)),
                        value_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            } else {
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidPropertyAssignmentValue,
                    format!(
                        "Property {}::${} expects {}, got {}",
                        class_name,
                        prop_name_str,
                        prop_type.get_id(Some(analyzer.interner)),
                        value_type.get_id(Some(analyzer.interner))
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
}

/// Try to extract a class name from an expression, resolving self/static/parent.
fn get_class_name(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<String> {
    let resolve_alias = |class_id| {
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .filter(|alias_target| analyzer.codebase.get_class(*alias_target).is_some())
            .unwrap_or(class_id)
    };

    match expr {
        // Handle special keywords: self, static, parent
        Expression::Self_(_) | Expression::Static(_) => {
            // Get the current class from the analyzer
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                let class_id = resolve_alias(declaring_class);
                return Some(analyzer.interner.lookup(class_id).to_string());
            }
            None
        }
        Expression::Parent(_) => {
            // Get the parent class
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                if let Some(class_info) = analyzer.codebase.get_class(declaring_class) {
                    if let Some(parent) = class_info.parent_class {
                        let class_id = resolve_alias(parent);
                        return Some(analyzer.interner.lookup(class_id).to_string());
                    }
                }
            }
            None
        }
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            let class_id = analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(id.value()));
            let class_id = resolve_alias(class_id);
            Some(analyzer.interner.lookup(class_id).to_string())
        }
        _ => None,
    }
}

fn has_mixed_array_key_property_coercion(
    analyzer: &StatementsAnalyzer<'_>,
    value_type: &TUnion,
    property_type: &TUnion,
) -> bool {
    for value_atomic in &value_type.types {
        let Some((value_key_type, value_value_type)) = get_array_key_value_union(value_atomic)
        else {
            continue;
        };

        if !is_broad_array_key_union(value_key_type) {
            continue;
        }

        for property_atomic in &property_type.types {
            let Some((property_key_type, property_value_type)) =
                get_array_key_value_union(property_atomic)
            else {
                continue;
            };

            if property_key_type.is_mixed() {
                continue;
            }

            let mut value_comparison = TypeComparisonResult::new();
            if !union_type_comparator::is_contained_by(
                analyzer.codebase,
                value_value_type,
                property_value_type,
                false,
                false,
                &mut value_comparison,
            ) {
                continue;
            }

            let mut key_comparison = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                analyzer.codebase,
                value_key_type,
                property_key_type,
                false,
                false,
                &mut key_comparison,
            ) {
                continue;
            }

            if union_type_comparator::can_be_contained_by(
                analyzer.codebase,
                property_key_type,
                value_key_type,
            ) {
                return true;
            }
        }
    }

    false
}

fn get_array_key_value_union(atomic: &TAtomic) -> Option<(&TUnion, &TUnion)> {
    // The pre-unification version matched only generic `array`/`non-empty-array`
    // (old `TArray`/`TNonEmptyArray`) — non-list arrays with typed params and no
    // known entries — so lists and keyed shapes are excluded here to preserve
    // behaviour exactly.
    if atomic.array_is_list() || !atomic.is_generic_array() {
        return None;
    }
    atomic.array_params()
}

fn is_broad_array_key_union(key_type: &TUnion) -> bool {
    key_type.is_mixed()
        || key_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TArrayKey | TAtomic::TMixed))
}
