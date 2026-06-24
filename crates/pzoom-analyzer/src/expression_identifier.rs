//! Helpers for converting expressions into reconciler-compatible variable keys.

use mago_syntax::cst::cst::access::{Access, ClassConstantAccess};
use mago_syntax::cst::cst::call::Call;
use mago_syntax::cst::cst::class_like::member::{
    ClassLikeConstantSelector, ClassLikeMemberSelector,
};
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::literal::Literal;
use mago_syntax::cst::cst::variable::Variable;
use pzoom_code_info::VarName;

/// Builds a variable key string for expressions that can be tracked in context.
///
/// Returned keys match the format expected by reconciler paths (e.g. `$a[0]`, `$this->prop`).
pub fn get_expression_var_key(expr: &Expression<'_>) -> Option<VarName> {
    match expr.unparenthesized() {
        Expression::Variable(Variable::Direct(direct)) => {
            Some(VarName::new(pzoom_syntax::bytes_to_str(direct.name)))
        }
        // Psalm's getExtendedVarId resolves a plain assignment to its target,
        // so `($a = expr) instanceof C` narrows `$a`.
        Expression::Assignment(assignment)
            if matches!(
                assignment.operator,
                mago_syntax::cst::cst::assignment::AssignmentOperator::Assign(_)
            ) =>
        {
            get_expression_var_key(assignment.lhs)
        }
        Expression::ArrayAccess(access) => {
            let base = get_expression_var_key(access.array)?;
            let key = get_array_index_key(access.index)?;
            Some(format!("{}[{}]", base, key).into())
        }
        Expression::Access(Access::Property(property_access)) => {
            let base = get_expression_var_key(property_access.object)?;
            let prop_name = match &property_access.property {
                ClassLikeMemberSelector::Identifier(identifier) => {
                    pzoom_syntax::bytes_to_str(identifier.value)
                }
                _ => return None,
            };
            Some(format!("{}->{}", base, prop_name).into())
        }
        Expression::Access(Access::NullSafeProperty(property_access)) => {
            let base = get_expression_var_key(property_access.object)?;
            let prop_name = match &property_access.property {
                ClassLikeMemberSelector::Identifier(identifier) => {
                    pzoom_syntax::bytes_to_str(identifier.value)
                }
                _ => return None,
            };
            Some(format!("{}->{}", base, prop_name).into())
        }
        Expression::Access(Access::StaticProperty(static_property_access)) => {
            let class_name = match static_property_access.class.unparenthesized() {
                Expression::Identifier(identifier) => {
                    pzoom_syntax::bytes_to_str(identifier.value()).to_string()
                }
                Expression::Self_(_) => "self".to_string(),
                Expression::Static(_) => "static".to_string(),
                Expression::Parent(_) => "parent".to_string(),
                _ => return None,
            };

            let property_name = match &static_property_access.property {
                Variable::Direct(direct) => {
                    pzoom_syntax::bytes_to_str(direct.name).trim_start_matches('$')
                }
                _ => return None,
            };

            Some(format!("{}::${}", class_name, property_name).into())
        }
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            build_class_constant_key(class_const_access).map(Into::into)
        }
        Expression::Call(Call::Method(method_call)) => build_method_call_key(
            method_call.object,
            &method_call.method,
            method_call.argument_list.arguments.is_empty(),
        ),
        Expression::Call(Call::NullSafeMethod(method_call)) => build_method_call_key(
            method_call.object,
            &method_call.method,
            method_call.argument_list.arguments.is_empty(),
        ),
        _ => None,
    }
}

fn build_method_call_key(
    object: &Expression<'_>,
    method: &ClassLikeMemberSelector<'_>,
    no_args: bool,
) -> Option<VarName> {
    if !no_args {
        return None;
    }

    let base = get_expression_var_key(object)?;
    let method_name = match method {
        ClassLikeMemberSelector::Identifier(identifier) => {
            pzoom_syntax::bytes_to_str(identifier.value)
        }
        _ => return None,
    };

    // PHP method names are case-insensitive; Psalm's getExtendedVarId
    // lowercases them so `$a->getArray()` and a docblock's
    // `$this->getarray()` key the same scope entry.
    Some(format!("{}->{}()", base, method_name.to_ascii_lowercase()).into())
}

fn get_array_index_key(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Literal(Literal::Integer(int_lit)) => {
            int_lit.value.map(|value| value.to_string())
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit.value.map(|value| {
            let escaped = pzoom_syntax::bytes_to_str(value).replace('\'', "\\'");
            format!("'{}'", escaped)
        }),
        Expression::Variable(Variable::Direct(direct)) => {
            Some(pzoom_syntax::bytes_to_str(direct.name).to_string())
        }
        Expression::Access(Access::ClassConstant(class_const_access)) => {
            build_class_constant_dim_key(class_const_access)
        }
        Expression::ArrayAccess(_)
        | Expression::Access(Access::Property(_))
        | Expression::Access(Access::NullSafeProperty(_))
        | Expression::Access(Access::StaticProperty(_))
        | Expression::Call(Call::Method(_))
        | Expression::Call(Call::NullSafeMethod(_)) => {
            get_expression_var_key(expr).map(|key| key.to_string())
        }
        _ => None,
    }
}

fn build_class_constant_key(access: &ClassConstantAccess<'_>) -> Option<String> {
    let class_name = match access.class.unparenthesized() {
        Expression::Identifier(identifier) => {
            pzoom_syntax::bytes_to_str(identifier.value()).to_string()
        }
        Expression::Self_(_) => "self".to_string(),
        Expression::Static(_) => "static".to_string(),
        Expression::Parent(_) => "parent".to_string(),
        _ => return None,
    };

    let constant_name = match &access.constant {
        ClassLikeConstantSelector::Identifier(identifier) => {
            pzoom_syntax::bytes_to_str(identifier.value)
        }
        _ => return None,
    };

    if constant_name.eq_ignore_ascii_case("class") {
        return None;
    }

    Some(format!("{}::{}", class_name, constant_name))
}

fn build_class_constant_dim_key(access: &ClassConstantAccess<'_>) -> Option<String> {
    let class_name = match access.class.unparenthesized() {
        Expression::Identifier(identifier) => {
            pzoom_syntax::bytes_to_str(identifier.value()).to_string()
        }
        Expression::Self_(_) => "self".to_string(),
        Expression::Static(_) => "static".to_string(),
        Expression::Parent(_) => "parent".to_string(),
        _ => return None,
    };

    let constant_name = match &access.constant {
        ClassLikeConstantSelector::Identifier(identifier) => {
            pzoom_syntax::bytes_to_str(identifier.value)
        }
        _ => return None,
    };

    Some(format!("{}::{}", class_name, constant_name))
}
