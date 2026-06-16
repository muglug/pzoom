use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::attribute::AttributeList;
use mago_syntax::ast::ast::class_like::Class;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::Method;
use mago_syntax::ast::ast::class_like::property::Property;
use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, Visibility};
use pzoom_code_info::{Issue, IssueKind};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expr::call::argument_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

const ATTR_TARGET_CLASS: u8 = 1;
const ATTR_TARGET_FUNCTION: u8 = 2;
const ATTR_TARGET_METHOD: u8 = 4;
const ATTR_TARGET_PROPERTY: u8 = 8;
const ATTR_TARGET_CLASS_CONSTANT: u8 = 16;
const ATTR_TARGET_PARAMETER: u8 = 32;
const ATTR_IS_REPEATABLE: u8 = 64;

#[derive(Clone, Copy, Debug)]
pub enum AttributeTarget {
    ClassLike,
    Function,
    Method,
    Property,
    Parameter,
    PromotedProperty,
    ClassLikeConstant,
}

impl AttributeTarget {
    fn required_flag(self) -> u8 {
        match self {
            Self::ClassLike => ATTR_TARGET_CLASS,
            Self::Function => ATTR_TARGET_FUNCTION,
            Self::Method => ATTR_TARGET_METHOD,
            Self::Property => ATTR_TARGET_PROPERTY,
            Self::Parameter => ATTR_TARGET_PARAMETER,
            Self::PromotedProperty => ATTR_TARGET_PARAMETER | ATTR_TARGET_PROPERTY,
            Self::ClassLikeConstant => ATTR_TARGET_CLASS_CONSTANT,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ClassLike => "class",
            Self::Function => "function",
            Self::Method => "method",
            Self::Property => "property",
            Self::Parameter => "function/method parameter",
            Self::PromotedProperty => "promoted property",
            Self::ClassLikeConstant => "class constant",
        }
    }
}

pub fn analyze_class_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    class_stmt: &Class<'_>,
    class_name_id: StrId,
    class_info: Option<&ClassLikeInfo>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut attribute_context = context.clone();
    attribute_context.self_class = Some(class_name_id);
    attribute_context.parent_class = class_info.and_then(|info| info.parent_class);

    analyze_attribute_lists(
        analyzer,
        class_stmt.attribute_lists.as_slice(),
        AttributeTarget::ClassLike,
        class_info,
        &mut attribute_context,
        analysis_data,
    );

    // PHP 8.2: `#[AllowDynamicProperties]` on a readonly class is invalid
    // (Psalm's ClassLikeNodeScanner InvalidAttribute).
    if class_info.is_some_and(|info| info.is_readonly) {
        for attribute in class_stmt
            .attribute_lists
            .iter()
            .flat_map(|attribute_list| attribute_list.attributes.iter())
        {
            if attribute
                .name
                .value()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("AllowDynamicProperties")
            {
                let attr_span = attribute.span();
                let (line, col) = analyzer.get_line_column(attr_span.start.offset);
                analysis_data.add_issue(pzoom_code_info::Issue::new(
                    pzoom_code_info::IssueKind::InvalidAttribute,
                    "Readonly classes cannot have dynamic properties",
                    analyzer.file_path,
                    attr_span.start.offset,
                    attr_span.end.offset,
                    line,
                    col,
                ));
            }
        }
    }

    analyze_class_member_attributes(
        analyzer,
        class_stmt.members.as_slice(),
        class_info,
        &attribute_context,
        analysis_data,
    );
}

pub fn analyze_function_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    function_stmt: &Function<'_>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut attribute_context = context.clone();
    analyze_attribute_lists(
        analyzer,
        function_stmt.attribute_lists.as_slice(),
        AttributeTarget::Function,
        None,
        &mut attribute_context,
        analysis_data,
    );

    for param in function_stmt.parameter_list.parameters.iter() {
        let target = if param.is_promoted_property() {
            AttributeTarget::PromotedProperty
        } else {
            AttributeTarget::Parameter
        };

        analyze_attribute_lists(
            analyzer,
            param.attribute_lists.as_slice(),
            target,
            None,
            &mut attribute_context,
            analysis_data,
        );
    }
}

