//! Atomic property fetch analyzer - handles property lookups on specific types.

use pzoom_code_info::VarName;
use pzoom_code_info::class_like_info::Visibility;
use pzoom_code_info::{DataFlowNode, Issue, IssueKind, PathKind, TUnion, VarId};

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

/// Hakana `atomic_property_fetch_analyzer::add_property_dataflow`. A
/// `@psalm-taint-specialize` class's instances track property taints through
/// the receiver variable's own dataflow (per-instance); other classes read
/// the global `Class::$prop` property node.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_property_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    lhs_pos: Option<Pos>,
    lhs_parent_nodes: &[DataFlowNode],
    name_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    mut stmt_type: TUnion,
    in_assignment: bool,
    property_id: (StrId, StrId),
    declaring_property_class: StrId,
    lhs_var_id: Option<&str>,
) -> TUnion {
    let specialize_instance = analyzer
        .codebase
        .get_class(property_id.0)
        .is_some_and(|class_info| class_info.specialize_instance);

    if specialize_instance {
        if let (Some(lhs_var_id), Some(lhs_pos)) = (lhs_var_id, lhs_pos) {
            let var_id = VarName::new(lhs_var_id);
            let var_node = DataFlowNode::get_for_lvar(
                VarId(analyzer.interner.intern(&var_id)),
                make_data_flow_node_position(analyzer, lhs_pos),
            );
            let property_node = DataFlowNode::get_for_local_property_fetch(
                VarId(analyzer.interner.intern(&var_id)),
                property_id.1,
                make_data_flow_node_position(analyzer, name_pos),
            );

            analysis_data.data_flow_graph.add_node(var_node.clone());
            analysis_data
                .data_flow_graph
                .add_node(property_node.clone());
            analysis_data.data_flow_graph.add_path(
                &var_node.id,
                &property_node.id,
                PathKind::PropertyFetch(property_id.0, property_id.1),
                vec![],
                vec![],
            );

            for parent_node in lhs_parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent_node.id,
                    &var_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }

            stmt_type.parent_nodes.push(property_node);
        }
    } else if let Some(lhs_var_id) = lhs_var_id {
        let var_id = VarName::new(lhs_var_id);
        stmt_type = add_unspecialized_property_fetch_dataflow(
            DataFlowNode::get_for_local_property_fetch(
                VarId(analyzer.interner.intern(&var_id)),
                property_id.1,
                make_data_flow_node_position(analyzer, name_pos),
            ),
            property_id,
            analysis_data,
            in_assignment,
            stmt_type,
        );
    }

    let localized_property_node = DataFlowNode::get_for_localized_property(
        (declaring_property_class, property_id.1),
        make_data_flow_node_position(analyzer, name_pos),
    );

    analysis_data
        .data_flow_graph
        .add_node(localized_property_node.clone());

    stmt_type.parent_nodes.push(localized_property_node);

    stmt_type
}

/// Hakana `add_unspecialized_property_fetch_dataflow`.
pub(crate) fn add_unspecialized_property_fetch_dataflow(
    localized_property_node: DataFlowNode,
    property_id: (StrId, StrId),
    analysis_data: &mut FunctionAnalysisData,
    in_assignment: bool,
    mut stmt_type: TUnion,
) -> TUnion {
    analysis_data
        .data_flow_graph
        .add_node(localized_property_node.clone());

    let property_node = DataFlowNode::get_for_property(property_id);

    if in_assignment {
        analysis_data.data_flow_graph.add_path(
            &property_node.id,
            &localized_property_node.id,
            PathKind::PropertyAssignment(property_id.0, property_id.1),
            vec![],
            vec![],
        );
    } else {
        analysis_data.data_flow_graph.add_path(
            &property_node.id,
            &localized_property_node.id,
            PathKind::PropertyFetch(property_id.0, property_id.1),
            vec![],
            vec![],
        );
    }

    stmt_type.parent_nodes = vec![localized_property_node];

    stmt_type
}

