//! Token → parse-tree stage.
//!
//! Faithful port of Psalm's `Internal/Type/ParseTreeCreator.php`. Every Psalm
//! `$this->current_leaf->parent`, `$parent->children[] = ...` and
//! `array_pop($parent->children)` maps onto an arena operation in
//! [`super::parse_tree`], so the control flow follows Psalm method-by-method,
//! block-by-block.

use super::parse_tree::{NodeId, NodeKind, ParseTreeArena};
use super::type_tokenizer::TypeToken;

/// Mirrors Psalm's `ParseTreeCreator`.
pub struct ParseTreeCreator {
    tree: ParseTreeArena,
    /// `$this->parse_tree` — the root of the tree (may be reassigned).
    parse_tree: NodeId,
    /// `$this->current_leaf`.
    current_leaf: NodeId,
    type_tokens: Vec<TypeToken>,
    type_token_count: usize,
    /// `$this->t`.
    t: usize,
}

impl ParseTreeCreator {
    pub fn new(type_tokens: Vec<TypeToken>) -> Self {
        let mut tree = ParseTreeArena::new();
        let root = tree.alloc(NodeKind::Root, None);
        let type_token_count = type_tokens.len();
        Self {
            tree,
            parse_tree: root,
            current_leaf: root,
            type_tokens,
            type_token_count,
            t: 0,
        }
    }

    // ---- token helpers ----------------------------------------------------

    /// `$this->type_tokens[idx][0]` (the token value), if it exists.
    fn tok(&self, idx: usize) -> Option<&str> {
        self.type_tokens.get(idx).map(|t| t.value.as_str())
    }

    /// First char of a token value, mirroring `$token[0][0]`.
    fn first_char(value: &str) -> Option<char> {
        value.chars().next()
    }

    // ---- entry point ------------------------------------------------------

    /// Build the tree. Returns the arena and the id of the root node.
    pub fn create(mut self) -> Result<(ParseTreeArena, NodeId), String> {
        while self.t < self.type_token_count {
            let value = self.type_tokens[self.t].value.clone();

            match value.as_str() {
                "{" | "]" => {
                    return Err(format!("Unexpected token {}", value));
                }
                "<" => self.handle_less_than()?,
                "[" => self.handle_open_square_bracket()?,
                "(" => self.handle_open_round_bracket()?,
                ")" => self.handle_closed_round_bracket()?,
                ">" => {
                    loop {
                        match self.tree.parent(self.current_leaf) {
                            None => return Err("Cannot parse generic type".to_string()),
                            Some(p) => self.current_leaf = p,
                        }
                        if self.tree.is_generic(self.current_leaf) {
                            break;
                        }
                    }
                    self.tree.set_terminated(self.current_leaf, true);
                }
                "}" => {
                    loop {
                        match self.tree.parent(self.current_leaf) {
                            None => return Err("Cannot parse array type".to_string()),
                            Some(p) => self.current_leaf = p,
                        }
                        if self.tree.is_keyed_array(self.current_leaf) {
                            break;
                        }
                    }
                    self.tree.set_terminated(self.current_leaf, true);
                }
                "," => self.handle_comma()?,
                "..." | "=" => self.handle_ellipsis_or_equals(&value)?,
                ":" => self.handle_colon()?,
                " " => self.handle_space()?,
                "?" => self.handle_question_mark()?,
                "|" => self.handle_bar()?,
                "&" => self.handle_ampersand()?,
                "is" | "as" | "of" => self.handle_is_or_as(&value)?,
                // Inline variance modifiers in generic type parameters
                // (PHPStan syntax, e.g. `Foo<covariant string>`) are silently
                // skipped. Mirrors Psalm f6cbb808.
                "covariant" | "contravariant" => {
                    if self.t + 1 < self.type_token_count
                        && self.type_tokens[self.t + 1].value == " "
                    {
                        self.t += 1;
                    }
                }
                _ => self.handle_value(self.t)?,
            }

            self.t += 1;
        }

        // $this->parse_tree->cleanParents(); — a no-op in the arena (parent
        // links are plain indices, not refcounted pointers).

        if self.current_leaf != self.parse_tree
            && (self.tree.is_generic(self.parse_tree)
                || self.tree.is_callable(self.parse_tree)
                || self.tree.is_keyed_array(self.parse_tree))
        {
            return Err("Unterminated bracket".to_string());
        }

        Ok((self.tree, self.parse_tree))
    }