pub fn analyze_interface_or_trait_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    attribute_lists: &[AttributeList<'_>],
    members: &[ClassLikeMember<'_>],
    class_info: Option<&ClassLikeInfo>,
    class_name_id: StrId,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut attribute_context = context.clone();
    attribute_context.self_class = Some(class_name_id);
    attribute_context.parent_class = class_info.and_then(|info| info.parent_class);

    analyze_attribute_lists(
        analyzer,
        attribute_lists,
        AttributeTarget::ClassLike,
        class_info,
        &mut attribute_context,
        analysis_data,
    );

    analyze_class_member_attributes(
        analyzer,
        members,
        class_info,
        &attribute_context,
        analysis_data,
    );
}

pub fn analyze_attribute_lists(
    analyzer: &StatementsAnalyzer<'_>,
    attribute_lists: &[AttributeList<'_>],
    target: AttributeTarget,
    current_class_info: Option<&ClassLikeInfo>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut seen_non_repeatable = FxHashSet::default();

    for attribute in attribute_lists
        .iter()
        .flat_map(|attribute_list| attribute_list.attributes.iter())
    {
        let attr_span = attribute.span();
        let name_span = attribute.name.span();
        let attr_name_offset = name_span.start.offset;

        let suppress_invalid = crate::issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            attr_span.start.offset,
            "InvalidAttribute",
        );
        let suppress_undefined = crate::issue_suppression::is_issue_suppressed_at(
            analyzer,
            analysis_data,
            attr_span.start.offset,
            "UndefinedAttributeClass",
        );

        let mut attribute_name_id = analyzer
            .get_resolved_name(attr_name_offset)
            .unwrap_or_else(|| analyzer.interner.intern(attribute.name.value()));
        if analyzer.codebase.get_class(attribute_name_id).is_none()
            && attribute
                .name
                .value()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("Attribute")
        {
            attribute_name_id = StrId::ATTRIBUTE;
        }
        let attribute_name = analyzer.interner.lookup(attribute_name_id).to_string();

        let is_attribute_attribute = attribute_name
            .rsplit('\\')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Attribute"));

        analyze_attribute_arguments(
            analyzer,
            attribute
                .argument_list
                .as_ref()
                .map(|a| a.arguments.as_slice()),
            context,
            is_attribute_attribute,
            analysis_data,
        );

        let Some(attribute_class_info) = analyzer.codebase.get_class(attribute_name_id) else {
            if !suppress_undefined {
                let (line, col) = analyzer.get_line_column(name_span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedAttributeClass,
                    format!("Attribute class {} not found", attribute.name.value()),
                    analyzer.file_path,
                    name_span.start.offset,
                    name_span.end.offset,
                    line,
                    col,
                ));
            }

            continue;
        };

        if is_attribute_attribute {
            validate_attribute_class_definition(
                analyzer,
                current_class_info,
                name_span.start.offset,
                name_span.end.offset,
                suppress_invalid,
                analysis_data,
            );
        }

        validate_attribute_constructor_call(
            analyzer,
            attribute
                .argument_list
                .as_ref()
                .map(|a| a.arguments.as_slice()),
            attribute_class_info,
            context,
            analysis_data,
        );

        if attribute_class_info.kind != ClassLikeKind::Class {
            if !suppress_invalid {
                emit_invalid_attribute_issue(
                    analyzer,
                    analysis_data,
                    name_span.start.offset,
                    name_span.end.offset,
                    format!(
                        "{} cannot be used as an attribute class",
                        analyzer.interner.lookup(attribute_name_id)
                    ),
                );
            }
            continue;
        }

        let Some(attribute_flags) =
            get_attribute_class_flags(analyzer, attribute_name_id, attribute_class_info)
        else {
            if !suppress_invalid {
                emit_invalid_attribute_issue(
                    analyzer,
                    analysis_data,
                    name_span.start.offset,
                    name_span.end.offset,
                    format!(
                        "The class {} doesn't have the Attribute attribute",
                        analyzer.interner.lookup(attribute_name_id)
                    ),
                );
            }
            continue;
        };

        if (attribute_flags & ATTR_IS_REPEATABLE) == 0
            && !seen_non_repeatable.insert(attribute_name_id)
            && !suppress_invalid
        {
            emit_invalid_attribute_issue(
                analyzer,
                analysis_data,
                name_span.start.offset,
                name_span.end.offset,
                format!(
                    "Attribute {} is not repeatable",
                    analyzer.interner.lookup(attribute_name_id)
                ),
            );
        }

        if (attribute_flags & target.required_flag()) == 0 && !suppress_invalid {
            emit_invalid_attribute_issue(
                analyzer,
                analysis_data,
                name_span.start.offset,
                name_span.end.offset,
                format!(
                    "Attribute {} cannot be used on a {}",
                    analyzer.interner.lookup(attribute_name_id),
                    target.description()
                ),
            );
        }
    }
}

