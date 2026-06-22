//! Instance property fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::{NullSafePropertyAccess, PropertyAccess};
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an instance property access expression ($obj->prop).
use super::atomic_property_fetch_analyzer::*;
use std::rc::Rc;

/// Attach Hakana-style property-fetch dataflow to a fetched property type
/// (Hakana `atomic_property_fetch_analyzer::add_property_dataflow`).
#[allow(clippy::too_many_arguments)]
fn attach_property_fetch_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    object_expr: &Expression<'_>,
    name_pos: Pos,
    obj_type: &TUnion,
    prop_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    in_assignment: bool,
    prop_type: TUnion,
) -> TUnion {
    let prop_id = analyzer.interner.intern(prop_name);
    let lookup_types = expand_intersection_lookup_types(&expand_template_object_union(obj_type));

    for atomic in &lookup_types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };

        // Psalm's AtomicPropertyFetchAnalyzer redirects a fetch of an
        // undeclared property to a `@mixin` class that declares it, so the
        // dataflow reads the mixin's property node (`B::$userId` with
        // `@mixin A` flows from `A::$userId`).
        let (fetch_class, prop_info) = if let Some(prop_info) = class_info.properties.get(&prop_id)
        {
            (*name, prop_info)
        } else if let Some((mixin_class, mixin_prop_info)) =
            class_info.named_mixins.iter().find_map(|mixin| {
                let TAtomic::TNamedObject {
                    name: mixin_name, ..
                } = mixin
                else {
                    return None;
                };
                analyzer
                    .codebase
                    .get_class(*mixin_name)
                    .and_then(|mixin_info| mixin_info.properties.get(&prop_id))
                    .map(|mixin_prop_info| (*mixin_name, mixin_prop_info))
            })
        {
            (mixin_class, mixin_prop_info)
        } else {
            continue;
        };

        let lhs_var_id = expression_identifier::get_expression_var_key(object_expr);
        let object_span = object_expr.span();
        return add_property_dataflow(
            analyzer,
            Some((object_span.start.offset, object_span.end.offset)),
            &obj_type.parent_nodes,
            name_pos,
            analysis_data,
            prop_type,
            in_assignment,
            (fetch_class, prop_id),
            prop_info.declaring_class,
            lhs_var_id.as_deref(),
        );
    }

    prop_type
}

/// Analyze an instance property access expression ($obj->prop).
/// Mark `$obj->prop` as read for find-unused-code. Mirrors the recording in
/// `analyze_property`, used on the in-scope-cache fast path where that lookup is
/// skipped. Resolves the declaring class through the receiver's atomics (and
/// `@mixin`s are already folded into the property set), so the reference is keyed
/// the same way the unused-property check looks it up.
fn record_property_read_for_unused(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    prop_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !analyzer.config.find_unused_code {
        return;
    }
    let prop_id = analyzer.interner.intern(prop_name);
    let lookup_types = expand_intersection_lookup_types(&expand_template_object_union(obj_type));
    for atomic in &lookup_types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        let Some(class_info) = analyzer.codebase.get_class(*name) else {
            continue;
        };
        if let Some(prop_info) = class_info.properties.get(&prop_id) {
            analysis_data
                .referenced_properties
                .insert((prop_info.declaring_class, prop_id));
            analysis_data.add_class_member_reference(
                &context.function_context,
                (prop_info.declaring_class, prop_id),
                false,
            );
        }
    }
}

pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &PropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    in_assignment: bool,
) {
    // Analyze the object expression
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let obj_type = analysis_data.expr_types.get(&obj_pos).cloned();

    // Get the property name
    // Dynamic property selectors (`$a->$k`) consume their expression
    // (Hakana analyzes the whole fetch under inside_general_use).
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    // A dynamic selector whose type is a single literal string names the
    // property directly (Psalm's `$stmt_name_type->isSingleStringLiteral()`).
    let mut dynamic_prop_name: Option<String> = None;
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        ClassLikeMemberSelector::Variable(var) => {
            let var_pos = expression_analyzer::analyze(
                analyzer,
                &Expression::Variable(var.clone()),
                analysis_data,
                context,
            );
            dynamic_prop_name = get_single_literal_string(analysis_data, var_pos);
            None
        }
        ClassLikeMemberSelector::Expression(expr) => {
            let expr_pos =
                expression_analyzer::analyze(analyzer, expr.expression, analysis_data, context);
            dynamic_prop_name = get_single_literal_string(analysis_data, expr_pos);
            None
        }
    };
    let prop_name = prop_name.or(dynamic_prop_name.as_deref());
    context.inside_general_use = was_inside_general_use;

    // Check if this is $this->prop
    let is_this_fetch = matches!(
        access.object,
        Expression::Variable(Variable::Direct(v)) if v.name == "$this"
    );

    if let Some(prop_name) = prop_name {
        if let Some(keyed_type) = get_reconciled_property_type(context, access.object, prop_name) {
            // The reconciled type is served straight from in-scope state (a
            // prior assignment or narrowing), short-circuiting the property
            // lookup below that normally records the read. Record it here too so
            // a property that is written-then-read (or read after an isset/
            // instanceof narrowing) is not wrongly reported as unused.
            if !in_assignment && !context.inside_array_append_root {
                if let Some(obj_t) = &obj_type {
                    record_property_read_for_unused(
                        analyzer,
                        obj_t,
                        prop_name,
                        analysis_data,
                        context,
                    );
                }
            }
            analysis_data.expr_types.insert(pos, Rc::new(keyed_type));
            return;
        }
    }

    // While re-analysing a constructor to collect initialisations, a read of
    // `$this->prop` that is not yet in scope is a read of an uninitialised
    // property (Psalm's InstancePropertyFetchAnalyzer: `!$stmt_type->initialized
    // && collect_initializations && getMethodName() === '__construct'`). It is
    // recorded for check_property_initialization to surface as
    // UninitializedProperty — only inside the constructor body itself, not the
    // helper methods it follows, and not inside isset()/unset().
    if context.collect_initializations
        && is_this_fetch
        && !in_assignment
        && !context.inside_isset
        && !context.inside_unset
        && analyzer
            .function_info
            .is_some_and(|info| info.name == pzoom_str::StrId::CONSTRUCT)
        && let Some(prop_name) = prop_name
    {
        let property_key = format!("$this->{}", prop_name);
        // Uninitialised only when the property has been assigned on *no* path so
        // far: an assignment on some earlier branch (recorded in
        // `vars_possibly_in_scope`) means a later read may be legitimately guarded
        // (an `isset`/`instanceof`), mirroring the previous scan-time
        // `may_assigned` suppression.
        if !context.locals.contains_key(property_key.as_str())
            && !context
                .vars_possibly_in_scope
                .contains(property_key.as_str())
        {
            let property_id = analyzer.interner.intern(prop_name);
            if !analysis_data
                .collected_uninitialized_reads
                .iter()
                .any(|(existing, _)| *existing == property_id)
            {
                analysis_data
                    .collected_uninitialized_reads
                    .push((property_id, pos.0));
            }
        }
    }

    // Psalm records NO type for an undefined-property fetch
    // (handleNonExistentProperty leaves the node untyped), so a chained
    // fetch on it stays silent. pzoom types the failed fetch `mixed` and
    // marks the position; suppress the chained report here, propagating
    // the marker so the whole chain stays untyped-like.
    if analysis_data
        .failed_property_fetch_positions
        .contains(&obj_pos)
    {
        analysis_data.failed_property_fetch_positions.insert(pos);
        analysis_data
            .expr_types
            .insert(pos, Rc::new(TUnion::mixed()));
        return;
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
            // PHP's ?-> short-circuits the rest of the chain: the null from an
            // upstream nullsafe never reaches this fetch (Psalm's
            // MethodCallAnalyzer::hasNullsafe gate).
            context.inside_isset
                || crate::expr::call::method_call_analyzer::has_nullsafe(access.object),
            context.has_this,
            context,
            false,
        ) {
            let name_span = access.property.span();
            let prop_type = attach_property_fetch_dataflow(
                analyzer,
                access.object,
                (name_span.start.offset, name_span.end.offset),
                &obj_t,
                prop_name,
                analysis_data,
                in_assignment,
                prop_type,
            );
            // Psalm's InstancePropertyFetchAnalyzer records the fetched type
            // in scope (`$context->vars_in_scope[$var_id] = $stmt_type`), so
            // assertions on the property path (isset etc.) narrow the actual
            // fetch type instead of a storage-derived reconstruction.
            store_property_fetch_in_scope(context, access.object, prop_name, &prop_type);
            analysis_data.expr_types.insert(pos, Rc::new(prop_type));
            return;
        }
        store_property_fetch_in_scope(context, access.object, prop_name, &TUnion::mixed());

        // A failed lookup on a known object type leaves the node effectively
        // untyped in Psalm — mark it so chained fetches stay silent. Mixed
        // receivers stay unmarked (Psalm types those fetches `mixed`, and a
        // chained fetch reports MixedPropertyFetch again).
        if obj_t.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TNamedObject { .. } | TAtomic::TObjectIntersection { .. }
            )
        }) {
            analysis_data.failed_property_fetch_positions.insert(pos);
        }
    }

    // Fall back to mixed
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::mixed()));
}

