//! Name resolver - resolves identifiers to fully qualified names.
//!
//! This module preprocesses the AST to resolve all class, function, and constant
//! names based on namespace context and use statements. The resolved names are
//! stored in a map keyed by the identifier's start offset.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::{Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::function_like::function::Function;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::instantiation::Instantiation;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::r#use::{Use, UseItem, UseItems};
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::call::{Call, StaticMethodCall};
use mago_syntax::ast::ast::type_hint::Hint;
use mago_syntax::ast::Program;
use pzoom_str::{Interner, StrId};
use rustc_hash::FxHashMap;

/// Resolved names map: start_offset -> resolved StrId
pub type ResolvedNames = FxHashMap<u32, StrId>;

/// Name resolution context tracking current namespace and aliases.
pub struct NameContext {
    /// Current namespace (None = global)
    namespace: Option<String>,
    /// Class/type aliases: alias -> fully qualified name
    type_aliases: FxHashMap<String, String>,
}

impl NameContext {
    pub fn new() -> Self {
        Self {
            namespace: None,
            type_aliases: FxHashMap::default(),
        }
    }

    /// Start a new namespace scope.
    pub fn start_namespace(&mut self, name: Option<&str>) {
        self.namespace = name.map(|n| n.to_string());
        // Reset aliases when entering a new namespace
        self.type_aliases.clear();
    }

    /// Add a use alias.
    pub fn add_alias(&mut self, name: &str, alias: &str) {
        // Strip leading backslash if present
        let name = name.strip_prefix('\\').unwrap_or(name);
        self.type_aliases.insert(alias.to_string(), name.to_string());
    }

    /// Resolve a name to its fully qualified form.
    pub fn resolve_name(&self, name: &str, interner: &Interner) -> StrId {
        // Fully qualified names (starting with \) are already resolved
        if let Some(stripped) = name.strip_prefix('\\') {
            return interner.intern(stripped);
        }

        // Check for special names
        match name {
            "self" | "static" | "parent" => return interner.intern(name),
            _ => {}
        }

        // Check if first part is an alias
        let parts: Vec<&str> = name.split('\\').collect();
        let first_part = parts[0];

        if let Some(resolved_alias) = self.type_aliases.get(first_part) {
            if parts.len() > 1 {
                // Qualified name with alias prefix: A\Foo -> resolved\Foo
                let rest = parts[1..].join("\\");
                return interner.intern(&format!("{}\\{}", resolved_alias, rest));
            } else {
                // Simple aliased name
                return interner.intern(resolved_alias);
            }
        }

        // No alias found - prepend current namespace if any
        match &self.namespace {
            Some(ns) => interner.intern(&format!("{}\\{}", ns, name)),
            None => interner.intern(name),
        }
    }
}

impl Default for NameContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Name resolver that traverses the AST and resolves all identifiers.
pub struct NameResolver<'a> {
    interner: &'a Interner,
    context: NameContext,
    resolved_names: ResolvedNames,
}

