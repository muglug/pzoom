//! Expression analyzer - dispatches to specific expression type analyzers.

use std::rc::Rc;
use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::AnonymousClass;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::MethodBody;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::string::{CompositeString, StringPart};

use pzoom_code_info::{
    DataFlowNode, FunctionLikeInfo, GraphKind, Issue, IssueKind, PathKind, TAtomic, TUnion,
    combine_union_types,
};
use pzoom_code_info::VarName;
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
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
            analyze_literal(lit, pos, analysis_data, analyzer.config.max_string_length);
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

            // Psalm's coerceValueAfterGatekeeperArgument: a mixed variable
            // passed to a natively-typed parameter is narrowed to the
            // signature type after the call (queued during argument
            // verification, which holds the context immutably).
            if !analysis_data.pending_gatekeeper_coercions.is_empty() {
                for (var_id, narrowed) in
                    std::mem::take(&mut analysis_data.pending_gatekeeper_coercions)
                {
                    if context.inside_conditional
                        && !context.assigned_var_ids.contains_key(&var_id)
                    {
                        context.assigned_var_ids.insert(var_id.clone(), 0);
                    }
                    context.locals.insert(var_id, narrowed);
                }
            }
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
            if let Some(inner_type) = analysis_data.expr_types.get(&inner_pos).cloned() {
                analysis_data.expr_types.insert(pos, Rc::new((*inner_type).clone()));
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
            let anon_class_id = anonymous_class_synthetic_id(analyzer, anonymous_class);
            if analyzer.codebase.get_class(anon_class_id).is_some() {
                // Constructor arguments evaluate in the enclosing scope.
                if let Some(argument_list) = &anonymous_class.argument_list {
                    for argument in argument_list.arguments.iter() {
                        analyze(analyzer, argument.value(), analysis_data, context);
                    }
                }

                analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TNamedObject {
                        name: anon_class_id,
                        type_params: None,
                        is_static: false,
                        remapped_params: false,
                    })));

                let _ = crate::stmt::class_analyzer::analyze_anonymous_class(
                    analyzer,
                    anonymous_class,
                    anon_class_id,
                    analysis_data,
                    context,
                );
            } else {
                analysis_data.expr_types.insert(pos, Rc::new(infer_anonymous_class_object_type(analyzer, anonymous_class)));
                analyze_anonymous_class_members(analyzer, anonymous_class, analysis_data, context);
            }
        }

        // Closures
        Expression::Closure(closure) => {
            closure_analyzer::analyze(analyzer, closure, pos, analysis_data, context);
        }
        Expression::ArrowFunction(arrow) => {
            closure_analyzer::analyze_arrow_function(analyzer, arrow, pos, analysis_data, context);
        }

        // First-class callables (`strlen(...)`) / partial application
        Expression::PartialApplication(partial_application) => {
            crate::expr::partial_application_analyzer::analyze(
                analyzer,
                partial_application,
                pos,
                analysis_data,
                context,
            );
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
            analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
        }

        // Default to mixed for unhandled cases
        _ => {
            analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
        }
    }

    pos
}