    // ---- createMethodParam ------------------------------------------------

    /// Port of `createMethodParam`. `current_token_idx` indexes the token that
    /// triggered this (the `&`, `...`, or `$name`).
    fn create_method_param(
        &mut self,
        mut current_token_idx: usize,
        current_parent: NodeId,
    ) -> Result<(), String> {
        let mut byref = false;
        let mut variadic = false;
        let mut has_default = false;
        let mut default = String::new();

        let mut current_value = self
            .type_tokens
            .get(current_token_idx)
            .map(|t| t.value.clone());

        match current_value.as_deref() {
            Some("&") => {
                byref = true;
                self.t += 1;
                current_token_idx = self.t;
                current_value = self.type_tokens.get(self.t).map(|t| t.value.clone());
            }
            Some("...") => {
                variadic = true;
                self.t += 1;
                current_token_idx = self.t;
                current_value = self.type_tokens.get(self.t).map(|t| t.value.clone());
            }
            _ => {}
        }
        let _ = current_token_idx;

        let name = match &current_value {
            Some(v) if Self::first_char(v) == Some('$') => v.clone(),
            _ => return Err("Unexpected token after space".to_string()),
        };

        let new_parent_leaf = self.tree.alloc(
            NodeKind::MethodParam {
                name,
                byref,
                variadic,
                default: String::new(),
            },
            Some(current_parent),
        );

        let mut j = self.t + 1;
        while j < self.type_token_count {
            let ahead = self.type_tokens[j].value.clone();

            if ahead == "," || (ahead == ")" && self.type_tokens[j - 1].value != "(") {
                self.t = j - 1;
                break;
            }

            if has_default {
                default.push_str(&ahead);
            }

            if ahead == "=" {
                has_default = true;
                j += 1;
                continue;
            }

            if j == self.type_token_count - 1 {
                return Err("Unterminated method".to_string());
            }

            j += 1;
        }

        if let NodeKind::MethodParam { default: d, .. } =
            &mut self.tree.get_mut(new_parent_leaf).kind
        {
            *d = default;
        }

        if self.current_leaf != current_parent {
            let cl = self.current_leaf;
            self.tree.set_children(new_parent_leaf, vec![cl]);
            self.tree.pop_child(current_parent);
        }

        self.tree.push_child(current_parent, new_parent_leaf);
        self.current_leaf = new_parent_leaf;
        Ok(())
    }

    // ---- parseCallableParam ----------------------------------------------

    fn parse_callable_param(
        &mut self,
        current_token_idx: usize,
        current_parent: NodeId,
    ) -> Result<(), String> {
        let mut variadic = false;
        let mut has_default = false;

        let mut current_value = self
            .type_tokens
            .get(current_token_idx)
            .map(|t| t.value.clone());

        match current_value.as_deref() {
            Some("&") => {
                self.t += 1;
                current_value = self.type_tokens.get(self.t).map(|t| t.value.clone());
            }
            Some("...") => {
                variadic = true;
                self.t += 1;
                current_value = self.type_tokens.get(self.t).map(|t| t.value.clone());
            }
            Some("=") => {
                has_default = true;
                self.t += 1;
                current_value = self.type_tokens.get(self.t).map(|t| t.value.clone());
            }
            _ => {}
        }

        let value = match &current_value {
            Some(v) if Self::first_char(v) == Some('$') && v.chars().count() >= 2 => v.clone(),
            _ => return Err("Unexpected token after space".to_string()),
        };

        let potential_name: String = value.chars().skip(1).collect();
        let name = if potential_name.is_empty() {
            None
        } else {
            Some(potential_name)
        };

        let new_leaf = self.tree.alloc(
            NodeKind::CallableParam {
                variadic,
                has_default,
                name,
            },
            Some(current_parent),
        );

        if current_parent != self.current_leaf {
            let cl = self.current_leaf;
            self.tree.set_children(new_leaf, vec![cl]);
            self.tree.pop_child(current_parent);
        }
        self.tree.push_child(current_parent, new_leaf);
        self.current_leaf = new_leaf;
        Ok(())
    }

    // ---- handlers ---------------------------------------------------------