impl<'a> NameResolver<'a> {
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            context: NameContext::new(),
            resolved_names: FxHashMap::default(),
        }
    }

    /// Resolve all names in a program and return the resolved names map.
    pub fn resolve(mut self, program: &Program<'_>) -> ResolvedNames {
        for statement in &program.statements {
            self.visit_statement(statement);
        }
        self.resolved_names
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>) {
        match stmt {
            Statement::Namespace(ns) => self.visit_namespace(ns),
            Statement::Use(use_stmt) => self.visit_use(use_stmt),
            Statement::Class(class) => self.visit_class(class),
            Statement::Interface(iface) => self.visit_interface(iface),
            Statement::Trait(tr) => self.visit_trait(tr),
            Statement::Enum(en) => self.visit_enum(en),
            Statement::Function(func) => self.visit_function(func),
            Statement::Expression(expr_stmt) => {
                self.visit_expression(expr_stmt.expression);
            }
            Statement::Return(ret) => {
                if let Some(expr) = &ret.value {
                    self.visit_expression(expr);
                }
            }
            Statement::Echo(echo) => {
                for expr in &echo.values {
                    self.visit_expression(expr);
                }
            }
            Statement::If(if_stmt) => {
                self.visit_expression(if_stmt.condition);
                for stmt in if_stmt.body.statements() {
                    self.visit_statement(stmt);
                }
                for (cond, stmts) in if_stmt.body.else_if_clauses() {
                    self.visit_expression(cond);
                    for stmt in stmts {
                        self.visit_statement(stmt);
                    }
                }
                if let Some(else_stmts) = if_stmt.body.else_statements() {
                    for stmt in else_stmts {
                        self.visit_statement(stmt);
                    }
                }
            }
            Statement::While(while_stmt) => {
                self.visit_expression(while_stmt.condition);
                for stmt in while_stmt.body.statements() {
                    self.visit_statement(stmt);
                }
            }
            Statement::For(for_stmt) => {
                for init in &for_stmt.initializations {
                    self.visit_expression(init);
                }
                for cond in &for_stmt.conditions {
                    self.visit_expression(cond);
                }
                for inc in &for_stmt.increments {
                    self.visit_expression(inc);
                }
                for stmt in for_stmt.body.statements() {
                    self.visit_statement(stmt);
                }
            }
            Statement::Foreach(foreach_stmt) => {
                self.visit_expression(foreach_stmt.expression);
                for stmt in foreach_stmt.body.statements() {
                    self.visit_statement(stmt);
                }
            }
            Statement::Block(block) => {
                for stmt in &block.statements {
                    self.visit_statement(stmt);
                }
            }
            Statement::Try(try_stmt) => {
                for stmt in &try_stmt.block.statements {
                    self.visit_statement(stmt);
                }
                for catch in &try_stmt.catch_clauses {
                    // Resolve the exception type hint
                    self.visit_hint(&catch.hint);
                    for stmt in &catch.block.statements {
                        self.visit_statement(stmt);
                    }
                }
                if let Some(finally) = &try_stmt.finally_clause {
                    for stmt in &finally.block.statements {
                        self.visit_statement(stmt);
                    }
                }
            }
            Statement::Switch(switch_stmt) => {
                self.visit_expression(switch_stmt.expression);
                for case in switch_stmt.body.cases() {
                    if let Some(expr) = case.expression() {
                        self.visit_expression(expr);
                    }
                    for stmt in case.statements() {
                        self.visit_statement(stmt);
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_hint(&mut self, hint: &Hint<'_>) {
        match hint {
            Hint::Identifier(id) => {
                self.resolve_identifier(id);
            }
            Hint::Nullable(nullable) => {
                self.visit_hint(&nullable.hint);
            }
            Hint::Union(union_hint) => {
                self.visit_hint(&union_hint.left);
                self.visit_hint(&union_hint.right);
            }
            Hint::Intersection(intersection) => {
                self.visit_hint(&intersection.left);
                self.visit_hint(&intersection.right);
            }
            Hint::Parenthesized(paren) => {
                self.visit_hint(&paren.hint);
            }
            // Built-in types don't need resolution
            _ => {}
        }
    }

    fn visit_namespace(&mut self, ns: &Namespace<'_>) {
        // Set namespace context
        let ns_name = ns.name.as_ref().map(|n| n.value());
        self.context.start_namespace(ns_name);

        // Visit statements in namespace
        let stmts = match &ns.body {
            NamespaceBody::Implicit(implicit) => implicit.statements.as_slice(),
            NamespaceBody::BraceDelimited(block) => block.statements.as_slice(),
        };

        for stmt in stmts {
            self.visit_statement(stmt);
        }
    }

    fn visit_use(&mut self, use_stmt: &Use<'_>) {
        match &use_stmt.items {
            UseItems::Sequence(seq) => {
                for item in &seq.items {
                    self.add_use_alias(item, None);
                }
            }
            UseItems::TypedSequence(seq) => {
                // For `use function` or `use const` - we only handle class aliases for now
                if seq.r#type.is_function() || seq.r#type.is_const() {
                    return;
                }
                for item in &seq.items {
                    self.add_use_alias(item, None);
                }
            }
            UseItems::TypedList(list) => {
                if list.r#type.is_function() || list.r#type.is_const() {
                    return;
                }
                let prefix = list.namespace.value();
                for item in &list.items {
                    self.add_use_alias(item, Some(prefix));
                }
            }
            UseItems::MixedList(list) => {
                let prefix = list.namespace.value();
                for maybe_typed in &list.items {
                    if maybe_typed.r#type.as_ref().map_or(true, |t| !t.is_function() && !t.is_const()) {
                        self.add_use_alias(&maybe_typed.item, Some(prefix));
                    }
                }
            }
        }
    }

    fn add_use_alias(&mut self, item: &UseItem<'_>, prefix: Option<&str>) {
        let name = item.name.value();
        let full_name = match prefix {
            Some(p) => format!("{}\\{}", p, name),
            None => name.to_string(),
        };

        // Alias is either explicit or the last part of the name
        let alias = match &item.alias {
            Some(alias) => alias.identifier.value.to_string(),
            None => name.split('\\').last().unwrap_or(name).to_string(),
        };

        self.context.add_alias(&full_name, &alias);
    }

    fn visit_class(&mut self, class: &Class<'_>) {
        // Resolve extends
        if let Some(extends) = &class.extends {
            for parent in &extends.types {
                self.resolve_identifier(parent);
            }
        }

        // Resolve implements
        if let Some(implements) = &class.implements {
            for iface in &implements.types {
                self.resolve_identifier(iface);
            }
        }

        // Visit members
        self.visit_class_members(&class.members);
    }

    fn visit_interface(&mut self, iface: &Interface<'_>) {
        // Resolve extends
        if let Some(extends) = &iface.extends {
            for parent in &extends.types {
                self.resolve_identifier(parent);
            }
        }

        self.visit_class_members(&iface.members);
    }

    fn visit_trait(&mut self, tr: &Trait<'_>) {
        self.visit_class_members(&tr.members);
    }

    fn visit_enum(&mut self, en: &Enum<'_>) {
        if let Some(implements) = &en.implements {
            for iface in &implements.types {
                self.resolve_identifier(iface);
            }
        }
        self.visit_class_members(&en.members);
    }

    fn visit_class_members(&mut self, members: &mago_syntax::ast::Sequence<'_, ClassLikeMember<'_>>) {
        use mago_syntax::ast::ast::class_like::method::MethodBody;

        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    // Visit method body if concrete (not abstract)
                    if let MethodBody::Concrete(block) = &method.body {
                        for stmt in &block.statements {
                            self.visit_statement(stmt);
                        }
                    }
                }
                ClassLikeMember::TraitUse(trait_use) => {
                    for trait_name in &trait_use.trait_names {
                        self.resolve_identifier(trait_name);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_function(&mut self, func: &Function<'_>) {
        for stmt in &func.body.statements {
            self.visit_statement(stmt);
        }
    }

    fn visit_expression(&mut self, expr: &Expression<'_>) {
        use mago_syntax::ast::ast::access::Access;

        match expr {
            Expression::Instantiation(inst) => {
                self.visit_instantiation(inst);
            }
            Expression::Call(call) => {
                self.visit_call(call);
            }
            Expression::Assignment(assignment) => {
                self.visit_expression(assignment.lhs);
                self.visit_expression(assignment.rhs);
            }
            Expression::Binary(binary) => {
                self.visit_expression(binary.lhs);
                self.visit_expression(binary.rhs);
            }
            Expression::UnaryPrefix(unary) => {
                self.visit_expression(unary.operand);
            }
            Expression::UnaryPostfix(unary) => {
                self.visit_expression(unary.operand);
            }
            Expression::Parenthesized(paren) => {
                self.visit_expression(paren.expression);
            }
            Expression::Array(array) => {
                use mago_syntax::ast::ast::array::ArrayElement;
                for element in &array.elements {
                    match element {
                        ArrayElement::KeyValue(kv) => {
                            self.visit_expression(kv.key);
                            self.visit_expression(kv.value);
                        }
                        ArrayElement::Value(v) => {
                            self.visit_expression(v.value);
                        }
                        ArrayElement::Variadic(v) => {
                            self.visit_expression(v.value);
                        }
                        ArrayElement::Missing(_) => {}
                    }
                }
            }
            Expression::ArrayAccess(access) => {
                self.visit_expression(access.array);
                self.visit_expression(access.index);
            }
            Expression::Access(access) => {
                match access {
                    Access::Property(prop) => {
                        self.visit_expression(prop.object);
                    }
                    Access::NullSafeProperty(prop) => {
                        self.visit_expression(prop.object);
                    }
                    Access::StaticProperty(prop) => {
                        if let Expression::Identifier(id) = prop.class {
                            self.resolve_identifier(id);
                        }
                    }
                    Access::ClassConstant(cc) => {
                        if let Expression::Identifier(id) = cc.class {
                            self.resolve_identifier(id);
                        }
                    }
                }
            }
            Expression::Conditional(cond) => {
                self.visit_expression(cond.condition);
                if let Some(then_expr) = &cond.then {
                    self.visit_expression(then_expr);
                }
                self.visit_expression(cond.r#else);
            }
            Expression::Closure(closure) => {
                for stmt in &closure.body.statements {
                    self.visit_statement(stmt);
                }
            }
            Expression::ArrowFunction(arrow) => {
                self.visit_expression(arrow.expression);
            }
            Expression::Match(match_expr) => {
                self.visit_expression(match_expr.expression);
                // Visit match arms - structure varies, just visit what we can
            }
            Expression::Throw(throw) => {
                self.visit_expression(throw.exception);
            }
            Expression::Clone(clone) => {
                self.visit_expression(clone.object);
            }
            Expression::Yield(_) => {
                // Yield structure varies - skip for now
            }
            _ => {}
        }
    }

    fn visit_instantiation(&mut self, inst: &Instantiation<'_>) {
        // Resolve the class name
        if let Expression::Identifier(id) = inst.class {
            self.resolve_identifier(id);
        }

        // Visit constructor arguments
        if let Some(args) = &inst.argument_list {
            for arg in &args.arguments {
                self.visit_expression(arg.value());
            }
        }
    }

    fn visit_call(&mut self, call: &Call<'_>) {
        match call {
            Call::Function(func_call) => {
                // Could resolve function name here if needed
                for arg in &func_call.argument_list.arguments {
                    self.visit_expression(arg.value());
                }
            }
            Call::Method(method_call) => {
                self.visit_expression(method_call.object);
                for arg in &method_call.argument_list.arguments {
                    self.visit_expression(arg.value());
                }
            }
            Call::NullSafeMethod(method_call) => {
                self.visit_expression(method_call.object);
                for arg in &method_call.argument_list.arguments {
                    self.visit_expression(arg.value());
                }
            }
            Call::StaticMethod(static_call) => {
                self.visit_static_method_call(static_call);
            }
        }
    }

    fn visit_static_method_call(&mut self, call: &StaticMethodCall<'_>) {
        // Resolve the class name
        if let Expression::Identifier(id) = call.class {
            self.resolve_identifier(id);
        }

        // Visit arguments
        for arg in &call.argument_list.arguments {
            self.visit_expression(arg.value());
        }
    }

    fn resolve_identifier(&mut self, id: &Identifier<'_>) {
        let offset = id.span().start.offset;
        let name = id.value();
        let resolved = self.context.resolve_name(name, self.interner);
        self.resolved_names.insert(offset, resolved);
    }
}

/// Resolve all names in a program.
pub fn resolve_names(program: &Program<'_>, interner: &Interner) -> ResolvedNames {
    let resolver = NameResolver::new(interner);
    resolver.resolve(program)
}