pub fn analyze_reflection_get_attributes_call(
    analyzer: &StatementsAnalyzer<'_>,
    receiver_class_id: StrId,
    method_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    if !method_name.eq_ignore_ascii_case("getAttributes") {
        return;
    }

    let reflection_target = reflection_target_from_class_name(analyzer, receiver_class_id);
    let Some(target) = reflection_target else {
        return;
    };

    if args.len() != 1 {
        return;
    }

    let arg_index = match args[0] {
        Argument::Positional(_) => Some(0),
        Argument::Named(named) if named.name.value.eq_ignore_ascii_case("name") => Some(0),
        _ => None,
    };
    let Some(arg_index) = arg_index else {
        return;
    };

    let Some(arg_pos) = arg_positions.get(arg_index).copied() else {
        return;
    };
    let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned() else {
        return;
    };

    if !arg_type.is_single() {
        return;
    }

    let Some(class_name_id) = extract_single_literal_class_name(analyzer, &arg_type) else {
        return;
    };
    let class_name = analyzer.interner.lookup(class_name_id).to_string();
    let Some(class_info) = analyzer.codebase.get_class(class_name_id) else {
        return;
    };

    if class_info.kind != ClassLikeKind::Class {
        emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            arg_pos.0,
            arg_pos.1,
            format!("{} cannot be used as an attribute class", class_name),
        );
        return;
    }

    let Some(attribute_flags) = get_attribute_class_flags(analyzer, class_name_id, class_info)
    else {
        emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            arg_pos.0,
            arg_pos.1,
            format!(
                "The class {} doesn't have the Attribute attribute",
                class_name
            ),
        );
        return;
    };

    if (attribute_flags & target.required_flag()) == 0 {
        emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            arg_pos.0,
            arg_pos.1,
            format!(
                "Attribute {} cannot be used on a {}",
                class_name,
                target.description()
            ),
        );
    }
}

fn analyze_class_member_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    members: &[ClassLikeMember<'_>],
    class_info: Option<&ClassLikeInfo>,
    class_context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    for member in members {
        match member {
            ClassLikeMember::Constant(constant) => {
                let mut context = class_context.clone();
                analyze_attribute_lists(
                    analyzer,
                    constant.attribute_lists.as_slice(),
                    AttributeTarget::ClassLikeConstant,
                    class_info,
                    &mut context,
                    analysis_data,
                );
            }
            ClassLikeMember::Property(property) => {
                let mut context = class_context.clone();
                let property_attribute_lists = match property {
                    Property::Plain(plain) => plain.attribute_lists.as_slice(),
                    Property::Hooked(hooked) => hooked.attribute_lists.as_slice(),
                };

                analyze_attribute_lists(
                    analyzer,
                    property_attribute_lists,
                    AttributeTarget::Property,
                    class_info,
                    &mut context,
                    analysis_data,
                );
            }
            ClassLikeMember::Method(method) => {
                analyze_method_attributes(
                    analyzer,
                    method,
                    class_info,
                    class_context,
                    analysis_data,
                );
            }
            _ => {}
        }
    }
}

