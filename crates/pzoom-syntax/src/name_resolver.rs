//! Name resolver - resolves identifiers to fully qualified names.
//!
//! This module preprocesses the AST to resolve all class, function, and constant
//! names based on namespace context and use statements. The resolved names are
//! stored in a map keyed by the identifier's start offset.

use mago_span::HasSpan;
use mago_syntax::ast::Program;
use mago_syntax::ast::Sequence;
use mago_syntax::ast::ast::array::ArrayElement;
use mago_syntax::ast::ast::attribute::AttributeList;
use mago_syntax::ast::ast::call::{Call, StaticMethodCall};
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::property::Property;
use mago_syntax::ast::ast::class_like::{AnonymousClass, Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::control_flow::r#match::MatchArm;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::function_like::function::Function;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::instantiation::Instantiation;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::statement::Statement;
use mago_syntax::ast::ast::type_hint::Hint;
use mago_syntax::ast::ast::r#use::{Use, UseItem, UseItems, UseType};
use mago_syntax::ast::ast::r#yield::Yield;
use mago_syntax::ast::node::Node;
use pzoom_str::{StrId, ThreadedInterner};
use rustc_hash::FxHashMap;

/// Resolved names map: start_offset -> resolved StrId
pub type ResolvedNames = FxHashMap<u32, StrId>;

/// Name resolution context tracking current namespace and aliases.
pub struct NameContext {
    /// Current namespace (None = global)
    namespace: Option<String>,
    /// Class/type aliases: alias -> fully qualified name
    type_aliases: FxHashMap<String, String>,
    /// Function aliases imported via `use function`.
    function_aliases: FxHashMap<String, String>,
    /// Constant aliases imported via `use const`.
    constant_aliases: FxHashMap<String, String>,
}

impl NameContext {
    pub fn new() -> Self {
        Self {
            namespace: None,
            type_aliases: FxHashMap::default(),
            function_aliases: FxHashMap::default(),
            constant_aliases: FxHashMap::default(),
        }
    }

    /// Start a new namespace scope.
    pub fn start_namespace(&mut self, name: Option<&str>) {
        self.namespace = name.map(|n| n.to_string());
        // Reset aliases when entering a new namespace
        self.type_aliases.clear();
        self.function_aliases.clear();
        self.constant_aliases.clear();
    }

    /// Add a type/class alias imported via `use`.
    pub fn add_type_alias(&mut self, name: &str, alias: &str) {
        // Strip leading backslash if present
        let name = name.strip_prefix('\\').unwrap_or(name);
        self.type_aliases
            .insert(alias.to_ascii_lowercase(), name.to_string());
    }

    /// Add a function alias imported via `use function`.
    pub fn add_function_alias(&mut self, name: &str, alias: &str) {
        let name = name.strip_prefix('\\').unwrap_or(name);
        self.function_aliases
            .insert(alias.to_ascii_lowercase(), name.to_string());
    }

    /// Add a constant alias imported via `use const`.
    pub fn add_constant_alias(&mut self, name: &str, alias: &str) {
        let name = name.strip_prefix('\\').unwrap_or(name);
        self.constant_aliases
            .insert(alias.to_string(), name.to_string());
    }

    /// Resolve a type/class name to its fully qualified form.
    pub fn resolve_type_name(&self, name: &str, interner: &ThreadedInterner) -> StrId {
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
        let first_part_lc = first_part.to_ascii_lowercase();

        if let Some(resolved_alias) = self.type_aliases.get(&first_part_lc) {
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

    /// Resolve a function name to its fully qualified form.
    pub fn resolve_function_name(&self, name: &str, interner: &ThreadedInterner) -> StrId {
        if let Some(stripped) = name.strip_prefix('\\') {
            return interner.intern(stripped);
        }

        let parts: Vec<&str> = name.split('\\').collect();
        let first_part = parts[0];
        let first_part_lc = first_part.to_ascii_lowercase();

        if let Some(resolved_alias) = self.function_aliases.get(&first_part_lc) {
            if parts.len() > 1 {
                let rest = parts[1..].join("\\");
                return interner.intern(&format!("{}\\{}", resolved_alias, rest));
            } else {
                return interner.intern(resolved_alias);
            }
        }

        if let Some(resolved_alias) = self.type_aliases.get(&first_part_lc) {
            if parts.len() > 1 {
                let rest = parts[1..].join("\\");
                return interner.intern(&format!("{}\\{}", resolved_alias, rest));
            } else {
                return interner.intern(resolved_alias);
            }
        }

        match &self.namespace {
            Some(ns) => interner.intern(&format!("{}\\{}", ns, name)),
            None => interner.intern(name),
        }
    }

    /// Resolve a constant name to its fully qualified form.
    pub fn resolve_constant_name(&self, name: &str, interner: &ThreadedInterner) -> StrId {
        if let Some(stripped) = name.strip_prefix('\\') {
            return interner.intern(stripped);
        }

        let parts: Vec<&str> = name.split('\\').collect();
        let first_part = parts[0];
        let first_part_lc = first_part.to_ascii_lowercase();

        if let Some(resolved_alias) = self.constant_aliases.get(first_part) {
            if parts.len() > 1 {
                let rest = parts[1..].join("\\");
                return interner.intern(&format!("{}\\{}", resolved_alias, rest));
            } else {
                return interner.intern(resolved_alias);
            }
        }

        if let Some(resolved_alias) = self.type_aliases.get(&first_part_lc) {
            if parts.len() > 1 {
                let rest = parts[1..].join("\\");
                return interner.intern(&format!("{}\\{}", resolved_alias, rest));
            } else {
                return interner.intern(resolved_alias);
            }
        }

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
    interner: &'a ThreadedInterner,
    context: NameContext,
    resolved_names: ResolvedNames,
}

#[derive(Clone, Copy)]
enum UseAliasKind {
    Type,
    Function,
    Constant,
}

impl<'a> NameResolver<'a> {
    pub fn new(interner: &'a ThreadedInterner) -> Self {
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
            Statement::DoWhile(do_while) => {
                self.visit_statement(do_while.statement);
                self.visit_expression(do_while.condition);
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
                self.visit_hint(nullable.hint);
            }
            Hint::Union(union_hint) => {
                self.visit_hint(union_hint.left);
                self.visit_hint(union_hint.right);
            }
            Hint::Intersection(intersection) => {
                self.visit_hint(intersection.left);
                self.visit_hint(intersection.right);
            }
            Hint::Parenthesized(paren) => {
                self.visit_hint(paren.hint);
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
                    self.add_use_alias(item, None, UseAliasKind::Type);
                }
            }
            UseItems::TypedSequence(seq) => {
                let kind = use_type_to_alias_kind(&seq.r#type);
                for item in &seq.items {
                    self.add_use_alias(item, None, kind);
                }
            }
            UseItems::TypedList(list) => {
                let kind = use_type_to_alias_kind(&list.r#type);
                let prefix = normalize_use_name(list.namespace.value());
                for item in &list.items {
                    self.add_use_alias(item, Some(prefix.as_str()), kind);
                }
            }
            UseItems::MixedList(list) => {
                let prefix = normalize_use_name(list.namespace.value());
                for maybe_typed in &list.items {
                    let kind = maybe_typed
                        .r#type
                        .as_ref()
                        .map(use_type_to_alias_kind)
                        .unwrap_or(UseAliasKind::Type);
                    self.add_use_alias(&maybe_typed.item, Some(prefix.as_str()), kind);
                }
            }
        }
    }

    fn add_use_alias(&mut self, item: &UseItem<'_>, prefix: Option<&str>, kind: UseAliasKind) {
        let name = normalize_use_name(item.name.value());
        let full_name = match prefix {
            Some(p) => format!("{}\\{}", p, name),
            None => name.clone(),
        };

        // Alias is either explicit or the last part of the name
        let alias = match &item.alias {
            Some(alias) => alias.identifier.value.to_string(),
            None => name
                .split('\\')
                .next_back()
                .unwrap_or(name.as_str())
                .to_string(),
        };

        match kind {
            UseAliasKind::Type => self.context.add_type_alias(&full_name, &alias),
            UseAliasKind::Function => self.context.add_function_alias(&full_name, &alias),
            UseAliasKind::Constant => self.context.add_constant_alias(&full_name, &alias),
        }
    }

    fn visit_class(&mut self, class: &Class<'_>) {
        self.visit_attribute_lists(&class.attribute_lists);

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
        self.visit_attribute_lists(&iface.attribute_lists);

        // Resolve extends
        if let Some(extends) = &iface.extends {
            for parent in &extends.types {
                self.resolve_identifier(parent);
            }
        }

        self.visit_class_members(&iface.members);
    }

    fn visit_trait(&mut self, tr: &Trait<'_>) {
        self.visit_attribute_lists(&tr.attribute_lists);
        self.visit_class_members(&tr.members);
    }

    fn visit_enum(&mut self, en: &Enum<'_>) {
        self.visit_attribute_lists(&en.attribute_lists);

        if let Some(implements) = &en.implements {
            for iface in &implements.types {
                self.resolve_identifier(iface);
            }
        }
        self.visit_class_members(&en.members);
    }

    fn visit_class_members(
        &mut self,
        members: &mago_syntax::ast::Sequence<'_, ClassLikeMember<'_>>,
    ) {
        use mago_syntax::ast::ast::class_like::method::MethodBody;

        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    self.visit_attribute_lists(&method.attribute_lists);
                    for param in method.parameter_list.parameters.iter() {
                        self.visit_function_like_parameter(param);
                    }

                    // The return type hint references a class too (e.g. a method
                    // declared `: SomeClass`) — resolve it so on-demand scanning
                    // pulls in a dependency reachable only through a return type.
                    if let Some(return_type_hint) = &method.return_type_hint {
                        self.visit_hint(&return_type_hint.hint);
                    }

                    // Visit method body if concrete (not abstract)
                    if let MethodBody::Concrete(block) = &method.body {
                        for stmt in &block.statements {
                            self.visit_statement(stmt);
                        }
                    }
                }
                ClassLikeMember::Property(property) => match property {
                    Property::Plain(plain) => {
                        self.visit_attribute_lists(&plain.attribute_lists);
                    }
                    Property::Hooked(hooked) => {
                        self.visit_attribute_lists(&hooked.attribute_lists);
                    }
                },
                ClassLikeMember::Constant(class_const) => {
                    self.visit_attribute_lists(&class_const.attribute_lists);
                    for item in &class_const.items {
                        self.visit_expression(&item.value);
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
        self.visit_attribute_lists(&func.attribute_lists);
        for param in func.parameter_list.parameters.iter() {
            self.visit_function_like_parameter(param);
        }

        if let Some(return_type_hint) = &func.return_type_hint {
            self.visit_hint(&return_type_hint.hint);
        }

        for stmt in &func.body.statements {
            self.visit_statement(stmt);
        }
    }

    fn visit_function_like_parameter(&mut self, param: &FunctionLikeParameter<'_>) {
        self.visit_attribute_lists(&param.attribute_lists);

        if let Some(hint) = &param.hint {
            self.visit_hint(hint);
        }

        if let Some(default_value) = &param.default_value {
            self.visit_expression(&default_value.value);
        }
    }

    fn visit_attribute_lists(&mut self, attribute_lists: &Sequence<'_, AttributeList<'_>>) {
        for attribute_list in attribute_lists {
            for attribute in &attribute_list.attributes {
                self.resolve_identifier(&attribute.name);
                if let Some(argument_list) = &attribute.argument_list {
                    for arg in &argument_list.arguments {
                        self.visit_expression(arg.value());
                    }
                }
            }
        }
    }

    fn visit_expression(&mut self, expr: &Expression<'_>) {
        use mago_syntax::ast::ast::access::Access;

        match expr {
            Expression::Identifier(id) => {
                self.resolve_identifier(id);
            }
            Expression::ConstantAccess(constant_access) => {
                self.resolve_constant_identifier(&constant_access.name);
            }
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
                self.visit_array_elements(array.elements.iter());
            }
            Expression::LegacyArray(array) => {
                self.visit_array_elements(array.elements.iter());
            }
            Expression::List(list) => {
                self.visit_array_elements(list.elements.iter());
            }
            Expression::ArrayAccess(access) => {
                self.visit_expression(access.array);
                self.visit_expression(access.index);
            }
            Expression::Access(access) => match access {
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
            },
            Expression::Conditional(cond) => {
                self.visit_expression(cond.condition);
                if let Some(then_expr) = &cond.then {
                    self.visit_expression(then_expr);
                }
                self.visit_expression(cond.r#else);
            }
            Expression::Closure(closure) => {
                self.visit_attribute_lists(&closure.attribute_lists);
                for param in closure.parameter_list.parameters.iter() {
                    self.visit_function_like_parameter(param);
                }
                if let Some(return_type_hint) = &closure.return_type_hint {
                    self.visit_hint(&return_type_hint.hint);
                }
                for stmt in &closure.body.statements {
                    self.visit_statement(stmt);
                }
            }
            Expression::ArrowFunction(arrow) => {
                self.visit_attribute_lists(&arrow.attribute_lists);
                for param in arrow.parameter_list.parameters.iter() {
                    self.visit_function_like_parameter(param);
                }
                if let Some(return_type_hint) = &arrow.return_type_hint {
                    self.visit_hint(&return_type_hint.hint);
                }
                self.visit_expression(arrow.expression);
            }
            Expression::Match(match_expr) => {
                self.visit_expression(match_expr.expression);
                for arm in match_expr.arms.iter() {
                    match arm {
                        MatchArm::Expression(expression_arm) => {
                            for condition in expression_arm.conditions.iter() {
                                self.visit_expression(condition);
                            }
                            self.visit_expression(expression_arm.expression);
                        }
                        MatchArm::Default(default_arm) => {
                            self.visit_expression(default_arm.expression);
                        }
                    }
                }
            }
            Expression::Construct(construct) => {
                self.visit_construct(construct);
            }
            Expression::Throw(throw) => {
                self.visit_expression(throw.exception);
            }
            Expression::Clone(clone) => {
                self.visit_expression(clone.object);
            }
            Expression::AnonymousClass(anonymous_class) => {
                self.visit_anonymous_class(anonymous_class);
            }
            Expression::Yield(yield_expr) => match yield_expr {
                Yield::Value(yield_value) => {
                    if let Some(value) = yield_value.value {
                        self.visit_expression(value);
                    }
                }
                Yield::Pair(yield_pair) => {
                    self.visit_expression(yield_pair.key);
                    self.visit_expression(yield_pair.value);
                }
                Yield::From(yield_from) => {
                    self.visit_expression(yield_from.iterator);
                }
            },
            _ => {}
        }
    }

    /// Resolve names inside an anonymous class (`new class(...) extends X
    /// implements Y { ... }`): its parents, interfaces, and member bodies.
    fn visit_anonymous_class(&mut self, anonymous_class: &AnonymousClass<'_>) {
        self.visit_attribute_lists(&anonymous_class.attribute_lists);

        if let Some(argument_list) = &anonymous_class.argument_list {
            for arg in &argument_list.arguments {
                self.visit_expression(arg.value());
            }
        }

        if let Some(extends) = &anonymous_class.extends {
            for parent in &extends.types {
                self.resolve_identifier(parent);
            }
        }

        if let Some(implements) = &anonymous_class.implements {
            for iface in &implements.types {
                self.resolve_identifier(iface);
            }
        }

        self.visit_class_members(&anonymous_class.members);
    }

    /// Visit the elements of an array/list literal (short `[]`, legacy
    /// `array(...)`, or `list(...)`), resolving any identifiers inside.
    fn visit_array_elements<'b>(&mut self, elements: impl Iterator<Item = &'b ArrayElement<'b>>) {
        for element in elements {
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

    fn visit_construct(&mut self, construct: &Construct<'_>) {
        match construct {
            Construct::Isset(isset) => {
                for value in isset.values.iter() {
                    self.visit_expression(value);
                }
            }
            Construct::Empty(empty) => {
                self.visit_expression(empty.value);
            }
            Construct::Eval(eval) => {
                self.visit_expression(eval.value);
            }
            Construct::Include(include) => {
                self.visit_expression(include.value);
            }
            Construct::IncludeOnce(include_once) => {
                self.visit_expression(include_once.value);
            }
            Construct::Require(require) => {
                self.visit_expression(require.value);
            }
            Construct::RequireOnce(require_once) => {
                self.visit_expression(require_once.value);
            }
            Construct::Print(print_construct) => {
                self.visit_expression(print_construct.value);
            }
            Construct::Exit(exit_construct) => {
                if let Some(args) = &exit_construct.arguments {
                    for arg in &args.arguments {
                        self.visit_expression(arg.value());
                    }
                }
            }
            Construct::Die(die_construct) => {
                if let Some(args) = &die_construct.arguments {
                    for arg in &args.arguments {
                        self.visit_expression(arg.value());
                    }
                }
            }
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
                match func_call.function.unparenthesized() {
                    Expression::Identifier(id) => self.resolve_function_identifier(id),
                    Expression::ConstantAccess(constant_access) => {
                        self.resolve_constant_identifier(&constant_access.name);
                    }
                    other => self.visit_expression(other),
                }

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
        let resolved = self.context.resolve_type_name(name, self.interner);
        self.resolved_names.insert(offset, resolved);
    }

    fn resolve_function_identifier(&mut self, id: &Identifier<'_>) {
        let offset = id.span().start.offset;
        let name = id.value();
        let resolved = self.context.resolve_function_name(name, self.interner);
        self.resolved_names.insert(offset, resolved);
    }

    fn resolve_constant_identifier(&mut self, id: &Identifier<'_>) {
        let offset = id.span().start.offset;
        let name = id.value();
        let resolved = self.context.resolve_constant_name(name, self.interner);
        self.resolved_names.insert(offset, resolved);
    }
}

fn use_type_to_alias_kind(use_type: &UseType<'_>) -> UseAliasKind {
    if use_type.is_function() {
        UseAliasKind::Function
    } else if use_type.is_const() {
        UseAliasKind::Constant
    } else {
        UseAliasKind::Type
    }
}

fn normalize_use_name(name: &str) -> String {
    name.strip_prefix('\\').unwrap_or(name).to_string()
}

/// Resolve all names in a program.
pub fn resolve_names(program: &Program<'_>, interner: &ThreadedInterner) -> ResolvedNames {
    // Intern every local variable name in the file. Analysis builds data-flow
    // node ids from these names with a read-only `find` (it cannot intern), so
    // they must already live in the interner. Mirrors Hakana's naming_visitor,
    // which interns every `Lid` during scanning.
    for variable_name in Node::Program(program).filter_map(|node| match node {
        Node::DirectVariable(var) => Some(var.name),
        _ => None,
    }) {
        interner.intern(variable_name);
    }

    let resolver = NameResolver::new(interner);
    resolver.resolve(program)
}