    fn handle_less_than(&mut self) -> Result<(), String> {
        if !self.tree.is_field_ellipsis(self.current_leaf) {
            return Err("Unexpected token <".to_string());
        }

        let current_parent = match self.tree.parent(self.current_leaf) {
            Some(p) if self.tree.is_keyed_array(p) => p,
            _ => return Err("Unexpected token <".to_string()),
        };

        self.tree.pop_child(current_parent);

        let generic_leaf = self.tree.alloc(
            NodeKind::Generic {
                value: String::new(),
            },
            Some(current_parent),
        );
        self.tree.push_child(current_parent, generic_leaf);
        self.current_leaf = generic_leaf;
        Ok(())
    }

    fn handle_open_square_bracket(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected token [".to_string());
        }

        let mut indexed_access = false;

        let next_value = self.tok(self.t + 1).map(|s| s.to_string());

        if next_value.as_deref() != Some("]") {
            let next_next_value = self.tok(self.t + 2).map(|s| s.to_string());
            if next_next_value.as_deref() == Some("]") {
                indexed_access = true;
                self.t += 1;
            } else {
                return Err("Unexpected token [".to_string());
            }
        }

        let current_parent = self.tree.parent(self.current_leaf);

        let new_parent_leaf = if indexed_access {
            let next_value = match next_value {
                Some(v) => v,
                None => return Err("Unexpected token [".to_string()),
            };
            self.tree.alloc(
                NodeKind::IndexedAccess { value: next_value },
                current_parent,
            )
        } else {
            if self.tree.is_keyed_array_property(self.current_leaf) {
                return Err("Unexpected token [".to_string());
            }
            self.tree.alloc(
                NodeKind::Generic {
                    value: "array".to_string(),
                },
                current_parent,
            )
        };

        let cl = self.current_leaf;
        self.tree.set_parent(cl, Some(new_parent_leaf));
        self.tree.set_children(new_parent_leaf, vec![cl]);

        if let Some(cp) = current_parent {
            self.tree.pop_child(cp);
            self.tree.push_child(cp, new_parent_leaf);
        } else {
            self.parse_tree = new_parent_leaf;
        }

        self.current_leaf = new_parent_leaf;
        self.t += 1;
        Ok(())
    }

    fn handle_open_round_bracket(&mut self) -> Result<(), String> {
        if self.tree.is_value(self.current_leaf) {
            return Err("Unrecognised token (".to_string());
        }

        let new_parent = if self.tree.is_root(self.current_leaf) {
            None
        } else {
            Some(self.current_leaf)
        };

        let new_leaf = self.tree.alloc(NodeKind::Encapsulation, new_parent);

        if self.tree.is_root(self.current_leaf) {
            self.current_leaf = new_leaf;
            self.parse_tree = new_leaf;
            return Ok(());
        }

        if let Some(p) = self.tree.parent(new_leaf) {
            self.tree.push_child(p, new_leaf);
        }

        self.current_leaf = new_leaf;
        Ok(())
    }

    fn handle_closed_round_bracket(&mut self) -> Result<(), String> {
        let prev_is_open = self.t > 0 && self.tok(self.t - 1) == Some("(");

        if prev_is_open && self.tree.is_callable(self.current_leaf) {
            return Ok(());
        }

        loop {
            match self.tree.parent(self.current_leaf) {
                None => break,
                Some(p) => self.current_leaf = p,
            }
            if self.tree.is_encapsulation(self.current_leaf)
                || self.tree.is_callable(self.current_leaf)
                || self.tree.is_method(self.current_leaf)
            {
                break;
            }
        }

        if self.tree.is_encapsulation(self.current_leaf) || self.tree.is_callable(self.current_leaf)
        {
            self.tree.set_terminated(self.current_leaf, true);
        }
        Ok(())
    }

    fn handle_comma(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected token ,".to_string());
        }

        if self.tree.parent(self.current_leaf).is_none() {
            return Err("Cannot parse comma without a parent node".to_string());
        }

        let mut context_node = Some(self.current_leaf);

        if let Some(cn) = context_node
            && self.is_generic_keyed_callable_method(cn)
        {
            context_node = self.tree.parent(cn);
        }

