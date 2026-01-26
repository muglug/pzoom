//! Parse tree creator - builds parse trees from tokens.
//!
//! Based on Psalm's ParseTreeCreator.php

use super::parse_tree::*;
use super::type_tokenizer::TypeToken;

/// Creates a parse tree from a list of type tokens.
pub struct ParseTreeCreator {
    tokens: Vec<TypeToken>,
    current_index: usize,
    parse_tree: ParseTree,
}

impl ParseTreeCreator {
    pub fn new(tokens: Vec<TypeToken>) -> Self {
        Self {
            tokens,
            current_index: 0,
            parse_tree: ParseTree::Root(ParseTreeNode::new()),
        }
    }

    pub fn create(mut self) -> Result<ParseTree, String> {
        while self.current_index < self.tokens.len() {
            let token = self.tokens[self.current_index].clone();

            match token.value.as_str() {
                "{" | "]" => {
                    return Err(format!("Unexpected token {}", token.value));
                }
                "<" => self.handle_less_than()?,
                "[" => self.handle_open_square_bracket()?,
                "(" => self.handle_open_round_bracket()?,
                ")" => self.handle_closed_round_bracket()?,
                ">" => self.handle_greater_than()?,
                "}" => self.handle_close_brace()?,
                "," => self.handle_comma()?,
                "..." | "=" => self.handle_ellipsis_or_equals(&token)?,
                ":" => self.handle_colon()?,
                " " => self.handle_space()?,
                "?" => self.handle_question_mark()?,
                "|" => self.handle_bar()?,
                "&" => self.handle_ampersand()?,
                "is" | "as" | "of" => self.handle_is_or_as(&token)?,
                _ => self.handle_value(&token)?,
            }

            self.current_index += 1;
        }

        Ok(self.parse_tree)
    }

    fn handle_less_than(&mut self) -> Result<(), String> {
        // < after field ellipsis creates a generic node for fallback types
        if matches!(&self.parse_tree, ParseTree::FieldEllipsis(_)) {
            // Create generic node as child of keyed array
            let generic = ParseTree::Generic(GenericNode::new(""));
            self.add_child(generic);
        }
        Ok(())
    }

    fn handle_open_square_bracket(&mut self) -> Result<(), String> {
        // [ creates array suffix or indexed access
        let next_token = self.tokens.get(self.current_index + 1);

        if next_token.map(|t| t.value.as_str()) == Some("]") {
            // Simple array suffix []
            let array_generic = ParseTree::Generic(GenericNode::new("array"));
            self.wrap_current_with(array_generic);
            self.current_index += 1;
        } else if let Some(next) = next_token {
            // Indexed access T[K]
            if let Some(next_next) = self.tokens.get(self.current_index + 2) {
                if next_next.value == "]" {
                    let indexed = ParseTree::IndexedAccess(IndexedAccessNode::new(&next.value));
                    self.wrap_current_with(indexed);
                    self.current_index += 2;
                } else {
                    return Err("Unexpected token [".to_string());
                }
            } else {
                return Err("Unexpected token [".to_string());
            }
        } else {
            return Err("Unexpected token [".to_string());
        }
        Ok(())
    }

    fn handle_open_round_bracket(&mut self) -> Result<(), String> {
        // ( creates encapsulation or is part of callable
        let encap = ParseTree::Encapsulation(EncapsulationNode::new());
        self.add_child(encap);
        Ok(())
    }

    fn handle_closed_round_bracket(&mut self) -> Result<(), String> {
        // ) closes encapsulation or callable
        self.close_to_type(&[
            "Encapsulation",
            "Callable",
            "Method",
        ]);

        // Mark as terminated
        self.parse_tree.set_terminated(true);
        Ok(())
    }

    fn handle_greater_than(&mut self) -> Result<(), String> {
        // > closes generic
        self.close_to_type(&["Generic"]);
        self.parse_tree.set_terminated(true);
        Ok(())
    }

    fn handle_close_brace(&mut self) -> Result<(), String> {
        // } closes keyed array
        self.close_to_type(&["KeyedArray"]);
        self.parse_tree.set_terminated(true);
        Ok(())
    }

    fn handle_comma(&mut self) -> Result<(), String> {
        // , separates items in generic, keyed array, or callable
        self.close_to_type(&["Generic", "KeyedArray", "Callable", "Method"]);
        Ok(())
    }