/// Hakana `expression_analyzer::add_decision_dataflow`: funnel both operands'
/// parents into an unlabelled sink node and record the resulting type.
pub(crate) fn add_decision_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    lhs_expr: &Expression<'_>,
    rhs_expr: Option<&Expression<'_>>,
    expr_pos: Pos,
    mut cond_type: TUnion,
) {
    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        // Hakana skips decision dataflow entirely in whole-program graphs; pzoom
        // still records the resulting type so inference is unchanged.
        analysis_data.expr_types.insert(expr_pos, Rc::new(cond_type));
        return;
    }

    let decision_node =
        DataFlowNode::get_for_unlabelled_sink(make_data_flow_node_position(analyzer, expr_pos));

    let lhs_span = lhs_expr.span();
    if let Some(lhs_type) = analysis_data.expr_types.get(&(lhs_span.start.offset, lhs_span.end.offset)).cloned()
    {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &old_parent_node.id,
                &decision_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    if let Some(rhs_expr) = rhs_expr {
        let rhs_span = rhs_expr.span();
        if let Some(rhs_type) =
            analysis_data.expr_types.get(&(rhs_span.start.offset, rhs_span.end.offset)).cloned()
        {
            cond_type.parent_nodes.push(decision_node.clone());

            for old_parent_node in &rhs_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }
    }

    analysis_data.expr_types.insert(expr_pos, Rc::new(cond_type));

    analysis_data.data_flow_graph.add_node(decision_node);
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
            // Hakana marks eval'd expressions as general use.
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;
            let value_pos = analyze(analyzer, eval_construct.value, analysis_data, context);
            context.inside_general_use = was_inside_general_use;
            context.check_variables = false;

            // Psalm `EvalAnalyzer`: the eval'd expression is an `eval` taint
            // sink (TaintedEval).
            if analyzer.config.taint_analysis
                && let Some(value_type) = analysis_data.expr_types.get(&value_pos).cloned()
            {
                crate::expr::echo_analyzer::add_construct_argument_dataflow(
                    analyzer,
                    "eval",
                    &[pzoom_code_info::data_flow::node::SinkType::Eval],
                    0,
                    value_pos,
                    &value_type,
                    pos,
                    analysis_data,
                    context,
                );
            }

            analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
        }
    }
}

/// Analyze a literal expression.
fn analyze_literal(
    lit: &mago_syntax::ast::ast::literal::Literal<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    max_string_length: usize,
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
                // mago's unescaper drops the backslash of an unrecognized
                // double-quoted escape ("\/" → "/"); PHP keeps both
                // ("\/" === '\/' — only listed sequences are special). Regex
                // patterns written in double quotes depend on this. Re-derive
                // the value from the raw token for double-quoted strings.
                let value = if matches!(
                    string_lit.kind,
                    Some(mago_syntax::ast::ast::literal::LiteralStringKind::DoubleQuoted)
                ) && string_lit.raw.len() >= 2
                    && string_lit.raw.contains('\\')
                {
                    php_unescape_double_quoted(&string_lit.raw[1..string_lit.raw.len() - 1])
                } else {
                    value.to_string()
                };
                TUnion::new(TAtomic::string_from_literal(value, max_string_length))
            } else {
                TUnion::string()
            }
        }
        Literal::True(_) => TUnion::new(TAtomic::TTrue),
        Literal::False(_) => TUnion::new(TAtomic::TFalse),
        Literal::Null(_) => TUnion::null(),
    };

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
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
        // Psalm types __FUNCTION__/__METHOD__ as the literal enclosing
        // function/method name ('A::method' for __METHOD__ in a method).
        MagicConstant::Function(_) => match analyzer.function_info {
            Some(function_info) if function_info.name != pzoom_str::StrId::EMPTY => {
                TUnion::new(TAtomic::TLiteralString {
                    value: analyzer.interner.lookup(function_info.name).to_string(),
                })
            }
            _ => TUnion::string(),
        },
        MagicConstant::Method(_) => match analyzer.function_info {
            Some(function_info) if function_info.name != pzoom_str::StrId::EMPTY => {
                let method_name = analyzer.interner.lookup(function_info.name).to_string();
                let value = match analyzer.get_declaring_class() {
                    Some(class_id) => format!(
                        "{}::{}",
                        analyzer.interner.lookup(class_id),
                        method_name
                    ),
                    None => method_name,
                };
                TUnion::new(TAtomic::TLiteralString { value })
            }
            _ => TUnion::string(),
        },
        MagicConstant::File(_)
        | MagicConstant::Directory(_)
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

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
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
        analysis_data.expr_types.insert(pos, Rc::new(TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::nothing()),
                value_type: Box::new(TUnion::nothing()),
            })));
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

                let key_type = analysis_data.expr_types.get(&key_pos).cloned();
                if let Some(vt) = analysis_data.expr_types.get(&value_pos).cloned() {
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
                if let Some(vt) = analysis_data.expr_types.get(&value_pos).cloned() {
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
            properties: std::sync::Arc::new(known_items),
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

    analysis_data.expr_types.insert(pos, Rc::new(expr_type));
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
        is_static: false, remapped_params: false });
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
            is_static: false, remapped_params: false };
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

    let anon_class_id = anonymous_class_synthetic_id(analyzer, anonymous_class);

    TUnion::new(TAtomic::TNamedObject {
        name: anon_class_id,
        type_params: None,
    is_static: false, remapped_params: false })
}

