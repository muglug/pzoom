//! Expression analyzer - dispatches to specific expression type analyzers.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::AnonymousClass;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::MethodBody;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::string::{CompositeString, StringPart};

use pzoom_code_info::{FunctionLikeInfo, Issue, IssueKind, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

// Import expression-specific analyzers
use crate::expr::call::new_analyzer;
use crate::expr::fetch::{
    array_fetch_analyzer, class_constant_fetch_analyzer, instance_property_fetch_analyzer,
    static_property_fetch_analyzer,
};
use crate::expr::{
    array_analyzer, assignment_analyzer, binop_analyzer, call_analyzer, clone_analyzer,
    closure_analyzer, const_fetch_analyzer, echo_analyzer, exit_analyzer, include_analyzer,
    isset_analyzer, match_analyzer, ternary_analyzer, throw_analyzer, unop_analyzer,
    variable_fetch_analyzer, yield_analyzer,
};

/// Analyze an expression, determining its type and recording it in analysis_data.
///
/// Returns the position of the expression for type lookup.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    let span = expr.span();
    let pos: Pos = (span.start.offset, span.end.offset);

    match expr {
        // Literals
        Expression::Literal(lit) => {
            analyze_literal(lit, pos, analysis_data);
        }

        // Strings (including shell execute/backticks)
        Expression::CompositeString(string_expr) => {
            analyze_composite_string(analyzer, string_expr, pos, analysis_data, context);
        }

        // Variables
        Expression::Variable(var) => {
            variable_fetch_analyzer::analyze(analyzer, var, pos, analysis_data, context);
        }

        // Binary operations
        Expression::Binary(binop) => {
            binop_analyzer::analyze(analyzer, binop, pos, analysis_data, context);
        }

        // Assignment
        Expression::Assignment(assignment) => {
            assignment_analyzer::analyze(analyzer, assignment, pos, analysis_data, context);
        }

        // Function/method calls
        Expression::Call(call) => {
            call_analyzer::analyze(analyzer, call, pos, analysis_data, context);
        }

        // Array creation
        Expression::Array(array) => {
            array_analyzer::analyze_array(analyzer, array, pos, analysis_data, context);
        }
        Expression::LegacyArray(array) => {
            // Legacy array uses same element structure
            analyze_legacy_array(analyzer, array, pos, analysis_data, context);
        }
        Expression::List(list) => {
            array_analyzer::analyze_list(analyzer, list, pos, analysis_data, context);
        }

        // Array access ($arr[key])
        Expression::ArrayAccess(access) => {
            array_fetch_analyzer::analyze(analyzer, access, pos, analysis_data, context);
        }

        // Parenthesized expression - analyze inner
        Expression::Parenthesized(paren) => {
            let inner_pos = analyze(analyzer, paren.expression, analysis_data, context);
            if let Some(inner_type) = analysis_data.get_expr_type(inner_pos) {
                analysis_data.set_expr_type(pos, (*inner_type).clone());
            }
        }

        // Property/array access
        Expression::Access(access) => {
            analyze_access(analyzer, access, pos, analysis_data, context);
        }

        // Unary operations
        Expression::UnaryPrefix(unary) => {
            unop_analyzer::analyze_prefix(analyzer, unary, pos, analysis_data, context);
        }
        Expression::UnaryPostfix(unary) => {
            unop_analyzer::analyze_postfix(analyzer, unary, pos, analysis_data, context);
        }

        // Ternary/conditional
        Expression::Conditional(cond) => {
            ternary_analyzer::analyze(analyzer, cond, pos, analysis_data, context);
        }

        // Match expression
        Expression::Match(match_expr) => {
            match_analyzer::analyze(analyzer, match_expr, pos, analysis_data, context);
        }

        // Object instantiation
        Expression::Instantiation(inst) => {
            new_analyzer::analyze(analyzer, inst, pos, analysis_data, context);
        }
        Expression::AnonymousClass(anonymous_class) => {
            analysis_data.set_expr_type(
                pos,
                infer_anonymous_class_object_type(analyzer, anonymous_class),
            );
            analyze_anonymous_class_members(analyzer, anonymous_class, analysis_data, context);
        }

        // Closures
        Expression::Closure(closure) => {
            closure_analyzer::analyze(analyzer, closure, pos, analysis_data, context);
        }
        Expression::ArrowFunction(arrow) => {
            closure_analyzer::analyze_arrow_function(analyzer, arrow, pos, analysis_data, context);
        }

        // Clone
        Expression::Clone(clone_expr) => {
            clone_analyzer::analyze(analyzer, clone_expr, pos, analysis_data, context);
        }

        // Throw (PHP 8+ expression)
        Expression::Throw(throw_expr) => {
            throw_analyzer::analyze(analyzer, throw_expr, pos, analysis_data, context);
        }

        // Yield
        Expression::Yield(yield_expr) => {
            yield_analyzer::analyze(analyzer, yield_expr, pos, analysis_data, context);
        }

        // Magic constants
        Expression::MagicConstant(mc) => {
            analyze_magic_constant(analyzer, mc, pos, analysis_data);
        }

        // Language constructs
        Expression::Construct(construct) => {
            analyze_construct(analyzer, construct, pos, analysis_data, context);
        }

        // Constant access (HELLO, PHP_VERSION, etc.)
        Expression::ConstantAccess(const_access) => {
            const_fetch_analyzer::analyze(analyzer, const_access, pos, analysis_data, context);
        }

        Expression::ArrayAppend(_) => {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::UnrecognizedExpression,
                "Unsupported expression: array append (`$array[]`) cannot be analyzed in expression context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }

        // Default to mixed for unhandled cases
        _ => {
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    }

    pos
}

fn analyze_construct(
    analyzer: &StatementsAnalyzer<'_>,
    construct: &Construct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match construct {
        Construct::Isset(isset) => {
            isset_analyzer::analyze(analyzer, isset, pos, analysis_data, context);
        }
        Construct::Empty(empty) => {
            isset_analyzer::analyze_empty(analyzer, empty, pos, analysis_data, context);
        }
        Construct::Include(include) => {
            include_analyzer::analyze_include(analyzer, include, pos, analysis_data, context);
        }
        Construct::IncludeOnce(include_once) => {
            include_analyzer::analyze_include_once(
                analyzer,
                include_once,
                pos,
                analysis_data,
                context,
            );
        }
        Construct::Require(require) => {
            include_analyzer::analyze_require(analyzer, require, pos, analysis_data, context);
        }
        Construct::RequireOnce(require_once) => {
            include_analyzer::analyze_require_once(
                analyzer,
                require_once,
                pos,
                analysis_data,
                context,
            );
        }
        Construct::Print(print_construct) => {
            echo_analyzer::analyze_print(
                analyzer,
                print_construct.value,
                pos,
                analysis_data,
                context,
            );
        }
        Construct::Exit(exit) => {
            exit_analyzer::analyze_exit(analyzer, exit, pos, analysis_data, context);
        }
        Construct::Die(die) => {
            exit_analyzer::analyze_die(analyzer, die, pos, analysis_data, context);
        }
        Construct::Eval(eval_construct) => {
            let _ = analyze(analyzer, eval_construct.value, analysis_data, context);
            context.check_variables = false;
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    }
}

/// Analyze a literal expression.
fn analyze_literal(
    lit: &mago_syntax::ast::ast::literal::Literal<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_syntax::ast::ast::literal::Literal;

    let expr_type = match lit {
        Literal::Integer(int_lit) => {
            // value is Option<u64>
            if let Some(value) = int_lit.value {
                TUnion::new(TAtomic::TLiteralInt {
                    value: value as i64,
                })
            } else {
                TUnion::int()
            }
        }
        Literal::Float(float_lit) => {
            // value is OrderedFloat<f64>
            TUnion::new(TAtomic::TLiteralFloat {
                value: float_lit.value.into_inner(),
            })
        }
        Literal::String(string_lit) => {
            if let Some(value) = string_lit.value {
                TUnion::new(TAtomic::TLiteralString {
                    value: value.to_string(),
                })
            } else {
                TUnion::string()
            }
        }
        Literal::True(_) => TUnion::new(TAtomic::TTrue),
        Literal::False(_) => TUnion::new(TAtomic::TFalse),
        Literal::Null(_) => TUnion::null(),
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze a magic constant.
fn analyze_magic_constant(
    analyzer: &StatementsAnalyzer<'_>,
    mc: &mago_syntax::ast::ast::magic_constant::MagicConstant<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_syntax::ast::ast::magic_constant::MagicConstant;

    let expr_type = match mc {
        MagicConstant::Line(_) => TUnion::int(),
        MagicConstant::File(_)
        | MagicConstant::Directory(_)
        | MagicConstant::Function(_)
        | MagicConstant::Method(_)
        | MagicConstant::Namespace(_)
        | MagicConstant::Trait(_)
        | MagicConstant::Property(_) => TUnion::string(),
        MagicConstant::Class(_) => {
            if let Some(class_id) = analyzer.get_declaring_class() {
                TUnion::new(TAtomic::TLiteralClassString {
                    name: analyzer.interner.lookup(class_id).to_string(),
                })
            } else {
                TUnion::new(TAtomic::TClassString { as_type: None })
            }
        }
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze legacy array (array(...) syntax).
fn analyze_legacy_array(
    analyzer: &StatementsAnalyzer<'_>,
    array: &mago_syntax::ast::ast::array::LegacyArray<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::array::ArrayElement;
    use pzoom_code_info::t_atomic::ArrayKey;
    use rustc_hash::FxHashMap;

    if array.elements.is_empty() {
        analysis_data.set_expr_type(
            pos,
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            }),
        );
        return;
    }

    let mut known_items: FxHashMap<ArrayKey, TUnion> = FxHashMap::default();
    let mut key_types: Vec<TAtomic> = Vec::new();
    let mut value_types: Vec<TUnion> = Vec::new();
    let mut next_int_key: i64 = 0;
    let mut is_list = true;
    let mut all_keys_known = true;

    for element in array.elements.iter() {
        match element {
            ArrayElement::KeyValue(kv) => {
                let key_pos = analyze(analyzer, kv.key, analysis_data, context);
                let value_pos = analyze(analyzer, kv.value, analysis_data, context);

                let key_type = analysis_data.get_expr_type(key_pos);
                if let Some(vt) = analysis_data.get_expr_type(value_pos) {
                    let value_type = (*vt).clone();

                    if let Some(kt) = key_type {
                        match kt.types.first() {
                            Some(TAtomic::TLiteralInt { value }) => {
                                known_items.insert(ArrayKey::Int(*value), value_type.clone());
                                key_types.push(TAtomic::TInt);
                                if *value != next_int_key {
                                    is_list = false;
                                }
                                next_int_key = value + 1;
                            }
                            Some(TAtomic::TLiteralString { value }) => {
                                known_items
                                    .insert(ArrayKey::String(value.clone()), value_type.clone());
                                key_types.push(TAtomic::TString);
                                is_list = false;
                            }
                            _ => {
                                all_keys_known = false;
                                if let Some(first) = kt.types.first() {
                                    key_types.push(first.clone());
                                }
                                is_list = false;
                            }
                        }
                    } else {
                        all_keys_known = false;
                        is_list = false;
                    }

                    value_types.push(value_type);
                }
            }
            ArrayElement::Value(val) => {
                let value_pos = analyze(analyzer, val.value, analysis_data, context);
                if let Some(vt) = analysis_data.get_expr_type(value_pos) {
                    let value_type = (*vt).clone();
                    known_items.insert(ArrayKey::Int(next_int_key), value_type.clone());
                    key_types.push(TAtomic::TInt);
                    value_types.push(value_type);
                    next_int_key += 1;
                }
            }
            ArrayElement::Variadic(variadic) => {
                let _ = analyze(analyzer, variadic.value, analysis_data, context);
                all_keys_known = false;
                is_list = false;
            }
            ArrayElement::Missing(_) => {}
        }
    }

    let expr_type = if all_keys_known && !known_items.is_empty() {
        TUnion::new(TAtomic::TKeyedArray {
            properties: known_items,
            is_list,
            sealed: true,
            fallback_key_type: None,
            fallback_value_type: None,
        })
    } else if is_list && !value_types.is_empty() {
        let mut value_union = value_types.remove(0);
        for t in value_types {
            value_union = combine_union_types(&value_union, &t, false);
        }

        TUnion::new(TAtomic::TNonEmptyList {
            value_type: Box::new(value_union),
        })
    } else {
        let key_union = if key_types.is_empty() {
            TUnion::array_key()
        } else {
            TUnion::from_types(pzoom_code_info::ttype::type_combiner::combine(
                key_types, false,
            ))
        };

        let value_union = if value_types.is_empty() {
            TUnion::mixed()
        } else {
            let mut value_union = value_types.remove(0);
            for t in value_types {
                value_union = combine_union_types(&value_union, &t, false);
            }
            value_union
        };

        TUnion::new(TAtomic::TNonEmptyArray {
            key_type: Box::new(key_union),
            value_type: Box::new(value_union),
        })
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze property/array access.
fn analyze_access(
    analyzer: &StatementsAnalyzer<'_>,
    access: &mago_syntax::ast::ast::access::Access<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::access::Access;

    match access {
        Access::Property(prop_access) => {
            // Use the property fetch analyzer which handles errors and types
            instance_property_fetch_analyzer::analyze(
                analyzer,
                prop_access,
                pos,
                analysis_data,
                context,
                false, // not in assignment (that's handled separately)
            );
        }
        Access::NullSafeProperty(prop_access) => {
            // Use the nullsafe property fetch analyzer
            instance_property_fetch_analyzer::analyze_nullsafe(
                analyzer,
                prop_access,
                pos,
                analysis_data,
                context,
            );
        }
        Access::StaticProperty(static_prop) => {
            static_property_fetch_analyzer::analyze(
                analyzer,
                static_prop,
                pos,
                analysis_data,
                context,
            );
        }
        Access::ClassConstant(const_access) => {
            // Use the class constant fetch analyzer which handles errors and types
            class_constant_fetch_analyzer::analyze(
                analyzer,
                const_access,
                pos,
                analysis_data,
                context,
            );
        }
    }
}

fn infer_anonymous_class_object_type(
    analyzer: &StatementsAnalyzer<'_>,
    anonymous_class: &AnonymousClass<'_>,
) -> TUnion {
    let mut object_parts = Vec::new();

    if let Some(parent) = anonymous_class
        .extends
        .as_ref()
        .and_then(|extends| extends.types.first())
    {
        let offset = parent.span().start.offset;
        let parent_id = analyzer
            .get_resolved_name(offset)
            .unwrap_or_else(|| analyzer.interner.intern(parent.value()));
        object_parts.push(TAtomic::TNamedObject {
            name: parent_id,
            type_params: None,
        });
    }

    if let Some(implements) = &anonymous_class.implements {
        for interface in implements.types.iter() {
            let offset = interface.span().start.offset;
            let interface_id = analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(interface.value()));
            let interface_atomic = TAtomic::TNamedObject {
                name: interface_id,
                type_params: None,
            };
            if !object_parts.contains(&interface_atomic) {
                object_parts.push(interface_atomic);
            }
        }
    }

    if object_parts.len() == 1 {
        return TUnion::new(object_parts.remove(0));
    }

    if object_parts.len() > 1 {
        return TUnion::new(TAtomic::TObjectIntersection {
            types: object_parts,
        });
    }

    let anon_class_id = analyzer.interner.intern(&format!(
        "@anonymous-class:{}:{}",
        analyzer.interner.lookup(analyzer.file_path),
        anonymous_class.span().start.offset
    ));

    TUnion::new(TAtomic::TNamedObject {
        name: anon_class_id,
        type_params: None,
    })
}

fn analyze_anonymous_class_members(
    analyzer: &StatementsAnalyzer<'_>,
    anonymous_class: &AnonymousClass<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let anonymous_this_type = infer_anonymous_class_object_type(analyzer, anonymous_class);

    for member in anonymous_class.members.iter() {
        let ClassLikeMember::Method(method) = member else {
            continue;
        };

        let mut method_info = FunctionLikeInfo {
            name: analyzer.interner.intern(method.name.value),
            start_offset: method.span().start.offset as u32,
            end_offset: method.span().end.offset as u32,
            ..FunctionLikeInfo::default()
        };

        if let Some(return_type_hint) = &method.return_type_hint
            && let Some(return_type) = parse_type_hint_union_from_source(
                analyzer,
                return_type_hint.hint.span().start.offset as usize,
                return_type_hint.hint.span().end.offset as usize,
            )
        {
            method_info.signature_return_type = Some(return_type.clone());
            method_info.return_type = Some(return_type);
        }

        let method_analyzer = StatementsAnalyzer {
            codebase: analyzer.codebase,
            interner: analyzer.interner,
            function_info: Some(&method_info),
            file_path: analyzer.file_path,
            source: analyzer.source,
            resolved_names: analyzer.resolved_names,
            config: analyzer.config,
        };

        let mut method_context = BlockContext::new();
        method_context.namespace = context.namespace;
        method_context.has_this = true;
        method_context.set_var_type(StrId::THIS_VAR, anonymous_this_type.clone());

        for param in method.parameter_list.parameters.iter() {
            let param_name = analyzer.interner.intern(param.variable.name);
            let param_type = param
                .hint
                .as_ref()
                .and_then(|param_hint| {
                    parse_type_hint_union_from_source(
                        analyzer,
                        param_hint.span().start.offset as usize,
                        param_hint.span().end.offset as usize,
                    )
                })
                .unwrap_or_else(TUnion::mixed);
            method_context.set_var_type(param_name, param_type);
        }

        if let MethodBody::Concrete(body) = &method.body {
            let _ = crate::stmt_analyzer::analyze_stmts(
                &method_analyzer,
                body.statements.as_slice(),
                analysis_data,
                &mut method_context,
            );
        }
    }
}

fn parse_type_hint_union_from_source(
    analyzer: &StatementsAnalyzer<'_>,
    start: usize,
    end: usize,
) -> Option<TUnion> {
    if start >= end || end > analyzer.source.len() {
        return None;
    }

    let hint_text = analyzer.source[start..end].trim();
    if hint_text.is_empty() {
        return None;
    }

    let mut parsed = pzoom_syntax::docblock::parse_type_string(hint_text, analyzer.interner);
    parsed.from_docblock = false;
    Some(parsed)
}

fn analyze_composite_string(
    analyzer: &StatementsAnalyzer<'_>,
    string_expr: &CompositeString<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for part in string_expr.parts().iter() {
        match part {
            StringPart::Expression(expr) => {
                let _ = analyze(analyzer, expr, analysis_data, context);
            }
            StringPart::BracedExpression(braced_expr) => {
                let _ = analyze(analyzer, braced_expr.expression, analysis_data, context);
            }
            StringPart::Literal(_) => {}
        }
    }

    if matches!(string_expr, CompositeString::ShellExecute(_))
        && analyzer.config.forbidden_functions.iter().any(|forbidden| {
            forbidden
                .strip_prefix('\\')
                .unwrap_or(forbidden.as_str())
                .eq_ignore_ascii_case("shell_exec")
        })
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ForbiddenCode,
            "Shell execution using backticks is forbidden",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    analysis_data.set_expr_type(pos, TUnion::string());
}