    fn handle_ellipsis_or_equals(&mut self, token: &TypeToken) -> Result<(), String> {
        if token.value == "..." {
            // Check if we're in a keyed array - creates field ellipsis
            if matches!(&self.parse_tree, ParseTree::KeyedArray(_)) {
                let ellipsis = ParseTree::FieldEllipsis(ParseTreeNode::new());
                self.add_child(ellipsis);
                return Ok(());
            }

            // In callable context - variadic param
            let param = ParseTree::CallableParam(CallableParamNode {
                children: Vec::new(),
                variadic: true,
                has_default: false,
                name: None,
            });
            self.wrap_current_with(param);
        } else {
            // = means optional param
            let param = ParseTree::CallableParam(CallableParamNode {
                children: Vec::new(),
                variadic: false,
                has_default: true,
                name: None,
            });
            self.wrap_current_with(param);
        }
        Ok(())
    }

    fn handle_colon(&mut self) -> Result<(), String> {
        // : either introduces return type or keyed array property

        // Check if current is a callable - this is return type
        if matches!(&self.parse_tree, ParseTree::Callable(_)) {
            let ret = ParseTree::CallableWithReturnType(ParseTreeNode::new());
            self.wrap_current_with(ret);
            return Ok(());
        }

        // Check if current is a method
        if matches!(&self.parse_tree, ParseTree::Method(_)) {
            let ret = ParseTree::MethodWithReturnType(ParseTreeNode::new());
            self.wrap_current_with(ret);
            return Ok(());
        }

        // Otherwise this is a keyed array property
        if let ParseTree::Value(ref v) = self.parse_tree {
            let prev_token = if self.current_index > 0 {
                self.tokens.get(self.current_index - 1)
            } else {
                None
            };
            let possibly_undefined = prev_token.map(|t| t.value == "?").unwrap_or(false);

            let prop = KeyedArrayPropertyNode {
                value: v.value.clone(),
                children: Vec::new(),
                possibly_undefined,
            };
            self.parse_tree = ParseTree::KeyedArrayProperty(prop);
        }

        Ok(())
    }

    fn handle_space(&mut self) -> Result<(), String> {
        // Space in callable context creates param with name
        let next = self.tokens.get(self.current_index + 1);
        if let Some(next_token) = next {
            if next_token.value.starts_with('$') {
                // This is a parameter name
                let name = next_token.value[1..].to_string();
                let param = CallableParamNode {
                    children: Vec::new(),
                    variadic: false,
                    has_default: false,
                    name: if name.is_empty() { None } else { Some(name) },
                };
                self.wrap_current_with(ParseTree::CallableParam(param));
                self.current_index += 1;
            }
        }
        Ok(())
    }

    fn handle_question_mark(&mut self) -> Result<(), String> {
        let next = self.tokens.get(self.current_index + 1);

        // Check if this is ?: for keyed array optional
        if next.map(|t| t.value.as_str()) == Some(":") {
            // This will be handled by handle_colon
            return Ok(());
        }

        // Check for conditional type
        if matches!(&self.parse_tree, ParseTree::TemplateIs(_)) {
            let condition = std::mem::replace(
                &mut self.parse_tree,
                ParseTree::Root(ParseTreeNode::new()),
            );
            self.parse_tree = ParseTree::Conditional(ConditionalNode::new(condition));
            return Ok(());
        }

        // Otherwise this is nullable
        let nullable = ParseTree::Nullable(ParseTreeNode::new());
        self.add_child(nullable);
        Ok(())
    }

    fn handle_bar(&mut self) -> Result<(), String> {
        // | creates union
        self.close_to_union_level();

        if !matches!(&self.parse_tree, ParseTree::Union(_)) {
            let union_node = ParseTree::Union(ParseTreeNode {
                children: vec![self.parse_tree.clone()],
                terminated: false,
            });
            self.parse_tree = union_node;
        }
        Ok(())
    }

    fn handle_ampersand(&mut self) -> Result<(), String> {
        // & creates intersection
        if !matches!(&self.parse_tree, ParseTree::Intersection(_)) {
            let intersection = ParseTree::Intersection(ParseTreeNode {
                children: vec![self.parse_tree.clone()],
                terminated: false,
            });
            self.parse_tree = intersection;
        }
        Ok(())
    }