/// A dynamic property selector resolves to a concrete name when its type is
/// a single literal string (Psalm's `getSingleStringLiteral`).
fn get_single_literal_string(analysis_data: &FunctionAnalysisData, pos: Pos) -> Option<String> {
    let expr_type = analysis_data.expr_types.get(&pos).cloned()?;
    if expr_type.types.len() != 1 {
        return None;
    }
    match &expr_type.types[0] {
        TAtomic::TLiteralString { value } => Some(value.clone()),
        _ => None,
    }
}

/// Psalm's `$context->vars_in_scope[$var_id] = $stmt_type` after a property
/// fetch: the path becomes a tracked scope entry when the receiver has a
/// stable var key.
fn store_property_fetch_in_scope(
    context: &mut BlockContext,
    object_expr: &Expression<'_>,
    prop_name: &str,
    prop_type: &TUnion,
) {
    // While re-analysing a constructor to collect initialisations, a property
    // *read* must not seed `$this->prop` into scope — otherwise reading an
    // uninitialised property would make it look assigned (pzoom has no separate
    // `initialized` flag, so it keys the check on scope presence).
    if context.collect_initializations {
        return;
    }
    if let Some(object_key) = expression_identifier::get_expression_var_key(object_expr) {
        context.locals.insert(
            pzoom_code_info::VarName::from(format!("{}->{}", object_key, prop_name)),
            prop_type.clone(),
        );
    }
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
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let obj_pos = expression_analyzer::analyze(analyzer, access.object, analysis_data, context);
    context.inside_general_use = was_inside_general_use;
    let obj_type = analysis_data.expr_types.get(&obj_pos).cloned();

    // Get the property name
    let prop_name = match &access.property {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    };

    if let Some(prop_name) = prop_name {
        if let Some(mut keyed_type) =
            get_reconciled_property_type(context, access.object, prop_name)
        {
            if obj_type.is_some_and(|obj_t| obj_t.is_nullable()) {
                keyed_type.add_type(TAtomic::TNull);
            }
            analysis_data.expr_types.insert(pos, Rc::new(keyed_type));
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
            false,
        ) {
            // If the object could be null, the result could be null
            if obj_t.is_nullable() {
                prop_type.add_type(TAtomic::TNull);
            }
            let name_span = access.property.span();
            let prop_type = attach_property_fetch_dataflow(
                analyzer,
                access.object,
                (name_span.start.offset, name_span.end.offset),
                &obj_t,
                prop_name,
                analysis_data,
                false,
                prop_type,
            );
            analysis_data.expr_types.insert(pos, Rc::new(prop_type));
            return;
        }
    }

    // Fall back to mixed|null
    let mut result = TUnion::mixed();
    result.add_type(TAtomic::TNull);
    analysis_data.expr_types.insert(pos, Rc::new(result));
}
