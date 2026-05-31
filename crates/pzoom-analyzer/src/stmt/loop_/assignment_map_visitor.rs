//! Builds a map of which variables are assigned in a loop, and what each
//! assignment depends on. Used by the loop analyzer to bound the number of
//! fixpoint iterations. Mirrors Hakana's `assignment_map_visitor`.

use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::assignment::Assignment;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::unary::{UnaryPostfixOperator, UnaryPrefixOperator};
use mago_syntax::ast::node::Node;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::expression_identifier;

struct Scanner {
    assignment_map: FxHashMap<String, FxHashSet<String>>,
    first_var_id: Option<String>,
}

impl Scanner {
    fn new() -> Self {
        Self {
            assignment_map: FxHashMap::default(),
            first_var_id: None,
        }
    }

    fn insert(&mut self, left: String, right: Option<String>) {
        if self.first_var_id.is_none() {
            self.first_var_id = Some(left.clone());
        }
        self.assignment_map
            .entry(left)
            .or_default()
            .insert(right.unwrap_or_else(|| "isset".to_string()));
    }

    fn record_assignment(&mut self, assign: &Assignment<'_>) {
        let right_var_id = get_root_var_id(assign.rhs);

        match assign.lhs.unparenthesized() {
            Expression::List(list) => {
                for element in list.elements.iter() {
                    self.record_destructuring_element(element, &right_var_id);
                }
            }
            Expression::Array(array) => {
                for element in array.elements.iter() {
                    self.record_destructuring_element(element, &right_var_id);
                }
            }
            other => {
                if let Some(left_var_id) = get_root_var_id(other) {
                    self.insert(left_var_id, right_var_id);
                }
            }
        }
    }

    fn record_destructuring_element(&mut self, element: &ArrayElement<'_>, right: &Option<String>) {
        let value = match element {
            ArrayElement::Value(value_element) => value_element.value,
            ArrayElement::KeyValue(key_value) => key_value.value,
            ArrayElement::Variadic(variadic) => variadic.value,
            ArrayElement::Missing(_) => return,
        };
        if let Some(left_var_id) = get_root_var_id(value) {
            self.insert(left_var_id, right.clone());
        }
    }

    fn record_incdec(&mut self, operand: &Expression<'_>) {
        if let Some(var_id) = get_root_var_id(operand) {
            self.insert(var_id.clone(), Some(var_id));
        }
    }
}

/// Resolve the root variable of an assignable expression (e.g. `$this->a[$x]` -> `$this`).
fn get_root_var_id(expr: &Expression<'_>) -> Option<String> {
    let var_key = expression_identifier::get_expression_var_key(expr)?;
    let split_at = ["[", "->", "::"]
        .iter()
        .filter_map(|delim| var_key.find(delim))
        .min();

    match split_at {
        Some(offset) if offset > 0 => Some(var_key[..offset].to_string()),
        _ => Some(var_key),
    }
}

fn scan_node(node: Node<'_, '_>, scanner: &mut Scanner) {
    match node {
        // Nested function-like / class-like scopes have their own variable scope.
        Node::Closure(_)
        | Node::ArrowFunction(_)
        | Node::AnonymousClass(_)
        | Node::Function(_)
        | Node::Class(_)
        | Node::Interface(_)
        | Node::Trait(_)
        | Node::Enum(_) => return,
        Node::Assignment(assign) => scanner.record_assignment(assign),
        Node::UnaryPrefix(unary) => {
            if matches!(
                unary.operator,
                UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_)
            ) {
                scanner.record_incdec(unary.operand);
            }
        }
        Node::UnaryPostfix(unary) => {
            if matches!(
                unary.operator,
                UnaryPostfixOperator::PostIncrement(_) | UnaryPostfixOperator::PostDecrement(_)
            ) {
                scanner.record_incdec(unary.operand);
            }
        }
        _ => {}
    }

    for child in node.children() {
        scan_node(child, scanner);
    }
}

/// Build the assignment map and identify the first assigned variable, scanning the
/// loop's pre-conditions, body statements, and post-expressions in document order.
pub fn get_assignment_map(
    pre_conditions: &[&Expression<'_>],
    post_expressions: &[&Expression<'_>],
    stmts: &[Statement<'_>],
) -> (FxHashMap<String, FxHashSet<String>>, Option<String>) {
    let mut scanner = Scanner::new();

    for pre_condition in pre_conditions {
        scan_node(Node::Expression(pre_condition), &mut scanner);
    }

    for stmt in stmts {
        scan_node(Node::Statement(stmt), &mut scanner);
    }

    for post_expression in post_expressions {
        scan_node(Node::Expression(post_expression), &mut scanner);
    }

    (scanner.assignment_map, scanner.first_var_id)
}