/// Prefix shared by all synthetic anonymous-class names.
pub(crate) const ANONYMOUS_CLASS_PREFIX: &str = pzoom_code_info::ANONYMOUS_CLASS_PREFIX;

/// Intern the synthetic classlike name for an anonymous class expression:
/// `@anonymous-class:{file}:{offset}`. Anonymous classes are not registered in
/// the codebase, so this name keys the per-scope side table on
/// [`FunctionAnalysisData::anonymous_class_methods`] instead.
fn anonymous_class_synthetic_id(
    analyzer: &StatementsAnalyzer<'_>,
    anonymous_class: &AnonymousClass<'_>,
) -> pzoom_str::StrId {
    analyzer.interner.intern(&format!(
        "{}:{}:{}",
        ANONYMOUS_CLASS_PREFIX,
        analyzer.interner.lookup(analyzer.file_path),
        anonymous_class.span().start.offset
    ))
}

fn analyze_anonymous_class_members(
    analyzer: &StatementsAnalyzer<'_>,
    anonymous_class: &AnonymousClass<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let anonymous_this_type = infer_anonymous_class_object_type(analyzer, anonymous_class);

    // Method calls on the synthetic `@anonymous-class:...` object resolve
    // through this side table (anonymous classes are not in the codebase).
    // Register the class even when it declares no methods: the call analyzer
    // recognises anonymous receivers by table membership.
    let anon_class_id = anonymous_class_synthetic_id(analyzer, anonymous_class);
    analysis_data
        .anonymous_class_methods
        .entry(anon_class_id)
        .or_default();

    // The anonymous class's own properties — declared members and promoted
    // constructor params — seed every method's `\$this-><prop>` path locals
    // (Psalm registers anonymous classes with full property storage; pzoom's
    // side-table model resolves their fetches through the tracked paths).
    let mut anon_property_types: Vec<(String, TUnion)> = Vec::new();
    for member in anonymous_class.members.iter() {
        match member {
            ClassLikeMember::Property(property) => {
                if let mago_syntax::ast::ast::class_like::property::Property::Plain(plain) =
                    property
                {
                    let property_type = plain
                        .hint
                        .as_ref()
                        .map(|hint| {
                            pzoom_syntax::resolve_hint(
                                hint,
                                analyzer.interner,
                                context.namespace,
                                None,
                                None,
                                None,
                                Some(analyzer.resolved_names),
                            )
                        })
                        .unwrap_or_else(TUnion::mixed);
                    for item in plain.items.iter() {
                        anon_property_types.push((
                            item.variable().name.trim_start_matches('$').to_string(),
                            property_type.clone(),
                        ));
                    }
                }
            }
            ClassLikeMember::Method(method) if method.name.value == "__construct" => {
                for param in method.parameter_list.parameters.iter() {
                    if !param.is_promoted_property() {
                        continue;
                    }
                    let property_type = param
                        .hint
                        .as_ref()
                        .map(|hint| {
                            pzoom_syntax::resolve_hint(
                                hint,
                                analyzer.interner,
                                context.namespace,
                                None,
                                None,
                                None,
                                Some(analyzer.resolved_names),
                            )
                        })
                        .unwrap_or_else(TUnion::mixed);
                    anon_property_types.push((
                        param.variable.name.trim_start_matches('$').to_string(),
                        property_type,
                    ));
                }
            }
            _ => {}
        }
    }

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

        if let Some(return_type_hint) = &method.return_type_hint {
            let return_type = pzoom_syntax::resolve_hint(
                &return_type_hint.hint,
                analyzer.interner,
                context.namespace,
                None,
                None,
                None,
                Some(analyzer.resolved_names),
            );
            method_info.signature_return_type = Some(return_type.clone());
            method_info.return_type = Some(return_type);
        }

        let mut method_context = BlockContext::new();
        method_context.namespace = context.namespace;
        method_context.has_this = true;
        method_context.set_var_type(VarName::new_static("$this"), anonymous_this_type.clone());
        for (property_name, property_type) in &anon_property_types {
            method_context.set_var_type(
                VarName::from(format!("$this->{}", property_name)),
                property_type.clone(),
            );
        }

        for param in method.parameter_list.parameters.iter() {
            let param_name = analyzer.interner.intern(param.variable.name);
            let mut param_type = param
                .hint
                .as_ref()
                .map(|param_hint| {
                    // The file's preprocessed name resolution (use imports
                    // included) covers the hint's offsets, exactly as for
                    // closures.
                    pzoom_syntax::resolve_hint(
                        param_hint,
                        analyzer.interner,
                        context.namespace,
                        None,
                        None,
                        None,
                        Some(analyzer.resolved_names),
                    )
                })
                .unwrap_or_else(TUnion::mixed);
            resolve_unqualified_named_objects(analyzer, &mut param_type, context.namespace);
            method_info.params.push(pzoom_code_info::functionlike_info::ParamInfo {
                name: param_name,
                param_type: Some(param_type.clone()),
                signature_type: param.hint.as_ref().map(|_| param_type.clone()),
                is_optional: param.default_value.is_some(),
                is_variadic: param.ellipsis.is_some(),
                start_offset: param.span().start.offset,
                ..pzoom_code_info::functionlike_info::ParamInfo::default()
            });
            method_context.set_var_type(VarName::new(&analyzer.interner.lookup(param_name)), param_type);
        }

        let method_analyzer = analyzer.for_nested_function(Some(&method_info));

        if let MethodBody::Concrete(body) = &method.body {
            // The anonymous method's return/yield types must not leak into the
            // enclosing function-like's inferred slices.
            let return_types_mark = analysis_data.inferred_return_types.len();
            let yield_types_mark = analysis_data.inferred_yield_types.len();
            let _ = crate::stmt_analyzer::analyze_stmts(
                &method_analyzer,
                body.statements.as_slice(),
                analysis_data,
                &mut method_context,
            );
            analysis_data.inferred_return_types.truncate(return_types_mark);
            analysis_data.inferred_yield_types.truncate(yield_types_mark);
        }

        analysis_data
            .anonymous_class_methods
            .entry(anon_class_id)
            .or_default()
            .insert(method_info.name, method_info);
    }
}