        while let Some(cn) = context_node {
            if self.is_generic_keyed_callable_method(cn) {
                break;
            }
            context_node = self.tree.parent(cn);
        }

        match context_node {
            Some(cn) => {
                self.current_leaf = cn;
                Ok(())
            }
            None => Err("Cannot parse comma in non-generic/array type".to_string()),
        }
    }

    fn is_generic_keyed_callable_method(&self, id: NodeId) -> bool {
        self.tree.is_generic(id)
            || self.tree.is_keyed_array(id)
            || self.tree.is_callable(id)
            || self.tree.is_method(id)
    }

    fn handle_ellipsis_or_equals(&mut self, type_token: &str) -> Result<(), String> {
        let prev = if self.t > 0 {
            self.tok(self.t - 1)
        } else {
            None
        };
        if matches!(prev, Some("...") | Some("=")) {
            return Err("Cannot have duplicate tokens".to_string());
        }

        let mut current_parent = self.tree.parent(self.current_leaf);

        if self.tree.is_method(self.current_leaf) && type_token == "..." {
            return self.create_method_param(self.t, self.current_leaf);
        }

        if self.tree.is_keyed_array(self.current_leaf) && type_token == "..." {
            let cl = self.current_leaf;
            let leaf = self.tree.alloc(NodeKind::FieldEllipsis, Some(cl));
            self.tree.push_child(cl, leaf);
            self.current_leaf = leaf;
            return Ok(());
        }

        while let Some(cp) = current_parent {
            if self.tree.is_callable(cp) || self.tree.is_callable_param(cp) {
                break;
            }
            self.current_leaf = cp;
            current_parent = self.tree.parent(cp);
        }

        if current_parent.is_none() {
            if type_token == "..." {
                if self.tree.is_callable(self.current_leaf) {
                    current_parent = Some(self.current_leaf);
                } else {
                    return Err(format!("Unexpected token {}", type_token));
                }
            } else {
                return Err(format!("Unexpected token {}", type_token));
            }
        }

        let current_parent = current_parent.unwrap();

        if self.tree.is_callable_param(current_parent) {
            return Err("Cannot have variadic param with a default".to_string());
        }

        let new_leaf = self.tree.alloc(
            NodeKind::CallableParam {
                variadic: type_token == "...",
                has_default: type_token == "=",
                name: None,
            },
            Some(current_parent),
        );

        if current_parent != self.current_leaf {
            let cl = self.current_leaf;
            self.tree.set_children(new_leaf, vec![cl]);
            self.tree.pop_child(current_parent);
        }
        self.tree.push_child(current_parent, new_leaf);
        self.current_leaf = new_leaf;
        Ok(())
    }

    fn handle_colon(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected token :".to_string());
        }

        let current_parent = self.tree.parent(self.current_leaf);

        if self.tree.is_callable(self.current_leaf) {
            let new_parent_leaf = self
                .tree
                .alloc(NodeKind::CallableWithReturnType, current_parent);
            let cl = self.current_leaf;
            self.tree.set_parent(cl, Some(new_parent_leaf));
            self.tree.set_children(new_parent_leaf, vec![cl]);

            if let Some(cp) = current_parent {
                self.tree.pop_child(cp);
                self.tree.push_child(cp, new_parent_leaf);
            } else {
                self.parse_tree = new_parent_leaf;
            }

            self.current_leaf = new_parent_leaf;
            return Ok(());
        }

        if self.tree.is_method(self.current_leaf) {
            let new_parent_leaf = self
                .tree
                .alloc(NodeKind::MethodWithReturnType, current_parent);
            let cl = self.current_leaf;
            self.tree.set_parent(cl, Some(new_parent_leaf));
            self.tree.set_children(new_parent_leaf, vec![cl]);

            if let Some(cp) = current_parent {
                self.tree.pop_child(cp);
                self.tree.push_child(cp, new_parent_leaf);
            } else {
                self.parse_tree = new_parent_leaf;
            }

            self.current_leaf = new_parent_leaf;
            return Ok(());
        }

        let mut current_parent = current_parent;

        if let Some(cp) = current_parent
            && self.tree.is_keyed_array_property(cp)
        {
            return Ok(());
        }

        while matches!(current_parent, Some(cp) if self.tree.is_union(cp) || self.tree.is_callable_with_return_type(cp))
            && self.tree.parent(self.current_leaf).is_some()
        {
            self.current_leaf = self.tree.parent(self.current_leaf).unwrap();
            current_parent = self.tree.parent(self.current_leaf);
        }

        if let Some(cp) = current_parent
            && self.tree.is_conditional(cp)
        {
            if self.tree.children(cp).len() > 1 {
                return Err("Cannot process colon in conditional twice".to_string());
            }
            self.current_leaf = cp;
            return Ok(());
        }

        let current_parent = match current_parent {
            Some(cp) => cp,
            None => return Err("Cannot process colon without parent".to_string()),
        };

        if !self.tree.is_value(self.current_leaf) {
            return Err("Unexpected LHS of property".to_string());
        }

        if !self.tree.is_keyed_array(current_parent) {
            return Err("Saw : outside of object-like array".to_string());
        }

        let prev_is_question = self.t > 0 && self.tok(self.t - 1) == Some("?");
        let value = self.tree.value(self.current_leaf).unwrap_or("").to_string();

        let new_parent_leaf = self
            .tree
            .alloc(NodeKind::KeyedArrayProperty { value }, Some(current_parent));
        self.tree
            .set_possibly_undefined(new_parent_leaf, prev_is_question);
        self.tree.pop_child(current_parent);
        self.tree.push_child(current_parent, new_parent_leaf);
        self.current_leaf = new_parent_leaf;
        Ok(())
    }

    fn handle_space(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected space".to_string());
        }

        if self.tree.is_keyed_array(self.current_leaf) {
            return Ok(());
        }

        let mut current_parent = self.tree.parent(self.current_leaf);

        while let Some(cp) = current_parent {
            if self.tree.is_method(cp) || self.tree.is_callable(cp) {
                break;
            }
            self.current_leaf = cp;
            current_parent = self.tree.parent(cp);
        }

        let next_exists = self.t + 1 < self.type_token_count;

        let cp = match current_parent {
            Some(cp) if (self.tree.is_method(cp) || self.tree.is_callable(cp)) && next_exists => cp,
            _ => return Err("Unexpected space".to_string()),
        };

        if self.tree.is_method(cp) {
            self.t += 1;
            self.create_method_param(self.t, cp)?;
        } else if self.tree.is_callable(cp) {
            self.t += 1;
            self.parse_callable_param(self.t, cp)?;
        }
        Ok(())
    }

    fn handle_question_mark(&mut self) -> Result<(), String> {
        let next_value = self.tok(self.t + 1).map(|s| s.to_string());

        if next_value.as_deref() == Some(":") {
            return Ok(());
        }

        // Walk up over already-complete leaves.
        while self.question_mark_should_ascend(self.current_leaf)
            && self.tree.parent(self.current_leaf).is_some()
        {
            self.current_leaf = self.tree.parent(self.current_leaf).unwrap();
        }

        if self.tree.is_template_is(self.current_leaf)
            && self.tree.parent(self.current_leaf).is_some()
        {
            let condition = self.current_leaf;
            let current_parent = self.tree.parent(self.current_leaf).unwrap();

            let new_leaf = self
                .tree
                .alloc(NodeKind::Conditional { condition }, Some(current_parent));

            self.tree.pop_child(current_parent);
            self.tree.push_child(current_parent, new_leaf);
            self.current_leaf = new_leaf;
        } else {
            let new_parent = if self.tree.is_root(self.current_leaf) {
                None
            } else {
                Some(self.current_leaf)
            };

            if next_value.is_none() {
                return Err("Unexpected token ?".to_string());
            }

            let new_leaf = self.tree.alloc(NodeKind::Nullable, new_parent);

            if self.tree.is_root(self.current_leaf) {
                self.current_leaf = new_leaf;
                self.parse_tree = new_leaf;
                return Ok(());
            }

            if let Some(p) = self.tree.parent(new_leaf) {
                self.tree.push_child(p, new_leaf);
            }

            self.current_leaf = new_leaf;
        }
        Ok(())
    }

    fn question_mark_should_ascend(&self, id: NodeId) -> bool {
        self.tree.is_value(id)
            || self.tree.is_union(id)
            || (self.tree.is_keyed_array(id) && self.tree.terminated(id))
            || (self.tree.is_generic(id) && self.tree.terminated(id))
            || (self.tree.is_encapsulation(id) && self.tree.terminated(id))
            || (self.tree.is_callable(id) && self.tree.terminated(id))
            || self.tree.is_intersection(id)
    }

    fn handle_bar(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected token |".to_string());
        }

        let mut current_parent = self.tree.parent(self.current_leaf);

        if matches!(current_parent, Some(cp) if self.tree.is_callable_with_return_type(cp)) {
            self.current_leaf = current_parent.unwrap();
            current_parent = self.tree.parent(self.current_leaf);
        }

        if matches!(current_parent, Some(cp) if self.tree.is_nullable(cp)) {
            self.current_leaf = current_parent.unwrap();
            current_parent = self.tree.parent(self.current_leaf);
        }

        if self.tree.is_union(self.current_leaf) {
            return Err("Unexpected token |".to_string());
        }

        if matches!(current_parent, Some(cp) if self.tree.is_union(cp)) {
            self.current_leaf = current_parent.unwrap();
            return Ok(());
        }

        if matches!(current_parent, Some(cp) if self.tree.is_intersection(cp)) {
            self.current_leaf = current_parent.unwrap();
            current_parent = self.tree.parent(self.current_leaf);
        }

        // Both Psalm branches end up with parent = current_parent and
        // children = [current_leaf]; the TemplateIs branch only differs in the
        // (immediately overwritten) constructor argument.
        let cl = self.current_leaf;
        let new_parent_leaf = self.tree.alloc(NodeKind::Union, current_parent);
        self.tree.set_children(new_parent_leaf, vec![cl]);

        if let Some(cp) = current_parent {
            self.tree.pop_child(cp);
            self.tree.push_child(cp, new_parent_leaf);
        } else {
            self.parse_tree = new_parent_leaf;
        }

        self.current_leaf = new_parent_leaf;
        Ok(())
    }

    fn handle_ampersand(&mut self) -> Result<(), String> {
        if self.tree.is_root(self.current_leaf) {
            return Err("Unexpected &".to_string());
        }

        let current_parent = self.tree.parent(self.current_leaf);

        if let Some(cp) = current_parent {
            if self.tree.is_method(cp) {
                return self.create_method_param(self.t, cp);
            }
            if self.tree.is_intersection(cp) {
                self.current_leaf = cp;
                return Ok(());
            }
        }

        let cl = self.current_leaf;
        let new_parent_leaf = self.tree.alloc(NodeKind::Intersection, current_parent);
        self.tree.set_children(new_parent_leaf, vec![cl]);

        if let Some(cp) = current_parent {
            self.tree.pop_child(cp);
            self.tree.push_child(cp, new_parent_leaf);
        } else {
            self.parse_tree = new_parent_leaf;
        }

        self.current_leaf = new_parent_leaf;
        Ok(())
    }

    fn handle_is_or_as(&mut self, type_token: &str) -> Result<(), String> {
        if self.t == 0 {
            return self.handle_value(self.t);
        }

        let current_parent = self.tree.parent(self.current_leaf);

        if let Some(cp) = current_parent {
            self.tree.pop_child(cp);
        }

        if type_token == "as" || type_token == "of" {
            let next_value = self.tok(self.t + 1).map(|s| s.to_string());

            let is_value = self.tree.is_value(self.current_leaf);
            let parent_is_generic = matches!(current_parent, Some(cp) if self.tree.is_generic(cp));

            if !is_value || !parent_is_generic || next_value.is_none() {
                return Err(format!("Unexpected token {}", type_token));
            }

            let param_name = self.tree.value(self.current_leaf).unwrap_or("").to_string();
            let as_type = next_value.unwrap();
            let cp = current_parent.unwrap();

            let new_leaf = self.tree.alloc(
                NodeKind::TemplateAs {
                    param_name,
                    as_type,
                },
                Some(cp),
            );
            self.tree.push_child(cp, new_leaf);
            self.current_leaf = new_leaf;
            self.t += 1;
        } else if self.tree.is_value(self.current_leaf) {
            let param_name = self.tree.value(self.current_leaf).unwrap_or("").to_string();
            let new_leaf = self
                .tree
                .alloc(NodeKind::TemplateIs { param_name }, current_parent);
            if let Some(cp) = current_parent {
                self.tree.push_child(cp, new_leaf);
            }
            self.current_leaf = new_leaf;
        }
        Ok(())
    }

    fn handle_value(&mut self, type_token_idx: usize) -> Result<(), String> {
        let new_parent = if self.tree.is_root(self.current_leaf) {
            None
        } else {
            Some(self.current_leaf)
        };

        let mut value = self.type_tokens[type_token_idx].value.clone();
        let offset = self.type_tokens[type_token_idx].offset;
        let text = self.type_tokens[type_token_idx].text.clone();

        if self.tree.is_method(self.current_leaf) && Self::first_char(&value) == Some('$') {
            return self.create_method_param(type_token_idx, self.current_leaf);
        }

        let next_value = self.tok(self.t + 1).map(|s| s.to_string());

        let new_leaf: NodeId = match next_value.as_deref() {
            Some("<") => {
                let id = self.tree.alloc(NodeKind::Generic { value }, new_parent);
                self.t += 1;
                id
            }
            Some("{") => {
                self.t += 1;

                let nexter_value = self.tok(self.t + 1).map(|s| s.to_string());

                if nexter_value
                    .as_deref()
                    .map(|s| s.contains('@'))
                    .unwrap_or(false)
                    && value != "list"
                    && value != "array"
                {
                    self.t = self.type_token_count;
                    if value == "$this" {
                        value = "static".to_string();
                    }
                    let len = value.chars().count();
                    self.tree.alloc(
                        NodeKind::Value {
                            value,
                            offset_start: offset,
                            offset_end: offset + len,
                            text,
                        },
                        new_parent,
                    )
                } else {
                    let id = self.tree.alloc(NodeKind::KeyedArray { value }, new_parent);

                    match nexter_value.as_deref() {
                        Some("}") => {
                            self.tree.set_terminated(id, true);
                            self.t += 1;
                        }
                        None => {
                            return Err("Unclosed bracket in keyed array".to_string());
                        }
                        _ => {}
                    }
                    id
                }
            }
            Some("(") => {
                let id = if matches!(
                    value.as_str(),
                    "callable" | "pure-callable" | "Closure" | "\\Closure" | "pure-Closure"
                ) {
                    self.tree.alloc(NodeKind::Callable { value }, new_parent)
                } else if Self::first_char(&value) != Some('\\')
                    && self.tree.is_root(self.current_leaf)
                {
                    self.tree.alloc(NodeKind::Method { value }, new_parent)
                } else {
                    return Err(
                        "Parenthesis must be preceded by \u{201c}Closure\u{201d}, \u{201c}callable\u{201d}, \"pure-callable\" or a valid @method name"
                            .to_string(),
                    );
                };
                self.t += 1;
                id
            }
            Some("::") => {
                let nexter_value = self.tok(self.t + 2).map(|s| s.to_string());

                let valid = match &nexter_value {
                    Some(n) => is_class_constant_name(n) || n.to_lowercase() == "class",
                    None => false,
                };
                if !valid {
                    return Err(format!(
                        "Invalid class constant {}",
                        nexter_value.as_deref().unwrap_or("<empty>")
                    ));
                }
                let nexter = nexter_value.unwrap();
                let combined = format!("{}::{}", value, nexter);
                let len = nexter.chars().count();
                let id = self.tree.alloc(
                    NodeKind::Value {
                        value: combined,
                        offset_start: offset,
                        offset_end: offset + 2 + len,
                        text,
                    },
                    new_parent,
                );
                self.t += 2;
                id
            }
            _ => {
                if value == "$this" {
                    value = "static".to_string();
                }
                let len = value.chars().count();
                self.tree.alloc(
                    NodeKind::Value {
                        value,
                        offset_start: offset,
                        offset_end: offset + len,
                        text,
                    },
                    new_parent,
                )
            }
        };

        if self.tree.is_root(self.current_leaf) {
            self.current_leaf = new_leaf;
            self.parse_tree = new_leaf;
            return Ok(());
        }

        if let Some(p) = self.tree.parent(new_leaf) {
            self.tree.push_child(p, new_leaf);
        }

        self.current_leaf = new_leaf;
        Ok(())
    }
}

