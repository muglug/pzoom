//! Expression analyzer - dispatches to specific expression type analyzers.

use mago_span::HasSpan;
use mago_syntax::ast::ast::control_flow::r#match::Match;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion, combine_union_types};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

// Import expression-specific analyzers
use crate::expr::fetch::{class_constant_fetch_analyzer, instance_property_fetch_analyzer};
use crate::expr::{
    array_access_analyzer, array_analyzer, assignment_analyzer, binop_analyzer, call_analyzer,
    const_fetch_analyzer, ternary_analyzer, throw_analyzer, variable_fetch_analyzer,
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
            array_access_analyzer::analyze(analyzer, access, pos, analysis_data, context);
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
            analyze_unary_prefix(analyzer, unary, pos, analysis_data, context);
        }
        Expression::UnaryPostfix(unary) => {
            analyze_unary_postfix(analyzer, unary, pos, analysis_data, context);
        }

        // Ternary/conditional
        Expression::Conditional(cond) => {
            ternary_analyzer::analyze(analyzer, cond, pos, analysis_data, context);
        }

        // Match expression
        Expression::Match(match_expr) => {
            analyze_match(analyzer, match_expr, pos, analysis_data, context);
        }

        // Object instantiation
        Expression::Instantiation(inst) => {
            analyze_instantiation(analyzer, inst, pos, analysis_data, context);
        }

        // Closures
        Expression::Closure(closure) => {
            analyze_closure(analyzer, closure, pos, analysis_data, context);
        }
        Expression::ArrowFunction(arrow) => {
            analyze_arrow_function(analyzer, arrow, pos, analysis_data, context);
        }

        // Clone
        Expression::Clone(clone) => {
            let inner_pos = analyze(analyzer, clone.object, analysis_data, context);
            if let Some(inner_type) = analysis_data.get_expr_type(inner_pos) {
                analysis_data.set_expr_type(pos, (*inner_type).clone());
            }
        }

        // Throw (PHP 8+ expression)
        Expression::Throw(throw_expr) => {
            throw_analyzer::analyze(analyzer, throw_expr, pos, analysis_data, context);
        }

        // Yield
        Expression::Yield(_) => {
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }

        // Magic constants
        Expression::MagicConstant(mc) => {
            analyze_magic_constant(mc, pos, analysis_data);
        }

        // Include/require
        Expression::Construct(_) => {
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }

        // Constant access (HELLO, PHP_VERSION, etc.)
        Expression::ConstantAccess(const_access) => {
            const_fetch_analyzer::analyze(analyzer, const_access, pos, analysis_data, context);
        }

        Expression::ArrayAppend(array_append) => todo!(),

        // Default to mixed for unhandled cases
        _ => {
            analysis_data.set_expr_type(pos, TUnion::mixed());
        }
    }

    pos
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
        Literal::String(_) => TUnion::string(),
        Literal::True(_) => TUnion::new(TAtomic::TTrue),
        Literal::False(_) => TUnion::new(TAtomic::TFalse),
        Literal::Null(_) => TUnion::null(),
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze a magic constant.
fn analyze_magic_constant(
    mc: &mago_syntax::ast::ast::magic_constant::MagicConstant<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
) {
    use mago_syntax::ast::ast::magic_constant::MagicConstant;

    let expr_type = match mc {
        MagicConstant::Line(_) => TUnion::int(),
        MagicConstant::File(_)
        | MagicConstant::Directory(_)
        | MagicConstant::Class(_)
        | MagicConstant::Function(_)
        | MagicConstant::Method(_)
        | MagicConstant::Namespace(_)
        | MagicConstant::Trait(_)
        | MagicConstant::Property(_) => TUnion::string(),
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

    let mut value_types = Vec::new();

    for element in array.elements.iter() {
        match element {
            ArrayElement::KeyValue(kv) => {
                let _key_pos = analyze(analyzer, kv.key, analysis_data, context);
                let value_pos = analyze(analyzer, kv.value, analysis_data, context);
                if let Some(vt) = analysis_data.get_expr_type(value_pos) {
                    value_types.push((*vt).clone());
                }
            }
            ArrayElement::Value(val) => {
                let value_pos = analyze(analyzer, val.value, analysis_data, context);
                if let Some(vt) = analysis_data.get_expr_type(value_pos) {
                    value_types.push((*vt).clone());
                }
            }
            ArrayElement::Variadic(variadic) => {
                let _spread_pos = analyze(analyzer, variadic.value, analysis_data, context);
            }
            ArrayElement::Missing(_) => {}
        }
    }

    let value_union = if value_types.is_empty() {
        TUnion::mixed()
    } else {
        let mut result = value_types.remove(0);
        for t in value_types {
            result = combine_union_types(&result, &t, false);
        }
        result
    };

    analysis_data.set_expr_type(
        pos,
        TUnion::new(TAtomic::TNonEmptyArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(value_union),
        }),
    );
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
            // Analyze the class expression
            let _ = analyze(analyzer, static_prop.class, analysis_data, context);

            // Try to get property type from class info
            if let Some(prop_type) = get_static_property_type(analyzer, static_prop) {
                analysis_data.set_expr_type(pos, prop_type);
                return;
            }
            analysis_data.set_expr_type(pos, TUnion::mixed());
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

/// Find a property in a class or its parent classes.
fn find_property_in_hierarchy(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: pzoom_str::StrId,
    prop_name: pzoom_str::StrId,
) -> Option<TUnion> {
    let mut current_class = Some(class_name);

    while let Some(class_id) = current_class {
        if let Some(class_info) = analyzer.codebase.get_class(class_id) {
            // Check for the property
            if let Some(prop_info) = class_info.properties.get(&prop_name) {
                return prop_info.get_type().cloned();
            }
            // Move to parent class
            current_class = class_info.parent_class;
        } else {
            break;
        }
    }

    None
}

/// Get static property type from class.
fn get_static_property_type(
    analyzer: &StatementsAnalyzer<'_>,
    access: &mago_syntax::ast::ast::access::StaticPropertyAccess<'_>,
) -> Option<TUnion> {
    use mago_syntax::ast::ast::variable::Variable;

    let class_name = match access.class {
        Expression::Identifier(id) => id.value(),
        _ => return None,
    };

    let prop_name = match &access.property {
        Variable::Direct(d) => d.name,
        _ => return None,
    };

    let class_id = analyzer.interner.intern(class_name);
    let prop_id = analyzer.interner.intern(prop_name);

    find_property_in_hierarchy(analyzer, class_id, prop_id)
}

/// Analyze unary prefix expression.
fn analyze_unary_prefix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &mago_syntax::ast::ast::unary::UnaryPrefix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::unary::UnaryPrefixOperator;

    let operand_pos = analyze(analyzer, unary.operand, analysis_data, context);

    let expr_type = match &unary.operator {
        UnaryPrefixOperator::Not(_) => TUnion::bool(),
        UnaryPrefixOperator::Negation(_) | UnaryPrefixOperator::Plus(_) => {
            // Returns int or float depending on operand
            if let Some(op_type) = analysis_data.get_expr_type(operand_pos) {
                if op_type
                    .types
                    .iter()
                    .any(|t| matches!(t, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
                {
                    TUnion::float()
                } else {
                    TUnion::int()
                }
            } else {
                TUnion::new(TAtomic::TNumeric)
            }
        }
        UnaryPrefixOperator::BitwiseNot(_) => TUnion::int(),
        UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_) => {
            analysis_data
                .get_expr_type(operand_pos)
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed)
        }
        UnaryPrefixOperator::ErrorControl(_) => {
            // Error suppression - type is same as operand
            analysis_data
                .get_expr_type(operand_pos)
                .map(|t| (*t).clone())
                .unwrap_or_else(TUnion::mixed)
        }
        UnaryPrefixOperator::Reference(_) => analysis_data
            .get_expr_type(operand_pos)
            .map(|t| (*t).clone())
            .unwrap_or_else(TUnion::mixed),
        // Type casts
        UnaryPrefixOperator::IntCast(_, _) | UnaryPrefixOperator::IntegerCast(_, _) => {
            TUnion::int()
        }
        UnaryPrefixOperator::FloatCast(_, _)
        | UnaryPrefixOperator::DoubleCast(_, _)
        | UnaryPrefixOperator::RealCast(_, _) => TUnion::float(),
        UnaryPrefixOperator::StringCast(_, _) | UnaryPrefixOperator::BinaryCast(_, _) => {
            TUnion::string()
        }
        UnaryPrefixOperator::BoolCast(_, _) | UnaryPrefixOperator::BooleanCast(_, _) => {
            TUnion::bool()
        }
        UnaryPrefixOperator::ArrayCast(_, _) => TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        }),
        UnaryPrefixOperator::ObjectCast(_, _) => TUnion::new(TAtomic::TObject),
        UnaryPrefixOperator::UnsetCast(_, _) => TUnion::null(),
        UnaryPrefixOperator::VoidCast(_, _) => TUnion::void(),
    };

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze unary postfix expression.
fn analyze_unary_postfix(
    analyzer: &StatementsAnalyzer<'_>,
    unary: &mago_syntax::ast::ast::unary::UnaryPostfix<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let operand_pos = analyze(analyzer, unary.operand, analysis_data, context);
    // Post increment/decrement returns the original value
    let expr_type = analysis_data
        .get_expr_type(operand_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);
    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze match expression.
fn analyze_match(
    analyzer: &StatementsAnalyzer<'_>,
    match_expr: &Match<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let _subject_pos = analyze(analyzer, match_expr.expression, analysis_data, context);

    // Collect types from all arms
    let mut result_types = Vec::new();

    for arm in match_expr.arms.iter() {
        let arm_pos = analyze(analyzer, arm.expression(), analysis_data, context);
        if let Some(arm_type) = analysis_data.get_expr_type(arm_pos) {
            result_types.push((*arm_type).clone());
        }
    }

    // Combine all arm types using the type combiner
    let result_type = if result_types.is_empty() {
        TUnion::nothing()
    } else {
        let mut combined = result_types.remove(0);
        for t in result_types {
            combined = combine_union_types(&combined, &t, false);
        }
        combined
    };

    analysis_data.set_expr_type(pos, result_type);
}

/// Analyze object instantiation (new).
fn analyze_instantiation(
    analyzer: &StatementsAnalyzer<'_>,
    inst: &mago_syntax::ast::ast::instantiation::Instantiation<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
) {
    use mago_syntax::ast::ast::keyword::Keyword;

    // Get the class name from the instantiation expression
    let class_type = match inst.class {
        Expression::Identifier(ident) => {
            use mago_span::HasSpan;

            // Look up the resolved name using the offset
            let offset = ident.span().start.offset;
            let class_name_id = analyzer
                .get_resolved_name(offset)
                .unwrap_or_else(|| analyzer.interner.intern(ident.value()));

            let class_name = analyzer.interner.lookup(class_name_id);

            // Check if the class exists and is instantiable
            if let Some(class_info) = analyzer.codebase.get_class(class_name_id) {
                if class_info.is_abstract {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::AbstractInstantiation,
                        format!("Cannot instantiate abstract class {}", class_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
                if class_info.kind == pzoom_code_info::class_like_info::ClassLikeKind::Interface {
                    let (line, col) = analyzer.get_line_column(pos.0);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::AbstractInstantiation,
                        format!("Cannot instantiate interface {}", class_name),
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    ));
                }
            } else {
                // Class doesn't exist
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
            }

            TUnion::new(TAtomic::TNamedObject {
                name: class_name_id,
                type_params: None,
            })
        }
        Expression::Self_(Keyword { .. }) => {
            // Get the declaring class
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                TUnion::new(TAtomic::TNamedObject {
                    name: declaring_class,
                    type_params: None,
                })
            } else {
                TUnion::new(TAtomic::TObject)
            }
        }
        Expression::Parent(Keyword { .. }) => {
            // Get the parent class - need to look it up
            TUnion::new(TAtomic::TObject)
        }
        Expression::Static(Keyword { .. }) => {
            // Static is late-bound self
            if let Some(declaring_class) = analyzer.get_declaring_class() {
                TUnion::new(TAtomic::TNamedObject {
                    name: declaring_class,
                    type_params: None,
                })
            } else {
                TUnion::new(TAtomic::TObject)
            }
        }
        _ => {
            // Dynamic class name or other expression - can't determine type
            TUnion::new(TAtomic::TObject)
        }
    };

    analysis_data.set_expr_type(pos, class_type);
}

/// Analyze closure expression.
fn analyze_closure(
    _analyzer: &StatementsAnalyzer<'_>,
    closure: &mago_syntax::ast::ast::function_like::closure::Closure<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    _context: &mut BlockContext,
) {
    // TODO: Analyze closure body and capture return type
    let return_type = closure.return_type_hint.as_ref().map(|_| TUnion::mixed()); // TODO: Resolve return type hint

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: None, // TODO: Extract params
        return_type: return_type.map(Box::new),
    });

    analysis_data.set_expr_type(pos, expr_type);
}

/// Analyze arrow function expression.
fn analyze_arrow_function(
    analyzer: &StatementsAnalyzer<'_>,
    arrow: &mago_syntax::ast::ast::function_like::arrow_function::ArrowFunction<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the body expression to infer return type
    let body_pos = analyze(analyzer, arrow.expression, analysis_data, context);
    let return_type = analysis_data.get_expr_type(body_pos).map(|t| (*t).clone());

    let expr_type = TUnion::new(TAtomic::TClosure {
        params: None, // TODO: Extract params
        return_type: return_type.map(Box::new),
    });

    analysis_data.set_expr_type(pos, expr_type);
}