fn analyze_method_attributes(
    analyzer: &StatementsAnalyzer<'_>,
    method: &Method<'_>,
    class_info: Option<&ClassLikeInfo>,
    class_context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut context = class_context.clone();
    analyze_attribute_lists(
        analyzer,
        method.attribute_lists.as_slice(),
        AttributeTarget::Method,
        class_info,
        &mut context,
        analysis_data,
    );

    for param in method.parameter_list.parameters.iter() {
        let mut param_context = class_context.clone();
        let target = if param.is_promoted_property() {
            AttributeTarget::PromotedProperty
        } else {
            AttributeTarget::Parameter
        };
        analyze_attribute_lists(
            analyzer,
            param.attribute_lists.as_slice(),
            target,
            class_info,
            &mut param_context,
            analysis_data,
        );
    }
}

fn analyze_attribute_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    args: Option<&[Argument<'_>]>,
    context: &BlockContext,
    use_fresh_context: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(args) = args else {
        return;
    };

    let issue_start = analysis_data.issues.len();
    let mut argument_context = if use_fresh_context {
        BlockContext::new()
    } else {
        context.clone()
    };

    for argument in args {
        expression_analyzer::analyze(
            analyzer,
            argument.value(),
            analysis_data,
            &mut argument_context,
        );
    }

    if use_fresh_context {
        for issue in &mut analysis_data.issues[issue_start..] {
            issue.kind = match issue.kind {
                IssueKind::UndefinedGlobalVariable => IssueKind::UndefinedVariable,
                other => other,
            };
        }
    }
}

