//! Instance property assignment analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::PropertyAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{DataFlowNode, GraphKind, Issue, IssueKind, PathKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::expr::call::function_call_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use std::rc::Rc;

/// Analyze an instance property assignment ($obj->prop = value).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    value_expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_compound: bool,
) {
    // Analyze the value expression
    // Hakana's instance_property_assignment_analyzer analyzes the assigned
    // value as general use (the data escapes into the object).
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

    // Psalm InstancePropertyAssignmentAnalyzer type-coverage: a property
    // assignment counts as mixed when the assigned value is mixed.
    analysis_data.record_mixedness(context, value_type.is_mixed());

    // A statement-level `/** @var T */` overrides the assigned type (Psalm's
    // AssignmentAnalyzer applies var comments to any assignment target,
    // including instance properties — e.g. `$this->cache = unserialize(...)`).
    if let Some(annotation_type) = analysis_data.current_stmt_start.and_then(|stmt_start| {
        let annotations = analyzer.get_inline_var_annotations(stmt_start)?;
        let prop_key = match &access.property {
            ClassLikeMemberSelector::Identifier(id) => {
                expression_identifier::get_expression_var_key(access.object)
                    .map(|object_key| format!("{}->{}", object_key, id.value))
            }
            _ => None,
        };
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

    analyze_with_known_type(
        analyzer,
        access,
        value_type,
        pos,
        analysis_data,
        context,
        is_compound,
    );
}

/// Analyze an instance property assignment using a precomputed assigned value type.
///
/// This is used by destructuring assignments where each target receives a value type
/// inferred from the RHS container.
pub fn analyze_with_known_type(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    value_type: TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_compound: bool,
) {
    let explicit_mutation_free_context = is_explicit_mutation_free_context(analyzer);

    // Analyze the object expression. The receiver of a property write is
    // used by the assignment (`$x->getSource()->prop = …` consumes the call's
    // return value) — Psalm analyzes it inside the assignment context.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let raw_obj_type = analysis_data.expr_types.get(&obj_pos).cloned();
    let receiver_reference_free = raw_obj_type.as_ref().is_some_and(|t| t.reference_free);
    let receiver_allow_mutations = raw_obj_type.as_ref().is_some_and(|t| t.allow_mutations);
    let obj_type = raw_obj_type.map(|obj_type| expand_template_object_union(&obj_type));

    // Get the property name. A dynamic selector (`$obj->$name = …`) is an
    // expression in its own right — Psalm analyzes it as a general use, so
    // `$name` counts as used.
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        ClassLikeMemberSelector::Variable(selector_var) => {
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;
            let _ = expression_analyzer::analyze(
                analyzer,
                &Expression::Variable(selector_var.clone()),
                analysis_data,
                context,
            );
            context.inside_general_use = was_inside_general_use;
            None
        }
        // `$obj->{$expr} = …`: the selector expression is consumed by the
        // write (its variables count as used).
        ClassLikeMemberSelector::Expression(selector_expr) => {
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;
            let _ = expression_analyzer::analyze(
                analyzer,
                selector_expr.expression,
                analysis_data,
                context,
            );
            context.inside_general_use = was_inside_general_use;
            None
        }
    };

    // Check if this is $this->prop
    let is_this_assignment = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    // Psalm `InstancePropertyAssignmentAnalyzer`: assigning to a property is
    // impure from a mutation-free context only when the receiver's type does
    // not allow mutations ($this outside the constructor of a mutation-free
    // method; fresh `new`/`clone` values keep allow_mutations and are fine).
    // Readonly / immutable-class properties are policed by the
    // InaccessibleProperty path instead, so they're exempt here.
    let lhs_var_disallows_mutations =
        crate::expression_identifier::get_expression_var_key(access.object)
            .and_then(|var_id| context.get_var_type(&var_id).cloned())
            .is_some_and(|lhs_type| !lhs_type.allow_mutations);

    let impure_assignment_candidate = explicit_mutation_free_context
        && lhs_var_disallows_mutations
        && !is_special_write_method(analyzer);
    let mut impure_assignment_emitted = false;

    // Psalm `AssignmentAnalyzer`'s blanket check: in a mutation-free or
    // external-mutation-free context, writing to a property of anything that
    // isn't pure-compatible (reference-free `$this`, fresh `new`/`clone`
    // values) is impure.
    let receiver_pure_compatible = receiver_reference_free;
    let context_forbids_property_writes = analyzer.function_info.is_some_and(|fi| {
        fi.is_pure
            || (!fi.mutation_free_inferred
                && (fi.is_external_mutation_free
                    || (fi.is_mutation_free && fi.name != pzoom_str::StrId::CONSTRUCT)))
    });
    if context_forbids_property_writes
        && !receiver_pure_compatible
        && !is_special_write_method(analyzer)
    {
        impure_assignment_emitted = true;
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ImpurePropertyAssignment,
            "Cannot assign to a property from a mutation-free context",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Verify property type if we can resolve it
    if let Some(obj_type) = obj_type {
        if let Some(prop_name) = prop_name {
            // A `false`/`null` member the receiver marks ignorable
            // (`@psalm-ignore-falsable-return` / `-nullable-return`) is not a
            // real non-object case — Psalm runs the assignment containment with
            // those flags so the false/null part raises nothing. Drop them
            // before classifying the union (e.g. DOMDocument::createElement
            // returns `DOMElement|false` with `@psalm-ignore-falsable-return`).
            let lookup_obj_type = strip_ignored_null_false(&obj_type);
            let lookup_types = expand_intersection_lookup_types(&lookup_obj_type);

            // Check for null/invalid types in the union
            let has_object_type = lookup_types
                .iter()
                .any(|t| matches!(t, TAtomic::TNamedObject { .. } | TAtomic::TObject));
            let has_null = lookup_types.iter().any(|t| matches!(t, TAtomic::TNull));
            let has_invalid_type = lookup_types.iter().any(|t| {
                !matches!(
                    t,
                    TAtomic::TNamedObject { .. }
                        | TAtomic::TObject
                        | TAtomic::TNull
                        | TAtomic::TMixed
                )
            });

            // Check for purely null type (NullPropertyAssignment)
            if obj_type.is_null() {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NullPropertyAssignment,
                    format!("Cannot assign to property ${} on null", prop_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.expr_types.insert(pos, Rc::new(value_type));
                return;
            }

            // Check for nullable type (PossiblyNullPropertyAssignment); the
            // union's ignore-nullable flag silences it (Psalm checks
            // `!$lhs_type->ignore_nullable_issues`).
            if has_null && has_object_type && !obj_type.ignore_nullable_issues {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyNullPropertyAssignment,
                    format!(
                        "Cannot assign to property ${} on possibly null type",
                        prop_name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Check for invalid (non-object) types
            if has_invalid_type && !has_object_type {
                // Purely invalid type
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InvalidPropertyAssignment,
                    format!(
                        "Cannot assign to property ${} on {}",
                        prop_name,
                        obj_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.expr_types.insert(pos, Rc::new(value_type));
                return;
            }

            // Some members are objects and some are not (`A|int`): the write is
            // valid for the object part but possibly wrong for the rest — Psalm's
            // PossiblyInvalidPropertyAssignment. The object part is still
            // processed below (no early return).
            if has_invalid_type && has_object_type {
                let invalid_type_id = lookup_types
                    .iter()
                    .find(|t| {
                        !matches!(
                            t,
                            TAtomic::TNamedObject { .. }
                                | TAtomic::TObject
                                | TAtomic::TNull
                                | TAtomic::TMixed
                        )
                    })
                    .map(|t| t.get_id(Some(analyzer.interner)))
                    .unwrap_or_else(|| obj_type.get_id(Some(analyzer.interner)));
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyInvalidPropertyAssignment,
                    format!(
                        "Cannot assign to property ${prop_name} with possible non-object type '{invalid_type_id}'",
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            let prop_id = analyzer.interner.intern(prop_name);
            let has_concrete_property_candidate = lookup_types.iter().any(|atomic| {
                let TAtomic::TNamedObject { name, .. } = atomic else {
                    return false;
                };

                analyzer
                    .codebase
                    .get_class(*name)
                    .is_some_and(|class_info| class_info.properties.contains_key(&prop_id))
            });

            for atomic in &lookup_types {
                match atomic {
                    TAtomic::TNamedObject {
                        name, type_params, ..
                    } => {
                        // A private property is not inherited into a subclass's
                        // table: retarget to the enclosing class when it
                        // declares the property (Psalm's context-self fallback
                        // for instanceof-narrowed $this).
                        let name = &crate::expr::fetch::atomic_property_fetch_analyzer::retarget_property_class_for_context(
                            analyzer, *name, prop_id,
                        );
                        // Look up the class and property; a static property
                        // is invisible to instance access (Psalm reports
                        // UndefinedPropertyAssignment for `$obj->staticProp = ...`).
                        if let Some(class_info) = analyzer.codebase.get_class(*name) {
                            if let Some(prop_info) = class_info
                                .properties
                                .get(&prop_id)
                                .filter(|prop_info| !prop_info.is_static)
                            {
                                // A compound assignment (`$obj->prop += …`)
                                // reads the old value, so it marks the property
                                // used for find_unused_code (a plain `=` write
                                // does not count as a read).
                                if is_compound && analyzer.config.find_unused_code {
                                    analysis_data
                                        .referenced_properties
                                        .insert((prop_info.declaring_class, prop_id));
                                    analysis_data.add_class_member_reference(
                                        &context.function_context,
                                        (prop_info.declaring_class, prop_id),
                                        false,
                                    );
                                }

                                // Hakana `add_instance_property_dataflow`: a
                                // `@psalm-taint-specialize` class routes the
                                // assignment through the receiver variable's
                                // own dataflow (per-instance); other classes
                                // write the global `Class::$prop` node.
                                if let GraphKind::WholeProgram(_) =
                                    analysis_data.data_flow_graph.kind
                                {
                                    let name_span = access.property.span();
                                    if class_info.specialize_instance {
                                        if let Some(lhs_var_id) =
                                            crate::expression_identifier::get_expression_var_key(
                                                access.object,
                                            )
                                        {
                                            let object_span = access.object.span();
                                            add_instance_property_assignment_dataflow(
                                                analyzer,
                                                analysis_data,
                                                &lhs_var_id,
                                                (object_span.start.offset, object_span.end.offset),
                                                (name_span.start.offset, name_span.end.offset),
                                                (*name, prop_id),
                                                &value_type,
                                                context,
                                            );
                                        }
                                    } else {
                                        add_unspecialized_property_assignment_dataflow(
                                            analyzer,
                                            (*name, prop_id),
                                            (name_span.start.offset, name_span.end.offset),
                                            analysis_data,
                                            &value_type,
                                            Some(prop_info.declaring_class),
                                        );
                                    }
                                }

                                // Check property visibility - private properties are only accessible within the same class
                                if prop_info.visibility == Visibility::Private {
                                    let is_same_class =
                                        crate::expr::fetch::atomic_property_fetch_analyzer::calling_context_owns_class(
                                            analyzer, *name,
                                        );

                                    if !is_same_class
                                        && !receiver_allows_property_visibility_override(
                                            analyzer, &obj_type, *name,
                                        )
                                    {
                                        let class_name = analyzer.interner.lookup(*name);
                                        let (line, col) = analyzer.get_line_column(pos.0);
                                        analysis_data.add_issue(Issue::new(
                                            IssueKind::InaccessibleProperty,
                                            format!(
                                                "Cannot access private property {}::${}",
                                                class_name, prop_name
                                            ),
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                    }
                                }

                                // Check if property is readonly. A
                                // @psalm-immutable class implicitly marks all
                                // its properties readonly (Psalm's scanner).
                                if prop_info.is_readonly || class_info.is_immutable {
                                    let restricting_class = *name;
                                    let readonly_allow_private_mutation =
                                        prop_info.readonly_allow_private_mutation;
                                    let class_name = analyzer.interner.lookup(restricting_class);
                                    // Psalm `InstancePropertyAssignmentAnalyzer::
                                    // can_set_readonly_property`: writing a
                                    // readonly / immutable-class property is only
                                    // allowed from code that owns the appearing
                                    // class (`$context->self` is, or extends, it —
                                    // and a trait body owns its using class, since
                                    // it is analysed with `$this` retargeted there)
                                    // AND one of: a special init method
                                    // (__construct/unserialize/__unserialize/
                                    // __clone), the property allows private
                                    // mutation, or the receiver value is
                                    // pure-compatible. A receiver is pure-compatible
                                    // when its type is reference-free *and* still
                                    // allows mutations — i.e. a fresh `new`/`clone`
                                    // of an immutable class (the "wither"), but not
                                    // `$this` outside the constructor and not an
                                    // external param of an immutable type.
                                    let owns_class =
                                        crate::expr::fetch::atomic_property_fetch_analyzer::calling_context_owns_class(
                                            analyzer, restricting_class,
                                        );
                                    let property_var_pure_compatible =
                                        receiver_reference_free && receiver_allow_mutations;
                                    let can_write_restricted_property = owns_class
                                        && (is_special_write_method(analyzer)
                                            || readonly_allow_private_mutation
                                            || property_var_pure_compatible);

                                    if !can_write_restricted_property {
                                        // Psalm runs the mutation-free check
                                        // independently of the readonly one, so a
                                        // restricted write from a mutation-free
                                        // context is *both* InaccessibleProperty and
                                        // ImpurePropertyAssignment. The readonly path
                                        // returns early, so emit the impure
                                        // diagnostic here too (the standalone per-
                                        // atomic check below is skipped by the
                                        // `continue`). Guarded so it never doubles a
                                        // diagnostic already emitted upstream.
                                        if impure_assignment_candidate && !impure_assignment_emitted
                                        {
                                            impure_assignment_emitted = true;
                                            let (line, col) = analyzer.get_line_column(pos.0);
                                            analysis_data.add_issue(Issue::new(
                                                IssueKind::ImpurePropertyAssignment,
                                                "Cannot assign to a property from a mutation-free context",
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        }
                                        let (line, col) = analyzer.get_line_column(pos.0);
                                        let message = format!(
                                            "{}::${} is marked readonly",
                                            class_name, prop_name
                                        );
                                        analysis_data.add_issue(Issue::new(
                                            IssueKind::InaccessibleProperty,
                                            message,
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                        continue;
                                    }
                                }

                                if impure_assignment_candidate
                                    && !impure_assignment_emitted
                                    && !prop_info.is_readonly
                                    && !class_info.is_immutable
                                {
                                    impure_assignment_emitted = true;
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::ImpurePropertyAssignment,
                                        "Cannot assign to a property from a mutation-free context",
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }

                                if is_unserialize_method(analyzer) && is_this_assignment {
                                    continue;
                                }

                                if prop_info.is_deprecated {
                                    let class_name = analyzer.interner.lookup(*name);
                                    let (line, col) = analyzer.get_line_column(pos.0);
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::DeprecatedProperty,
                                        format!(
                                            "Property {}::${} is deprecated",
                                            class_name, prop_name
                                        ),
                                        analyzer.file_path,
                                        pos.0,
                                        pos.1,
                                        line,
                                        col,
                                    ));
                                }

                                if !can_access_internal(
                                    analyzer,
                                    &class_info.internal,
                                    Some(context),
                                ) {
                                    let scope_phrase = format_internal_scope_phrase(
                                        analyzer,
                                        &class_info.internal,
                                    );
                                    let class_name = analyzer.interner.lookup(*name);
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

                                if !can_access_internal(
                                    analyzer,
                                    &prop_info.internal,
                                    Some(context),
                                ) {
                                    let scope_phrase =
                                        format_internal_scope_phrase(analyzer, &prop_info.internal);
                                    let class_name = analyzer.interner.lookup(*name);
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

                                // Verify type compatibility using proper type comparator
                                // Only check if property has a declared type; an
                                // untyped redeclaration inherits the overridden
                                // ancestor property's type (Psalm's
                                // Properties::getPropertyType fallback).
                                let inherited_prop_type = if prop_info.get_type().is_none() {
                                    crate::expr::fetch::atomic_property_fetch_analyzer::
                                        get_overridden_property_type(
                                            analyzer.codebase,
                                            *name,
                                            prop_id,
                                        )
                                } else {
                                    None
                                };
                                if let Some(prop_type) =
                                    prop_info.get_type().or(inherited_prop_type.as_ref())
                                {
                                    // Psalm skips the property-type containment
                                    // check for a mixed value and reports
                                    // MixedAssignment instead ("Unable to
                                    // determine the type that $x->p is being
                                    // assigned to").
                                    if value_type.is_mixed() {
                                        if !prop_type.is_mixed()
                                            && !crate::issue_suppression::is_issue_suppressed_at(
                                                analyzer,
                                                analysis_data,
                                                pos.0,
                                                "MixedAssignment",
                                            )
                                        {
                                            let var_id =
                                                expression_identifier::get_expression_var_key(
                                                    access.object,
                                                )
                                                .map(|object_key| {
                                                    format!("{}->{}", object_key, prop_name)
                                                });
                                            let message = match var_id {
                                                Some(var_id) => format!(
                                                    "Unable to determine the type that {} is being assigned to",
                                                    var_id
                                                ),
                                                None => "Unable to determine the type of this assignment"
                                                    .to_string(),
                                            };
                                            let (line, col) = analyzer.get_line_column(pos.0);
                                            analysis_data.add_issue(Issue::new(
                                                IssueKind::MixedAssignment,
                                                message,
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        }
                                        continue;
                                    }

                                    let prop_type = substitute_class_template_params(
                                        class_info,
                                        type_params.as_deref(),
                                        prop_type,
                                    );
                                    // `static` in the declared property type
                                    // binds to the receiver class — concrete
                                    // when the receiver is final (Psalm's
                                    // TypeExpander final flag).
                                    let prop_type =
                                        crate::type_expander::localize_special_class_type_union_final(
                                            analyzer.codebase,
                                            analyzer.interner,
                                            &prop_type,
                                            prop_info.declaring_class,
                                            *name,
                                            class_info.parent_class,
                                            class_info.is_final,
                                        );
                                    let localized_value_type = substitute_class_template_params(
                                        class_info,
                                        type_params.as_deref(),
                                        &value_type,
                                    );
                                    // Psalm's property assignment ignores
                                    // null/false members whose union carries
                                    // ignore_nullable/falsable_issues (e.g.
                                    // \$argv's CLI-only null).
                                    let localized_value_type =
                                        strip_ignored_null_false(&localized_value_type);
                                    let mut comparison_result = TypeComparisonResult::new();
                                    // Psalm's property containment ignores null
                                    // and false; the dedicated PossiblyNull/
                                    // PossiblyFalsePropertyAssignmentValue
                                    // checks below handle those on a match.
                                    let is_contained = union_type_comparator::is_contained_by(
                                        analyzer.codebase,
                                        &localized_value_type,
                                        &prop_type,
                                        true,
                                        true,
                                        &mut comparison_result,
                                    );

                                    if is_contained {
                                        // Hakana: transfer type-variable bounds
                                        // recorded while checking the assignment.
                                        let bound_pos =
                                            crate::template::bound_location(analyzer, pos);
                                        crate::template::record_type_variable_bounds(
                                            analysis_data,
                                            std::mem::take(
                                                &mut comparison_result.type_variable_lower_bounds,
                                            ),
                                            std::mem::take(
                                                &mut comparison_result.type_variable_upper_bounds,
                                            ),
                                            Some(bound_pos),
                                        );

                                        let class_name = analyzer.interner.lookup(*name);
                                        let (line, col) = analyzer.get_line_column(pos.0);
                                        // A template-typed property accepts what its
                                        // bound accepts; a mixed property accepts
                                        // anything (Psalm skips both).
                                        let prop_accepts_null = prop_type.is_nullable()
                                            || prop_type.is_mixed()
                                            || prop_type.types.iter().any(|atomic| match atomic {
                                                TAtomic::TTemplateParam { as_type, .. } => {
                                                    as_type.is_nullable() || as_type.is_mixed()
                                                }
                                                _ => false,
                                            });
                                        let prop_accepts_false = prop_type.is_falsable()
                                            || prop_type.is_mixed()
                                            || prop_type.types.iter().any(|atomic| match atomic {
                                                TAtomic::TTemplateParam { as_type, .. } => {
                                                    as_type.is_falsable() || as_type.is_mixed()
                                                }
                                                _ => false,
                                            });
                                        if !localized_value_type.ignore_nullable_issues
                                            && localized_value_type.is_nullable()
                                            && !prop_accepts_null
                                        {
                                            analysis_data.add_issue(Issue::new(
                                                IssueKind::PossiblyNullPropertyAssignmentValue,
                                                format!(
                                                    "Property {}::${} expects {}, possibly different type {} provided",
                                                    class_name,
                                                    prop_name,
                                                    prop_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                ),
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        } else if !localized_value_type.ignore_falsable_issues
                                            && localized_value_type.is_falsable()
                                            && !prop_accepts_false
                                            && !prop_type.types.iter().any(|atomic| {
                                                matches!(
                                                    atomic,
                                                    TAtomic::TBool
                                                        | TAtomic::TTrue
                                                        | TAtomic::TScalar
                                                )
                                            })
                                        {
                                            analysis_data.add_issue(Issue::new(
                                                IssueKind::PossiblyFalsePropertyAssignmentValue,
                                                format!(
                                                    "Property {}::${} expects {}, possibly different type {} provided",
                                                    class_name,
                                                    prop_name,
                                                    prop_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                ),
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        }
                                    }

                                    if !is_contained {
                                        let class_name = analyzer.interner.lookup(*name);
                                        let (line, col) = analyzer.get_line_column(pos.0);

                                        // Psalm: assigning a stringable object to a
                                        // string-typed property is an implicit
                                        // __toString cast, not an invalid assignment.
                                        if comparison_result.to_string_cast
                                            || (!crate::expr::call::callable_validation::file_uses_strict_types(analyzer)
                                                && crate::expr::call::callable_validation::param_allows_string_like(&prop_type)
                                                && crate::expr::call::callable_validation::union_is_stringable_object(
                                                    analyzer,
                                                    &localized_value_type,
                                                ))
                                        {
                                            analysis_data.add_issue(Issue::new(
                                                IssueKind::ImplicitToStringCast,
                                                format!(
                                                    "Property {}::${} expects {}, object converted via __toString",
                                                    class_name,
                                                    prop_name,
                                                    prop_type.get_id(Some(analyzer.interner)),
                                                ),
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                            continue;
                                        }

                                        // Check for type coercion
                                        if comparison_result.type_coerced.unwrap_or(false) {
                                            if comparison_result
                                                .type_coerced_from_mixed
                                                .unwrap_or(false)
                                            {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::MixedPropertyTypeCoercion,
                                                    format!(
                                                        "Property {}::${} expects {}, parent type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
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
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
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
                                            if has_mixed_array_key_property_coercion(
                                                analyzer,
                                                &localized_value_type,
                                                &prop_type,
                                            ) {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::MixedPropertyTypeCoercion,
                                                    format!(
                                                        "Property {}::${} expects {}, parent type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                                continue;
                                            }

                                            let can_be_contained =
                                                union_type_comparator::can_be_contained_by(
                                                    analyzer.codebase,
                                                    &localized_value_type,
                                                    &prop_type,
                                                );

                                            if can_be_contained {
                                                analysis_data.add_issue(Issue::new(
                                                    IssueKind::PossiblyInvalidPropertyAssignmentValue,
                                                    format!(
                                                        "Property {}::${} expects {}, possibly different type {} provided",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
                                                    ),
                                                    analyzer.file_path,
                                                    pos.0,
                                                    pos.1,
                                                    line,
                                                    col,
                                                ));
                                            } else {
                                                // Psalm: when the receiver is a
                                                // union and another member accepts
                                                // this value, the assignment is only
                                                // POSSIBLY invalid
                                                // (has_valid_assignment_type).
                                                let another_member_accepts =
                                                    lookup_types.iter().any(|other| {
                                                        if std::ptr::eq(other, atomic) {
                                                            return false;
                                                        }
                                                        let TAtomic::TNamedObject {
                                                            name: other_name,
                                                            type_params: other_params,
                                                            ..
                                                        } = other
                                                        else {
                                                            return false;
                                                        };
                                                        let Some(other_info) = analyzer
                                                            .codebase
                                                            .get_class(*other_name)
                                                        else {
                                                            return false;
                                                        };
                                                        let Some(other_prop) =
                                                            other_info.properties.get(&prop_id)
                                                        else {
                                                            return false;
                                                        };
                                                        let Some(other_prop_type) =
                                                            other_prop.get_type()
                                                        else {
                                                            return true;
                                                        };
                                                        let other_prop_type =
                                                            substitute_class_template_params(
                                                                other_info,
                                                                other_params.as_deref(),
                                                                other_prop_type,
                                                            );
                                                        union_type_comparator::is_contained_by(
                                                            analyzer.codebase,
                                                            &value_type,
                                                            &other_prop_type,
                                                            false,
                                                            false,
                                                            &mut TypeComparisonResult::new(),
                                                        )
                                                    });
                                                let issue_kind = if another_member_accepts {
                                                    IssueKind::PossiblyInvalidPropertyAssignmentValue
                                                } else {
                                                    IssueKind::InvalidPropertyAssignmentValue
                                                };
                                                analysis_data.add_issue(Issue::new(
                                                    issue_kind,
                                                    format!(
                                                        "Property {}::${} expects {}, got {}",
                                                        class_name,
                                                        prop_name,
                                                        prop_type.get_id(Some(analyzer.interner)),
                                                        localized_value_type
                                                            .get_id(Some(analyzer.interner))
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
                            } else {
                                let class_name = analyzer.interner.lookup(*name);
                                let (line, col) = analyzer.get_line_column(pos.0);

                                if class_info.kind == ClassLikeKind::Interface {
                                    if !has_concrete_property_candidate {
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

                                    if has_concrete_property_candidate {
                                        continue;
                                    }
                                }

                                if class_has_magic_setter(class_info) {
                                    // Psalm `InstancePropertyAssignmentAnalyzer::analyzeSetCall`
                                    // routes the assignment through a fake
                                    // `$obj->__set($name, $value)` call; pzoom
                                    // verifies the two arguments against the
                                    // declared `__set` signature directly
                                    // (templated `__set(K $prop, TData[K] $v)`
                                    // params bind from the literal property
                                    // name and the assigned value).
                                    verify_magic_set_call_arguments(
                                        analyzer,
                                        class_info,
                                        type_params.as_deref(),
                                        prop_name,
                                        &value_type,
                                        pos,
                                        analysis_data,
                                    );

                                    if let Some(pseudo_type) = get_pseudo_property_set_type(
                                        class_info,
                                        type_params.as_deref(),
                                        prop_id,
                                    ) {
                                        let localized_value_type = substitute_class_template_params(
                                            class_info,
                                            type_params.as_deref(),
                                            &value_type,
                                        );
                                        let mut comparison_result = TypeComparisonResult::new();
                                        let is_contained = union_type_comparator::is_contained_by(
                                            analyzer.codebase,
                                            &localized_value_type,
                                            &pseudo_type,
                                            false,
                                            false,
                                            &mut comparison_result,
                                        );

                                        if !is_contained {
                                            let can_be_contained =
                                                union_type_comparator::can_be_contained_by(
                                                    analyzer.codebase,
                                                    &localized_value_type,
                                                    &pseudo_type,
                                                );

                                            let issue_kind = if can_be_contained {
                                                IssueKind::PossiblyInvalidPropertyAssignmentValue
                                            } else {
                                                IssueKind::InvalidPropertyAssignmentValue
                                            };
                                            let message = if can_be_contained {
                                                format!(
                                                    "Property {}::${} expects {}, possibly different type {} provided",
                                                    class_name,
                                                    prop_name,
                                                    pseudo_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                )
                                            } else {
                                                format!(
                                                    "Property {}::${} expects {}, got {}",
                                                    class_name,
                                                    prop_name,
                                                    pseudo_type.get_id(Some(analyzer.interner)),
                                                    localized_value_type
                                                        .get_id(Some(analyzer.interner))
                                                )
                                            };

                                            analysis_data.add_issue(Issue::new(
                                                issue_kind,
                                                message,
                                                analyzer.file_path,
                                                pos.0,
                                                pos.1,
                                                line,
                                                col,
                                            ));
                                        }

                                        continue;
                                    }

                                    if class_has_sealed_properties(class_info) {
                                        let kind = if is_this_assignment {
                                            IssueKind::UndefinedThisPropertyAssignment
                                        } else {
                                            IssueKind::UndefinedMagicPropertyAssignment
                                        };
                                        let message = if is_this_assignment {
                                            format!(
                                                "Property {}::${} does not exist",
                                                class_name, prop_name
                                            )
                                        } else {
                                            format!(
                                                "Magic property {}::${} does not exist",
                                                class_name, prop_name
                                            )
                                        };

                                        analysis_data.add_issue(Issue::new(
                                            kind,
                                            message,
                                            analyzer.file_path,
                                            pos.0,
                                            pos.1,
                                            line,
                                            col,
                                        ));
                                        continue;
                                    }

                                    continue;
                                }

                                if class_allows_dynamic_property_assignment(analyzer, class_info) {
                                    continue;
                                }

                                if is_this_assignment {
                                    analysis_data.add_issue(Issue::new(
                                        IssueKind::UndefinedThisPropertyAssignment,
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
                                        IssueKind::UndefinedPropertyAssignment,
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
                                }
                            }
                        }
                    }
                    TAtomic::TNull => {
                        // Already handled above
                    }
                    TAtomic::TMixed => {
                        if is_this_assignment && !context.has_this {
                            continue;
                        }
                        if crate::issue_suppression::is_issue_suppressed_at(
                            analyzer,
                            analysis_data,
                            pos.0,
                            "MixedPropertyAssignment",
                        ) {
                            continue;
                        }
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::MixedPropertyAssignment,
                            format!("Cannot assign to property ${} on mixed type", prop_name),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(object_key) = expression_identifier::get_expression_var_key(access.object) {
        // Pre-existence must be sampled before the member-tracking clear:
        // Psalm's removeDescendents gate checks vars_in_scope at assignment
        // time.
        let path_was_tracked = prop_name.is_some_and(|prop_name| {
            context
                .locals
                .contains_key(format!("{}->{}", object_key, prop_name).as_str())
        });
        clear_object_member_tracking(context, &object_key, prop_name);

        if let Some(prop_name) = prop_name {
            let property_key = format!("{}->{}", object_key, prop_name);
            let property_id = VarName::new(&property_key);
            // A statement-level `/** @var T \$obj->prop */` overrides the
            // assigned type for the tracked property path (Psalm's
            // CommentAnalyzer accepts property-path @var targets).
            let stored_type = analysis_data
                .current_stmt_start
                .and_then(|stmt_start| {
                    crate::expr::variable_fetch_analyzer::get_inline_var_annotation_type(
                        analyzer,
                        stmt_start,
                        &property_key,
                    )
                })
                .unwrap_or_else(|| value_type.clone());
            context.set_var_type(property_id, stored_type);
            // During a collect_initializations pass, remember which class context
            // assigned this `$this->prop` (Psalm's `initialized_class`): a parent
            // constructor's `$this->b = …` sets the *parent's* private `$b`, which
            // the property-init check must tell apart from a same-named private
            // `$b` on the child.
            if context.collect_initializations
                && object_key == "$this"
                && let Some(self_class) = context.self_class
            {
                let property_name_id = analyzer.interner.intern(prop_name);
                context
                    .initialized_prop_classes
                    .insert(property_name_id, self_class);
            }
            // Psalm's AssignmentAnalyzer calls removeDescendents →
            // removeVarFromConflictingClauses for property-path assignments
            // too: clauses mentioning `$obj->prop` (or paths under it) are
            // stale, and the eviction lands in parent_remove_vars so later
            // if-statement boundaries replay it on their outer contexts.
            // removeDescendents only fires when the path was already in
            // scope, so a first write doesn't seed the replay.
            if path_was_tracked {
                context.remove_var_name_from_conflicting_clauses(&property_key);
            } else {
                context.remove_var_name_clauses(&property_key);
            }
        }
    }

    // The assignment expression returns the assigned value
    analysis_data.expr_types.insert(pos, Rc::new(value_type));
}

/// Hakana `add_unspecialized_property_assignment_dataflow`: links a localized
/// property-assignment node to the declared property, and the assigned value's
/// parents to the localized node. (Hakana also removes taints declared in inline
/// comments here; pzoom does not track those.)
/// Hakana `add_instance_property_assignment_dataflow`
/// (`specialize_instance` classes): the assigned value flows into a local
/// `$var->prop` node, then into the receiver variable's own node, which is
/// pushed onto the receiver's in-scope type — instance state stays tied to
/// the variable instead of a global property node.
#[allow(clippy::too_many_arguments)]
fn add_instance_property_assignment_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    lhs_var_id: &str,
    var_pos: Pos,
    name_pos: Pos,
    property_id: (StrId, StrId),
    assignment_value_type: &TUnion,
    context: &mut crate::context::BlockContext,
) {
    let var_str_id = pzoom_code_info::VarId(
        analyzer
            .interner
            .intern(&pzoom_code_info::VarName::new(lhs_var_id)),
    );
    let var_node =
        DataFlowNode::get_for_lvar(var_str_id, make_data_flow_node_position(analyzer, var_pos));
    let property_node = DataFlowNode::get_for_local_property_fetch(
        var_str_id,
        property_id.1,
        make_data_flow_node_position(analyzer, name_pos),
    );

    analysis_data.data_flow_graph.add_node(var_node.clone());
    analysis_data
        .data_flow_graph
        .add_node(property_node.clone());
    analysis_data.data_flow_graph.add_path(
        &property_node.id,
        &var_node.id,
        PathKind::PropertyAssignment(property_id.0, property_id.1),
        vec![],
        vec![],
    );
    for parent_node in assignment_value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &property_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    if let Some(stmt_var_type) = context.locals.get_mut_owned(lhs_var_id) {
        if !stmt_var_type
            .parent_nodes
            .iter()
            .any(|node| node.id == var_node.id)
        {
            stmt_var_type.parent_nodes.push(var_node);
        }
    }
}

pub(crate) fn add_unspecialized_property_assignment_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    property_id: (StrId, StrId),
    stmt_name_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    assignment_value_type: &TUnion,
    declaring_property_class: Option<StrId>,
) {
    let localized_property_node = DataFlowNode::get_for_localized_property(
        property_id,
        make_data_flow_node_position(analyzer, stmt_name_pos),
    );

    analysis_data
        .data_flow_graph
        .add_node(localized_property_node.clone());

    let property_node = DataFlowNode::get_for_property(property_id);

    analysis_data
        .data_flow_graph
        .add_node(property_node.clone());
    analysis_data.data_flow_graph.add_path(
        &localized_property_node.id,
        &property_node.id,
        PathKind::PropertyAssignment(property_id.0, property_id.1),
        vec![],
        vec![],
    );

    for parent_node in assignment_value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &localized_property_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    if let Some(declaring_property_class) = declaring_property_class
        && declaring_property_class != property_id.0
    {
        let declaring_property_node =
            DataFlowNode::get_for_property((declaring_property_class, property_id.1));

        analysis_data.data_flow_graph.add_path(
            &property_node.id,
            &declaring_property_node.id,
            PathKind::PropertyAssignment(property_id.0, property_id.1),
            vec![],
            vec![],
        );

        analysis_data
            .data_flow_graph
            .add_node(declaring_property_node);
    }
}

pub(crate) fn is_special_write_method(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let method_name = analyzer.interner.lookup(function_info.name);
    matches!(
        method_name.as_ref(),
        "__construct" | "unserialize" | "__unserialize" | "__clone"
    )
}

fn clear_object_member_tracking(
    context: &mut BlockContext,
    object_key: &str,
    assigned_prop: Option<&str>,
) {
    let property_prefix = format!("{object_key}->");
    let keys_to_clear: Vec<_> = context
        .locals
        .keys()
        .filter(|var_id| {
            let Some(member) = var_id.strip_prefix(property_prefix.as_str()) else {
                return false;
            };
            // Memoized method-call results on the object always go (the write
            // may change what they return). For plain property entries Psalm
            // only invalidates the ASSIGNED property's path — a sibling
            // narrowing like `$info->id` survives `$info->flag = ...`
            // (InstancePropertyAssignmentAnalyzer only removes descendants of
            // the assigned var id).
            if member.contains("()") {
                return true;
            }
            match assigned_prop {
                Some(prop) => {
                    member == prop
                        || member
                            .strip_prefix(prop)
                            .is_some_and(|rest| rest.starts_with("->") || rest.starts_with('['))
                }
                None => true,
            }
        })
        .cloned()
        .collect();

    for var_id in keys_to_clear {
        context.locals.remove(&var_id);
        context.assigned_var_ids.remove(&var_id);
        context.possibly_assigned_var_ids.remove(&var_id);
    }
}

fn is_unserialize_method(analyzer: &StatementsAnalyzer<'_>) -> bool {
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    let method_name = analyzer.interner.lookup(function_info.name);
    matches!(method_name.as_ref(), "unserialize" | "__unserialize")
}

fn is_explicit_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    // Psalm FunctionLikeAnalyzer: `$context->mutation_free` is set for
    // mutation-free storage except constructors and inferred getters.
    analyzer.function_info.is_some_and(|function_info| {
        function_info.is_pure
            || (function_info.is_mutation_free
                && !function_info.mutation_free_inferred
                && function_info.name != pzoom_str::StrId::CONSTRUCT)
    })
}

fn class_allows_dynamic_property_assignment(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> bool {
    if class_info.no_seal_properties || class_info.sealed_properties == Some(false) {
        return true;
    }

    for parent_id in &class_info.all_parent_classes {
        let Some(parent_info) = analyzer.codebase.get_class(*parent_id) else {
            continue;
        };

        if parent_info.no_seal_properties || parent_info.sealed_properties == Some(false) {
            return true;
        }
    }

    false
}

fn class_has_magic_setter(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::SET)
}

/// Psalm `InstancePropertyAssignmentAnalyzer::analyzeSetCall`: an assignment
/// to an undeclared property of a class with `__set` becomes a fake
/// `$obj->__set('prop', $value)` call, whose arguments are verified like any
/// method call's. The literal property name and the assigned value bind the
/// method's own templates (`@template K as key-of<TData>`), the receiver's
/// type arguments bind the class templates, and a mismatch is an
/// `InvalidArgument` — "Argument 1 of CharacterRow::__set expects
/// 'height'|'id'|'name', but 'ame' provided".
fn verify_magic_set_call_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    prop_name: &str,
    value_type: &TUnion,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(set_info) = class_info.methods.get(&StrId::SET) else {
        return;
    };

    if set_info.params.len() < 2 {
        return;
    }

    let declaring_class_info = analyzer
        .codebase
        .get_classlike_storage_for_method(class_info.name, StrId::SET)
        .unwrap_or(class_info);

    // Class template context: defaults + the receiver's view of the
    // hierarchy (extended params and explicit type arguments), mirroring
    // `build_method_template_context` for a non-self call.
    let mut template_result =
        function_call_analyzer::get_class_template_defaults(declaring_class_info);
    for template_type in &set_info.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    let lhs_type_part = TAtomic::TNamedObject {
        name: class_info.name,
        type_params: type_params.map(|params| params.to_vec()),
        is_static: false,
        remapped_params: false,
    };
    if let Some(collected) = crate::expr::call::class_template_param_collector::collect(
        analyzer.codebase,
        declaring_class_info,
        class_info,
        Some(&lhs_type_part),
        false,
    ) {
        template_result.lower_bounds = collected;
    }

    let arg_types = [
        TUnion::new(TAtomic::TLiteralString {
            value: prop_name.to_string(),
        }),
        value_type.clone(),
    ];

    // Bind the method's own templates from the fake call's arguments, only
    // filling slots the receiver left unbound (matching
    // `build_method_template_context`).
    let mut arg_template_result = pzoom_code_info::TemplateResult {
        template_types: template_result.template_types.clone(),
        ..Default::default()
    };
    for (param, arg_type) in set_info.params.iter().zip(arg_types.iter()) {
        if let Some(param_type) = param.get_type() {
            crate::template::standin_type_replacer::infer_template_replacements_from_union(
                analyzer,
                param_type,
                arg_type,
                &mut arg_template_result,
            );
        }
    }
    for (name, entity, replacement) in
        crate::template::lower_bounds_iter(&arg_template_result).collect::<Vec<_>>()
    {
        match crate::template::lower_bounds_get(&template_result, name, entity) {
            Some(existing) if !existing.is_nothing() => {}
            _ => {
                crate::template::lower_bounds_insert(
                    &mut template_result,
                    name,
                    entity,
                    replacement,
                );
            }
        }
    }

    for (index, (param, arg_type)) in set_info.params.iter().zip(arg_types.iter()).enumerate() {
        let Some(param_type) = param.get_type() else {
            continue;
        };

        let effective_param_type = if crate::template::template_result_is_empty(&template_result) {
            param_type.clone()
        } else {
            function_call_analyzer::replace_templates_in_union(param_type, &template_result)
        };

        if effective_param_type.is_mixed() {
            continue;
        }

        // An unresolved template left in the param type cannot be checked
        // rigidly here (mirrors verify_type's unresolved-template gap).
        if effective_param_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TTemplateParam { .. } | TAtomic::TTypeVariable { .. }
            )
        }) {
            continue;
        }

        let mut comparison_result = TypeComparisonResult::new();
        let is_contained = union_type_comparator::is_contained_by(
            analyzer.codebase,
            arg_type,
            &effective_param_type,
            true,
            false,
            &mut comparison_result,
        );

        if !is_contained && !comparison_result.type_coerced.unwrap_or(false) {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidArgument,
                format!(
                    "Argument {} of {}::__set expects {}, but {} provided",
                    index + 1,
                    analyzer.interner.lookup(class_info.name),
                    effective_param_type.get_id(Some(analyzer.interner)),
                    arg_type.get_id(Some(analyzer.interner))
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

fn class_has_sealed_properties(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    if class_info.no_seal_properties {
        return false;
    }
    // Psalm's `ClassLikeStorage::hasSealedProperties`:
    //   sealed_properties ?? (user_defined ? config->seal_all_properties : false)
    // `seal_all_properties` defaults to true, so a user-defined class with a
    // `__set` and `@property` declarations seals its magic properties by
    // default (an undeclared one is UndefinedMagicPropertyAssignment); a
    // stubbed/builtin class does not seal unless it opts in explicitly.
    class_info
        .sealed_properties
        .unwrap_or(!class_info.is_stubbed)
}

fn get_pseudo_property_set_type(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    prop_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let pseudo_type = class_info.pseudo_property_set_types.get(&prop_id)?;
    Some(substitute_class_template_params(
        class_info,
        type_params,
        pseudo_type,
    ))
}
/// Drop null/false atomics the union marks ignorable
/// (`ignore_nullable_issues` / `ignore_falsable_issues`), mirroring Psalm's
/// property-assignment containment which is invoked with ignore flags and
/// reports null/false separately gated on these union flags.
fn strip_ignored_null_false(union: &TUnion) -> TUnion {
    if (!union.ignore_nullable_issues || !union.is_nullable())
        && (!union.ignore_falsable_issues
            || !union
                .types
                .iter()
                .any(|atomic| matches!(atomic, TAtomic::TFalse)))
    {
        return union.clone();
    }
    let kept: Vec<TAtomic> = union
        .types
        .iter()
        .filter(|atomic| {
            !(union.ignore_nullable_issues && matches!(atomic, TAtomic::TNull))
                && !(union.ignore_falsable_issues && matches!(atomic, TAtomic::TFalse))
        })
        .cloned()
        .collect();
    if kept.is_empty() {
        return union.clone();
    }
    let mut stripped = union.clone();
    stripped.types = kept;
    stripped
}

fn substitute_class_template_params(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    property_type: &TUnion,
) -> TUnion {
    if class_info.template_types.is_empty() && class_info.template_extended_params.is_empty() {
        return property_type.clone();
    }

    let mut template_result = function_call_analyzer::get_class_template_defaults(class_info);
    function_call_analyzer::infer_class_template_replacements_from_extended_params(
        &mut template_result,
        class_info,
    );
    function_call_analyzer::overlay_template_replacements(
        &mut template_result,
        function_call_analyzer::infer_class_template_replacements_from_type_params(
            class_info,
            type_params,
        ),
    );

    if crate::template::template_result_is_empty(&template_result) {
        return property_type.clone();
    }

    function_call_analyzer::replace_templates_in_union(property_type, &template_result)
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

    let mut expanded = TUnion::from_types(expanded_types);
    expanded.ignore_nullable_issues = obj_type.ignore_nullable_issues;
    expanded.ignore_falsable_issues = obj_type.ignore_falsable_issues;
    expanded
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
