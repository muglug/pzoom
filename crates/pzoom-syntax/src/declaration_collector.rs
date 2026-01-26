//! Declaration collector - extracts class, function, and constant declarations from AST.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::property::{Property, PropertyItem};
use mago_syntax::ast::ast::class_like::{Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::constant::Constant;
use mago_syntax::ast::ast::function_like::function::Function;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::modifier::Modifier;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::type_hint::Hint;
use mago_syntax::ast::sequence::TokenSeparatedSequence;
use mago_syntax::ast::{Program, Sequence, Statement, Trivia, TriviaKind};

use pzoom_code_info::class_like_info::{
    ClassConstantInfo, ClassLikeInfo, ClassLikeKind, PropertyInfo, Visibility,
};
use pzoom_code_info::codebase_info::ConstantInfo;
use pzoom_code_info::functionlike_info::{FunctionLikeInfo, ParamInfo};
use pzoom_code_info::TUnion;
use pzoom_str::{Interner, StrId};

use crate::type_resolver::resolve_hint;

/// Collected declarations from a PHP file.
#[derive(Debug, Default)]
pub struct CollectedDeclarations {
    pub classes: Vec<ClassLikeInfo>,
    pub functions: Vec<FunctionLikeInfo>,
    pub constants: Vec<ConstantInfo>,
}

/// Collects declarations from a parsed PHP program.
pub struct DeclarationCollector<'a, 'p> {
    interner: &'a mut Interner,
    file_path: StrId,
    current_namespace: Option<StrId>,
    declarations: CollectedDeclarations,
    /// Trivia (comments) from the program for docblock parsing
    trivia: &'p Sequence<'p, Trivia<'p>>,
}

impl<'a, 'p> DeclarationCollector<'a, 'p> {
    pub fn new(interner: &'a mut Interner, file_path: StrId, trivia: &'p Sequence<'p, Trivia<'p>>) -> Self {
        Self {
            interner,
            file_path,
            current_namespace: None,
            declarations: CollectedDeclarations::default(),
            trivia,
        }
    }

    /// Collect all declarations from a program.
    pub fn collect(mut self, program: &Program<'_>) -> CollectedDeclarations {
        for statement in &program.statements {
            self.visit_statement(statement);
        }
        self.declarations
    }

    /// Find the docblock comment that precedes a given position.
    fn find_preceding_docblock(&self, start_offset: u32) -> Option<&'p str> {
        // Find the docblock that ends closest to (but before) the start_offset
        let mut best_match: Option<&'p Trivia<'p>> = None;

        for trivia in self.trivia.iter() {
            if trivia.kind == TriviaKind::DocBlockComment {
                let end = trivia.span.end.offset;
                // The docblock must end before the target position
                // and be reasonably close (within 100 bytes to account for whitespace/modifiers)
                if end < start_offset && start_offset - end < 100 {
                    if best_match
                        .map(|b| trivia.span.end.offset > b.span.end.offset)
                        .unwrap_or(true)
                    {
                        best_match = Some(trivia);
                    }
                }
            }
        }

        best_match.map(|t| t.value)
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>) {
        match stmt {
            Statement::Namespace(ns) => self.visit_namespace(ns),
            Statement::Class(class) => self.visit_class(class),
            Statement::Interface(iface) => self.visit_interface(iface),
            Statement::Trait(tr) => self.visit_trait(tr),
            Statement::Enum(en) => self.visit_enum(en),
            Statement::Function(func) => self.visit_function(func),
            Statement::Constant(constant) => self.visit_constant(constant),
            _ => {}
        }
    }

    fn visit_namespace(&mut self, ns: &Namespace<'_>) {
        // Set current namespace
        let ns_name = ns.name.as_ref().map(|n| self.interner.intern(n.value()));
        self.current_namespace = ns_name;

        // Visit statements in namespace
        match &ns.body {
            NamespaceBody::Implicit(implicit) => {
                for stmt in &implicit.statements {
                    self.visit_statement(stmt);
                }
            }
            NamespaceBody::BraceDelimited(block) => {
                for stmt in &block.statements {
                    self.visit_statement(stmt);
                }
            }
        }
    }

    fn visit_class(&mut self, class: &Class<'_>) {
        let name = self.make_fqn(class.name.value);
        let span = class.span();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Class,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        // Parse modifiers
        for modifier in &class.modifiers {
            match modifier {
                Modifier::Final(_) => info.is_final = true,
                Modifier::Abstract(_) => info.is_abstract = true,
                Modifier::Readonly(_) => info.is_readonly = true,
                _ => {}
            }
        }

        // Parse extends (class can only extend one class)
        if let Some(extends) = &class.extends {
            if let Some(parent) = extends.types.first() {
                let parent_name = self.resolve_identifier(parent);
                info.parent_class = Some(parent_name);
            }
        }

        // Parse implements
        if let Some(implements) = &class.implements {
            for iface in &implements.types {
                let iface_name = self.resolve_identifier(iface);
                info.interfaces.insert(iface_name);
            }
        }

        // Parse members
        self.collect_class_members(&mut info, &class.members);

        self.declarations.classes.push(info);
    }

    fn visit_interface(&mut self, iface: &Interface<'_>) {
        let name = self.make_fqn(iface.name.value);
        let span = iface.span();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Interface,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        // Parse extends (interfaces can extend multiple interfaces)
        if let Some(extends) = &iface.extends {
            for parent in &extends.types {
                let parent_name = self.resolve_identifier(parent);
                info.interfaces.insert(parent_name);
            }
        }

        // Parse members
        self.collect_class_members(&mut info, &iface.members);

        self.declarations.classes.push(info);
    }

    fn visit_trait(&mut self, tr: &Trait<'_>) {
        let name = self.make_fqn(tr.name.value);
        let span = tr.span();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Trait,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        // Parse members
        self.collect_class_members(&mut info, &tr.members);

        self.declarations.classes.push(info);
    }

    fn visit_enum(&mut self, en: &Enum<'_>) {
        let name = self.make_fqn(en.name.value);
        let span = en.span();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Enum,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        // Parse implements
        if let Some(implements) = &en.implements {
            for iface in &implements.types {
                let iface_name = self.resolve_identifier(iface);
                info.interfaces.insert(iface_name);
            }
        }

        // Parse members
        self.collect_class_members(&mut info, &en.members);

        self.declarations.classes.push(info);
    }

    fn visit_function(&mut self, func: &Function<'_>) {
        let name = self.make_fqn(func.name.value);
        let span = func.span();

        let return_type = func
            .return_type_hint
            .as_ref()
            .map(|rth| self.resolve_type(&rth.hint));

        let params = self.collect_params(&func.parameter_list.parameters);

        let info = FunctionLikeInfo {
            name,
            params,
            return_type,
            returns_by_ref: func.ampersand.is_some(),
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        self.declarations.functions.push(info);
    }

    fn visit_constant(&mut self, constant: &Constant<'_>) {
        for item in &constant.items {
            let name = self.make_fqn(item.name.value);
            let span = item.span();

            let info = ConstantInfo {
                name,
                constant_type: TUnion::mixed(), // TODO: Infer from value
                file_path: self.file_path,
                start_offset: span.start.offset,
            };

            self.declarations.constants.push(info);
        }
    }

    fn collect_class_members(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
    ) {
        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    let method_name = self.interner.intern(method.name.value);
                    let span = method.span();

                    let return_type = method
                        .return_type_hint
                        .as_ref()
                        .map(|rth| self.resolve_type(&rth.hint));

                    let params = self.collect_params(&method.parameter_list.parameters);

                    let (visibility, is_static, is_abstract, is_final) =
                        parse_method_modifiers(&method.modifiers);

                    let method_info = FunctionLikeInfo {
                        name: method_name,
                        declaring_class: Some(class_info.name),
                        params,
                        return_type,
                        is_static,
                        is_abstract,
                        is_final,
                        visibility,
                        returns_by_ref: method.ampersand.is_some(),
                        file_path: self.file_path,
                        start_offset: span.start.offset,
                        end_offset: span.end.offset,
                        ..Default::default()
                    };

                    class_info.methods.insert(method_name, method_info);
                }
                ClassLikeMember::Property(property) => {
                    self.collect_property(class_info, property);
                }
                ClassLikeMember::Constant(class_const) => {
                    let visibility = parse_const_visibility(&class_const.modifiers);

                    let const_type = class_const
                        .hint
                        .as_ref()
                        .map(|h| self.resolve_type(h))
                        .unwrap_or_else(TUnion::mixed);

                    for item in &class_const.items {
                        let const_name = self.interner.intern(item.name.value);
                        let span = item.span();

                        let const_info = ClassConstantInfo {
                            name: const_name,
                            declaring_class: class_info.name,
                            constant_type: const_type.clone(),
                            visibility,
                            is_final: class_const
                                .modifiers
                                .iter()
                                .any(|m| matches!(m, Modifier::Final(_))),
                            is_deprecated: false,
                            start_offset: span.start.offset,
                        };

                        class_info.constants.insert(const_name, const_info);
                    }
                }
                ClassLikeMember::TraitUse(trait_use) => {
                    for trait_name in &trait_use.trait_names {
                        let name = self.resolve_identifier(trait_name);
                        class_info.used_traits.insert(name);
                    }
                }
                ClassLikeMember::EnumCase(_) => {
                    // Enum cases are handled differently
                }
            }
        }
    }

    fn collect_property(&mut self, class_info: &mut ClassLikeInfo, property: &Property<'_>) {
        let (visibility, is_static, is_readonly) = parse_property_modifiers(property.modifiers());

        // Get native PHP type hint (signature_type)
        let signature_type = property.hint().map(|h| self.resolve_type(h));

        // Get property start offset for docblock lookup
        let prop_span = property.span();

        // Get docblock type if present
        let docblock_type = self
            .find_preceding_docblock(prop_span.start.offset)
            .and_then(|docblock| {
                let parsed = crate::docblock::parse(docblock, 0);
                parsed.get_var().and_then(|var_content| {
                    crate::docblock::extract_type_string_from_content(var_content)
                        .map(|type_str| crate::docblock::parse_type_string(type_str, self.interner))
                })
            });

        // Effective type is docblock type if present, else signature type
        let property_type = docblock_type.clone().or_else(|| signature_type.clone());

        match property {
            Property::Plain(plain) => {
                for item in &plain.items {
                    self.add_property_item(
                        class_info,
                        item,
                        property_type.clone(),
                        signature_type.clone(),
                        visibility,
                        is_static,
                        is_readonly,
                    );
                }
            }
            Property::Hooked(hooked) => {
                self.add_property_item(
                    class_info,
                    &hooked.item,
                    property_type.clone(),
                    signature_type.clone(),
                    visibility,
                    is_static,
                    is_readonly,
                );
            }
        }
    }

    fn add_property_item(
        &mut self,
        class_info: &mut ClassLikeInfo,
        item: &PropertyItem<'_>,
        property_type: Option<TUnion>,
        signature_type: Option<TUnion>,
        visibility: Visibility,
        is_static: bool,
        is_readonly: bool,
    ) {
        let variable = item.variable();
        // Strip the leading $ from property names to match how they're referenced
        let prop_name_str = variable.name.strip_prefix('$').unwrap_or(variable.name);
        let prop_name = self.interner.intern(prop_name_str);
        let span = item.span();
        let has_default = matches!(item, PropertyItem::Concrete(_));

        let prop_info = PropertyInfo {
            name: prop_name,
            declaring_class: class_info.name,
            property_type,
            signature_type,
            visibility,
            is_static,
            is_readonly,
            has_default,
            is_promoted: false,
            is_deprecated: false,
            description: None,
            start_offset: span.start.offset,
        };

        class_info.properties.insert(prop_name, prop_info);
    }

    fn collect_params(
        &mut self,
        params: &TokenSeparatedSequence<'_, FunctionLikeParameter<'_>>,
    ) -> Vec<ParamInfo> {
        params
            .iter()
            .map(|param| {
                let name = self.interner.intern(param.variable.name);
                // Native PHP type hint is the signature_type
                let signature_type = param.hint.as_ref().map(|h| self.resolve_type(h));

                // For now, param_type is same as signature_type
                // Docblock param types will be resolved during analysis
                let param_type = signature_type.clone();

                ParamInfo {
                    name,
                    param_type,
                    signature_type,
                    has_docblock_type: false,
                    is_optional: param.default_value.is_some(),
                    is_variadic: param.ellipsis.is_some(),
                    by_ref: param.ampersand.is_some(),
                    is_promoted: param.is_promoted_property(),
                    default_type: None, // TODO: Infer from default value
                    description: None,
                    start_offset: param.span().start.offset,
                }
            })
            .collect()
    }

    /// Resolve a type hint using the type resolver.
    fn resolve_type(&mut self, hint: &Hint<'_>) -> TUnion {
        resolve_hint(hint, self.interner, self.current_namespace)
    }

    /// Create a fully qualified name from a local name.
    fn make_fqn(&mut self, local_name: &str) -> StrId {
        if let Some(ns) = self.current_namespace {
            let ns_str = self.interner.lookup(ns);
            let full_name = format!("{}\\{}", ns_str, local_name);
            self.interner.intern(&full_name)
        } else {
            self.interner.intern(local_name)
        }
    }

    /// Resolve an identifier to a fully qualified name.
    fn resolve_identifier(&mut self, ident: &Identifier<'_>) -> StrId {
        if ident.is_fully_qualified() {
            // Strip leading backslash
            let value = ident.value().strip_prefix('\\').unwrap_or(ident.value());
            self.interner.intern(value)
        } else {
            self.make_fqn(ident.value())
        }
    }
}