/// `preg_match('/^([a-zA-Z_][a-zA-Z_0-9]*\*?|\*)$/', ...)` — a class-constant
/// name (optionally a trailing `*` wildcard) or a bare `*`.
fn is_class_constant_name(s: &str) -> bool {
    if s == "*" {
        return true;
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let rest: Vec<char> = chars.collect();
    let (body, last_is_star) = match rest.last() {
        Some('*') => (&rest[..rest.len() - 1], true),
        _ => (&rest[..], false),
    };
    let _ = last_is_star;
    body.iter().all(|c| c.is_ascii_alphanumeric() || *c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docblock::parse_tree::NodeKind;
    use crate::docblock::type_tokenizer::tokenize;

    fn build(s: &str) -> Result<(ParseTreeArena, NodeId), String> {
        ParseTreeCreator::new(tokenize(s).unwrap()).create()
    }

    #[test]
    fn simple_value() {
        let (tree, root) = build("int").unwrap();
        assert!(matches!(tree.kind(root), NodeKind::Value { value, .. } if value == "int"));
    }

    #[test]
    fn union() {
        let (tree, root) = build("int|string").unwrap();
        assert!(tree.is_union(root));
        assert_eq!(tree.children(root).len(), 2);
    }

    #[test]
    fn generic_array() {
        let (tree, root) = build("array<int, string>").unwrap();
        assert!(matches!(tree.kind(root), NodeKind::Generic { value, .. } if value == "array"));
        assert_eq!(tree.children(root).len(), 2);
    }

    #[test]
    fn array_suffix_wraps() {
        let (tree, root) = build("string[]").unwrap();
        assert!(matches!(tree.kind(root), NodeKind::Generic { value, .. } if value == "array"));
        assert_eq!(tree.children(root).len(), 1);
    }

    #[test]
    fn keyed_array() {
        let (tree, root) = build("array{a: int, b?: string}").unwrap();
        assert!(tree.is_keyed_array(root));
        assert!(tree.terminated(root));
        let children = tree.children(root);
        assert_eq!(children.len(), 2);
        // second property is possibly_undefined (b?:)
        assert!(tree.possibly_undefined(children[1]));
    }

    #[test]
    fn nullable() {
        let (tree, root) = build("?string").unwrap();
        assert!(tree.is_nullable(root));
        assert_eq!(tree.children(root).len(), 1);
    }

    #[test]
    fn intersection() {
        let (tree, root) = build("A&B").unwrap();
        assert!(tree.is_intersection(root));
        assert_eq!(tree.children(root).len(), 2);
    }

    #[test]
    fn callable_with_return() {
        let (tree, root) = build("callable(int, string): bool").unwrap();
        assert!(tree.is_callable_with_return_type(root));
        // child 0 is the CallableTree, child 1 is the return value
        let children = tree.children(root);
        assert_eq!(children.len(), 2);
        assert!(tree.is_callable(children[0]));
    }

    #[test]
    fn class_constant_value() {
        let (tree, root) = build("Foo::BAR").unwrap();
        assert!(matches!(tree.kind(root), NodeKind::Value { value, .. } if value == "Foo::BAR"));
    }

    #[test]
    fn conditional() {
        // A conditional only forms when the `T is ...` clause has a parent
        // (here the surrounding parentheses), matching Psalm: bare top-level
        // `T is X ? A : B` leaves the TemplateIs at the root with no parent.
        let (tree, root) = build("(T is string ? int : bool)").unwrap();
        assert!(tree.is_encapsulation(root));
        let conditional = tree.children(root)[0];
        assert!(tree.is_conditional(conditional));
        assert_eq!(tree.children(conditional).len(), 2);
    }

    #[test]
    fn unterminated_generic_errors() {
        assert!(build("array<int").is_err());
    }

    #[test]
    fn this_becomes_static() {
        let (tree, root) = build("$this").unwrap();
        assert!(matches!(tree.kind(root), NodeKind::Value { value, .. } if value == "static"));
    }

    #[test]
    fn template_as_in_generic() {
        let (tree, root) = build("class-string-map<T as Foo, T>").unwrap();
        assert!(
            matches!(tree.kind(root), NodeKind::Generic { value, .. } if value == "class-string-map")
        );
        let children = tree.children(root);
        assert!(matches!(
            tree.kind(children[0]),
            NodeKind::TemplateAs { .. }
        ));
    }
}
