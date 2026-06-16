//! Class constant fetch analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::ClassConstantAccess;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::class_like_info::{ClassConstantInfo, Visibility};
use pzoom_code_info::VarName;
use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::object_type_comparator;
use std::rc::Rc;

/// Analyze a class constant access expression (Foo::BAR).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    access: &ClassConstantAccess<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the class expression. Psalm's ClassConstAnalyzer sets
    // inside_general_use around it, so a variable receiver
    // (`$issue_class::SHORTCODE`) counts as a use of that variable.
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let _class_pos = expression_analyzer::analyze(analyzer, access.class, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    // Try to get the resolved class ID
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
                analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
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
                analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
                return;
            }
            _ => {}
        }

        // Psalm's ClassConstAnalyzer: a variable receiver whose single type
        // is a plain string (not a class-string) cannot name a class.
        if matches!(
            access.class.unparenthesized(),
            Expression::Variable(_)
        ) && let Some(receiver_type) = analysis_data.expr_types.get(&_class_pos).cloned()
            && receiver_type.types.len() == 1
            && matches!(
                receiver_type.types[0],
                TAtomic::TString
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TNonEmptyString
                    | TAtomic::TNumericString
                    | TAtomic::TLowercaseString
            )
        {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidStringClass,
                "String cannot be used as a class",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    let classlike_name = class_id.map(|id| analyzer.interner.lookup(id));

    // A constant fetch (incl. `Foo::class`) references the class for
    // find_unused_code (Psalm records it at scan time).
    if analyzer.config.find_unused_code
        && let Some(class_id) = class_id
        && context.self_class != Some(class_id)
    {
        analysis_data.referenced_classes.insert(class_id);
        analysis_data.add_symbol_reference(&context.function_context, class_id, false);
    }

    // Get the constant name
    let const_name = match &access.constant {
        ClassLikeConstantSelector::Identifier(id) => Some(id.value),
        ClassLikeConstantSelector::Expression(expr) => {
            // Dynamic constant selectors (`static::{$var}`, `E::{$var}`)
            // consume their inner expression (general use).
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;
            let _ =
                expression_analyzer::analyze(analyzer, expr.expression, analysis_data, context);
            context.inside_general_use = was_inside_general_use;
            // Psalm's ClassConstAnalyzer: dynamic class-constant fetch is a
            // PHP 8.3 feature.
            if analyzer.config.php_version_id() < 80300 {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ParseError,
                    "Dynamically fetching class constants and enums requires PHP 8.3",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            None
        }
    };

    // Handle ::class pseudo-constant
    if let Some(const_name) = const_name {
        if const_name.eq_ignore_ascii_case("class") {
            // `$obj::class` (expression receiver): Psalm's ClassConstAnalyzer
            // types it `class-string<T>` from the receiver object's type.
            if class_id.is_none()
                && let Some(receiver_type) = analysis_data.expr_types.get(&_class_pos).cloned()
            {
                let class_strings: Vec<TAtomic> = receiver_type
                    .types
                    .iter()
                    .filter_map(|atomic| match atomic {
                        // `$obj::class` on a template object `T` is `class-string<T>`.
                        TAtomic::TNamedObject { .. } | TAtomic::TTemplateParam { .. } => {
                            Some(TAtomic::TClassString {
                                as_type: Some(Box::new(atomic.clone())),
                            })
                        }
                        TAtomic::TObject => Some(TAtomic::TClassString { as_type: None }),
                        _ => None,
                    })
                    .collect();
                if !class_strings.is_empty() {
                    analysis_data.expr_types.insert(pos, Rc::new(TUnion::from_types(class_strings)));
                    return;
                }
            }

            if matches!(access.class.unparenthesized(), Expression::Static(_)) {
                analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TClassString {
                        as_type: Some(Box::new(TAtomic::TNamedObject {
                            name: StrId::STATIC,
                            type_params: None,
                        is_static: false, remapped_params: false })),
                    })));
                return;
            }

            if let (Some(class_name), Some(class_id)) = (classlike_name.as_ref(), class_id) {
                if analyzer.codebase.get_class(class_id).is_none()
                    && !context.inside_class_exists
                    && matches!(access.class.unparenthesized(), Expression::Identifier(_))
                    && !is_class_guarded_by_exists(context, analyzer, class_id)
                    && !is_known_class_alias(context, analyzer, class_id)
                {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UndefinedClass,
                        crate::class_casing::undefined_class_message(analyzer, class_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                if let Some(class_info) = analyzer.codebase.get_class(class_id) {
                    let (line, col) = analyzer.get_line_column(pos.0);

                    if class_info.is_deprecated
                        && analyzer
                            .get_declaring_class()
                            .is_none_or(|declaring_class| declaring_class != class_id)
                    {
                        analysis_data.add_issue(Issue::new(
                            IssueKind::DeprecatedClass,
                            format!("Class {} is deprecated", class_name),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }

                    if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                        let scope_phrase =
                            format_internal_scope_phrase(analyzer, &class_info.internal);
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
                }

                // Psalm's ClassConstAnalyzer types a resolved `Foo::class` as a
                // literal class-string (`Type::getLiteralClassString`); only
                // `static::class` stays a constrained `class-string<...>`.
                analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TLiteralClassString {
                        name: class_name.to_string(),
                    })));
                return;
            }
        }
    }

    // Try to look up class constant type
    if let (Some(class_id), Some(class_name), Some(const_name)) =
        (class_id, classlike_name, const_name)
    {
        let const_id = analyzer.interner.intern(const_name);

        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            let (line, col) = analyzer.get_line_column(pos.0);

            if class_info.is_deprecated
                && analyzer
                    .get_declaring_class()
                    .is_none_or(|declaring_class| declaring_class != class_id)
            {
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedClass,
                    format!("Class {} is deprecated", class_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if !can_access_internal(analyzer, &class_info.internal, Some(context)) {
                let scope_phrase = format_internal_scope_phrase(analyzer, &class_info.internal);
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

            // Look for constant in class hierarchy (class, parents, interfaces)
            if let Some(const_info) = find_constant_in_hierarchy(analyzer, class_id, const_id) {
                // Check visibility
                if const_info.visibility == Visibility::Private {
                    let is_same_class = analyzer
                        .get_declaring_class()
                        .is_some_and(|calling_class| calling_class == const_info.declaring_class);

                    if !is_same_class {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InaccessibleClassConstant,
                            format!(
                                "Cannot access private constant {}::{}",
                                class_name, const_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                } else if const_info.visibility == Visibility::Protected {
                    // Psalm's ClassConstAnalyzer grants protected visibility
                    // when the calling class extends the FETCHED class or
                    // vice versa (`classExtends` in both directions).
                    let can_access = analyzer.get_declaring_class().is_some_and(|calling_class| {
                        calling_class == class_id
                            || object_type_comparator::is_class_subtype_of(
                                calling_class,
                                class_id,
                                analyzer.codebase,
                            )
                            || object_type_comparator::is_class_subtype_of(
                                class_id,
                                calling_class,
                                analyzer.codebase,
                            )
                    });

                    if !can_access {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InaccessibleClassConstant,
                            format!(
                                "Cannot access protected constant {}::{}",
                                class_name, const_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }

                // Check for deprecated constants
                if const_info.is_deprecated {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::DeprecatedConstant,
                        format!("Constant {}::{} is deprecated", class_name, const_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }

                // Return the constant's type
                analysis_data.expr_types.insert(pos, Rc::new(const_info.constant_type.clone()));
                return;
            } else {
                // Constant not found in class hierarchy
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedConstant,
                    format!("Constant {}::{} does not exist", class_name, const_name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
            // Silence unused variable warning
            let _ = class_info;
        } else {
            // Class not found
            if !is_class_guarded_by_exists(context, analyzer, class_id)
                && !is_known_class_alias(context, analyzer, class_id)
            {
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
    }

    // Fall back to mixed
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
}

/// Get the resolved class ID from an expression using resolved_names.
fn get_resolved_class_id(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<StrId> {
    let class_id = match expr {
        Expression::Identifier(id) => {
            let offset = id.span().start.offset;
            let mut resolved = analyzer
                .get_resolved_name(offset)
                .or_else(|| Some(analyzer.interner.intern(id.value())))?;

            if analyzer.codebase.get_class(resolved).is_none()
                && id.value().eq_ignore_ascii_case("Attribute")
            {
                resolved = StrId::ATTRIBUTE;
            }

            Some(resolved)
        }
        Expression::Self_(_) | Expression::Static(_) => {
            analyzer.get_declaring_class().or(context.self_class)
        }
        Expression::Parent(_) => analyzer
            .get_declaring_class()
            .or(context.self_class)
            .and_then(|class_id| {
                analyzer
                    .codebase
                    .get_class(class_id)
                    .and_then(|class_info| class_info.parent_class)
            }),
        _ => None,
    }?;

    Some(
        context
            .class_aliases
            .get(&class_id)
            .copied()
            .filter(|alias_target| analyzer.codebase.get_class(*alias_target).is_some())
            .unwrap_or(class_id),
    )
}

/// Find a constant in a class's hierarchy (class, parent classes, interfaces).
fn find_constant_in_hierarchy<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    class_id: StrId,
    const_id: StrId,
) -> Option<&'a ClassConstantInfo> {
    // Check the class itself
    if let Some(class_info) = analyzer.codebase.get_class(class_id) {
        if let Some(const_info) = class_info.constants.get(&const_id) {
            return Some(const_info);
        }

        // Check parent class
        if let Some(parent_id) = class_info.parent_class {
            if let Some(const_info) = find_constant_in_hierarchy(analyzer, parent_id, const_id) {
                return Some(const_info);
            }
        }

        // Check interfaces
        for iface_id in &class_info.interfaces {
            if let Some(const_info) = find_constant_in_hierarchy(analyzer, *iface_id, const_id) {
                return Some(const_info);
            }
        }
    }

    None
}

fn is_class_guarded_by_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> bool {
    let class_name = analyzer.interner.lookup(class_id);
    let key = format!(
        "@class_exists({})",
        class_name.trim_start_matches('\\').to_ascii_lowercase()
    );
    let key_id = VarName::new(&key);

    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

fn is_known_class_alias(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> bool {
    let class_name = analyzer
        .interner
        .lookup(class_id)
        .trim_start_matches('\\')
        .to_ascii_lowercase();

    context.class_aliases.keys().any(|alias_id| {
        analyzer
            .interner
            .lookup(*alias_id)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case(class_name.as_str())
    })
}