/// Analyze a property fetch on a known class type.
pub fn analyze_property(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
    _in_assignment: bool,
) -> Option<TUnion> {
    let class_id = analyzer.interner.intern(class_name);
    let prop_id = analyzer.interner.intern(prop_name);

    // Look up the class in the codebase
    if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        // Look up the property
        if let Some(prop_info) = class_info.properties.get(&prop_id) {
            // Reads mark the property used (Psalm records the reference for
            // find_unused_code; writes don't count).
            if analyzer.config.find_unused_code
                && !_in_assignment
                && !_context.inside_array_append_root
            {
                analysis_data
                    .referenced_properties
                    .insert((prop_info.declaring_class, prop_id));
                analysis_data.add_class_member_reference(
                    &_context.function_context,
                    (prop_info.declaring_class, prop_id),
                    false,
                );
            }
            // Visibility, scoped to the class that declares the property (matches Psalm):
            // - private: only the declaring class itself;
            // - protected: the declaring class and any class in its hierarchy.
            let calling_class = analyzer.get_declaring_class();
            let declaring_class = prop_info.declaring_class;
            let inaccessible_kind = match prop_info.visibility {
                Visibility::Public => None,
                Visibility::Private => {
                    let accessible =
                        calling_class.is_some_and(|calling| calling == declaring_class);
                    (!accessible).then_some("private")
                }
                Visibility::Protected => {
                    let accessible = calling_class.is_some_and(|calling| {
                        calling == declaring_class
                            || object_type_comparator::is_class_subtype_of(
                                calling,
                                declaring_class,
                                analyzer.codebase,
                            )
                            || object_type_comparator::is_class_subtype_of(
                                declaring_class,
                                calling,
                                analyzer.codebase,
                            )
                    });
                    (!accessible).then_some("protected")
                }
            };

            if let Some(visibility_word) = inaccessible_kind {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InaccessibleProperty,
                    format!(
                        "Cannot access {} property {}::${}",
                        visibility_word, class_name, prop_name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Check for deprecated properties
            if prop_info.is_deprecated {
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

            // Return the property's declared type, resolving `self`/`static` to the
            // fetched class (Psalm expands the property type at the use site).
            // An untyped redeclaration falls back to the overridden ancestor
            // property's type (Psalm's Properties::getPropertyType
            // overridden_property_ids loop).
            let own_or_overridden_type = prop_info
                .get_type()
                .cloned()
                .or_else(|| get_overridden_property_type(analyzer.codebase, class_id, prop_id));
            return own_or_overridden_type.map(|mut property_type| {
                crate::type_expander::expand_union(
                    analyzer.codebase,
                    analyzer.interner,
                    &mut property_type,
                    &crate::type_expander::TypeExpansionOptions {
                        self_class: Some(class_id),
                        static_class_type: crate::type_expander::StaticClassType::Name(class_id),
                        ..Default::default()
                    },
                );
                property_type
            });
        } else if let Some(mixin_property_type) =
            get_mixin_property_type(analyzer, class_info, prop_id)
        {
            // A `@mixin` class contributes its declared and `@property`
            // (pseudo) members, which take precedence over the class's own
            // `__get` (Psalm merges the mixin's members onto the class).
            return Some(mixin_property_type);
        } else {
            // Property not found - fall back to magic __get return type when available.
            let magic_get_id = StrId::GET;
            if let Some(magic_get_info) = class_info.methods.get(&magic_get_id) {
                if let Some(return_type) = magic_get_info
                    .return_type
                    .as_ref()
                    .or(magic_get_info.signature_return_type.as_ref())
                {
                    return Some(return_type.clone());
                }
            } else {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedProperty,
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

    // Couldn't determine property type - return None to fall back to mixed
    None
}

/// The type of `prop_id` as contributed by one of `class_info`'s `@mixin`
/// classes — its declared property type, else its `@property` (pseudo) type.
fn get_mixin_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    prop_id: StrId,
) -> Option<TUnion> {
    for mixin in &class_info.named_mixins {
        let TAtomic::TNamedObject {
            name: mixin_name, ..
        } = mixin
        else {
            continue;
        };
        let Some(mixin_info) = analyzer.codebase.get_class(*mixin_name) else {
            continue;
        };
        if let Some(prop_info) = mixin_info.properties.get(&prop_id)
            && let Some(prop_type) = prop_info.get_type()
        {
            return Some(prop_type.clone());
        }
        if let Some(pseudo_property_type) = mixin_info.pseudo_property_get_types.get(&prop_id) {
            return Some(pseudo_property_type.clone());
        }
    }
    None
}

// ---- per-atomic property fetch (moved from instance_property_fetch_analyzer; Psalm AtomicPropertyFetchAnalyzer) ----
use crate::expr::call::function_call_analyzer;
use crate::expression_identifier;
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use mago_syntax::ast::ast::expression::Expression;
use pzoom_code_info::TAtomic;
use pzoom_code_info::class_like_info::ClassLikeKind;
use pzoom_str::StrId;

pub(crate) fn get_reconciled_property_type(
    context: &BlockContext,
    object_expr: &Expression<'_>,
    prop_name: &str,
) -> Option<TUnion> {
    let object_key = expression_identifier::get_expression_var_key(object_expr)?;
    let property_key = format!("{}->{}", object_key, prop_name);
    context.locals.get(property_key.as_str()).cloned()
}

/// Psalm's AtomicPropertyFetchAnalyzer: a property missing on the receiver
/// class is retargeted to the enclosing class when the receiver extends it and
/// the enclosing class declares the property — e.g. a private property
/// accessed through `$this` after an `instanceof` narrowed it to a subclass
/// (private properties are not inherited into the child's table).
pub(crate) fn retarget_property_class_for_context(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class: pzoom_str::StrId,
    prop_id: pzoom_str::StrId,
) -> pzoom_str::StrId {
    if analyzer
        .codebase
        .get_class(receiver_class)
        .is_some_and(|class_info| class_info.properties.contains_key(&prop_id))
    {
        return receiver_class;
    }

    // A trait method's `$this->prop` resolves against the using class — the
    // property is declared by the class that uses the trait, not the bare
    // trait. Psalm analyses trait bodies per using class; pzoom analyses the
    // trait once with `$this` typed as the trait, so retarget to a using class
    // that declares the property (the readonly / mutation-free diagnostics, and
    // the property type, then resolve as they would in the using class).
    if analyzer
        .codebase
        .get_class(receiver_class)
        .is_some_and(|class_info| class_info.kind == ClassLikeKind::Trait)
        && let Some(user) = trait_user_declaring_property(analyzer, receiver_class, prop_id)
    {
        return user;
    }

    let Some(self_class) = analyzer.get_declaring_class() else {
        return receiver_class;
    };
    if self_class == receiver_class {
        return receiver_class;
    }

    if crate::type_comparator::object_type_comparator::is_class_subtype_of(
        receiver_class,
        self_class,
        analyzer.codebase,
    ) && analyzer
        .codebase
        .get_class(self_class)
        .is_some_and(|class_info| class_info.properties.contains_key(&prop_id))
    {
        return self_class;
    }

    receiver_class
}

/// Whether the code currently being analysed belongs to `target_class` for
/// member-visibility purposes. True when the enclosing class *is* `target_class`,
/// or when it is a trait used by `target_class` — a trait body is analysed with
/// `$this` retargeted to a using class, and the trait's code is part of that
/// class, so it may touch the class's private and protected members.
pub(crate) fn calling_context_owns_class(
    analyzer: &StatementsAnalyzer<'_>,
    target_class: pzoom_str::StrId,
) -> bool {
    let Some(self_class) = analyzer.get_declaring_class() else {
        return false;
    };
    if self_class == target_class {
        return true;
    }
    analyzer
        .codebase
        .get_class(self_class)
        .is_some_and(|info| info.kind == ClassLikeKind::Trait)
        && analyzer
            .codebase
            .all_classlike_descendants
            .get(&self_class)
            .is_some_and(|users| users.contains(&target_class))
}

/// When the calling scope is a trait, returns a using class under which
/// `prop_id` is write-restricted — the declaring class is immutable, or it
/// declares the property readonly. Psalm analyses a trait body once per using
/// class and so emits the readonly violation for the immutable user even when a
/// mutable user shares the trait; pzoom analyses the trait once, so it must look
/// across the users to police the write the same way. Chosen deterministically
/// by name. Returns `None` outside a trait scope or when no user restricts it.
pub(crate) fn trait_restricting_property_owner(
    analyzer: &StatementsAnalyzer<'_>,
    prop_id: pzoom_str::StrId,
) -> Option<pzoom_str::StrId> {
    let self_class = analyzer.get_declaring_class()?;
    if analyzer
        .codebase
        .get_class(self_class)
        .is_none_or(|info| info.kind != ClassLikeKind::Trait)
    {
        return None;
    }
    let users = analyzer.codebase.all_classlike_descendants.get(&self_class)?;
    users
        .iter()
        .filter(|user| {
            analyzer.codebase.get_class(**user).is_some_and(|info| {
                info.properties
                    .get(&prop_id)
                    .is_some_and(|prop| !prop.is_static && (info.is_immutable || prop.is_readonly))
            })
        })
        .min_by(|a, b| {
            analyzer
                .interner
                .lookup(**a)
                .as_ref()
                .cmp(analyzer.interner.lookup(**b).as_ref())
        })
        .copied()
}

/// The using class (or descendant) of `trait_id` that declares `prop_id`, chosen
/// deterministically by name so the retarget is stable run-to-run. `all_classlike_descendants`
/// already records trait users (and their subclasses).
fn trait_user_declaring_property(
    analyzer: &StatementsAnalyzer<'_>,
    trait_id: pzoom_str::StrId,
    prop_id: pzoom_str::StrId,
) -> Option<pzoom_str::StrId> {
    let users = analyzer.codebase.all_classlike_descendants.get(&trait_id)?;
    users
        .iter()
        .filter(|user| {
            analyzer
                .codebase
                .get_class(**user)
                .is_some_and(|user_info| user_info.properties.contains_key(&prop_id))
        })
        .min_by(|a, b| {
            analyzer
                .interner
                .lookup(**a)
                .as_ref()
                .cmp(analyzer.interner.lookup(**b).as_ref())
        })
        .copied()
}

/// Look up the type of a property on a type.
pub(crate) fn get_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    is_this_fetch: bool,
    suppress_null_issues: bool,
    has_this: bool,
    context: &BlockContext,
    is_static_access: bool,
) -> Option<TUnion> {
    // Psalm's InstancePropertyFetchAnalyzer: `@psalm-ignore-nullable-return`
    // receivers suppress the possibly-null fetch issues AND propagate the
    // flag onto the fetched property type.
    let suppress_null_issues = suppress_null_issues || obj_type.ignore_nullable_issues;
    let mut result = get_property_type_inner(
        analyzer,
        obj_type,
        prop_name,
        pos,
        analysis_data,
        is_this_fetch,
        suppress_null_issues,
        has_this,
        context,
        is_static_access,
    )?;
    if obj_type.ignore_nullable_issues {
        result.ignore_nullable_issues = true;
    }
    Some(result)
}

fn get_property_type_inner(
    analyzer: &StatementsAnalyzer<'_>,
    obj_type: &TUnion,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    is_this_fetch: bool,
    suppress_null_issues: bool,
    has_this: bool,
    context: &BlockContext,
    is_static_access: bool,
) -> Option<TUnion> {
    let prop_id = analyzer.interner.intern(prop_name);
    let expanded_obj_type = expand_template_object_union(obj_type);
    let mut lookup_types = expand_intersection_lookup_types(&expanded_obj_type);

    // A known enum case resolves ->name to the literal case name and ->value
    // to the case's backed value (Psalm's handleEnumName/handleEnumValue);
    // other properties fall through to the enum class.
    if lookup_types.len() == 1
        && let TAtomic::TEnumCase {
            enum_name,
            case_name,
        } = &lookup_types[0]
    {
        if prop_id == StrId::NAME {
            return Some(TUnion::new(TAtomic::TLiteralString {
                value: analyzer.interner.lookup(*case_name).to_string(),
            }));
        }
        if prop_id == StrId::VALUE
            && let Some(case_value) = analyzer
                .codebase
                .get_class(*enum_name)
                .and_then(|class_info| class_info.constants.get(case_name))
                .and_then(|const_info| const_info.enum_case_value.clone())
        {
            return Some(case_value);
        }
    }

    for atomic in &mut lookup_types {
        if let TAtomic::TEnumCase { enum_name, .. } = atomic {
            *atomic = TAtomic::TNamedObject {
                name: *enum_name,
                type_params: None,
                is_static: false,
                remapped_params: false,
            };
        }
    }

    // Mirrors Psalm `AtomicPropertyFetchAnalyzer`: reading a property of a mutable object
    // from a pure context (`$context->pure`) is impure. Immutable (external-mutation-free)
    // receivers are exempt; mixed/templated receivers get their own diagnostics.
    if analyzer.function_info.is_some_and(|info| info.is_pure) {
        let reads_mutable_object = lookup_types.iter().any(|atomic| match atomic {
            TAtomic::TNamedObject { name, .. } => analyzer
                .codebase
                .get_class(*name)
                .is_some_and(|class_info| !class_info.is_immutable),
            _ => false,
        });

        if reads_mutable_object {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ImpurePropertyFetch,
                "Cannot access a property on a mutable object from a pure context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Check for null/invalid types in the union
    let has_object_type = lookup_types.iter().any(|t| {
        matches!(
            t,
            TAtomic::TNamedObject { .. } | TAtomic::TObject | TAtomic::TObjectWithProperties { .. }
        )
    });
    let has_null = lookup_types.iter().any(|t| matches!(t, TAtomic::TNull));
    let has_invalid_type = lookup_types.iter().any(|t| {
        !matches!(
            t,
            TAtomic::TNamedObject { .. }
                | TAtomic::TObject
                | TAtomic::TObjectWithProperties { .. }
                | TAtomic::TNull
                | TAtomic::TMixed
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

    // In a trait body `$this` is generic; an impossible `instanceof` narrows it
    // to `never` for a non-matching using class, but that branch is meaningful
    // for the matching one, so Psalm doesn't report a property fetch there (its
    // source-is-trait guard, as for never-receiver method calls).
    if analysis_data.in_trait_body && expanded_obj_type.is_nothing() {
        return None;
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
    // Other union members contribute their own view of the property (a declared
    // property elsewhere, a magic __get return, an object-shape entry) — Psalm's
    // InstancePropertyFetchAnalyzer combines the per-atomic results.
    let mut resolved_property: Option<(pzoom_str::StrId, Visibility, bool, Option<TUnion>)> = None;
    let mut additional_member_types: Vec<TUnion> = Vec::new();

    for atomic in &lookup_types {
        match atomic {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                let Some(class_info) = analyzer.codebase.get_class(*name) else {
                    // Psalm's AtomicPropertyFetchAnalyzer reports a property
                    // fetch on an undefined class (UndefinedDocblockClass when
                    // the receiver type came from a docblock, UndefinedClass
                    // otherwise). The `class_exists()`-guarded case stays silent.
                    if !context.inside_class_exists {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        let class_label = analyzer.interner.lookup(*name);
                        analysis_data.add_issue(Issue::new(
                            if obj_type.from_docblock {
                                IssueKind::UndefinedDocblockClass
                            } else {
                                IssueKind::UndefinedClass
                            },
                            format!("Cannot get properties of undefined class {class_label}"),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                    continue;
                };
                if let Some(prop_info) = class_info
                    .properties
                    .get(&prop_id)
                    // A static property is invisible to instance access
                    // (Psalm treats `$obj->staticProp` as non-existent),
                    // but `$obj::$staticProp` reaches it.
                    .filter(|prop_info| is_static_access || !prop_info.is_static)
                {
                    // Reads mark the property used for find_unused_code (Psalm's
                    // isPropertyReferenced; writes don't count).
                    if analyzer.config.find_unused_code && !context.inside_array_append_root {
                        analysis_data
                            .referenced_properties
                            .insert((prop_info.declaring_class, prop_id));
                        analysis_data.add_class_member_reference(
                            &context.function_context,
                            (prop_info.declaring_class, prop_id),
                            false,
                        );
                    }
                    let property_type =
                        get_pseudo_property_get_type(class_info, type_params.as_deref(), prop_id)
                            .or_else(|| {
                                // An untyped redeclaration inherits the overridden
                                // ancestor property's declared type (Psalm's
                                // Properties::getPropertyType fallback).
                                prop_info
                                    .get_type()
                                    .cloned()
                                    .or_else(|| {
                                        get_overridden_property_type(
                                            analyzer.codebase,
                                            *name,
                                            prop_id,
                                        )
                                    })
                                    .map(|property_type| {
                                        substitute_class_template_params(
                                            class_info,
                                            type_params.as_deref(),
                                            &property_type,
                                        )
                                    })
                            })
                            .map(|mut property_type| {
                                // Psalm's AtomicPropertyFetchAnalyzer expands the stored
                                // property type at the use site against the declaring
                                // class (resolves self/static and class-constant
                                // references like `@var Foo::VISIBILITY_*`).
                                let declaring_class = prop_info.declaring_class;
                                crate::type_expander::expand_union(
                                    analyzer.codebase,
                                    analyzer.interner,
                                    &mut property_type,
                                    &crate::type_expander::TypeExpansionOptions {
                                        self_class: Some(declaring_class),
                                        static_class_type:
                                            crate::type_expander::StaticClassType::Name(*name),
                                        parent_class: analyzer
                                            .codebase
                                            .get_class(declaring_class)
                                            .and_then(|info| info.parent_class),
                                        ..Default::default()
                                    },
                                );
                                property_type
                            });
                    if resolved_property.is_none() {
                        resolved_property = Some((
                            *name,
                            prop_info.visibility,
                            prop_info.is_deprecated,
                            property_type,
                        ));
                    } else if let Some(property_type) = property_type {
                        additional_member_types.push(property_type);
                    }
                } else if !class_has_sealed_properties(class_info) {
                    // A union member without the declared property answers
                    // through its magic __get (or reads as mixed).
                    if let Some(magic_get_return_type) =
                        get_magic_get_return_type(class_info, type_params.as_deref())
                    {
                        additional_member_types.push(magic_get_return_type);
                    } else if class_has_magic_getter(class_info) {
                        additional_member_types.push(TUnion::mixed());
                    }
                }
            }
            // An object-shape intersection/union part that declares the
            // property answers directly (`a&object{test2: "lmao"}`).
            TAtomic::TObjectWithProperties { properties, .. } => {
                let key = pzoom_code_info::ArrayKey::String(prop_name.to_string());
                if let Some((_, prop_type)) = properties.get(&key) {
                    return Some(prop_type.clone());
                }
            }
            _ => {}
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
                let is_same_class =
                    calling_context_owns_class(analyzer, visibility_scope_class_id);

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
        for member_type in additional_member_types {
            final_property_type =
                pzoom_code_info::combine_union_types(&final_property_type, &member_type, false);
        }
        if has_null && has_object_type {
            final_property_type.add_type(TAtomic::TNull);
        }

        return Some(final_property_type);
    }

    for atomic in &lookup_types {
        match atomic {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                // Look up the class
                if let Some(class_info) = analyzer.codebase.get_class(*name) {
                    // Look up the property; a static property is invisible
                    // to instance access (Psalm reports UndefinedPropertyFetch
                    // for `$obj->staticProp`) but visible to `$obj::$prop`.
                    if let Some(prop_info) = class_info
                        .properties
                        .get(&prop_id)
                        .filter(|prop_info| is_static_access || !prop_info.is_static)
                    {
                        let visibility_scope_class_id =
                            get_property_visibility_scope_class_id(class_info, prop_id);

                        match prop_info.visibility {
                            Visibility::Public => {}
                            Visibility::Private => {
                                let is_same_class =
                                    calling_context_owns_class(analyzer, visibility_scope_class_id);

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

                        // Return the property's type. An untyped redeclaration
                        // (`public $a = "foo";` overriding a parent's
                        // `@var string|null`) falls back to the overridden
                        // property's declared type — Psalm's
                        // Properties::getPropertyType overridden_property_ids
                        // loop.
                        let own_or_overridden_type = prop_info.get_type().cloned().or_else(|| {
                            get_overridden_property_type(analyzer.codebase, *name, prop_id)
                        });
                        return own_or_overridden_type.map(|property_type| {
                            substitute_class_template_params(
                                class_info,
                                type_params.as_deref(),
                                &property_type,
                            )
                        });
                    } else {
                        // Psalm reports a property fetch on an interface as
                        // NoInterfaceProperties (handleNonExistentClass), *before*
                        // the @property/__get resolution and regardless of a
                        // __get magic method. Enum interfaces and PHP 8.4
                        // get-hooks (handled below) are the only exemptions.
                        if class_info.kind == ClassLikeKind::Interface {
                            // PHP core enum interfaces have properties: an
                            // interface extending UnitEnum/BackedEnum exposes
                            // the stub-declared $name/$value (Psalm's
                            // is_enum_interface path).
                            if let Some(enum_interface_prop_type) = class_info
                                .interfaces
                                .iter()
                                .filter(|interface_id| {
                                    matches!(
                                        &*analyzer.interner.lookup(**interface_id),
                                        "UnitEnum" | "BackedEnum"
                                    )
                                })
                                .find_map(|interface_id| {
                                    analyzer
                                        .codebase
                                        .get_class(*interface_id)
                                        .and_then(|interface_info| {
                                            interface_info.properties.get(&prop_id)
                                        })
                                        .and_then(|prop_info| prop_info.get_type().cloned())
                                })
                            {
                                return Some(enum_interface_prop_type);
                            }

                            // An intersection with an enum interface allows
                            // the fetch — another part supplies the property
                            // (Psalm's intersects_with_enum).
                            if lookup_types.iter().any(|other| {
                                if other == atomic {
                                    return false;
                                }
                                let TAtomic::TNamedObject {
                                    name: other_name, ..
                                } = other
                                else {
                                    return false;
                                };
                                matches!(
                                    &*analyzer.interner.lookup(*other_name),
                                    "UnitEnum" | "BackedEnum"
                                ) || analyzer.codebase.get_class(*other_name).is_some_and(
                                    |other_info| {
                                        other_info.kind == ClassLikeKind::Enum
                                            || other_info.interfaces.iter().any(|interface_id| {
                                                matches!(
                                                    &*analyzer.interner.lookup(*interface_id),
                                                    "UnitEnum" | "BackedEnum"
                                                )
                                            })
                                    },
                                )
                            }) {
                                continue;
                            }

                            // Psalm returns only when the issue is actually
                            // reported; a *suppressed* NoInterfaceProperties
                            // falls through to the magic-getter resolution below
                            // (so an undeclared property still becomes
                            // UndefinedMagicPropertyFetch) — unless the interface
                            // has no __set, where Psalm stops regardless.
                            if !crate::issue_suppression::is_issue_suppressed_at(
                                analyzer,
                                analysis_data,
                                pos.0,
                                "NoInterfaceProperties",
                            ) {
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
                                return None;
                            }
                            if !class_has_magic_setter(class_info) {
                                return None;
                            }
                        }

                        // A `@mixin` class contributes its declared and
                        // `@property` (pseudo) members, taking precedence over
                        // the class's own `__get` (Psalm merges the mixin's
                        // members onto the class).
                        if let Some(mixin_property_type) =
                            get_mixin_property_type(analyzer, class_info, prop_id)
                        {
                            return Some(mixin_property_type);
                        }

                        if class_has_magic_getter(class_info) {
                            if let Some(pseudo_property_type) = get_pseudo_property_get_type(
                                class_info,
                                type_params.as_deref(),
                                prop_id,
                            ) {
                                return Some(pseudo_property_type);
                            }

                            // Psalm gates the magic-property-missing report on
                            // sealed-ness only; declaring *some* @property
                            // annotations does not seal an unsealed class.
                            if class_has_sealed_properties(class_info)
                                || (!class_info.pseudo_property_get_types.is_empty()
                                    && !class_info.no_seal_properties)
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
                                if let Some(magic_get_return_type) =
                                    get_magic_get_return_type(class_info, type_params.as_deref())
                                {
                                    return Some(magic_get_return_type);
                                }

                                return Some(TUnion::mixed());
                            }

                            continue;
                        }

                        if class_info.no_seal_properties {
                            // Psalm's handleNonExistentProperty
                            // (AtomicPropertyFetchAnalyzer.php): a class with
                            // #[AllowDynamicProperties] (own or inherited)
                            // resolves an undeclared property through its
                            // `@property`/`@property-read` docblock type when
                            // one exists; only otherwise is the fetch dynamic.
                            if let Some(pseudo_property_type) = get_pseudo_property_get_type(
                                class_info,
                                type_params.as_deref(),
                                prop_id,
                            ) {
                                return Some(pseudo_property_type);
                            }
                            return Some(TUnion::mixed());
                        }

                        // isset($obj->undefined) is a legitimate existence
                        // probe — Psalm reports nothing inside isset().
                        if context.inside_isset {
                            continue;
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
            TAtomic::TObjectWithProperties { properties, .. } => {
                // `object{foo: T, ...}` — a known property resolves to its
                // declared type; other properties are allowed (these objects are
                // not sealed), so they read back as `mixed`.
                let key = pzoom_code_info::ArrayKey::String(prop_name.to_string());
                if let Some((_, prop_type)) = properties.get(&key) {
                    return Some(prop_type.clone());
                }
                return Some(TUnion::mixed());
            }
            TAtomic::TMixed => {
                if is_this_fetch && !has_this {
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
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
    prop_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let pseudo_type = class_info.pseudo_property_get_types.get(&prop_id)?;
    Some(substitute_class_template_params(
        class_info,
        type_params,
        pseudo_type,
    ))
}

fn get_magic_get_return_type(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
) -> Option<TUnion> {
    let method_info = class_info.methods.get(&pzoom_str::StrId::GET)?;
    let return_type = method_info.get_return_type()?;

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
            type_params,
        ),
    );

    if crate::template::template_result_is_empty(&template_result) {
        Some(return_type.clone())
    } else {
        Some(function_call_analyzer::replace_templates_in_union(
            return_type,
            &template_result,
        ))
    }
}

fn class_has_magic_getter(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::GET)
}

fn class_has_magic_setter(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.methods.contains_key(&pzoom_str::StrId::SET)
}

pub(crate) fn class_has_sealed_properties(class_info: &pzoom_code_info::ClassLikeInfo) -> bool {
    class_info.sealed_properties.unwrap_or(false) && !class_info.no_seal_properties
}

pub(crate) fn substitute_class_template_params(
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

pub(crate) fn expand_template_object_union(obj_type: &TUnion) -> TUnion {
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

pub(crate) fn expand_intersection_lookup_types(obj_type: &TUnion) -> Vec<TAtomic> {
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

pub(crate) fn receiver_allows_property_visibility_override(
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

/// Psalm's `Properties::getPropertyType` overridden-property fallback: an
/// untyped property redeclaration inherits the overridden (ancestor)
/// property's declared type.
pub(crate) fn get_overridden_property_type(
    codebase: &pzoom_code_info::CodebaseInfo,
    class_id: pzoom_str::StrId,
    prop_id: pzoom_str::StrId,
) -> Option<TUnion> {
    let class_info = codebase.get_class(class_id)?;
    class_info
        .all_parent_classes
        .iter()
        .chain(class_info.interfaces.iter())
        .find_map(|ancestor_id| {
            codebase
                .get_class(*ancestor_id)
                .and_then(|ancestor_info| ancestor_info.properties.get(&prop_id))
                .and_then(|ancestor_prop| ancestor_prop.get_type().cloned())
        })
}

pub(crate) fn get_property_visibility_scope_class_id(
    class_info: &pzoom_code_info::ClassLikeInfo,
    prop_id: StrId,
) -> StrId {
    class_info
        .appearing_property_ids
        .get(&prop_id)
        .copied()
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