// Helper functions that don't need self

fn parse_method_modifiers(modifiers: &Sequence<'_, Modifier<'_>>) -> (Visibility, bool, bool, bool) {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_abstract = false;
    let mut is_final = false;

    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => visibility = Visibility::Public,
            Modifier::Protected(_) => visibility = Visibility::Protected,
            Modifier::Private(_) => visibility = Visibility::Private,
            Modifier::Static(_) => is_static = true,
            Modifier::Abstract(_) => is_abstract = true,
            Modifier::Final(_) => is_final = true,
            _ => {}
        }
    }

    (visibility, is_static, is_abstract, is_final)
}

fn parse_property_modifiers(modifiers: &Sequence<'_, Modifier<'_>>) -> (Visibility, bool, bool) {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_readonly = false;

    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => visibility = Visibility::Public,
            Modifier::Protected(_) => visibility = Visibility::Protected,
            Modifier::Private(_) => visibility = Visibility::Private,
            Modifier::Static(_) => is_static = true,
            Modifier::Readonly(_) => is_readonly = true,
            _ => {}
        }
    }

    (visibility, is_static, is_readonly)
}

fn parse_const_visibility(modifiers: &Sequence<'_, Modifier<'_>>) -> Visibility {
    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => return Visibility::Public,
            Modifier::Protected(_) => return Visibility::Protected,
            Modifier::Private(_) => return Visibility::Private,
            _ => {}
        }
    }
    Visibility::Public
}