/// Raw type-hint parsing inside anonymous-class members has no name-resolution
/// context; qualify unknown named objects against the enclosing namespace.
fn resolve_unqualified_named_objects(
    analyzer: &StatementsAnalyzer<'_>,
    union: &mut TUnion,
    namespace: Option<StrId>,
) {
    let Some(namespace) = namespace else {
        return;
    };

    for atomic in &mut union.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };
        if analyzer.codebase.get_class(*name).is_some() {
            continue;
        }

        let candidate = analyzer.interner.intern(&format!(
            "{}\\{}",
            analyzer.interner.lookup(namespace),
            analyzer.interner.lookup(*name)
        ));
        if analyzer.codebase.get_class(candidate).is_some() {
            *name = candidate;
        }
    }
}

/// PHP double-quoted string unescaping (Zend `php_var_unserialize`-style
/// rules): `\\ \" \$ \n \t \r \v \e \f`, octal `\[0-7]{1,3}`, hex
/// `\x[0-9A-Fa-f]{1,2}` and unicode `\u{…}` are special; ANY other
/// backslash sequence keeps the backslash verbatim (`"\/"` is `\/`).
fn php_unescape_double_quoted(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }

        match chars.peek().copied() {
            None => result.push('\\'),
            Some('\\') => {
                chars.next();
                result.push('\\');
            }
            Some('"') => {
                chars.next();
                result.push('"');
            }
            Some('$') => {
                chars.next();
                result.push('$');
            }
            Some('n') => {
                chars.next();
                result.push('\n');
            }
            Some('t') => {
                chars.next();
                result.push('\t');
            }
            Some('r') => {
                chars.next();
                result.push('\r');
            }
            Some('v') => {
                chars.next();
                result.push('\u{0B}');
            }
            Some('e') => {
                chars.next();
                result.push('\u{1B}');
            }
            Some('f') => {
                chars.next();
                result.push('\u{0C}');
            }
            Some(digit) if digit.is_digit(8) => {
                let mut octal_value = 0u32;
                let mut octal_len = 0;
                while octal_len < 3
                    && let Some(peeked) = chars.peek()
                    && peeked.is_digit(8)
                {
                    octal_value = octal_value * 8 + peeked.to_digit(8).unwrap();
                    octal_len += 1;
                    chars.next();
                }
                // PHP wraps octal overflow at a byte.
                result.push(char::from((octal_value & 0xFF) as u8));
            }
            Some('x') => {
                // Only special when followed by at least one hex digit.
                let mut lookahead = chars.clone();
                lookahead.next();
                if lookahead.peek().is_some_and(|peeked| peeked.is_ascii_hexdigit()) {
                    chars.next();
                    let mut hex_value = 0u32;
                    let mut hex_len = 0;
                    while hex_len < 2
                        && let Some(peeked) = chars.peek()
                        && peeked.is_ascii_hexdigit()
                    {
                        hex_value = hex_value * 16 + peeked.to_digit(16).unwrap();
                        hex_len += 1;
                        chars.next();
                    }
                    result.push(char::from(hex_value as u8));
                } else {
                    result.push('\\');
                }
            }
            Some('u') => {
                // `\u{codepoint}`; a bare `\u` stays literal.
                let mut lookahead = chars.clone();
                lookahead.next();
                if lookahead.peek() == Some(&'{') {
                    chars.next();
                    chars.next();
                    let mut codepoint = 0u32;
                    let mut valid = false;
                    while let Some(peeked) = chars.peek().copied() {
                        if peeked == '}' {
                            chars.next();
                            break;
                        }
                        if let Some(digit) = peeked.to_digit(16) {
                            codepoint = codepoint.saturating_mul(16).saturating_add(digit);
                            valid = true;
                            chars.next();
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    match (valid, char::from_u32(codepoint)) {
                        (true, Some(unicode_char)) => result.push(unicode_char),
                        _ => result.push_str("\\u"),
                    }
                } else {
                    result.push('\\');
                }
            }
            Some(other) => {
                // Unrecognized escape: PHP keeps the backslash AND the char.
                chars.next();
                result.push('\\');
                result.push(other);
            }
        }
    }

    result
}