fn validate_attribute_constructor_call(
    analyzer: &StatementsAnalyzer<'_>,
    args: Option<&[Argument<'_>]>,
    attribute_class_info: &ClassLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let Some(args) = args else {
        return;
    };

    let Some(constructor_info) = attribute_class_info.methods.get(&StrId::CONSTRUCT) else {
        if !args.is_empty() {
            let span = args[0].span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooManyArguments,
                format!(
                    "Too many arguments to attribute constructor {}::__construct, 0 expected, {} provided",
                    analyzer.interner.lookup(attribute_class_info.name),
                    args.len()
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
        return;
    };

    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();
    let has_spread = args.iter().any(Argument::is_unpacked);
    let required_params = constructor_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    let call_span = if let (Some(first), Some(last)) = (args.first(), args.last()) {
        (first.span().start.offset, last.span().end.offset)
    } else {
        (constructor_info.start_offset, constructor_info.start_offset)
    };

    if !has_spread && args.len() < required_params {
        let (line, col) = analyzer.get_line_column(call_span.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to attribute constructor {}::__construct, {} expected, {} provided",
                analyzer.interner.lookup(attribute_class_info.name),
                required_params,
                args.len()
            ),
            analyzer.file_path,
            call_span.0,
            call_span.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = constructor_info
        .params
        .last()
        .is_some_and(|p| p.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > constructor_info.params.len() {
        let (line, col) = analyzer.get_line_column(call_span.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to attribute constructor {}::__construct, {} expected, {} provided",
                analyzer.interner.lookup(attribute_class_info.name),
                constructor_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            call_span.0,
            call_span.1,
            line,
            col,
        ));
    }

    let mut next_positional_param = 0usize;
    for (arg_index, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            continue;
        }

        let param_index = match arg {
            Argument::Positional(_) => {
                let current = next_positional_param;
                next_positional_param = next_positional_param.saturating_add(1);
                current
            }
            Argument::Named(named_arg) => constructor_info
                .params
                .iter()
                .position(|param| {
                    analyzer
                        .interner
                        .lookup(param.name)
                        .trim_start_matches('$')
                        .eq_ignore_ascii_case(named_arg.name.value)
                })
                .unwrap_or(usize::MAX),
        };

        let param = if param_index < constructor_info.params.len() {
            constructor_info.params.get(param_index)
        } else {
            constructor_info.params.last().filter(|p| p.is_variadic)
        };

        let Some(param) = param else {
            continue;
        };

        let Some(arg_pos) = arg_positions.get(arg_index).copied() else {
            continue;
        };
        let Some(arg_type) = analysis_data.expr_types.get(&arg_pos).cloned() else {
            continue;
        };

        argument_analyzer::verify_type(
            analyzer,
            arg,
            arg_pos,
            &arg_type,
            param,
            arg_index,
            &format!(
                "{}::__construct",
                analyzer.interner.lookup(attribute_class_info.name)
            ),
            analysis_data,
            context,
            // Attribute instantiations are declarative, not executed dataflow.
            None,
        );
    }
}

fn validate_attribute_class_definition(
    analyzer: &StatementsAnalyzer<'_>,
    current_class_info: Option<&ClassLikeInfo>,
    issue_start: u32,
    issue_end: u32,
    suppress_invalid: bool,
    analysis_data: &mut FunctionAnalysisData,
) {
    if suppress_invalid {
        return;
    }

    let Some(class_info) = current_class_info else {
        return;
    };

    match class_info.kind {
        ClassLikeKind::Trait => emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            issue_start,
            issue_end,
            "Traits cannot act as attribute classes".to_string(),
        ),
        ClassLikeKind::Interface => emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            issue_start,
            issue_end,
            "Interfaces cannot act as attribute classes".to_string(),
        ),
        ClassLikeKind::Enum => emit_invalid_attribute_issue(
            analyzer,
            analysis_data,
            issue_start,
            issue_end,
            "Enums cannot act as attribute classes".to_string(),
        ),
        ClassLikeKind::Class => {
            if class_info.is_abstract {
                emit_invalid_attribute_issue(
                    analyzer,
                    analysis_data,
                    issue_start,
                    issue_end,
                    "Abstract classes cannot act as attribute classes".to_string(),
                );
                return;
            }

            if class_info
                .methods
                .get(&StrId::CONSTRUCT)
                .is_some_and(|constructor| constructor.visibility != Visibility::Public)
            {
                emit_invalid_attribute_issue(
                    analyzer,
                    analysis_data,
                    issue_start,
                    issue_end,
                    "Classes with protected/private constructors cannot act as attribute classes"
                        .to_string(),
                );
            }
        }
    }
}

fn reflection_target_from_class_name(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
) -> Option<AttributeTarget> {
    let class_name = analyzer.interner.lookup(class_id);
    if class_name.eq_ignore_ascii_case("ReflectionClass") {
        Some(AttributeTarget::ClassLike)
    } else if class_name.eq_ignore_ascii_case("ReflectionFunction") {
        Some(AttributeTarget::Function)
    } else if class_name.eq_ignore_ascii_case("ReflectionMethod") {
        Some(AttributeTarget::Method)
    } else if class_name.eq_ignore_ascii_case("ReflectionProperty") {
        Some(AttributeTarget::Property)
    } else if class_name.eq_ignore_ascii_case("ReflectionClassConstant") {
        Some(AttributeTarget::ClassLikeConstant)
    } else if class_name.eq_ignore_ascii_case("ReflectionParameter") {
        Some(AttributeTarget::Parameter)
    } else {
        None
    }
}

fn get_attribute_class_flags(
    analyzer: &StatementsAnalyzer<'_>,
    class_name_id: StrId,
    class_info: &ClassLikeInfo,
) -> Option<u8> {
    if analyzer
        .interner
        .lookup(class_name_id)
        .eq_ignore_ascii_case("Attribute")
    {
        return Some(ATTR_TARGET_CLASS);
    }

    class_info.attribute_flags
}

fn extract_single_literal_class_name(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &pzoom_code_info::TUnion,
) -> Option<StrId> {
    let atomic = arg_type.get_single()?;
    match atomic {
        pzoom_code_info::TAtomic::TLiteralString { value } => analyzer.interner.find(value),
        pzoom_code_info::TAtomic::TLiteralClassString { name } => analyzer.interner.find(name),
        pzoom_code_info::TAtomic::TClassString {
            as_type: Some(as_type),
        } => match as_type.as_ref() {
            pzoom_code_info::TAtomic::TNamedObject { name, .. } => Some(*name),
            _ => None,
        },
        _ => None,
    }
}

fn emit_invalid_attribute_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    start_offset: u32,
    end_offset: u32,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(start_offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::InvalidAttribute,
        message,
        analyzer.file_path,
        start_offset,
        end_offset,
        line,
        col,
    ));
}