    fn handle_is_or_as(&mut self, token: &TypeToken) -> Result<(), String> {
        if self.current_index == 0 {
            // At start, treat as value
            return self.handle_value(token);
        }

        match token.value.as_str() {
            "as" | "of" => {
                let next = self.tokens.get(self.current_index + 1);
                if let (ParseTree::Value(v), Some(next_token)) = (&self.parse_tree, next) {
                    let template_as = ParseTree::TemplateAs(TemplateAsNode::new(
                        &v.value,
                        &next_token.value,
                    ));
                    self.parse_tree = template_as;
                    self.current_index += 1;
                } else {
                    return Err(format!("Unexpected token {}", token.value));
                }
            }
            "is" => {
                if let ParseTree::Value(v) = &self.parse_tree {
                    let template_is = ParseTree::TemplateIs(TemplateIsNode::new(&v.value));
                    self.parse_tree = template_is;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_value(&mut self, token: &TypeToken) -> Result<(), String> {
        let next = self.tokens.get(self.current_index + 1);

        let new_node = match next.map(|t| t.value.as_str()) {
            Some("<") => {
                // Generic type
                self.current_index += 1;
                ParseTree::Generic(GenericNode::new(&token.value))
            }
            Some("{") => {
                // Keyed array
                self.current_index += 1;

                // Check for empty {}
                let nexter = self.tokens.get(self.current_index + 1);
                let mut node = KeyedArrayNode::new(&token.value);
                if nexter.map(|t| t.value.as_str()) == Some("}") {
                    node.terminated = true;
                    self.current_index += 1;
                }
                ParseTree::KeyedArray(node)
            }
            Some("(") => {
                // Callable or method
                let value_lower = token.value.to_lowercase();
                self.current_index += 1;

                if matches!(
                    value_lower.as_str(),
                    "callable" | "pure-callable" | "closure" | "\\closure" | "pure-closure"
                ) {
                    ParseTree::Callable(CallableNode::new(&token.value))
                } else if matches!(&self.parse_tree, ParseTree::Root(_)) {
                    // Method at root
                    ParseTree::Method(MethodNode::new(&token.value))
                } else {
                    return Err(format!(
                        "Parenthesis must be preceded by callable or a method name"
                    ));
                }
            }
            Some("::") => {
                // Class constant
                let nexter = self.tokens.get(self.current_index + 2);
                if let Some(const_token) = nexter {
                    let full_value = format!("{}::{}", token.value, const_token.value);
                    self.current_index += 2;
                    ParseTree::Value(ValueNode::new(
                        full_value,
                        token.offset,
                        const_token.offset + const_token.value.len(),
                    ))
                } else {
                    return Err("Invalid class constant".to_string());
                }
            }
            _ => {
                // Simple value
                let value = if token.value == "$this" {
                    "static".to_string()
                } else {
                    token.value.clone()
                };
                ParseTree::Value(ValueNode::with_text(
                    value,
                    token.offset,
                    token.offset + token.value.len(),
                    token.original_text.clone(),
                ))
            }
        };

        self.add_child(new_node);
        Ok(())
    }

    fn add_child(&mut self, child: ParseTree) {
        match &mut self.parse_tree {
            ParseTree::Root(n) => {
                if n.children.is_empty() {
                    self.parse_tree = child;
                } else {
                    n.children.push(child);
                }
            }
            ParseTree::Generic(n) => n.children.push(child),
            ParseTree::Union(n) => n.children.push(child),
            ParseTree::Intersection(n) => n.children.push(child),
            ParseTree::Nullable(n) => n.children.push(child),
            ParseTree::KeyedArray(n) => n.children.push(child),
            ParseTree::KeyedArrayProperty(n) => n.children.push(child),
            ParseTree::Callable(n) => n.children.push(child),
            ParseTree::CallableWithReturnType(n) => n.children.push(child),
            ParseTree::CallableParam(n) => n.children.push(child),
            ParseTree::Encapsulation(n) => n.children.push(child),
            ParseTree::Method(n) => n.children.push(child),
            ParseTree::MethodWithReturnType(n) => n.children.push(child),
            ParseTree::MethodParam(n) => n.children.push(child),
            ParseTree::TemplateAs(n) => n.children.push(child),
            ParseTree::TemplateIs(n) => n.children.push(child),
            ParseTree::Conditional(n) => n.children.push(child),
            ParseTree::IndexedAccess(n) => n.children.push(child),
            ParseTree::FieldEllipsis(n) => n.children.push(child),
            ParseTree::Value(_) => {
                // Replace with new node
                self.parse_tree = child;
            }
        }
    }

    fn wrap_current_with(&mut self, mut wrapper: ParseTree) {
        let current = std::mem::replace(&mut self.parse_tree, ParseTree::Root(ParseTreeNode::new()));
        wrapper.children_mut().push(current);
        self.parse_tree = wrapper;
    }

    fn close_to_type(&mut self, type_names: &[&str]) {
        // This is a simplified version - in the full implementation,
        // we'd navigate the tree structure
        let _ = type_names;
    }

    fn close_to_union_level(&mut self) {
        // Navigate up to union level
    }
}