/// Whether an interpolated part's atomic counts as a literal for Psalm's
/// `allLiterals()` grading (literal strings — including the non-specific
/// `literal-string` marker — and literal ints/floats/bools).
fn atomic_is_literal_stringable(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TNonspecificLiteralInt
            | TAtomic::TLiteralFloat { .. }
            | TAtomic::TTrue
            | TAtomic::TFalse
    )
}

fn analyze_composite_string(
    analyzer: &StatementsAnalyzer<'_>,
    string_expr: &CompositeString<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let mut part_positions: Vec<Pos> = vec![];

    // Psalm's EncapsulatedStringAnalyzer tracks literalness/emptiness across
    // the parts to grade the result: exact literal → non-empty-literal-string
    // → non-empty-string → literal-string → string.
    let mut non_empty = false;
    let mut all_literals = true;
    let mut literal_string: Option<String> = Some(String::new());

    for part in string_expr.parts().iter() {
        let expr_pos = match part {
            StringPart::Expression(expr) => Some(analyze(analyzer, expr, analysis_data, context)),
            StringPart::BracedExpression(braced_expr) => Some(analyze(
                analyzer,
                braced_expr.expression,
                analysis_data,
                context,
            )),
            StringPart::Literal(literal) => {
                if let Some(accumulated) = literal_string.as_mut() {
                    accumulated.push_str(literal.value);
                }
                non_empty = non_empty || !literal.value.is_empty();
                None
            }
        };

        let Some(expr_pos) = expr_pos else {
            continue;
        };
        part_positions.push(expr_pos);

        let Some(part_type) = analysis_data.expr_types.get(&expr_pos).cloned() else {
            all_literals = false;
            literal_string = None;
            continue;
        };

        // Psalm routes every interpolated part through castStringAttempt,
        // which reports InvalidCast for objects without __toString.
        crate::expr::cast_analyzer::maybe_emit_invalid_string_cast(
            analyzer,
            &part_type,
            expr_pos,
            analysis_data,
        );

        if !part_type.types.iter().all(atomic_is_literal_stringable) {
            all_literals = false;
        } else if !non_empty {
            // Check if all literals are non-empty (Psalm: literal ints/floats
            // always stringify non-empty; literal strings need a value).
            non_empty = part_type.types.iter().all(|atomic| match atomic {
                TAtomic::TLiteralInt { .. }
                | TAtomic::TNonspecificLiteralInt
                | TAtomic::TLiteralFloat { .. } => true,
                TAtomic::TLiteralString { value } => {
                    !value.is_empty()
                        && value != pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE
                }
                _ => false,
            });
        }

        if let Some(accumulated) = literal_string.as_mut() {
            match part_type.get_single() {
                Some(TAtomic::TLiteralString { value })
                    if value != pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE =>
                {
                    accumulated.push_str(value);
                }
                Some(TAtomic::TLiteralInt { value }) => {
                    accumulated.push_str(&value.to_string());
                }
                _ => {
                    literal_string = None;
                }
            }
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

    // Psalm's result grading; backtick strings stay plain `string` (their
    // value is the command output, not the interpolation).
    let mut result_type = if matches!(string_expr, CompositeString::ShellExecute(_)) {
        TUnion::string()
    } else if non_empty {
        if let Some(literal_string) = literal_string {
            TUnion::new(TAtomic::string_from_literal(
                literal_string,
                analyzer.config.max_string_length,
            ))
        } else if all_literals {
            TUnion::new(TAtomic::TLiteralString {
                value: pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
            })
        } else {
            TUnion::new(TAtomic::TNonEmptyString)
        }
    } else if all_literals {
        TUnion::new(TAtomic::TLiteralString {
            value: pzoom_code_info::t_atomic::NON_SPECIFIC_LITERAL_STRING_VALUE.to_string(),
        })
    } else {
        TUnion::string()
    };
    {
        let decision_node = pzoom_code_info::DataFlowNode::get_for_composition(
            crate::data_flow::make_data_flow_node_position(analyzer, pos),
        );

        for part_pos in part_positions {
            let Some(part_type) = analysis_data.expr_types.get(&part_pos).cloned() else {
                continue;
            };
            let mut parent_nodes = part_type.parent_nodes.clone();
            // Psalm's EncapsulatedStringAnalyzer casts every part through
            // `castStringAttempt`: interpolated objects route their dataflow
            // through __toString.
            if part_type.has_object() {
                parent_nodes.extend(crate::expr::cast_analyzer::add_to_string_call_dataflow(
                    analyzer,
                    analysis_data,
                    &part_type,
                ));
            }
            for old_parent_node in &parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    pzoom_code_info::PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }

        result_type.parent_nodes.push(decision_node.clone());
        analysis_data.data_flow_graph.add_node(decision_node);

        // Executing the command consumes the interpolated parts even when the
        // output is discarded (Psalm marks backtick interpolations used —
        // ShellExecAnalyzer routes them through general use).
        if matches!(string_expr, CompositeString::ShellExecute(_)) {
            if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody {
                let use_sink = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
                    crate::data_flow::make_data_flow_node_position(analyzer, pos),
                );
                for parent_node in &result_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &parent_node.id,
                        &use_sink.id,
                        pzoom_code_info::PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
                analysis_data.data_flow_graph.add_node(use_sink);
            }

            // Psalm's ShellExecAnalyzer (ExpressionAnalyzer): the backtick
            // command is a `shell_exec` taint sink.
            if analyzer.config.taint_analysis {
                let interpolation_type = TUnion {
                    parent_nodes: result_type.parent_nodes.clone(),
                    ..TUnion::string()
                };
                crate::expr::echo_analyzer::add_construct_argument_dataflow(
                    analyzer,
                    "shell_exec",
                    &[pzoom_code_info::data_flow::node::SinkType::Shell],
                    0,
                    pos,
                    &interpolation_type,
                    pos,
                    analysis_data,
                    context,
                );
            }
        }
    }

    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}
