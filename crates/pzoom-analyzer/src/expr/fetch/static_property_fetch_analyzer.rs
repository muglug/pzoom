//! Static property fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::StaticPropertyAccess;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::class_like_info::{ClassLikeInfo, Visibility};
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;

use super::atomic_property_fetch_analyzer::get_property_type;

/// Analyze a static property access expression (Foo::$bar).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &StaticPropertyAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression
    let class_pos = expression_analyzer::analyze(analyzer, access.class, analysis_data, context);

    // Psalm: static properties are global mutable state, so reading one from a
    // `@psalm-pure` context is impure.
    emit_impure_static_property(analyzer, pos, analysis_data);

    // Get the property name from the Variable
    let prop_name = match &access.property {
        Variable::Direct(direct) => Some(direct.name.trim_start_matches('$')),
        _ => None,
    };
    let Some(prop_name) = prop_name else {
        analysis_data.set_expr_type(pos, TUnion::mixed());
        return;
    };

    // When the class is given by an expression rather than a name
    // (`$obj::$prop`, `$class::$prop`), Psalm reroutes through the analyzed
    // type of that expression (StaticPropertyFetchAnalyzer::
    // analyzeVariableStaticPropertyFetch): class-string targets resolve to a
    // static fetch, other (object) receivers route through the instance fetch.
    if !is_class_name_expression(access.class) {
        analyze_variable_static_property_fetch(
            analyzer,
            class_pos,
            prop_name,
            pos,
            analysis_data,
            context,
        );
        return;
    }

    // Try to get the resolved class ID from the name.
    let class_id = get_resolved_class_id(analyzer, access.class, context);

    if class_id.is_none() {
        match access.class.unparenthesized() {
            Expression::Parent(_) => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ParentNotFound,
                    "Cannot access parent as this class does not extend another",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, TUnion::mixed());
                return;
            }
            Expression::Self_(_) | Expression::Static(_) => {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::NonStaticSelfCall,
                    "Cannot use self/static in a non-class context",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
                analysis_data.set_expr_type(pos, TUnion::mixed());
                return;
            }
            _ => {}
        }
    }

    let property_type = class_id
        .and_then(|class_id| {
            fetch_static_property(analyzer, class_id, prop_name, pos, analysis_data, context)
        })
        .unwrap_or_else(TUnion::mixed);
    analysis_data.set_expr_type(pos, property_type);
}

/// Whether the class portion of a static fetch is a class *name* (`Foo::`,
/// `self::`, `static::`, `parent::`) as opposed to an arbitrary expression.
/// Emit `ImpureStaticProperty` when a static property is accessed from a mutation-free
/// context. Mirrors Psalm `StaticPropertyFetchAnalyzer`, which gates on
/// `$context->mutation_free`. Static properties are global mutable state, so both reads
/// and writes are impure regardless of the declaring class.
pub(crate) fn emit_impure_static_property(
    analyzer: &StatementsAnalyzer<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    if !crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer) {
        return;
    }

    let (line, col) = analyzer.get_line_column(pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::ImpureStaticProperty,
        "Cannot use a static property in a mutation-free context",
        analyzer.file_path,
        pos.0,
        pos.1,
        line,
        col,
    ));
}

fn is_class_name_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Identifier(_)
            | Expression::Self_(_)
            | Expression::Static(_)
            | Expression::Parent(_)
    )
}

/// Look up the type of a static property on a concretely-resolved class,
/// emitting the relevant issues (undefined class/property, @internal,
/// non-static access, visibility, deprecation). Returns the expanded property
/// type when one is declared, otherwise `None` (the caller falls back to mixed).
fn fetch_static_property(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let class_name = analyzer.interner.lookup(class_id);
    let prop_id = analyzer.interner.intern(prop_name);

    let Some(class_info) = analyzer.codebase.get_class(class_id) else {
        // Class not found
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedClass,
            format!("Class {} does not exist", class_name),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
        return None;
    };

    if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
        let scope_phrase = format_internal_scope_phrase(analyzer, &class_info.internal);
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InternalClass,
            format!("{} is internal to {}", class_name, scope_phrase),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let Some(prop_info) = class_info.properties.get(&prop_id) else {
        // Property not found
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
        return None;
    };

    if !can_access_internal(analyzer, &prop_info.internal, Some(context)) {
        let scope_phrase = format_internal_scope_phrase(analyzer, &prop_info.internal);
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

    // Check that property is static
    if !prop_info.is_static {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidStaticPropertyFetch,
            format!(
                "Cannot access non-static property {}::${} statically",
                class_name, prop_name
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let visibility_scope_class_id = get_property_visibility_scope_class_id(class_info, prop_id);

    match prop_info.visibility {
        Visibility::Public => {}
        Visibility::Private => {
            let is_same_class = analyzer
                .get_declaring_class()
                .is_some_and(|calling_class| calling_class == visibility_scope_class_id);

            if !is_same_class {
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

            if !can_access {
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

    // Return the property's type, resolving `self`/`static`/`parent` against
    // the class that DECLARES the property (Psalm expands the property type
    // using the declaring class storage, not the fetched class —
    // StaticPropertyFetchAnalyzer.php:388-395).
    let prop_type = prop_info.get_type()?;
    let declaring_class_id = class_info
        .declaring_property_ids
        .get(&prop_id)
        .copied()
        .unwrap_or(class_id);
    let declaring_parent_class = analyzer
        .codebase
        .get_class(declaring_class_id)
        .and_then(|declaring_info| declaring_info.parent_class);
    let mut prop_type = prop_type.clone();
    crate::type_expander::expand_union(
        analyzer.codebase,
        analyzer.interner,
        &mut prop_type,
        &crate::type_expander::TypeExpansionOptions {
            self_class: Some(declaring_class_id),
            static_class_type: crate::type_expander::StaticClassType::Name(declaring_class_id),
            parent_class: declaring_parent_class,
            ..Default::default()
        },
    );
    Some(prop_type)
}

/// Mirror Psalm's `analyzeVariableStaticPropertyFetch`: when the class side of
/// a static property fetch is an arbitrary expression, dispatch on each atomic
/// type of that expression. Class-string targets (`class-string<Foo>`,
/// literal `Foo::class`) resolve to a normal static fetch, while object and
/// other receivers route through the instance property fetch. Results are
/// combined into a single union.
fn analyze_variable_static_property_fetch(
    analyzer: &StatementsAnalyzer<'_>,
    class_pos: Pos,
    prop_name: &str,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let class_type = analysis_data
        .get_expr_type(class_pos)
        .map(|rc| (*rc).clone())
        .unwrap_or_else(TUnion::mixed);

    let mut result_type: Option<TUnion> = None;
    let mut object_atomics: Vec<TAtomic> = Vec::new();

    for atomic in &class_type.types {
        let target_class_id = match atomic {
            TAtomic::TLiteralClassString { name } => Some(analyzer.interner.intern(name.as_str())),
            TAtomic::TClassString {
                as_type: Some(inner),
            } => match inner.as_ref() {
                TAtomic::TNamedObject { name, .. } => Some(*name),
                _ => None,
            },
            _ => None,
        };

        if let Some(target_class_id) = target_class_id {
            let fetched =
                fetch_static_property(analyzer, target_class_id, prop_name, pos, analysis_data, context)
                    .unwrap_or_else(TUnion::mixed);
            result_type = Some(match result_type {
                Some(existing) => combine_union_types(&existing, &fetched, false),
                None => fetched,
            });
        } else {
            // Object (or other) receiver — route through the instance fetch.
            object_atomics.push(atomic.clone());
        }
    }

    if !object_atomics.is_empty() {
        let object_union = TUnion::from_types(object_atomics);
        let fetched = get_property_type(
            analyzer,
            &object_union,
            prop_name,
            pos,
            analysis_data,
            false,
            context.inside_isset,
            context.has_this,
            context,
        )
        .unwrap_or_else(TUnion::mixed);
        result_type = Some(match result_type {
            Some(existing) => combine_union_types(&existing, &fetched, false),
            None => fetched,
        });
    }

    analysis_data.set_expr_type(pos, result_type.unwrap_or_else(TUnion::mixed));
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    let resolve_alias = |class_id| {
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .filter(|alias_target| analyzer.codebase.get_class(*alias_target).is_some())
            .unwrap_or(class_id)
    };

    let class_id = match expr.unparenthesized() {
        Expression::Identifier(id) => {
            let value = id.value();
            if value.eq_ignore_ascii_case("self") || value.eq_ignore_ascii_case("static") {
                analyzer.get_declaring_class()
            } else if value.eq_ignore_ascii_case("parent") {
                analyzer.get_declaring_class().and_then(|declaring_class| {
                    analyzer
                        .codebase
                        .get_class(declaring_class)
                        .and_then(|class_info| class_info.parent_class)
                })
            } else {
                let offset = id.span().start.offset;
                analyzer
                    .get_resolved_name(offset)
                    .or_else(|| Some(analyzer.interner.intern(value)))
            }
        }
        Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
        Expression::Parent(_) => analyzer.get_declaring_class().and_then(|declaring_class| {
            analyzer
                .codebase
                .get_class(declaring_class)
                .and_then(|class_info| class_info.parent_class)
        }),
        _ => None,
    }?;

    Some(resolve_alias(class_id))
}

fn get_property_visibility_scope_class_id(class_info: &ClassLikeInfo, prop_id: StrId) -> StrId {
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
