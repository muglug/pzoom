//! Declaration collector - extracts class, function, and constant declarations from AST.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::attribute::AttributeList;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::class_like::enum_case::EnumCaseItem;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;
use mago_syntax::ast::ast::class_like::property::{Property, PropertyItem};
use mago_syntax::ast::ast::class_like::trait_use::{
    TraitUseAdaptation, TraitUseMethodReference, TraitUseSpecification,
};
use mago_syntax::ast::ast::constant::Constant;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::modifier::Modifier;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::type_hint::Hint;
use mago_syntax::ast::ast::r#use::{Use, UseItem, UseItems};
use mago_syntax::ast::sequence::TokenSeparatedSequence;
use mago_syntax::ast::{Program, Sequence, Statement, Trivia, TriviaKind};

use pzoom_code_info::class_like_info::{
    ClassConstantInfo, ClassLikeInfo, ClassLikeKind, DocblockIssue, DuplicatePropertyIssue,
    PropertyInfo, TemplateType, TemplateVariance, TraitMethodAlias, Visibility,
};
use pzoom_code_info::class_type_alias::ClassTypeAlias;
use pzoom_code_info::codebase_info::{
    ConstantInfo, GlobalDefine, GlobalDefineValue, InlineCallableParamType,
    InlineCallableTypeAnnotation, InlineCheckTypeAnnotation, InlineTraceAnnotation,
    InlineTypeAnnotations, InlineVarTypeAnnotation,
};
use pzoom_code_info::functionlike_info::{
    Assertion, AssertionType, ConditionalReturnType, FunctionLikeInfo,
    FunctionTemplateType, ParamInfo,
};
use pzoom_code_info::t_atomic::PropertiesOfVisibility;
use pzoom_code_info::type_resolution::TypeResolutionContext;
use pzoom_code_info::{GenericParent, TAtomic, TUnion, combine_union_types};
use pzoom_str::{Interner, StrId, ThreadedInterner};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::type_resolver::resolve_hint;

mod classlike_scanner;
mod functionlike_scanner;
mod initializer_summary;
pub mod simple_type_inferer;
mod taint_scanner;

/// Collected declarations from a PHP file.
#[derive(Debug, Default)]
pub struct CollectedDeclarations {
    pub classes: Vec<ClassLikeInfo>,
    pub functions: Vec<FunctionLikeInfo>,
    pub constants: Vec<ConstantInfo>,
    pub type_aliases: Vec<ClassTypeAlias>,
    pub inline_annotations: InlineTypeAnnotations,
    /// File-level docblock problems as (offset, message), surfaced as
    /// InvalidDocblock during analysis.
    pub docblock_parse_issues: Vec<(u32, String)>,
    /// Every `define()` seen in the file (Psalm's ExpressionScanner); becomes
    /// a global constant after populate under `allConstantsGlobal`.
    pub global_defines: Vec<GlobalDefine>,
    /// `@psalm-import-type ALIAS from CLASS` records, validated against the
    /// populated codebase (the class may live elsewhere).
    pub type_alias_imports: Vec<(StrId, String)>,
}

/// Collects declarations from a parsed PHP program.
pub struct DeclarationCollector<'a, 'p> {
    interner: &'a ThreadedInterner,
    file_path: StrId,
    source: &'p str,
    current_namespace: Option<StrId>,
    use_aliases: FxHashMap<String, StrId>,
    declarations: CollectedDeclarations,
    known_type_aliases: &'a FxHashMap<StrId, ClassTypeAlias>,
    active_docblock_type_aliases: FxHashMap<String, TUnion>,
    /// When set, docblock alias lookups see only `active_docblock_type_aliases`
    /// (the class's own definitions + imports). Psalm scopes CLASSLIKE-level
    /// docblocks (own @psalm-type definitions, @template-extends/-implements
    /// arguments) to the class; only function-likes see the file-wide map.
    restrict_aliases_to_active: bool,
    /// Trivia (comments) from the program for docblock parsing
    trivia: &'p Sequence<'p, Trivia<'p>>,
    /// Class names from positive `class_exists()`-style guards of the `if`
    /// blocks currently being walked (Psalm's exists_cond_expr).
    pub(crate) current_guard_classes: Vec<StrId>,
    /// Lowercased names from enclosing `!function_exists('name')` guards.
    pub(crate) current_not_exists_function_guards: Vec<String>,
    /// Conditional-type parsing scope for the function-like currently being
    /// scanned (subject entity, params, generated synthetic templates).
    conditional_subject_scope: ConditionalSubjectScope,
}

#[derive(Clone)]
pub(crate) struct DocblockTemplateBinding {
    name: StrId,
    defining_entity: GenericParent,
    as_type: TUnion,
    variance: TemplateVariance,
}

type TemplateMap = FxHashMap<String, DocblockTemplateBinding>;

/// Scratch state for conditional-type parsing: the function-like whose
/// docblock is being parsed, its params, and the synthetic templates the
/// conditional subjects generate (Psalm's TGeneratedFromParam /
/// TFunctionArgCount / TPhpMajorVersion model — registered on the function so
/// call sites bind them like any other template).
#[derive(Default)]
struct ConditionalSubjectScope {
    entity: Option<GenericParent>,
    params: Vec<(StrId, Option<TUnion>)>,
    generated_templates: Vec<pzoom_code_info::functionlike_info::FunctionTemplateType>,
    /// Names used as conditional-type subjects (`T is ...`) while scanning
    /// this function-like; declared templates matching them get
    /// `conditional_subject` set when the template list is assembled.
    subject_names: Vec<StrId>,
}

fn docblock_conditional_union(
    conditional: pzoom_code_info::t_atomic::ConditionalReturnType,
) -> TUnion {
    let mut union = TUnion::new(TAtomic::TConditional(Box::new(conditional)));
    union.from_docblock = true;
    union
}

impl<'a, 'p> DeclarationCollector<'a, 'p> {
    pub fn new(
        interner: &'a ThreadedInterner,
        file_path: StrId,
        source: &'p str,
        known_type_aliases: &'a FxHashMap<StrId, ClassTypeAlias>,
        trivia: &'p Sequence<'p, Trivia<'p>>,
    ) -> Self {
        Self {
            interner,
            file_path,
            source,
            current_namespace: None,
            use_aliases: FxHashMap::default(),
            declarations: CollectedDeclarations::default(),
            known_type_aliases,
            active_docblock_type_aliases: FxHashMap::default(),
            restrict_aliases_to_active: false,
            conditional_subject_scope: ConditionalSubjectScope::default(),
            trivia,
            current_guard_classes: Vec::new(),
            current_not_exists_function_guards: Vec::new(),
        }
    }

    /// Collect all declarations from a program.
    pub fn collect(mut self, program: &Program<'_>) -> CollectedDeclarations {
        for statement in &program.statements {
            self.visit_statement(statement);
        }

        self.collect_top_level_inline_docblock_annotations(program.statements.as_slice());
        self.collect_inline_trace_annotations_from_source();
        self.collect_inline_check_type_annotations_from_source();
        self.validate_type_alias_definitions_in_all_docblocks();
        self.declarations
    }

    /// Validate every `@psalm-type`/`@phpstan-type` definition in the file's
    /// docblocks: a definition the type parser rejects records an
    /// InvalidDocblock issue (Psalm reports these at scan, even for orphan
    /// docblocks attached to nothing).
    fn validate_type_alias_definitions_in_all_docblocks(&mut self) {
        let mut entries: Vec<(u32, String)> = Vec::new();
        for trivia in self.trivia.iter() {
            if trivia.kind != TriviaKind::DocBlockComment {
                continue;
            }
            let parsed = crate::docblock::parse(trivia.value, 0);
            // Definitions resolve sequentially (Psalm's ClassLikeNodeScanner
            // feeds each alias the map built so far): a reference to an alias
            // defined LATER in the same docblock is undefined at that point.
            let mut ordered_definitions: Vec<(usize, String, String)> = Vec::new();
            for key in ["phpstan-type", "psalm-type"] {
                if let Some(tags) = parsed.tags.get(key) {
                    for (tag_offset, content) in tags.iter() {
                        if let Some((alias_name, type_definition)) =
                            parse_type_alias_tag_content(content)
                        {
                            ordered_definitions.push((*tag_offset, alias_name, type_definition));
                        }
                    }
                }
            }
            ordered_definitions.sort_by_key(|(tag_offset, _, _)| *tag_offset);
            let all_alias_names: Vec<String> = ordered_definitions
                .iter()
                .map(|(_, alias_name, _)| alias_name.clone())
                .collect();
            let mut defined_so_far: Vec<&str> = Vec::new();
            for (_, alias_name, type_definition) in &ordered_definitions {
                for other_alias in &all_alias_names {
                    if other_alias == alias_name
                        || defined_so_far.iter().any(|defined| defined == other_alias)
                    {
                        continue;
                    }
                    if definition_references_alias(type_definition, other_alias) {
                        entries.push((
                            trivia.span.start.offset,
                            format!(
                                "Docblock-defined class, interface or enum named {} does not exist",
                                other_alias
                            ),
                        ));
                    }
                }
                defined_so_far.push(alias_name);
            }

            for key in ["phpstan-type", "psalm-type"] {
                let Some(tags) = parsed.tags.get(key) else {
                    continue;
                };
                for content in tags.values() {
                    let Some((alias_name, type_definition)) =
                        parse_type_alias_tag_content(content)
                    else {
                        continue;
                    };
                    // The type parser is deliberately lenient (an empty
                    // shape-entry type recovers as mixed), so also flag
                    // entries whose `:` is followed by no type at all.
                    let has_empty_shape_entry = {
                        let compact: String =
                            type_definition.chars().filter(|c| !c.is_whitespace()).collect();
                        compact.ends_with(':')
                            || compact.contains(":}")
                            || compact.contains(":,")
                    };
                    // Psalm's TypeParser throws on duplicate shape keys and
                    // self-referencing definitions.
                    let has_duplicate_shape_key = shape_has_duplicate_keys(&type_definition);
                    let references_itself =
                        definition_references_alias(&type_definition, &alias_name);
                    if has_empty_shape_entry
                        || has_duplicate_shape_key
                        || references_itself
                        || crate::docblock::parse_type_string(&type_definition, self.interner.parent_ref())
                            .is_err()
                    {
                        entries.push((
                            trivia.span.start.offset,
                            format!("Invalid type definition for alias {}", alias_name),
                        ));
                    }
                }
            }

            // `@psalm-import-type X from Y [as Z]` syntax validation (Psalm's
            // ClassLikeDocblockParser InvalidTypeImport).
            for key in ["phpstan-import-type", "psalm-import-type"] {
                let Some(tags) = parsed.tags.get(key) else {
                    continue;
                };
                for content in tags.values() {
                    let parts: Vec<&str> = content.split_whitespace().collect();
                    let malformed = parts.len() < 3
                        || !parts[1].eq_ignore_ascii_case("from")
                        || parts[2].is_empty()
                        || (parts.len() >= 4
                            && parts[3].eq_ignore_ascii_case("as")
                            && parts.len() < 5);
                    if malformed {
                        entries.push((
                            trivia.span.start.offset,
                            "Invalid type import".to_string(),
                        ));
                    }
                }
            }
        }
        self.declarations.docblock_parse_issues.extend(entries);
    }

    /// All docblocks in the contiguous comment run preceding a position
    /// (each separated from the next, and from the position, by whitespace
    /// only), earliest first. Psalm's PhpParser attaches the whole run to the
    /// node (`$node->getComments()`), and ClassLikeNodeScanner reads
    /// `@psalm-type` aliases from every one of them — e.g. an alias block
    /// separated from the class by a second `@internal` docblock.
    fn find_preceding_docblock_run(&self, start_offset: u32) -> Vec<crate::docblock::ParsedDocblock> {
        let mut run: Vec<&'p Trivia<'p>> = Vec::new();

        let mut docblocks: Vec<&'p Trivia<'p>> = self
            .trivia
            .iter()
            .filter(|trivia| {
                trivia.kind == TriviaKind::DocBlockComment
                    && trivia.span.end.offset < start_offset
            })
            .collect();
        docblocks.sort_by_key(|trivia| trivia.span.start.offset);

        let mut boundary = start_offset;
        for trivia in docblocks.into_iter().rev() {
            let gap = &self.source[trivia.span.end.offset as usize..boundary as usize];
            if !gap.chars().all(char::is_whitespace) {
                break;
            }
            boundary = trivia.span.start.offset;
            run.push(trivia);
        }

        run.into_iter()
            .rev()
            .map(|trivia| crate::docblock::parse(trivia.value, 0))
            .collect()
    }


    /// Whether a docblock-to-node gap carries no real code: whitespace and
    /// LINE comments are permeable (Psalm attaches the docblock through a
    /// trailing `// note` between it and the declaration).
    fn gap_is_ignorable(gap: &str) -> bool {
        gap.lines().all(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#')
        })
    }

    /// Find the docblock comment that precedes a given position.
    fn find_preceding_docblock(&self, start_offset: u32) -> Option<&'p str> {
        // Find the docblock that ends closest to (but before) the start_offset
        let mut best_match: Option<&'p Trivia<'p>> = None;

        for trivia in self.trivia.iter() {
            if trivia.kind == TriviaKind::DocBlockComment {
                let end = trivia.span.end.offset;
                if end < start_offset {
                    let gap = &self.source[end as usize..start_offset as usize];
                    if !Self::gap_is_ignorable(gap) {
                        continue;
                    }

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

    /// Like [`Self::find_preceding_docblock`], also yielding the docblock's
    /// start offset (tag offsets from the parser are docblock-relative).
    fn find_preceding_docblock_with_offset(&self, start_offset: u32) -> Option<(u32, &'p str)> {
        let mut best_match: Option<&'p Trivia<'p>> = None;

        for trivia in self.trivia.iter() {
            if trivia.kind == TriviaKind::DocBlockComment {
                let end = trivia.span.end.offset;
                if end < start_offset {
                    let gap = &self.source[end as usize..start_offset as usize];
                    if !Self::gap_is_ignorable(gap) {
                        continue;
                    }

                    if best_match
                        .map(|b| trivia.span.end.offset > b.span.end.offset)
                        .unwrap_or(true)
                    {
                        best_match = Some(trivia);
                    }
                }
            }
        }

        best_match.map(|t| (t.span.start.offset, t.value))
    }

    fn collect_inline_docblock_annotations_in_span(
        &mut self,
        body_start: u32,
        body_end: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let docblocks: Vec<(u32, &'p str)> = self
            .trivia
            .iter()
            .filter(|trivia| {
                (trivia.kind == TriviaKind::DocBlockComment
                    || trivia.kind == TriviaKind::MultiLineComment)
                    && trivia.value.contains('@')
                    && trivia.span.start.offset >= body_start
                    && trivia.span.end.offset <= body_end
            })
            .map(|trivia| (trivia.span.end.offset, trivia.value))
            .collect();

        for (doc_end, docblock) in docblocks {
            let Some(target_offset) =
                self.find_next_non_whitespace_offset(doc_end.saturating_add(1))
            else {
                continue;
            };

            if target_offset < body_start || target_offset > body_end {
                continue;
            }

            let parsed = crate::docblock::parse(docblock, 0);
            self.collect_inline_var_annotations_from_docblock(
                &parsed,
                target_offset,
                self_class,
                parent_class,
                template_map,
            );
            self.collect_inline_callable_annotations_from_docblock(
                &parsed,
                target_offset,
                self_class,
                parent_class,
                template_map,
            );
            self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);
            self.collect_inline_scope_this_annotations_from_docblock(
                &parsed,
                target_offset,
                self_class,
                parent_class,
                template_map,
            );
        }
    }

    fn collect_top_level_inline_docblock_annotations(&mut self, statements: &[Statement<'_>]) {
        let statement_spans: Vec<(u32, u32)> = statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Expression(_)
                | Statement::Echo(_)
                | Statement::Return(_)
                | Statement::If(_)
                | Statement::While(_)
                | Statement::Foreach(_)
                | Statement::For(_)
                | Statement::Switch(_)
                | Statement::Try(_)
                | Statement::Block(_)
                | Statement::Unset(_)
                | Statement::Global(_)
                | Statement::Noop(_)
                // Template files: a `@var` docblock may precede a closing tag
                // or inline HTML (`/** @var Foo $this */ ?> <?= $this->... ?>`).
                // php-parser attaches such comments to the next statement and
                // Psalm's StatementsAnalyzer applies their var comments like
                // any other statement docblock, so these spans are eligible
                // targets too.
                | Statement::ClosingTag(_)
                | Statement::Inline(_)
                | Statement::EchoTag(_) => {
                    Some((statement.span().start.offset, statement.span().end.offset))
                }
                _ => None,
            })
            .collect();

        let docblocks: Vec<(u32, &'p str)> = self
            .trivia
            .iter()
            .filter(|trivia| {
                (trivia.kind == TriviaKind::DocBlockComment
                    || trivia.kind == TriviaKind::MultiLineComment)
                    && trivia.value.contains('@')
            })
            .map(|trivia| (trivia.span.end.offset, trivia.value))
            .collect();

        for (doc_end, docblock) in docblocks {
            let Some(target_offset) =
                self.find_next_non_whitespace_offset(doc_end.saturating_add(1))
            else {
                continue;
            };

            let parsed = crate::docblock::parse(docblock, 0);
            self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);
            // `@psalm-scope-this` may precede a closing tag / inline HTML
            // (template files), which the statement-span gate below excludes.
            // Bounded to this statement list so a namespace pass only claims
            // its own docblocks (alias resolution is namespace-scoped).
            if statements.first().zip(statements.last()).is_some_and(
                |(first_statement, last_statement)| {
                    target_offset >= first_statement.span().start.offset
                        && target_offset <= last_statement.span().end.offset
                },
            ) {
                self.collect_inline_scope_this_annotations_from_docblock(
                    &parsed,
                    target_offset,
                    None,
                    None,
                    None,
                );
            }

            if !statement_spans
                .iter()
                .any(|(start, end)| target_offset >= *start && target_offset <= *end)
            {
                continue;
            }

            self.collect_inline_var_annotations_from_docblock(
                &parsed,
                target_offset,
                None,
                None,
                None,
            );
            self.collect_inline_callable_annotations_from_docblock(
                &parsed,
                target_offset,
                None,
                None,
                None,
            );
        }
    }

    /// `@psalm-scope-this C`: from the annotated statement on, `$this` is an
    /// instance of `C`, resolved through the active namespace/use aliases
    /// (Psalm's StatementsAnalyzer `psalm-scope-this` handling).
    fn collect_inline_scope_this_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(tags) = parsed.tags.get("psalm-scope-this") else {
            return;
        };
        let Some(content) = tags.values().next() else {
            return;
        };

        let class_str = take_first_docblock_type_token(content.trim());
        if class_str.is_empty() {
            return;
        }

        let Ok(parsed_type) =
            crate::docblock::parse_type_string(class_str, self.interner.parent_ref())
        else {
            return;
        };
        let resolved =
            self.resolve_docblock_union_type(parsed_type, self_class, parent_class, template_map);
        let Some(TAtomic::TNamedObject { name, .. }) = resolved.types.first() else {
            return;
        };

        // First write wins: the namespace-scoped pass resolves aliases
        // correctly and runs before the whole-program sweep re-encounters the
        // same docblock with no namespace state.
        self.declarations
            .inline_annotations
            .scope_this_annotations
            .entry(target_offset)
            .or_insert(*name);
    }

    fn collect_inline_var_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(var_tags) = parsed.combined_tags.get("var") else {
            return;
        };

        let mut annotations = Vec::new();

        for content in var_tags.values() {
            // Psalm's CommentAnalyzer reads the FIRST token of a `@var` line as
            // the type; a leading `$var` (other than `$this`) is the legacy
            // name-first form and throws "Misplaced variable"
            // (MissingDocblockType), aborting the annotation entirely.
            let trimmed = content.trim_start();
            if trimmed.starts_with('$') && !trimmed.starts_with("$this") {
                let var_name = crate::docblock::extract_var_path_from_content(content)
                    .map(|name| self.interner.intern(name));
                annotations.push(InlineVarTypeAnnotation {
                    var_name,
                    var_type: TUnion::mixed(),
                    is_invalid: false,
                    is_misplaced_variable: true,
                });
                continue;
            }

            let var_name = crate::docblock::extract_var_path_from_content(content)
                .map(|name| self.interner.intern(name));
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                continue;
            };

            let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );
            let is_invalid = !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                None,
                &[],
            );

            annotations.push(InlineVarTypeAnnotation {
                var_name,
                var_type: resolved_type,
                is_invalid,
                is_misplaced_variable: false,
            });
        }

        if annotations.is_empty() {
            return;
        }

        self.declarations
            .inline_annotations
            .var_annotations
            .entry(target_offset)
            .or_default()
            .extend(annotations);
    }

    fn collect_inline_callable_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let has_template_annotation = parsed.combined_tags.contains_key("template")
            || parsed.combined_tags.contains_key("template-covariant");
        let is_pure = self.is_docblock_pure(parsed);

        let mut params = Vec::new();
        if let Some(param_tags) = parsed.combined_tags.get("param") {
            for content in param_tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let param_name = crate::docblock::extract_var_name_from_content(content)
                    .map(|name| self.interner.intern(name));

                params.push(InlineCallableParamType {
                    param_name,
                    param_type: resolved_type,
                });
            }
        }

        let return_type = parsed
            .combined_tags
            .get("return")
            .and_then(|tags| {
                tags.values()
                    .next()
                    .and_then(|content| crate::docblock::extract_type_string_from_content(content))
            })
            .map(|type_str| {
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                )
            });

        if params.is_empty() && return_type.is_none() && !has_template_annotation && !is_pure {
            return;
        }

        self.declarations
            .inline_annotations
            .callable_annotations
            .entry(target_offset)
            .and_modify(|existing| {
                existing.params.extend(params.clone());
                if existing.return_type.is_none() {
                    existing.return_type = return_type.clone();
                }
                existing.has_template_annotation |= has_template_annotation;
                existing.is_pure |= is_pure;
            })
            .or_insert_with(|| InlineCallableTypeAnnotation {
                params,
                return_type,
                has_template_annotation,
                is_pure,
            });
    }

    fn collect_inline_trace_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
    ) {
        let mut trace_annotations = Vec::new();

        for key in ["psalm-trace", "trace"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let var_names = crate::docblock::extract_var_names_from_content(content)
                    .into_iter()
                    .map(|name| self.interner.intern(name))
                    .collect::<Vec<_>>();

                if var_names.is_empty() {
                    continue;
                }

                trace_annotations.push(InlineTraceAnnotation { var_names });
            }
        }

        if trace_annotations.is_empty() {
            return;
        }

        let entry = self
            .declarations
            .inline_annotations
            .trace_annotations
            .entry(target_offset)
            .or_default();

        for annotation in trace_annotations {
            if !entry
                .iter()
                .any(|existing| existing.var_names == annotation.var_names)
            {
                entry.push(annotation);
            }
        }
    }

    fn collect_inline_trace_annotations_from_source(&mut self) {
        let mut cursor = 0usize;

        while let Some(start_rel) = self.source[cursor..].find("/**") {
            let start = cursor + start_rel;
            let comment_start = start + 3;
            let Some(end_rel) = self.source[comment_start..].find("*/") else {
                break;
            };

            let end = comment_start + end_rel + 2;
            let comment = &self.source[start..end];

            if comment.contains("@psalm-trace") || comment.contains("@trace") {
                let parsed = crate::docblock::parse(comment, 0);
                if let Some(target_offset) = self.find_next_non_whitespace_offset(end as u32) {
                    self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);
                }
            }

            cursor = end;
        }
    }

    /// Scans the raw source for `@psalm-check-type[-exact]` docblocks (Psalm's
    /// `StatementsAnalyzer` check-type handling). Each assertion is keyed by the
    /// offset of the statement it precedes so it can be evaluated against the
    /// in-scope variable types during analysis; when no statement follows (a
    /// malformed standalone annotation) it is keyed by the docblock's end offset
    /// so the `InvalidDocblock` is still reported.
    fn collect_inline_check_type_annotations_from_source(&mut self) {
        // Iterate the parser's trivia rather than raw text: a
        // '@psalm-check-type' docblock embedded in a string literal (test
        // fixtures holding PHP code) is not a comment and must not be
        // collected (or validated).
        let docblocks: Vec<(u32, &str)> = self
            .trivia
            .iter()
            .filter(|trivia| trivia.kind == TriviaKind::DocBlockComment)
            .map(|trivia| (trivia.span.end.offset, trivia.value))
            .collect();
        for (end, comment) in docblocks {
            if comment.contains("@psalm-check-type") {
                let parsed = crate::docblock::parse(comment, 0);
                let target_offset = self
                    .find_next_non_whitespace_offset(end)
                    .unwrap_or(end);
                self.collect_inline_check_type_annotations_from_docblock(&parsed, target_offset);
            }
        }
    }

    fn collect_inline_check_type_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
    ) {
        let mut annotations = Vec::new();

        for (key, is_exact) in [("psalm-check-type", false), ("psalm-check-type-exact", true)] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                // Psalm: `array_map('trim', explode('=', $line, 2)) + ['', '']`.
                let (lhs, rhs) = match content.split_once('=') {
                    Some((lhs, rhs)) => (lhs.trim(), rhs.trim()),
                    None => (content.trim(), ""),
                };

                let annotation_possibly_undefined = lhs.ends_with('?');
                let var_token = lhs.strip_suffix('?').unwrap_or(lhs).trim();

                let var_id = if var_token.starts_with('$') && var_token.len() > 1 {
                    Some(self.interner.intern(var_token))
                } else {
                    None
                };

                let check_type = if rhs.is_empty() {
                    None
                } else {
                    let parsed_type = crate::docblock::parse_type_string(rhs, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    Some(self.resolve_docblock_union_type(parsed_type, None, None, None))
                };

                annotations.push(InlineCheckTypeAnnotation {
                    checked_var_raw: if lhs.is_empty() {
                        None
                    } else {
                        Some(lhs.to_string())
                    },
                    var_id,
                    check_type,
                    annotation_possibly_undefined,
                    is_exact,
                });
            }
        }

        if annotations.is_empty() {
            return;
        }

        self.declarations
            .inline_annotations
            .check_type_annotations
            .entry(target_offset)
            .or_default()
            .extend(annotations);
    }

    fn find_next_non_whitespace_offset(&self, offset: u32) -> Option<u32> {
        let bytes = self.source.as_bytes();
        let mut i = offset as usize;

        while i < bytes.len() {
            match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < bytes.len() {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => return Some(i as u32),
            }
        }

        None
    }

    /// Register `function` declarations nested inside a function/method body
    /// (descending through if/else and block statements), as Psalm's
    /// ReflectorVisitor does for conditionally-declared functions.
    fn visit_nested_function_declarations(&mut self, statements: &[Statement<'_>]) {
        for stmt in statements {
            match stmt {
                Statement::Function(nested_function) => self.visit_function(nested_function),
                Statement::Block(block) => {
                    self.visit_nested_function_declarations(block.statements.as_slice());
                }
                Statement::If(if_stmt) => {
                    use mago_syntax::ast::ast::control_flow::r#if::IfBody;
                    match &if_stmt.body {
                        IfBody::Statement(body) => {
                            self.visit_nested_function_declarations(std::slice::from_ref(
                                body.statement,
                            ));
                            for else_if in body.else_if_clauses.iter() {
                                self.visit_nested_function_declarations(std::slice::from_ref(
                                    else_if.statement,
                                ));
                            }
                            if let Some(else_clause) = &body.else_clause {
                                self.visit_nested_function_declarations(std::slice::from_ref(
                                    else_clause.statement,
                                ));
                            }
                        }
                        IfBody::ColonDelimited(body) => {
                            self.visit_nested_function_declarations(body.statements.as_slice());
                            for else_if in body.else_if_clauses.iter() {
                                self.visit_nested_function_declarations(
                                    else_if.statements.as_slice(),
                                );
                            }
                            if let Some(else_clause) = &body.else_clause {
                                self.visit_nested_function_declarations(
                                    else_clause.statements.as_slice(),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>) {
        self.collect_preceding_statement_type_aliases(stmt.span().start.offset);

        // Register anonymous classes nested anywhere in this statement
        // (Psalm's ReflectorVisitor registers them as real classlikes).
        // Namespace/Block/If recurse back into visit_statement per child,
        // so only walk at the non-recursing arms to visit each once.
        if !matches!(
            stmt,
            Statement::Namespace(_) | Statement::Block(_) | Statement::If(_)
        ) {
            self.collect_anonymous_classes(stmt);
        }

        match stmt {
            Statement::Namespace(ns) => self.visit_namespace(ns),
            Statement::Use(r#use) => self.visit_use(r#use),
            Statement::Class(class) => self.visit_class(class),
            Statement::Interface(iface) => self.visit_interface(iface),
            Statement::Trait(tr) => self.visit_trait(tr),
            Statement::Enum(en) => self.visit_enum(en),
            Statement::Function(func) => self.visit_function(func),
            Statement::Constant(constant) => self.visit_constant(constant),
            Statement::Expression(_) => {
                self.collect_defined_constants_from_statements_inner(
                    std::slice::from_ref(stmt),
                    None,
                    None,
                    true,
                );
            }
            Statement::If(if_stmt) => self.visit_if(if_stmt),
            Statement::Block(block) => {
                for stmt in block.statements.iter() {
                    self.visit_statement(stmt);
                }
            }
            _ => {}
        }
    }

    /// Collect declarations nested inside an `if`/`elseif`/`else` block.
    ///
    /// Mirrors Psalm's `ReflectorVisitor` + `ExpressionResolver::enterConditional`:
    /// the guard expression is statically evaluated and only the branch(es) it does
    /// not exclude are scanned. A branch whose guard is statically true terminates the
    /// chain (later branches are skipped); a guard that cannot be resolved leaves the
    /// branch reachable. This is what lets a function declared inside `if
    /// (defined('GLOB_BRACE')) { ... } else { ... }` (e.g. `glob` in
    /// CoreGenericFunctions) be collected at all.
    fn visit_if(&mut self, if_stmt: &mago_syntax::ast::ast::control_flow::r#if::If<'_>) {
        // Whether a preceding branch was statically true (so the remaining branches
        // are unreachable). Mirrors Psalm's `skip_if_descendants` bookkeeping.
        let mut entered_definite_branch = false;

        let if_condition = Self::evaluate_stub_conditional(if_stmt.condition);
        if if_condition != Some(false) {
            // Record the positive class_exists() guards for declarations in
            // this branch (Psalm's enterConditional can only resolve them once
            // the whole codebase is known; analysis checks them instead).
            let guard_count = self.collect_guard_classes(if_stmt.condition);
            let not_exists_count = self.collect_not_exists_function_guards(if_stmt.condition);
            for stmt in if_stmt.body.statements() {
                self.visit_statement(stmt);
            }
            self.current_guard_classes
                .truncate(self.current_guard_classes.len() - guard_count);
            self.current_not_exists_function_guards.truncate(
                self.current_not_exists_function_guards.len() - not_exists_count,
            );
        }
        if if_condition == Some(true) {
            entered_definite_branch = true;
        }

        for (elseif_condition, elseif_statements) in if_stmt.body.else_if_clauses() {
            let resolved = if entered_definite_branch {
                Some(false)
            } else {
                Self::evaluate_stub_conditional(elseif_condition)
            };

            if resolved != Some(false) {
                for stmt in elseif_statements {
                    self.visit_statement(stmt);
                }
            }
            if resolved == Some(true) {
                entered_definite_branch = true;
            }
        }

        if let Some(else_statements) = if_stmt.body.else_statements()
            && !entered_definite_branch {
                for stmt in else_statements {
                    self.visit_statement(stmt);
                }
            }
    }

    /// Collect function names from `!function_exists('name')` conjuncts of an
    /// `if` guard onto `current_not_exists_function_guards`; returns how many
    /// were pushed.
    fn collect_not_exists_function_guards(&mut self, expr: &Expression<'_>) -> usize {
        match expr.unparenthesized() {
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::And(_) | BinaryOperator::LowAnd(_)
                ) =>
            {
                self.collect_not_exists_function_guards(binary.lhs)
                    + self.collect_not_exists_function_guards(binary.rhs)
            }
            Expression::UnaryPrefix(unary)
                if matches!(
                    unary.operator,
                    mago_syntax::ast::ast::unary::UnaryPrefixOperator::Not(_)
                ) =>
            {
                if let Expression::Call(mago_syntax::ast::ast::call::Call::Function(func_call)) =
                    unary.operand.unparenthesized()
                    && let Expression::Identifier(callee) = func_call.function.unparenthesized()
                    && callee
                        .value()
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("function_exists")
                    && let Some(name_arg) = func_call.argument_list.arguments.first()
                    && let Expression::Literal(Literal::String(name_literal)) =
                        name_arg.value().unparenthesized()
                    && let Some(name) = name_literal.value
                {
                    self.current_not_exists_function_guards
                        .push(name.trim_start_matches('\\').to_ascii_lowercase());
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Collect class names from positive `class_exists`/`interface_exists`/
    /// `trait_exists`/`enum_exists` conjuncts of an `if` guard onto
    /// `current_guard_classes`; returns how many were pushed.
    fn collect_guard_classes(&mut self, expr: &Expression<'_>) -> usize {
        match expr.unparenthesized() {
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::And(_) | BinaryOperator::LowAnd(_)
                ) =>
            {
                self.collect_guard_classes(binary.lhs) + self.collect_guard_classes(binary.rhs)
            }
            Expression::Call(mago_syntax::ast::ast::call::Call::Function(func_call)) => {
                let Expression::Identifier(identifier) = func_call.function.unparenthesized()
                else {
                    return 0;
                };
                let name = identifier.value().trim_start_matches('\\');
                if !matches!(
                    name,
                    "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
                ) {
                    return 0;
                }
                let Some(arg) = func_call.argument_list.arguments.first() else {
                    return 0;
                };
                let guard_class = match arg.value().unparenthesized() {
                    Expression::Literal(mago_syntax::ast::ast::literal::Literal::String(
                        string_lit,
                    )) => string_lit
                        .value
                        .map(|value| self.interner.intern(value.trim_start_matches('\\'))),
                    Expression::Access(Access::ClassConstant(class_constant_access)) => {
                        if let ClassLikeConstantSelector::Identifier(constant) =
                            &class_constant_access.constant
                            && constant.value.eq_ignore_ascii_case("class")
                            && let Expression::Identifier(class_identifier) =
                                class_constant_access.class.unparenthesized()
                        {
                            Some(self.interner.intern(
                                class_identifier.value().trim_start_matches('\\'),
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(guard_class) = guard_class {
                    self.current_guard_classes.push(guard_class);
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Statically evaluate a stub `if` guard the way Psalm's
    /// `ExpressionResolver::enterConditional` does: `Some(true)`/`Some(false)` when the
    /// outcome is known, `None` when it cannot be resolved (in which case the branch is
    /// treated as reachable). Only the boolean connectives are resolved here; the
    /// `defined()` / `function_exists()` style leaves stay unresolved, so both branches
    /// of a guard like `defined('GLOB_BRACE')` are scanned and reconciled by stub
    /// precedence (`register_function`), which keeps the first/curated declaration.
    fn evaluate_stub_conditional(expr: &Expression<'_>) -> Option<bool> {
        match expr.unparenthesized() {
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::And(_) | BinaryOperator::LowAnd(_)
                ) =>
            {
                let lhs = Self::evaluate_stub_conditional(binary.lhs);
                let rhs = Self::evaluate_stub_conditional(binary.rhs);
                Some(lhs != Some(false) && rhs != Some(false))
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Or(_) | BinaryOperator::LowOr(_)
                ) =>
            {
                let lhs = Self::evaluate_stub_conditional(binary.lhs);
                let rhs = Self::evaluate_stub_conditional(binary.rhs);
                Some(lhs != Some(false) || rhs != Some(false))
            }
            Expression::UnaryPrefix(unary) if unary.operator.is_not() => {
                Self::evaluate_stub_conditional(unary.operand).map(|value| !value)
            }
            _ => None,
        }
    }

    fn visit_namespace(&mut self, ns: &Namespace<'_>) {
        let previous_namespace = self.current_namespace;
        let previous_use_aliases = std::mem::take(&mut self.use_aliases);

        // Set current namespace
        let ns_name = ns.name.as_ref().map(|n| self.interner.intern(n.value()));
        self.current_namespace = ns_name;

        // Visit statements in namespace
        match &ns.body {
            NamespaceBody::Implicit(implicit) => {
                for stmt in &implicit.statements {
                    self.visit_statement(stmt);
                }
                self.collect_top_level_inline_docblock_annotations(implicit.statements.as_slice());
            }
            NamespaceBody::BraceDelimited(block) => {
                for stmt in &block.statements {
                    self.visit_statement(stmt);
                }
                self.collect_top_level_inline_docblock_annotations(block.statements.as_slice());
            }
        }

        self.current_namespace = previous_namespace;
        self.use_aliases = previous_use_aliases;
    }

    fn visit_use(&mut self, use_stmt: &Use<'_>) {
        match &use_stmt.items {
            UseItems::Sequence(sequence) => {
                for item in &sequence.items {
                    self.register_use_alias(item, None);
                }
            }
            UseItems::TypedSequence(sequence) => {
                if sequence.r#type.is_function() || sequence.r#type.is_const() {
                    return;
                }

                for item in &sequence.items {
                    self.register_use_alias(item, None);
                }
            }
            UseItems::TypedList(list) => {
                if list.r#type.is_function() || list.r#type.is_const() {
                    return;
                }

                let namespace = normalize_use_name(list.namespace.value());
                for item in &list.items {
                    self.register_use_alias(item, Some(namespace.as_str()));
                }
            }
            UseItems::MixedList(list) => {
                let namespace = normalize_use_name(list.namespace.value());
                for item in &list.items {
                    if item
                        .r#type
                        .as_ref()
                        .is_some_and(|t| t.is_function() || t.is_const())
                    {
                        continue;
                    }

                    self.register_use_alias(&item.item, Some(namespace.as_str()));
                }
            }
        }
    }

    fn register_use_alias(&mut self, item: &UseItem<'_>, namespace_prefix: Option<&str>) {
        let item_name = normalize_use_name(item.name.value());
        let full_name = if let Some(prefix) = namespace_prefix {
            format!("{}\\{}", prefix, item_name)
        } else {
            item_name
        };

        let alias = item
            .alias
            .as_ref()
            .map(|a| a.identifier.value.to_string())
            .unwrap_or_else(|| {
                full_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(full_name.as_str())
                    .to_string()
            });

        let alias_key = alias.to_ascii_lowercase();
        let target_id = self.interner.intern(&full_name);
        self.use_aliases.insert(alias_key, target_id);
    }


    fn visit_constant(&mut self, constant: &Constant<'_>) {
        for item in &constant.items {
            let name = self.make_fqn(item.name.value);
            let span = item.span();
            let resolve_class = |raw: &str| self.resolve_scanned_class_string(raw);
            let resolve_enum_case = |class_name: &str, case_name: &str, wants_name: bool| {
                self.resolve_scanned_enum_case(class_name, case_name, wants_name)
            };
            // `const TWO = ONE * 2;` — earlier constants of this file stand
            // in for Psalm's `$existing_constants`.
            let resolve_global_constant = |raw: &str| -> Option<TUnion> {
                let candidates: [Option<String>; 2] = [
                    Some(raw.to_string()),
                    self.current_namespace.map(|namespace| {
                        format!("{}\\{}", self.interner.lookup(namespace), raw)
                    }),
                ];
                self.declarations.constants.iter().rev().find_map(|existing| {
                    let existing_name = self.interner.lookup(existing.name);
                    candidates
                        .iter()
                        .flatten()
                        .any(|candidate| *candidate == *existing_name)
                        .then(|| existing.constant_type.clone())
                })
            };
            // Platform-dependent runtime constants take their curated types
            // regardless of the stub initializer (Psalm's ConstFetchAnalyzer
            // hardcodes PHP_INT_SIZE & co.); other constants infer from the
            // initializer.
            let constant_type = pzoom_code_info::runtime_constants::runtime_global_constant_type(
                &item.name.value.to_ascii_lowercase(),
            )
            .or_else(|| {
                simple_type_inferer::infer_with_context(
                    &item.value,
                    &simple_type_inferer::InferClassContext {
                        self_class: None,
                        parent_class: None,
                        class_resolver: Some(&resolve_class),
                        key_overflow_sink: None,
                        enum_case_resolver: Some(&resolve_enum_case),
                        global_constant_resolver: Some(&resolve_global_constant),
                    },
                )
            })
            .unwrap_or_else(TUnion::mixed);

            // `const X = Other::CONST;` defers to the populator, where the
            // referenced class's constants exist (Psalm's UnresolvedConstant
            // components, same as class constants).
            let unresolved_initializer = if constant_type.is_mixed() {
                let intern = |raw: &str| self.interner.intern(raw);
                simple_type_inferer::build_unresolved_const_expr(
                    &item.value,
                    &simple_type_inferer::InferClassContext {
                        self_class: None,
                        parent_class: None,
                        class_resolver: Some(&resolve_class),
                        key_overflow_sink: None,
                        enum_case_resolver: Some(&resolve_enum_case),
                        global_constant_resolver: Some(&resolve_global_constant),
                    },
                    &intern,
                )
                .filter(|expr| {
                    !matches!(
                        expr,
                        pzoom_code_info::class_constant_info::UnresolvedConstExpr::Resolved(_)
                    )
                })
            } else {
                None
            };

            let info = ConstantInfo {
                name,
                constant_type,
                file_path: self.file_path,
                start_offset: span.start.offset,
                unresolved_initializer,
            };

            self.declarations.constants.push(info);
        }
    }

    fn precollect_class_constants(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) {
        let parent_class_name =
            parent_class.map(|parent_id| self.interner.lookup(parent_id).to_string());
        for member in members {
            let ClassLikeMember::Constant(class_const) = member else {
                continue;
            };

            let visibility = parse_const_visibility(&class_const.modifiers);
            let hinted_const_type = class_const
                .hint
                .as_ref()
                .map(|hint| self.resolve_type(hint, self_class, parent_class));

            for item in &class_const.items {
                let const_name = self.interner.intern(item.name.value);
                let span = item.span();
                let self_class_name =
                    self_class.map(|class_id| self.interner.lookup(class_id).to_string());
                let alias_map: rustc_hash::FxHashMap<String, String> = self
                    .use_aliases
                    .iter()
                    .map(|(alias, target)| {
                        (alias.clone(), self.interner.lookup(*target).to_string())
                    })
                    .collect();
                let namespace_prefix = self
                    .current_namespace
                    .map(|ns| self.interner.lookup(ns).to_string());
                let resolve_class = move |raw: &str| -> String {
                    let (first_segment, remainder) = match raw.split_once('\\') {
                        Some((first, rest)) => (first, Some(rest)),
                        None => (raw, None),
                    };
                    if let Some(alias_target) = alias_map.get(&first_segment.to_ascii_lowercase())
                    {
                        return match remainder {
                            Some(rest) => format!("{}\\{}", alias_target, rest),
                            None => alias_target.clone(),
                        };
                    }
                    match &namespace_prefix {
                        Some(ns) => format!("{}\\{}", ns, raw),
                        None => raw.to_string(),
                    }
                };
                let constant_type = hinted_const_type
                    .clone()
                    .or_else(|| {
                        simple_type_inferer::infer_with_context(
                            &item.value,
                            &simple_type_inferer::InferClassContext {
                                self_class: self_class_name.as_deref(),
                                parent_class: parent_class_name.as_deref(),
                                class_resolver: Some(&resolve_class),
                                key_overflow_sink: None,
                                enum_case_resolver: None,
                                global_constant_resolver: None,
                            },
                        )
                    })
                    .unwrap_or_else(TUnion::mixed);

                // Psalm's ClassLikeNodeScanner: a constant name colliding
                // with an existing constant is a DuplicateConstant.
                if class_info.constants.contains_key(&const_name) {
                    class_info
                        .duplicate_constant_issues
                        .push(pzoom_code_info::class_like_info::DuplicatePropertyIssue {
                            property_name: const_name,
                            start_offset: span.start.offset,
                            end_offset: span.end.offset,
                        });
                }
                class_info.constants.insert(
                    const_name,
                    ClassConstantInfo {
                        name: const_name,
                        declaring_class: class_info.name,
                        constant_type,
                        visibility,
                        is_final: class_const
                            .modifiers
                            .iter()
                            .any(|modifier| matches!(modifier, Modifier::Final(_))),
                        is_deprecated: false,
                        start_offset: span.start.offset,
                        unresolved_initializer: None,
                        enum_case_value: None,
                        circular: false,
                    resolution_failures: Vec::new(),
                        declared_type: None,
                        has_type_hint: class_const.hint.is_some(),
                    },
                );
            }
        }
    }

    fn precollect_enum_case_constants(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        enum_backing_atomic: Option<&TAtomic>,
    ) {
        let mut case_name_types = Vec::new();
        let mut case_value_types = Vec::new();

        for member in members {
            let ClassLikeMember::EnumCase(enum_case) = member else {
                continue;
            };

            let case_name = self.interner.intern(enum_case.item.name().value);
            let case_name_span = enum_case.item.name().span();
            let case_type = TUnion::new(TAtomic::TEnumCase {
                enum_name: class_info.name,
                case_name,
            });

            let case_docblock = self
                .find_preceding_docblock(enum_case.span().start.offset)
                .map(|docblock| crate::docblock::parse(docblock, 0));
            let is_case_deprecated = case_docblock
                .as_ref()
                .is_some_and(|parsed| self.is_docblock_deprecated(parsed))
                || self.has_attribute_named(&enum_case.attribute_lists, "Deprecated");

            let mut case_value_initializer = None;
            let enum_case_value = if let EnumCaseItem::Backed(backed_case) = &enum_case.item {
                let inferred_case_value = simple_type_inferer::infer(&backed_case.value);
                if inferred_case_value.is_none() {
                    // `case K = self::CONST;` / `= Other::CASE->value`: defer
                    // to the populator's ConstantTypeResolver pass (Psalm
                    // stores an UnresolvedConstantComponent case value).
                    let class_fqn = self.interner.lookup(class_info.name).to_string();
                    let alias_map: rustc_hash::FxHashMap<String, String> = self
                        .use_aliases
                        .iter()
                        .map(|(alias, target)| {
                            (alias.clone(), self.interner.lookup(*target).to_string())
                        })
                        .collect();
                    let namespace_prefix = self
                        .current_namespace
                        .map(|ns| self.interner.lookup(ns).to_string());
                    let resolve_class = move |raw: &str| -> String {
                        let (first_segment, remainder) = match raw.split_once('\\') {
                            Some((first, rest)) => (first, Some(rest)),
                            None => (raw, None),
                        };
                        if let Some(alias_target) =
                            alias_map.get(&first_segment.to_ascii_lowercase())
                        {
                            return match remainder {
                                Some(rest) => format!("{}\\{}", alias_target, rest),
                                None => alias_target.clone(),
                            };
                        }
                        match &namespace_prefix {
                            Some(ns) => format!("{}\\{}", ns, raw),
                            None => raw.to_string(),
                        }
                    };
                    let infer_class_context = simple_type_inferer::InferClassContext {
                        self_class: Some(class_fqn.as_ref()),
                        parent_class: None,
                        class_resolver: Some(&resolve_class),
                        global_constant_resolver: None,
                        key_overflow_sink: None,
                        enum_case_resolver: None,
                    };
                    case_value_initializer = simple_type_inferer::build_unresolved_const_expr(
                        &backed_case.value,
                        &infer_class_context,
                        &|s| self.interner.intern(s),
                    );
                }
                let inferred_case_value = inferred_case_value.unwrap_or_else(TUnion::mixed);
                if let Some(single_case_value) = inferred_case_value.get_single() {
                    case_value_types.push(single_case_value.clone());
                } else {
                    case_value_types.push(TAtomic::TMixed);
                }
                Some(inferred_case_value)
            } else {
                None
            };

            // An enum case colliding with an existing constant (or case) is
            // a DuplicateConstant (Psalm's visitEnumDeclaration check).
            if class_info.constants.contains_key(&case_name) {
                class_info
                    .duplicate_constant_issues
                    .push(pzoom_code_info::class_like_info::DuplicatePropertyIssue {
                        property_name: case_name,
                        start_offset: case_name_span.start.offset,
                        end_offset: case_name_span.end.offset,
                    });
            }
            class_info.constants.insert(
                case_name,
                ClassConstantInfo {
                    name: case_name,
                    declaring_class: class_info.name,
                    constant_type: case_type,
                    visibility: Visibility::Public,
                    is_final: true,
                    is_deprecated: is_case_deprecated,
                    start_offset: case_name_span.start.offset,
                    unresolved_initializer: case_value_initializer,
                    enum_case_value,
                    circular: false,
                    resolution_failures: Vec::new(),
                    declared_type: None,
                    has_type_hint: false,
                },
            );

            case_name_types.push(TAtomic::TLiteralString {
                value: enum_case.item.name().value.to_string(),
            });
        }

        if !case_name_types.is_empty() {
            let name_property = StrId::NAME;
            class_info.properties.insert(
                name_property,
                std::sync::Arc::new(PropertyInfo {
                    name: name_property,
                    declaring_class: class_info.name,
                    property_type: Some(TUnion::from_types(case_name_types)),
                    signature_type: None,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_readonly: true,
                    is_readonly_native: true,
                    readonly_allow_private_mutation: false,
                    has_default: false,
                    is_promoted: false,
                    is_hooked: false,
                    is_deprecated: false,
                    location_free: false,
                    marked_initialized: false,
                    internal: Vec::new(),
                    description: None,
                    start_offset: class_info.start_offset,
                }),
            );
        }

        if enum_backing_atomic.is_some() && !case_value_types.is_empty() {
            let value_property = StrId::VALUE;
            class_info.properties.insert(
                value_property,
                std::sync::Arc::new(PropertyInfo {
                    name: value_property,
                    declaring_class: class_info.name,
                    property_type: Some(TUnion::from_types(case_value_types)),
                    signature_type: None,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_readonly: true,
                    is_readonly_native: true,
                    readonly_allow_private_mutation: false,
                    has_default: false,
                    is_promoted: false,
                    is_hooked: false,
                    is_deprecated: false,
                    location_free: false,
                    marked_initialized: false,
                    internal: Vec::new(),
                    description: None,
                    start_offset: class_info.start_offset,
                }),
            );
        }
    }

    fn inject_builtin_enum_methods(
        &mut self,
        class_info: &mut ClassLikeInfo,
        enum_backing_atomic: Option<&TAtomic>,
    ) {
        let enum_case_types: Vec<TAtomic> = class_info
            .constants
            .values()
            .filter_map(|constant| constant.constant_type.get_single().cloned())
            .filter(|atomic| matches!(atomic, TAtomic::TEnumCase { .. }))
            .collect();

        let has_enum_cases = !enum_case_types.is_empty();
        let enum_case_union = if has_enum_cases {
            TUnion::from_types(enum_case_types)
        } else {
            TUnion::new(TAtomic::TNamedObject {
                name: class_info.name,
                type_params: None,
            is_static: false, remapped_params: false })
        };

        let cases_return_type = TUnion::new(if has_enum_cases {
            TAtomic::TNonEmptyList {
                value_type: Box::new(enum_case_union),
            }
        } else {
            TAtomic::TList {
                value_type: Box::new(enum_case_union),
            }
        });

        let cases_name = StrId::CASES;
        class_info
            .methods
            .entry(cases_name)
            .or_insert_with(|| std::sync::Arc::new(FunctionLikeInfo {
                name: cases_name,
                declaring_class: Some(class_info.name),
                params: Vec::new(),
                return_type: Some(cases_return_type.clone()),
                signature_return_type: Some(cases_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            }));

        let Some(backing_atomic) = enum_backing_atomic.cloned() else {
            return;
        };

        let value_param = ParamInfo {
            name: StrId::VALUE_VAR,
            param_type: Some(TUnion::new(backing_atomic.clone())),
            param_out_type: None,
            signature_type: Some(TUnion::new(backing_atomic)),
            has_docblock_type: false,
            is_optional: false,
            is_variadic: false,
            by_ref: false,
            is_promoted: false,
            expect_variable: false,
            default_type: None,
            description: None,
            start_offset: class_info.start_offset,
            sinks: Vec::new(),
            assert_untainted: false,
        };

        let from_name = StrId::FROM;
        let from_return_type = TUnion::new(TAtomic::TNamedObject {
            name: class_info.name,
            type_params: None,
        is_static: false, remapped_params: false });
        class_info
            .methods
            .entry(from_name)
            .or_insert_with(|| std::sync::Arc::new(FunctionLikeInfo {
                name: from_name,
                declaring_class: Some(class_info.name),
                params: vec![value_param.clone()],
                return_type: Some(from_return_type.clone()),
                signature_return_type: Some(from_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            }));

        let try_from_name = StrId::TRY_FROM;
        let mut try_from_return_type = TUnion::new(TAtomic::TNamedObject {
            name: class_info.name,
            type_params: None,
        is_static: false, remapped_params: false });
        try_from_return_type.add_type(TAtomic::TNull);
        class_info
            .methods
            .entry(try_from_name)
            .or_insert_with(|| std::sync::Arc::new(FunctionLikeInfo {
                name: try_from_name,
                declaring_class: Some(class_info.name),
                params: vec![value_param],
                return_type: Some(try_from_return_type.clone()),
                signature_return_type: Some(try_from_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            }));
    }

    fn collect_class_members(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
    ) {
        let class_template_map = self.build_template_map_from_class_template_types(
            &class_info.template_types,
            GenericParent::ClassLike(class_info.name),
        );
        let member_self_class = if class_info.kind == ClassLikeKind::Trait {
            None
        } else {
            Some(class_info.name)
        };

        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    let method_name = self.interner.intern(method.name.value);
                    let span = method.span();

                    let mut signature_return_type = method.return_type_hint.as_ref().map(|rth| {
                        self.resolve_type(&rth.hint, member_self_class, class_info.parent_class)
                    });

                    let mut params = self.collect_params(
                        &method.parameter_list.parameters,
                        member_self_class,
                        class_info.parent_class,
                        Some(&class_info.constants),
                        Some(&class_template_map),
                    );
                    // Docblock-only return type (Psalm's model); native hint stays in
                    // signature_return_type. Effective reads use get_return_type().
                    let mut return_type = None;
                    let mut return_type_location: Option<(u32, u32)> =
                        method.return_type_hint.as_ref().map(|rth| {
                            let hint_span = rth.hint.span();
                            (hint_span.start.offset, hint_span.end.offset)
                        });
                    let mut return_type_mentions_static_const = false;
                    let mut is_pure = false;
                    let mut has_throws = false;
                    let mut member_is_public_api = false;
                    let mut unused_docblock_params: Vec<(String, u32)> = Vec::new();
                    let mut is_mutation_free = false;
                    let mut docblock_external_mutation_free = false;
                    let mut is_deprecated = false;
                    let mut deprecation_message = None;
                    let mut internal = Vec::new();
                    let mut assertions = Vec::new();
                    let mut if_true_assertions = Vec::new();
                    let mut if_false_assertions = Vec::new();
                    let mut template_types = Vec::new();
                    let mut if_this_is_type = None;
                    let mut inherits_docblock = false;
                    let mut no_named_arguments = false;
                    let mut method_template_map = class_template_map.clone();
                    let mut method_docblock_issues: Vec<DocblockIssue> = Vec::new();
                    let mut taints =
                        pzoom_code_info::functionlike_info::FunctionLikeTaints::default();

                    if let Some((docblock_start, docblock)) =
                        self.find_preceding_docblock_with_offset(span.start.offset)
                    {
                        let parsed = crate::docblock::parse(docblock, 0);
                        inherits_docblock = self.is_docblock_inheritdoc(&parsed);
                        is_pure = self.is_docblock_pure(&parsed);
                        has_throws = parsed.tags.contains_key("throws")
                            || parsed.tags.contains_key("phpstan-throws");
                        member_is_public_api = parsed.tags.contains_key("psalm-api")
                            || parsed.tags.contains_key("api");
                        is_mutation_free = self.is_docblock_mutation_free(&parsed);
                        docblock_external_mutation_free =
                            self.is_docblock_external_mutation_free(&parsed);
                        no_named_arguments = self.is_docblock_no_named_arguments(&parsed);
                        is_deprecated = self.is_docblock_deprecated(&parsed);
                        deprecation_message = self.get_docblock_deprecation_message(&parsed);
                        internal = self.get_docblock_internal_scopes(
                            &parsed,
                            class_info.name,
                            &mut class_info.docblock_issues,
                        );

                        let method_defining_entity = self.interner.intern(&format!(
                            "{}::{}",
                            self.interner.lookup(class_info.name),
                            self.interner.lookup(method_name)
                        ));
                        let method_template_bindings = self.parse_docblock_template_bindings(
                            &parsed,
                            GenericParent::FunctionLike(method_defining_entity),
                            member_self_class,
                            class_info.parent_class,
                            Some(&class_template_map),
                            Some(&class_info.constants),
                            &mut method_docblock_issues,
                        );
                        template_types = method_template_bindings
                            .iter()
                            .map(|binding| FunctionTemplateType {
                                name: binding.name,
                                conditional_subject: false,
                                defining_entity: binding.defining_entity,
                                as_type: binding.as_type.clone(),
                            })
                            .collect();
                        method_template_map = self.build_template_map_from_bindings(
                            &method_template_bindings,
                            Some(&class_template_map),
                        );
                        let method_param_names: Vec<StrId> =
                            params.iter().map(|param| param.name).collect();
                        self.validate_function_docblock_type_tags(
                            &parsed,
                            span.start.offset,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                            &method_param_names,
                            &mut method_docblock_issues,
                        );

                        // Psalm FunctionLikeNodeScanner: a promoted property
                        // documented via both the constructor docblock @param
                        // and its own /** @var */ is ambiguous.
                        self.check_promoted_property_duplicate_docs(
                            &parsed,
                            &params,
                            class_info.name,
                            method_name,
                            span.start.offset,
                            &mut method_docblock_issues,
                        );
                        let (method_unmatched_param_tags, method_has_undertyped_params) = self
                            .apply_docblock_param_types(
                                &parsed,
                                &mut params,
                                member_self_class,
                                class_info.parent_class,
                                Some(&method_template_map),
                                Some(&class_info.constants),
                            );
                        if method_has_undertyped_params {
                            for (tag_name, tag_offset) in &method_unmatched_param_tags {
                                method_docblock_issues.push(DocblockIssue {
                                    message: format!(
                                        "Incorrect param name ${} in docblock for {}",
                                        tag_name,
                                        self.interner.lookup(method_name)
                                    ),
                                    start_offset: *tag_offset,
                                    end_offset: tag_offset.saturating_add(1),
                                });
                            }
                        } else {
                            unused_docblock_params = method_unmatched_param_tags;
                        }
                        self.conditional_subject_scope = ConditionalSubjectScope {
                            entity: Some(GenericParent::FunctionLike(method_defining_entity)),
                            params: params
                                .iter()
                                .map(|param| (param.name, param.get_type().cloned()))
                                .collect(),
                            generated_templates: Vec::new(),
                            subject_names: Vec::new(),
                        };
                        self.apply_docblock_param_out_types(
                            &parsed,
                            &mut params,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );
                        if let Some((docblock_return, docblock_conditional_return)) = self
                            .get_docblock_return_type(
                                &parsed,
                                member_self_class,
                                class_info.parent_class,
                                Some(&method_template_map),
                                Some(&class_info.constants),
                                &method_param_names,
                            )
                        {
                            return_type = Some(match docblock_conditional_return {
                                Some(conditional) => {
                                    docblock_conditional_union(conditional)
                                }
                                None => docblock_return,
                            });
                            return_type_mentions_static_const = parsed
                                .get_return_with_offset()
                                .and_then(|(_, content)| {
                                    crate::docblock::extract_type_string_from_content(content)
                                })
                                .is_some_and(|type_str: &str| type_str.contains("static::"));
                            return_type_location = parsed.get_return_with_offset().and_then(
                                |(offset, content)| {
                                    crate::docblock::extract_type_string_from_content(content)
                                        .map(|type_str| {
                                            (
                                                docblock_start + offset as u32,
                                                docblock_start
                                                    + (offset + type_str.len()) as u32,
                                            )
                                        })
                                },
                            );
                        }
                        // `@psalm-taint-escape (<conditional>)` parses while
                        // the conditional-subject scope is alive (see
                        // visit_function).
                        let mut conditional_taint_escapes = self.parse_conditional_taint_escapes(
                            &parsed,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );

                        // Docblock assertions parse while the
                        // conditional-subject scope is alive too: a
                        // conditional assertion type (`@psalm-assert-if-true
                        // =(T is '' ? ...)`) registers its subject template so
                        // call sites keep literal bounds for it.
                        let parsed_assertions = self.get_docblock_assertions(
                            &parsed,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                        );
                        assertions.extend(parsed_assertions.assertions);
                        if_true_assertions.extend(parsed_assertions.if_true_assertions);
                        if_false_assertions.extend(parsed_assertions.if_false_assertions);

                        let generated_conditional_templates = std::mem::take(
                            &mut self.conditional_subject_scope.generated_templates,
                        );
                        template_types.extend(generated_conditional_templates);
                        for template_type in &mut template_types {
                            if self
                                .conditional_subject_scope
                                .subject_names
                                .contains(&template_type.name)
                            {
                                template_type.conditional_subject = true;
                            }
                        }
                        self.conditional_subject_scope = ConditionalSubjectScope::default();
                        if_this_is_type = self.get_docblock_if_this_is_type(
                            &parsed,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );

                        if let Some(return_type) =
                            return_type.as_mut().or(signature_return_type.as_mut())
                        {
                            if self.is_docblock_ignore_nullable_return(&parsed) {
                                return_type.ignore_nullable_issues = true;
                            }
                            if self.is_docblock_ignore_falsable_return(&parsed) {
                                return_type.ignore_falsable_issues = true;
                            }
                        }

                        clear_docblock_flag_when_signature_backed(
                            return_type.as_mut(),
                            signature_return_type.as_ref(),
                        );

                        let (scanned_taints, _raw_conditional_escapes) =
                            self.scan_docblock_taints(&parsed, &mut params, is_pure);
                        taints = scanned_taints;
                        taints.conditionally_removed_taints =
                            std::mem::take(&mut conditional_taint_escapes);
                    }

                    // JetBrains' #[Pure] attribute (phpstorm-stubs); see
                    // visit_function for the bare-vs-Pure(true) semantics.
                    is_pure = is_pure
                        || self.has_attribute_named(&method.attribute_lists, "Pure");

                    // Builtin sinks (Psalm's InternalTaintSinkMap) are
                    // looked up at call time, mirroring Hakana.
                    if is_pure {
                        taints.specialize_call = true;
                    }

                    let mut uses_variadic_builtin_args = false;
                    let mut this_property_mutations = Vec::new();
                    let mut defined_constants = Vec::new();
                    let mut initializer_events = Vec::new();
                    let mut initializer_uninit_reads = Vec::new();
                    if let mago_syntax::ast::ast::class_like::method::MethodBody::Concrete(body) =
                        &method.body
                    {
                        // Function declarations nested in a method body are
                        // real global functions once the method runs — Psalm's
                        // ReflectorVisitor registers them like any other,
                        // including ones guarded by if (!function_exists(...)).
                        self.visit_nested_function_declarations(body.statements.as_slice());
                        let body_span = body.span();
                        uses_variadic_builtin_args = self.span_contains_variadic_builtin_calls(
                            body_span.start.offset,
                            body_span.end.offset,
                        );
                        defined_constants = self.collect_defined_constants_from_statements(
                            body.statements.as_slice(),
                            member_self_class,
                            class_info.parent_class,
                        );
                        this_property_mutations =
                            self.collect_this_property_mutations(body.statements.as_slice());
                        let init_summary = initializer_summary::summarize_method_body(
                            body.statements.as_slice(),
                        );
                        initializer_events = init_summary
                            .events
                            .iter()
                            .map(|event| self.intern_initializer_event(event))
                            .collect();
                        initializer_uninit_reads = init_summary
                            .uninit_reads
                            .into_iter()
                            .map(|(name, offset)| (self.interner.intern(name), offset))
                            .collect();
                        self.collect_inline_docblock_annotations_in_span(
                            body_span.start.offset,
                            body_span.end.offset,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                        );

                        assertions.extend(self.get_implicit_assertions(
                            body.statements.as_slice(),
                            member_self_class,
                            class_info.parent_class,
                        ));
                    }

                    let (visibility, is_static, is_abstract, is_final) =
                        parse_method_modifiers(&method.modifiers);

                    // Hakana (`code_info_builder`): a static method that never
                    // touches static properties carries no cross-call state,
                    // so its taint nodes specialize per call site.
                    if is_static && !self.method_body_accesses_static_property(&method.body) {
                        taints.specialize_call = true;
                    }

                    if method_name == StrId::CONSTRUCT {
                        self.collect_promoted_properties(
                            class_info,
                            &method.parameter_list.parameters,
                            &params,
                        );

                        if !params.is_empty()
                            && let mago_syntax::ast::ast::class_like::method::MethodBody::Concrete(
                                body,
                            ) = &method.body
                        {
                            self.infer_property_types_from_constructor(
                                class_info,
                                body.statements.as_slice(),
                                &params,
                            );
                        }
                    }
                    let has_variadic_param = params.iter().any(|param| param.is_variadic);

                    // Infer mutation-free / external-mutation-free status from the
                    // body, mirroring Psalm's `FunctionLikeNodeScanner`: a getter
                    // (`return $this->prop;` with no params) is mutation-free, and a
                    // constructor that only assigns params/simple values to its own
                    // properties is external-mutation-free.
                    // Method-level flags only: class-level @psalm-immutable /
                    // @psalm-external-mutation-free propagate in the populator
                    // (Psalm Populator), not at scan.
                    let mut is_external_mutation_free =
                        is_mutation_free || docblock_external_mutation_free;
                    let mut mutation_free_inferred = false;
                    if !is_pure && !is_mutation_free
                        && let mago_syntax::ast::ast::class_like::method::MethodBody::Concrete(
                            body,
                        ) = &method.body
                        {
                            let stmts = body.statements.as_slice();
                            if method_name == StrId::CONSTRUCT {
                                if constructor_is_external_mutation_free(stmts) {
                                    is_external_mutation_free = true;
                                    mutation_free_inferred = true;
                                }
                            } else if params.is_empty()
                                && statements_are_simple_property_getter(stmts)
                                && !class_info.is_immutable
                            {
                                is_mutation_free = true;
                                is_external_mutation_free = true;
                                mutation_free_inferred = !is_final && !class_info.is_final;
                            }
                        }

                    let method_info = FunctionLikeInfo {
                        name: method_name,
                        declaring_class: Some(class_info.name),
                        params,
                        return_type,
                        return_type_mentions_static_const,
                        return_type_location,
                        name_location: {
                            let name_span = mago_span::HasSpan::span(&method.name);
                            Some((name_span.start.offset, name_span.end.offset))
                        },
                        signature_return_type,
                        is_pure,
                        has_throws,
                        is_public_api: member_is_public_api,
                        unused_docblock_params,
                        is_mutation_free,
                        is_external_mutation_free,
                        mutation_free_inferred,
                        is_deprecated: is_deprecated
                            || self.has_attribute_named(&method.attribute_lists, "Deprecated"),
                        deprecation_message,
                        is_internal: !internal.is_empty(),
                        internal,
                        is_static,
                        is_abstract,
                        is_final,
                        visibility,
                        returns_by_ref: method.ampersand.is_some(),
                        is_variadic: uses_variadic_builtin_args || has_variadic_param,
                        file_path: self.file_path,
                        start_offset: span.start.offset,
                        end_offset: span.end.offset,
                        assertions,
                        if_true_assertions,
                        if_false_assertions,
                        template_types,
                        if_this_is_type,
                        inherits_docblock,
                        no_named_arguments,
                        docblock_issues: method_docblock_issues,
                        has_override_attribute: self
                            .has_attribute_named(&method.attribute_lists, "Override"),
                        has_return_type_will_change_attribute: self.has_attribute_named(
                            &method.attribute_lists,
                            "ReturnTypeWillChange",
                        ),
                        this_property_mutations,
                        defined_constants,
                        taints,
                        initializer_events,
                        initializer_uninit_reads,
                        ..Default::default()
                    };

                    // Psalm's FunctionLikeNodeScanner: a method whose name is
                    // already declared on this class is a DuplicateMethod.
                    if class_info.methods.contains_key(&method_name) {
                        class_info
                            .duplicate_method_issues
                            .push(pzoom_code_info::class_like_info::DuplicatePropertyIssue {
                                property_name: method_name,
                                start_offset: span.start.offset,
                                end_offset: span.end.offset,
                            });
                    }

                    class_info
                        .methods
                        .insert(method_name, std::sync::Arc::new(method_info));
                }
                ClassLikeMember::Property(property) => {
                    self.collect_property(class_info, property);
                }
                ClassLikeMember::Constant(class_const) => {
                    let visibility = parse_const_visibility(&class_const.modifiers);
                    let const_docblock = self
                        .find_preceding_docblock(class_const.span().start.offset)
                        .map(|docblock| crate::docblock::parse(docblock, 0));
                    let is_const_deprecated = const_docblock
                        .as_ref()
                        .is_some_and(|parsed| self.is_docblock_deprecated(parsed))
                        || self.has_attribute_named(&class_const.attribute_lists, "Deprecated");

                    let hinted_const_type = class_const.hint.as_ref().map(|h| {
                        self.resolve_type(h, Some(class_info.name), class_info.parent_class)
                    });

                    // The DECLARED constant type: a `@var` docblock beats the
                    // native hint (Psalm's ClassConstantStorage::$type).
                    let mut declared_const_type = hinted_const_type.clone();
                    if let Some(parsed) = const_docblock.as_ref()
                        && let Some(var_content) = parsed.get_var()
                        && let Some(type_str) =
                            crate::docblock::extract_type_string_from_content(var_content)
                        && self.is_valid_docblock_type_string(
                            type_str,
                            Some(class_info.name),
                            class_info.parent_class,
                            None,
                            None,
                            &[],
                        )
                        && let Ok(parsed_type) = crate::docblock::parse_type_string(
                            type_str,
                            self.interner.parent_ref(),
                        )
                    {
                        declared_const_type = Some(self.resolve_docblock_union_type(
                            parsed_type,
                            Some(class_info.name),
                            class_info.parent_class,
                            None,
                        ));
                    }

                    for item in &class_const.items {
                        let const_name = self.interner.intern(item.name.value);
                        let span = item.span();
                        let class_fqn = self.interner.lookup(class_info.name).to_string();
                        let alias_map: rustc_hash::FxHashMap<String, String> = self
                            .use_aliases
                            .iter()
                            .map(|(alias, target)| {
                                (alias.clone(), self.interner.lookup(*target).to_string())
                            })
                            .collect();
                        let namespace_prefix = self
                            .current_namespace
                            .map(|ns| self.interner.lookup(ns).to_string());
                        let resolve_class = move |raw: &str| -> String {
                            let (first_segment, remainder) = match raw.split_once('\\') {
                                Some((first, rest)) => (first, Some(rest)),
                                None => (raw, None),
                            };
                            if let Some(alias_target) = alias_map.get(&first_segment.to_ascii_lowercase())
                            {
                                return match remainder {
                                    Some(rest) => format!("{}\\{}", alias_target, rest),
                                    None => alias_target.clone(),
                                };
                            }
                            match &namespace_prefix {
                                Some(ns) => format!("{}\\{}", ns, raw),
                                None => raw.to_string(),
                            }
                        };
                        let key_overflow_sink = std::cell::RefCell::new(Vec::new());
                        let resolve_enum_case =
                            |class_name: &str, case_name: &str, wants_name: bool| {
                                self.resolve_scanned_enum_case(class_name, case_name, wants_name)
                            };
                        let parent_class_name = class_info
                            .parent_class
                            .map(|parent_id| self.interner.lookup(parent_id).to_string());
                        let infer_class_context = simple_type_inferer::InferClassContext {
                            self_class: Some(class_fqn.as_ref()),
                            parent_class: parent_class_name.as_deref(),
                            class_resolver: Some(&resolve_class),
                            global_constant_resolver: None,
                            key_overflow_sink: Some(&key_overflow_sink),
                            enum_case_resolver: Some(&resolve_enum_case),
                        };
                        // The FETCH type stays the inferred value type
                        // (Psalm's getConstantType uses $inferred_type unless
                        // late-static-binding); ClassConstantStorage::$type —
                        // docblock/hint, else the inferred type — is kept
                        // separately for override covariance.
                        let inferred_const_type = hinted_const_type.clone().or_else(|| {
                            simple_type_inferer::infer_with_context(
                                &item.value,
                                &infer_class_context,
                            )
                        });
                        let declared_const_type_for_item = declared_const_type
                            .clone()
                            .or_else(|| inferred_const_type.clone());
                        for (start_offset, end_offset) in key_overflow_sink.take() {
                            class_info.docblock_issues.push(DocblockIssue {
                                message:
                                    "Cannot add an item with an offset beyond i64::MAX"
                                        .to_string(),
                                start_offset,
                                end_offset,
                            });
                        }

                        // Cross-class constant references can't be evaluated
                        // until every class is scanned; store Psalm's
                        // UnresolvedConstantComponent analog for the
                        // populator's ConstantTypeResolver pass.
                        let unresolved_initializer = if inferred_const_type.is_none() {
                            simple_type_inferer::build_unresolved_const_expr(
                                &item.value,
                                &infer_class_context,
                                &|s| self.interner.intern(s),
                            )
                        } else {
                            None
                        };

                        let const_info = ClassConstantInfo {
                            name: const_name,
                            declaring_class: class_info.name,
                            constant_type: inferred_const_type.unwrap_or_else(TUnion::mixed),
                            visibility,
                            is_final: class_const
                                .modifiers
                                .iter()
                                .any(|m| matches!(m, Modifier::Final(_))),
                            is_deprecated: is_const_deprecated,
                            start_offset: span.start.offset,
                            unresolved_initializer,
                            enum_case_value: None,
                            circular: false,
                    resolution_failures: Vec::new(),
                            declared_type: declared_const_type_for_item,
                            has_type_hint: class_const.hint.is_some(),
                        };

                        class_info.constants.insert(const_name, const_info);
                    }
                }
                ClassLikeMember::TraitUse(trait_use) => {
                    for trait_name in &trait_use.trait_names {
                        let name = self.resolve_identifier(trait_name);
                        class_info.used_traits.insert(name);
                    }

                    if let Some(docblock) =
                        self.find_preceding_docblock(trait_use.span().start.offset)
                    {
                        let parsed = crate::docblock::parse(docblock, 0);
                        let use_start = trait_use.span().start.offset;

                        // On a `use TraitName;` statement the only valid template
                        // annotation is `@use`/`@template-use`; `@extends` or
                        // `@implements` here is an InvalidDocblock (Psalm).
                        for wrong_tag in ["extends", "implements"] {
                            if parsed.combined_tags.contains_key(wrong_tag) {
                                self.push_docblock_issue(
                                    class_info,
                                    format!(
                                        "@{} annotation is not valid on a trait use; use @use",
                                        wrong_tag
                                    ),
                                    use_start,
                                    use_start.saturating_add(1),
                                );
                            }
                        }

                        if let Some(use_tags) = parsed.combined_tags.get("use") {
                            let use_start = trait_use.span().start.offset;
                            for content in use_tags.values() {
                                let parsed_type = match crate::docblock::parse_type_string(
                                    content,
                                    self.interner.parent_ref(),
                                ) {
                                    Ok(parsed_type) => parsed_type,
                                    Err(_) => {
                                        self.push_docblock_issue(
                                            class_info,
                                            format!(
                                                "@use annotation \"{}\" could not be parsed",
                                                content.trim()
                                            ),
                                            use_start,
                                            use_start.saturating_add(1),
                                        );
                                        continue;
                                    }
                                };
                                let resolved_type = self.resolve_docblock_union_type(
                                    parsed_type,
                                    Some(class_info.name),
                                    class_info.parent_class,
                                    Some(&class_template_map),
                                );

                                let mut inserted_object = false;
                                let mut saw_non_object = false;
                                for atomic in resolved_type.types {
                                    match atomic {
                                        TAtomic::TNamedObject {
                                            name,
                                            type_params: Some(type_params),
                                            ..
                                        } => {
                                            class_info
                                                .template_extended_offsets
                                                .insert(name, type_params);
                                            inserted_object = true;
                                        }
                                        TAtomic::TNamedObject { .. }
                                        | TAtomic::TTemplateParam { .. } => {
                                            inserted_object = true;
                                        }
                                        _ => {
                                            saw_non_object = true;
                                        }
                                    }
                                }

                                if saw_non_object && !inserted_object {
                                    self.push_docblock_issue(
                                        class_info,
                                        format!(
                                            "@use type \"{}\" must be a class type",
                                            content.trim()
                                        ),
                                        use_start,
                                        use_start.saturating_add(1),
                                    );
                                }
                            }
                        }
                    }

                    if let TraitUseSpecification::Concrete(specification) = &trait_use.specification
                    {
                        for adaptation in &specification.adaptations {
                            if let TraitUseAdaptation::Alias(alias_adaptation) = adaptation {
                                let (trait_name, original_name) =
                                    match &alias_adaptation.method_reference {
                                        TraitUseMethodReference::Identifier(method_name) => {
                                            (None, self.interner.intern(method_name.value))
                                        }
                                        TraitUseMethodReference::Absolute(method_ref) => (
                                            Some(self.resolve_identifier(&method_ref.trait_name)),
                                            self.interner.intern(method_ref.method_name.value),
                                        ),
                                    };

                                let alias_name = alias_adaptation
                                    .alias
                                    .as_ref()
                                    .map(|a| self.interner.intern(a.value))
                                    .unwrap_or(original_name);

                                let visibility = alias_adaptation
                                    .visibility
                                    .as_ref()
                                    .and_then(parse_visibility_modifier);

                                class_info.trait_method_aliases.push(TraitMethodAlias {
                                    trait_name,
                                    original_name,
                                    alias_name,
                                    visibility,
                                });
                            }
                        }
                    }
                }
                ClassLikeMember::EnumCase(_) => {
                    // Enum cases are handled differently
                }
            }
        }

        compute_template_readonly(class_info);
        crate::property_map::apply_property_map(class_info, self.interner);
    }

    fn apply_docblock_magic_properties(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        for tag_name in ["property", "property-read", "property-write"] {
            let Some(tags) = parsed.combined_tags.get(tag_name) else {
                continue;
            };

            let mut ordered_tags: Vec<_> = tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (_, content) in ordered_tags {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    self.push_docblock_issue(
                        class_info,
                        "Badly-formatted @property annotation".to_string(),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };
                let Some(var_name) = crate::docblock::extract_var_name_from_content(content) else {
                    self.push_docblock_issue(
                        class_info,
                        "Badly-formatted @property name".to_string(),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };

                let prop_name = self.interner.intern(var_name.trim_start_matches('$'));
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let resolved_type = self.expand_docblock_class_constant_wildcards(
                    resolved_type,
                    self_class,
                    parent_class,
                    Some(&class_info.constants),
                );

                if tag_name != "property-write" {
                    class_info
                        .pseudo_property_get_types
                        .insert(prop_name, resolved_type.clone());
                }

                if tag_name != "property-read" {
                    class_info
                        .pseudo_property_set_types
                        .insert(prop_name, resolved_type);
                }
            }
        }
    }

    fn apply_docblock_requirements(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        self.collect_required_classlikes_from_docblock_tags(
            class_info,
            parsed,
            &["psalm-require-extends", "require-extends"],
            self_class,
            parent_class,
            template_map,
            true,
        );
        self.collect_required_classlikes_from_docblock_tags(
            class_info,
            parsed,
            &["psalm-require-implements", "require-implements"],
            self_class,
            parent_class,
            template_map,
            false,
        );
    }

    fn collect_required_classlikes_from_docblock_tags(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        tag_keys: &[&str],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        is_extends_requirement: bool,
    ) {
        for tag_key in tag_keys {
            let Some(tags) = parsed.tags.get(*tag_key) else {
                continue;
            };

            let mut ordered_tags: Vec<_> = tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (_, content) in ordered_tags {
                let requirement = take_first_docblock_type_token(content.trim());
                if requirement.is_empty() {
                    self.push_docblock_issue(
                        class_info,
                        format!("{tag_key} annotation used without specifying class-like"),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                }

                let parsed_type = crate::docblock::parse_type_string(requirement, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                let Some(required_classlike) =
                    resolved_type.types.iter().find_map(|atomic| match atomic {
                        TAtomic::TNamedObject { name, .. } => Some(*name),
                        _ => None,
                    })
                else {
                    self.push_docblock_issue(
                        class_info,
                        format!("Badly-formatted {tag_key} annotation"),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };

                let target = if is_extends_requirement {
                    &mut class_info.required_extends
                } else {
                    &mut class_info.required_implements
                };

                if !target.contains(&required_classlike) {
                    target.push(required_classlike);
                }
            }
        }
    }

    fn apply_docblock_mixins(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(tags) = parsed.combined_tags.get("mixin") else {
            return;
        };

        if class_info.mixin_declaring_class.is_none() {
            class_info.mixin_declaring_class = Some(class_info.name);
        }

        let mut ordered_tags: Vec<_> = tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        for (_, content) in ordered_tags {
            let mixin = take_first_docblock_type_token(content.trim());
            if mixin.is_empty() {
                self.push_docblock_issue(
                    class_info,
                    "@mixin annotation used without specifying class".to_string(),
                    class_info.start_offset,
                    class_info.start_offset.saturating_add(1),
                );
                continue;
            }

            let parsed_type = crate::docblock::parse_type_string(mixin, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );

            for atomic in resolved_type.types {
                if !class_info.named_mixins.contains(&atomic) {
                    class_info.named_mixins.push(atomic);
                }
            }
        }
    }

    /// `@psalm-inheritors A|B`: the closed set of allowed subtypes (Psalm's
    /// ClassLikeNodeScanner `inheritors` handling — the first doc-line part,
    /// parsed as a union in class scope).
    fn apply_docblock_inheritors(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(tags) = parsed.tags.get("psalm-inheritors") else {
            return;
        };
        let Some(content) = tags.values().next() else {
            return;
        };

        let type_str = take_first_docblock_union_token(content.trim());
        if type_str.is_empty() {
            return;
        }

        let Ok(parsed_type) =
            crate::docblock::parse_type_string(&type_str, self.interner.parent_ref())
        else {
            return;
        };

        class_info.inheritors = self
            .resolve_docblock_union_type(parsed_type, self_class, parent_class, template_map)
            .types;
    }

    fn apply_docblock_magic_methods(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        // Psalm does not seal methods just because `@method` tags exist —
        // sealing comes from `@psalm-seal-methods` or, for user-defined
        // classes, the `sealAllMethods` config default (see
        // `class_has_sealed_methods`). `@psalm-method` entries are processed
        // last and OVERRIDE same-named `@method` entries (Psalm parses the
        // psalm- tags after the standard ones, overwriting by name).
        for (tag_name, tag_overrides) in [("method", false), ("psalm-method", true)] {
            let Some(tags) = parsed.tags.get(tag_name) else {
                continue;
            };

            let mut ordered_tags: Vec<_> = tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (_, content) in ordered_tags {
                let method_info = match self.parse_docblock_method_info(
                    class_info,
                    content,
                    self_class,
                    parent_class,
                    template_map,
                ) {
                    Ok(method_info) => method_info,
                    Err(message) => {
                        self.push_docblock_issue(
                            class_info,
                            message,
                            class_info.start_offset,
                            class_info.start_offset.saturating_add(1),
                        );
                        continue;
                    }
                };

                let target = if method_info.is_static {
                    &mut class_info.pseudo_static_methods
                } else {
                    &mut class_info.pseudo_methods
                };
                if tag_overrides {
                    target.insert(method_info.name, method_info);
                } else {
                    target.entry(method_info.name).or_insert(method_info);
                }
            }
        }
    }

    fn apply_docblock_template_extends(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        // Extends/implements/use type arguments only see the class's own
        // aliases (Psalm's ClassLikeNodeScanner per-class alias map).
        let previous_restrict = std::mem::replace(&mut self.restrict_aliases_to_active, true);
        self.apply_docblock_template_extends_inner(
            class_info,
            parsed,
            self_class,
            parent_class,
            template_map,
        );
        self.restrict_aliases_to_active = previous_restrict;
    }

    fn apply_docblock_template_extends_inner(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        for tag_name in ["extends", "implements", "use"] {
            let Some(tags) = parsed.combined_tags.get(tag_name) else {
                continue;
            };

            for content in tags.values() {
                // Psalm reports a malformed `@template-extends`/`@template-implements`/
                // `@template-use` type as an `InvalidDocblock` rather than silently
                // treating it as `mixed`.
                let parsed_type = match crate::docblock::parse_type_string(content, self.interner.parent_ref()) {
                    Ok(parsed_type) => parsed_type,
                    Err(_) => {
                        self.push_docblock_issue(
                            class_info,
                            format!(
                                "@{} annotation \"{}\" could not be parsed",
                                tag_name,
                                content.trim()
                            ),
                            class_info.start_offset,
                            class_info.start_offset.saturating_add(1),
                        );
                        continue;
                    }
                };
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                let mut inserted_object = false;
                let mut saw_non_object = false;
                let mut saw_wrong_relationship = false;
                for atomic in resolved_type.types {
                    match atomic {
                        TAtomic::TNamedObject {
                            name, type_params, ..
                        } => {
                            // Psalm reports an `InvalidDocblock` when the tag does
                            // not match the actual relationship, e.g. using
                            // `@template-extends` for an implemented interface or
                            // `@template-implements` for an extended class.
                            let relationship_ok = match tag_name {
                                "extends" => {
                                    if class_info.kind == ClassLikeKind::Interface {
                                        class_info.interfaces.contains(&name)
                                    } else {
                                        class_info.parent_class == Some(name)
                                    }
                                }
                                "implements" => class_info.interfaces.contains(&name),
                                // `@template-use` is handled when scanning the
                                // `use` statement itself.
                                _ => true,
                            };

                            if !relationship_ok {
                                saw_wrong_relationship = true;
                                continue;
                            }

                            if let Some(type_params) = type_params {
                                class_info
                                    .template_extended_offsets
                                    .insert(name, type_params);
                            }
                            inserted_object = true;
                        }
                        // A template param is a valid object target; only flag
                        // genuinely non-object targets such as `int` below.
                        TAtomic::TTemplateParam { .. } => {
                            inserted_object = true;
                        }
                        _ => {
                            saw_non_object = true;
                        }
                    }
                }

                if saw_wrong_relationship && !inserted_object {
                    self.push_docblock_issue(
                        class_info,
                        format!(
                            "@{} type \"{}\" does not match the class hierarchy",
                            tag_name,
                            content.trim()
                        ),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                }

                // `@template-extends int` and similar non-class targets are an
                // `InvalidDocblock` in Psalm.
                if saw_non_object && !inserted_object {
                    self.push_docblock_issue(
                        class_info,
                        format!(
                            "@{} type \"{}\" must be a class type",
                            tag_name,
                            content.trim()
                        ),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                }
            }
        }
    }

    fn parse_docblock_method_info(
        &mut self,
        class_info: &ClassLikeInfo,
        content: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Result<FunctionLikeInfo, String> {
        let mut signature = content.trim();
        if signature.is_empty() {
            return Err("No @method entry specified".to_string());
        }

        // Psalm's ClassLikeDocblockParser: `static` is the static MODIFIER
        // only when followed by a return type and then the method name
        // (`static self make()`); `static getStatic()` is an instance method
        // whose return type is `static`. Psalm's check is `!strpos(part, '(')`
        // — a paren at position 0 (`(string|int)[]`) still reads as a return
        // type, so only a paren later in the token marks the method name.
        let mut is_static = false;
        if let Some(rest) = signature.strip_prefix("static ") {
            let rest = rest.trim();
            let next_token_is_method_name = rest
                .split_whitespace()
                .next()
                .is_some_and(|token| token.find('(').is_some_and(|index| index > 0));
            if !next_token_is_method_name && rest.split_whitespace().count() >= 2 {
                is_static = true;
                signature = rest;
            }
        }

        let (open_paren, close_paren) = find_docblock_method_signature_bounds(signature)
            .ok_or_else(|| format!("{signature} is not a valid method"))?;

        let before_paren = signature[..open_paren].trim();
        let params_str = signature[open_paren + 1..close_paren].trim();
        if before_paren.is_empty() {
            return Err(format!("{signature} is not a valid method"));
        }

        let (before_return, method_name) = split_method_name(before_paren)
            .ok_or_else(|| format!("{signature} is not a valid method"))?;

        if !is_valid_docblock_method_name(method_name) {
            return Err(format!("{signature} is not a valid method"));
        }

        let mut return_type_str = if before_return.is_empty() {
            None
        } else {
            Some(before_return.to_string())
        };

        if return_type_str.is_none() {
            let after = signature[close_paren + 1..].trim_start();
            if let Some(return_fragment) = after.strip_prefix(':') {
                let type_fragment = take_first_docblock_type_token(return_fragment.trim_start());
                if !type_fragment.is_empty() {
                    return_type_str = Some(type_fragment.to_string());
                }
            }
        }

        let return_type = if let Some(return_type_str) = return_type_str {
            let parsed_type =
                crate::docblock::parse_type_string(return_type_str.trim(), self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
            Some(self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            ))
        } else {
            Some(TUnion::mixed())
        };

        let params =
            self.parse_docblock_method_params(params_str, self_class, parent_class, template_map)?;

        Ok(FunctionLikeInfo {
            name: self.interner.intern(method_name),
            declaring_class: Some(class_info.name),
            params,
            return_type,
            signature_return_type: None,
            is_pure: false,
            is_mutation_free: false,
            is_static,
            is_abstract: false,
            is_final: false,
            visibility: Visibility::Public,
            returns_by_ref: false,
            file_path: class_info.file_path,
            start_offset: class_info.start_offset,
            end_offset: class_info.end_offset,
            assertions: Vec::new(),
            if_true_assertions: Vec::new(),
            if_false_assertions: Vec::new(),
            template_types: Vec::new(),
            ..Default::default()
        })
    }

    fn parse_docblock_method_params(
        &mut self,
        params: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Result<Vec<ParamInfo>, String> {
        if params.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed = split_docblock_method_params(params)
            .into_iter()
            .enumerate()
            .map(|(idx, raw_param)| {
                let raw_param = raw_param.trim();
                if raw_param.is_empty() {
                    return Ok(None);
                }

                if raw_param.contains("& $") {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                let Some(param_name_raw) = extract_param_name_from_content(raw_param) else {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                };

                let param_name_str = format!("${param_name_raw}");
                let Some(param_name_offset) = raw_param.find(&param_name_str) else {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                };

                let mut before_name = raw_param[..param_name_offset].trim_end();
                let after_name = raw_param[param_name_offset + param_name_str.len()..].trim_start();

                let mut is_variadic = false;
                let mut by_ref = false;

                loop {
                    let trimmed = before_name.trim_end();

                    if let Some(stripped) = trimmed.strip_suffix("...") {
                        is_variadic = true;
                        before_name = stripped;
                        continue;
                    }

                    if let Some(stripped) = trimmed.strip_suffix('&') {
                        by_ref = true;
                        before_name = stripped;
                        continue;
                    }

                    break;
                }

                let type_source = before_name.trim();

                if type_source.contains('&')
                    || type_source.contains("...")
                    || after_name.contains("&$")
                    || after_name.contains("& $")
                {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                let type_union = if type_source.is_empty() {
                    None
                } else {
                    let parsed_type =
                        crate::docblock::parse_type_string(type_source, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    Some(self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    ))
                };

                let param_name = self.interner.intern(&param_name_str);
                let is_optional = after_name.contains('=');

                if by_ref && type_union.is_none() {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                Ok(Some(ParamInfo {
                    name: param_name,
                    param_type: Some(type_union.unwrap_or_else(TUnion::mixed)),
                    param_out_type: None,
                    signature_type: None,
                    has_docblock_type: true,
                    is_optional,
                    is_variadic,
                    by_ref,
                    is_promoted: false,
            expect_variable: false,
                    default_type: None,
                    description: None,
                    start_offset: idx as u32,
                    sinks: Vec::new(),
                    assert_untainted: false,
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(parsed.into_iter().flatten().collect())
    }

    fn collect_property(&mut self, class_info: &mut ClassLikeInfo, property: &Property<'_>) {
        let (visibility, is_static, mut is_readonly) =
            parse_property_modifiers(property.modifiers());
        let class_template_map = self.build_template_map_from_class_template_types(
            &class_info.template_types,
            GenericParent::ClassLike(class_info.name),
        );

        // Get native PHP type hint (signature_type)
        let signature_type = property
            .hint()
            .map(|h| self.resolve_type(h, Some(class_info.name), class_info.parent_class));

        // Get property start offset for docblock lookup
        let prop_span = property.span();

        // Get docblock type/flags if present
        let parsed_docblock = self
            .find_preceding_docblock(prop_span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        let property_attribute_lists = match property {
            Property::Plain(plain) => &plain.attribute_lists,
            Property::Hooked(hooked) => &hooked.attribute_lists,
        };
        let mut is_deprecated = self.has_attribute_named(property_attribute_lists, "Deprecated");
        let mut internal = Vec::new();

        if let Some(parsed) = parsed_docblock.as_ref() {
            self.validate_property_docblock_tags(class_info, parsed, prop_span.start.offset);
            is_deprecated |= self.is_docblock_deprecated(parsed);
            internal = self.get_docblock_internal_scopes(
                parsed,
                class_info.name,
                &mut class_info.docblock_issues,
            );
        }

        let mut docblock_type = None;
        // Psalm's CommentAnalyzer: a name-first `@var $prop type` is the
        // legacy misplaced-variable form — MissingDocblockType, and the
        // annotation is discarded.
        if let Some(parsed) = parsed_docblock.as_ref()
            && let Some(var_content) = parsed.get_var()
            && var_content.trim_start().starts_with('$')
            && !var_content.trim_start().starts_with("$this")
        {
            self.push_docblock_issue(
                class_info,
                "Missing docblock type".to_string(),
                prop_span.start.offset,
                prop_span.start.offset.saturating_add(1),
            );
        } else if let Some(parsed) = parsed_docblock.as_ref()
            && let Some(var_content) = parsed.get_var()
            && let Some(type_str) = crate::docblock::extract_type_string_from_content(var_content)
        {
            if self.is_valid_docblock_type_string(
                type_str,
                Some(class_info.name),
                class_info.parent_class,
                Some(&class_template_map),
                None,
                &[],
            ) {
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                docblock_type = Some(self.resolve_docblock_union_type(
                    parsed_type,
                    Some(class_info.name),
                    class_info.parent_class,
                    Some(&class_template_map),
                ));
            } else {
                self.push_docblock_issue(
                    class_info,
                    "Invalid docblock type".to_string(),
                    prop_span.start.offset,
                    prop_span.start.offset.saturating_add(1),
                );
            }
        }

        let is_readonly_native = is_readonly;
        let mut readonly_allow_private_mutation = false;
        if let Some(parsed) = parsed_docblock.as_ref() {
            if self.is_docblock_readonly(parsed) {
                is_readonly = true;
            }
            readonly_allow_private_mutation =
                self.is_docblock_readonly_allow_private_mutation(parsed);
        }

        // Match Psalm: `property_type` holds the docblock type only; the native hint stays
        // in `signature_type`. Effective reads use PropertyInfo::get_type().
        let property_type = docblock_type.clone();

        // Psalm's ClassLikeNodeScanner: `@psalm-suppress
        // PropertyNotSetInConstructor` on the property docblock marks the
        // property initialized for the whole hierarchy.
        let marked_initialized = parsed_docblock.as_ref().is_some_and(|parsed| {
            parsed.tags.get("psalm-suppress").is_some_and(|entries| {
                entries.values().any(|value| {
                    value
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .any(|token| token == "PropertyNotSetInConstructor")
                })
            })
        });

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
                        is_readonly_native,
                        readonly_allow_private_mutation,
                        is_deprecated,
                        internal.clone(),
                        false,
                        marked_initialized,
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
                    is_readonly_native,
                    readonly_allow_private_mutation,
                    is_deprecated,
                    internal,
                    true,
                    marked_initialized,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_property_item(
        &mut self,
        class_info: &mut ClassLikeInfo,
        item: &PropertyItem<'_>,
        property_type: Option<TUnion>,
        signature_type: Option<TUnion>,
        visibility: Visibility,
        is_static: bool,
        is_readonly: bool,
        is_readonly_native: bool,
        readonly_allow_private_mutation: bool,
        is_deprecated: bool,
        internal: Vec<StrId>,
        is_hooked: bool,
        marked_initialized: bool,
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
            is_readonly_native,
            readonly_allow_private_mutation,
            has_default,
            is_promoted: false,
            is_hooked,
            is_deprecated,
            location_free: false,
            marked_initialized,
            internal,
            description: None,
            start_offset: span.start.offset,
        };

        if class_info.properties.contains_key(&prop_name) {
            class_info
                .duplicate_property_issues
                .push(DuplicatePropertyIssue {
                    property_name: prop_name,
                    start_offset: span.start.offset,
                    end_offset: span.end.offset,
                });
        }

        class_info
            .properties
            .insert(prop_name, std::sync::Arc::new(prop_info));
    }

    fn collect_params(
        &mut self,
        params: &TokenSeparatedSequence<'_, FunctionLikeParameter<'_>>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        template_map: Option<&TemplateMap>,
    ) -> Vec<ParamInfo> {
        // Psalm's FunctionLikeNodeScanner: internal-stub params named
        // `haystack` expect a non-literal value (InvalidLiteralArgument).
        let in_internal_stub = self
            .interner
            .lookup(self.file_path)
            .as_ref()
            .ends_with(".phpstub");

        params
            .iter()
            .map(|param| {
                let name = self.interner.intern(param.variable.name);
                // Native PHP type hint is the signature_type
                let mut signature_type = param
                    .hint
                    .as_ref()
                    .map(|h| self.resolve_type(h, self_class, parent_class));
                let default_type = param.default_value.as_ref().and_then(|default_value| {
                    simple_type_inferer::infer_param_default_type(
                        &default_value.value,
                        self.interner.parent_ref(),
                        self_class,
                        class_constants,
                    )
                });

                // Legacy PHP signatures like `A $a = null` are nullable at runtime.
                if default_type.as_ref().is_some_and(TUnion::is_null)
                    && let Some(signature_type) = signature_type.as_mut()
                        && !signature_type.is_nullable() {
                            signature_type.add_type(TAtomic::TNull);
                        }

                // Method-level `@param` docblocks are resolved during analysis, but a
                // docblock attached directly to the parameter (e.g. a promoted property
                // with `/** @var T */`) constrains the parameter type here. `param_type`
                // holds the docblock type only (Psalm's model); the native hint stays in
                // `signature_type`. Effective reads use get_type().
                let mut param_type = None;
                let mut has_docblock_type = false;
                if let Some(parsed) = self
                    .find_preceding_docblock(param.span().start.offset)
                    .map(|docblock| crate::docblock::parse(docblock, 0))
                    && let Some(var_content) = parsed.get_var()
                    && let Some(type_str) =
                        crate::docblock::extract_type_string_from_content(var_content)
                    && self.is_valid_docblock_type_string(
                        type_str,
                        self_class,
                        parent_class,
                        template_map,
                        None,
                        &[],
                    )
                {
                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    param_type = Some(self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    ));
                    has_docblock_type = true;
                }

                ParamInfo {
                    name,
                    param_type,
                    param_out_type: None,
                    signature_type,
                    has_docblock_type,
                    is_optional: param.default_value.is_some(),
                    is_variadic: param.ellipsis.is_some(),
                    by_ref: param.ampersand.is_some(),
                    is_promoted: param.is_promoted_property(),
                    expect_variable: in_internal_stub
                        && param.variable.name.trim_start_matches('$') == "haystack",
                    default_type,
                    description: None,
                    start_offset: param.span().start.offset,
                    sinks: Vec::new(),
                    assert_untainted: false,
                }
            })
            .collect()
    }

    /// Psalm's `FunctionLikeNodeScanner::inferPropertyTypeFromConstructor`: a
    /// constructor consisting solely of `$this->prop = $param;` statements
    /// types each untyped property from its (typed) parameter — variadic
    /// params as `array<int, T>`. Any other statement shape, an unknown
    /// property, or an unknown parameter aborts the inference entirely.
    fn infer_property_types_from_constructor(
        &mut self,
        class_info: &mut ClassLikeInfo,
        stmts: &[Statement<'_>],
        params: &[ParamInfo],
    ) {
        use mago_syntax::ast::ast::access::Access;
        use mago_syntax::ast::ast::assignment::AssignmentOperator;
        use mago_syntax::ast::ast::variable::Variable;

        if stmts.is_empty() {
            return;
        }

        let mut assigned_properties: Vec<(StrId, TUnion)> = Vec::new();

        for stmt in stmts {
            let matched = (|| {
                let Statement::Expression(expr_stmt) = stmt else {
                    return None;
                };
                let Expression::Assignment(assignment) = expr_stmt.expression.unparenthesized()
                else {
                    return None;
                };
                if !matches!(assignment.operator, AssignmentOperator::Assign(_)) {
                    return None;
                }
                let Expression::Access(Access::Property(prop_access)) =
                    assignment.lhs.unparenthesized()
                else {
                    return None;
                };
                let Expression::Variable(Variable::Direct(object_var)) =
                    prop_access.object.unparenthesized()
                else {
                    return None;
                };
                if object_var.name.trim_start_matches('$') != "this" {
                    return None;
                }
                let mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector::Identifier(
                    property_identifier,
                ) = &prop_access.property
                else {
                    return None;
                };
                let Expression::Variable(Variable::Direct(value_var)) =
                    assignment.rhs.unparenthesized()
                else {
                    return None;
                };
                Some((
                    property_identifier.value,
                    value_var.name.trim_start_matches('$'),
                ))
            })();

            let Some((property_name, param_name)) = matched else {
                return;
            };

            let property_id = self.interner.intern(property_name);
            let Some(property_info) = class_info.properties.get(&property_id) else {
                return;
            };
            let Some(param_info) = params.iter().find(|param| {
                self.interner.lookup(param.name).as_ref().trim_start_matches('$') == param_name
            }) else {
                return;
            };

            // A typed property or an untyped parameter skips this statement
            // without aborting the rest (Psalm `continue`s).
            if property_info.has_type() {
                continue;
            }
            let Some(param_type) = param_info.get_type() else {
                continue;
            };

            let property_type = if param_info.is_variadic {
                TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::int()),
                    value_type: Box::new(param_type.clone()),
                })
            } else {
                param_type.clone()
            };

            assigned_properties.push((property_id, property_type));
        }

        for (property_id, property_type) in assigned_properties {
            if let Some(property_info) = class_info.properties.get_mut(&property_id) {
                std::sync::Arc::make_mut(property_info).property_type = Some(property_type);
            }
        }
    }

    fn collect_promoted_properties(
        &mut self,
        class_info: &mut ClassLikeInfo,
        ast_params: &TokenSeparatedSequence<'_, FunctionLikeParameter<'_>>,
        params: &[ParamInfo],
    ) {
        for (ast_param, param_info) in ast_params.iter().zip(params.iter()) {
            if !ast_param.is_promoted_property() {
                continue;
            }

            let (visibility, is_static, is_readonly) =
                parse_property_modifiers(&ast_param.modifiers);
            // A docblock directly above the promoted param can mark the
            // property @readonly / @psalm-readonly[-allow-private-mutation].
            let param_docblock = self
                .find_preceding_docblock(ast_param.span().start.offset)
                .map(|docblock| crate::docblock::parse(docblock, 0));
            let docblock_readonly = param_docblock
                .as_ref()
                .is_some_and(|parsed| self.is_docblock_readonly(parsed));
            let readonly_allow_private_mutation = param_docblock
                .as_ref()
                .is_some_and(|parsed| self.is_docblock_readonly_allow_private_mutation(parsed));
            let prop_name_str = ast_param
                .variable
                .name
                .strip_prefix('$')
                .unwrap_or(ast_param.variable.name);
            let prop_name = self.interner.intern(prop_name_str);

            if class_info.properties.contains_key(&prop_name) {
                continue;
            }

            let span = ast_param.span();
            let prop_info = PropertyInfo {
                name: prop_name,
                declaring_class: class_info.name,
                property_type: param_info
                    .param_type
                    .clone()
                    .or_else(|| param_info.signature_type.clone()),
                signature_type: param_info.signature_type.clone(),
                visibility,
                is_static,
                is_readonly: is_readonly || docblock_readonly,
                is_readonly_native: is_readonly,
                readonly_allow_private_mutation,
                has_default: ast_param.default_value.is_some(),
                is_promoted: true,
                is_hooked: false,
                is_deprecated: false,
                location_free: false,
                marked_initialized: false,
                internal: Vec::new(),
                description: None,
                start_offset: span.start.offset,
            };

            class_info
            .properties
            .insert(prop_name, std::sync::Arc::new(prop_info));
        }
    }

    fn apply_docblock_param_out_types(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        params: &mut [ParamInfo],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) {
        let Some(param_out_tags) = parsed.combined_tags.get("param-out") else {
            return;
        };

        let mut ordered_tags: Vec<_> = param_out_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        let mut parsed_tags: Vec<(Option<String>, TUnion)> = Vec::with_capacity(ordered_tags.len());
        for (_, content) in ordered_tags {
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                continue;
            };

            if !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                class_constants,
                &[],
            ) {
                continue;
            }

            // A conditional `@param-out` keeps its TConditional shape (like
            // `@return` conditionals) so the call site can pick a branch —
            // e.g. preg_match's TFlags-keyed $matches type.
            let parsed_type = if let Some(conditional) = self
                .parse_docblock_conditional_return_type(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                ) {
                docblock_conditional_union(conditional)
            } else {
                self.try_resolve_docblock_utility_type(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                )
                .unwrap_or_else(|| {
                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    )
                })
            };
            let param_name = extract_param_name_from_content(content).map(str::to_string);
            parsed_tags.push((param_name, parsed_type));
        }

        if parsed_tags.is_empty() {
            return;
        }

        let use_positional_fallback =
            parsed_tags.len() == params.len() && parsed_tags.iter().all(|(name, _)| name.is_none());

        for (idx, param) in params.iter_mut().enumerate() {
            let param_name = self.interner.lookup(param.name);
            let normalized_name = param_name
                .as_ref()
                .strip_prefix('$')
                .unwrap_or(param_name.as_ref());

            let docblock_type = parsed_tags
                .iter()
                .find_map(|(name, ty)| {
                    if name
                        .as_deref()
                        .map(|name| name.trim_start_matches('$'))
                        == Some(normalized_name)
                    {
                        Some(ty.clone())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if use_positional_fallback {
                        parsed_tags.get(idx).map(|(_, ty)| ty.clone())
                    } else {
                        None
                    }
                });

            if let Some(docblock_type) = docblock_type {
                param.param_out_type = Some(docblock_type);
            }
        }
    }

    /// Returns the named `@param` tags that matched no signature parameter
    /// (`(name, tag_offset)`), for InvalidDocblockParamName reporting.
    fn apply_docblock_param_types(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        params: &mut [ParamInfo],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> (Vec<(String, u32)>, bool) {
        let Some(param_tags) = parsed.combined_tags.get("param") else {
            return (Vec::new(), false);
        };

        let mut ordered_tags: Vec<_> = param_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        let mut parsed_tags: Vec<(Option<String>, TUnion, bool)> =
            Vec::with_capacity(ordered_tags.len());
        let mut tag_offsets: Vec<u32> = Vec::with_capacity(ordered_tags.len());
        for (offset, content) in ordered_tags {
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                continue;
            };

            // Leftover decoration / var-only tags carry no type to apply
            // (the docblock-issue pass reports MissingDocblockType).
            if is_missing_docblock_type(type_str) {
                continue;
            }

            if !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                class_constants,
                &[],
            ) {
                continue;
            }

            let parsed_type = self
                .try_resolve_docblock_utility_type(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                )
                .unwrap_or_else(|| {
                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    )
                });
            let param_name = extract_param_name_from_content(content).map(str::to_string);
            let docblock_variadic = docblock_param_is_variadic(content);
            parsed_tags.push((param_name, parsed_type, docblock_variadic));
            tag_offsets.push(*offset as u32);
        }

        if parsed_tags.is_empty() {
            return (Vec::new(), false);
        }

        let use_positional_fallback = parsed_tags.len() == params.len()
            && parsed_tags.iter().all(|(name, _, _)| name.is_none());

        for (idx, param) in params.iter_mut().enumerate() {
            let param_name = self.interner.lookup(param.name);
            let normalized_name = param_name
                .as_ref()
                .strip_prefix('$')
                .unwrap_or(param_name.as_ref());

            let docblock_type = parsed_tags
                .iter()
                .find_map(|(name, ty, docblock_variadic)| {
                    if name
                        .as_deref()
                        .map(|name| name.trim_start_matches('$'))
                        == Some(normalized_name)
                    {
                        Some((ty.clone(), *docblock_variadic))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if use_positional_fallback {
                        parsed_tags
                            .get(idx)
                            .map(|(_, ty, docblock_variadic)| (ty.clone(), *docblock_variadic))
                    } else {
                        None
                    }
                });

            if let Some((mut docblock_type, docblock_variadic)) = docblock_type {
                // Psalm's FunctionLikeDocblockScanner: a non-variadic `@param`
                // array docblock on a variadic signature param describes the
                // collected arguments — the per-argument param type is the
                // array's value type.
                if param.is_variadic
                    && !docblock_variadic
                    && let Some(element_type) = docblock_array_value_type(&docblock_type)
                {
                    docblock_type = element_type;
                }

                // Psalm's FunctionLikeDocblockScanner: a docblock atomic whose
                // key matches a signature atomic is just restating the
                // typehint and keeps runtime provenance; when every atomic
                // matches, the whole union loses from_docblock.
                if let Some(signature_type) = &param.signature_type {
                    let signature_keys: Vec<String> = signature_type
                        .types
                        .iter()
                        .map(|atomic| loose_atomic_key(self.interner.parent_ref(), atomic))
                        .collect();
                    let mut unmatched_bits: u32 = 0;
                    let mut any_unmatched = false;
                    for (index, atomic) in docblock_type.types.iter().enumerate() {
                        if !signature_keys.contains(&loose_atomic_key(self.interner.parent_ref(), atomic)) {
                            any_unmatched = true;
                            if index < 32 {
                                unmatched_bits |= 1 << index;
                            }
                        }
                    }
                    if !any_unmatched {
                        docblock_type.from_docblock = false;
                    }
                    if docblock_type.types.len() <= 32 && !docblock_type.types.is_empty() {
                        docblock_type.docblock_bits_len = docblock_type.types.len() as u8;
                        docblock_type.from_docblock_bits = if docblock_type.from_docblock {
                            unmatched_bits
                        } else {
                            0
                        };
                    }
                }

                // Psalm FunctionLikeDocblockScanner: a nullable param keeps
                // its null when the docblock omits it (`?string $s` +
                // `@param string $s` ⇒ `string|null`). Psalm's
                // `$storage_param->is_nullable` covers both a nullable
                // signature and an implicit `= null` default.
                let param_is_nullable = param
                    .signature_type
                    .as_ref()
                    .is_some_and(|signature_type| signature_type.is_nullable())
                    || param
                        .default_type
                        .as_ref()
                        .is_some_and(|default_type| default_type.is_nullable() || default_type.is_null());
                if param_is_nullable && !docblock_type.is_nullable() {
                    docblock_type.add_type(TAtomic::TNull);
                }

                param.param_type = Some(docblock_type);
                param.has_docblock_type = true;
            }
        }

        // Named tags that matched no signature parameter (Psalm's
        // InvalidDocblockParamName) — only reported when some signature param
        // is undertyped (untyped or array-typed without a docblock), i.e. the
        // docblock was presumably meant for it (Psalm's
        // has_undertyped_native_parameters gate; otherwise the tag is merely
        // an UnusedDocblockParam under find_unused_code).
        let has_undertyped_native_parameters = params.iter().any(|param| {
            !param.has_docblock_type
                && param.get_type().is_none_or(|param_type| {
                    param_type.types.iter().any(|atomic| {
                        matches!(
                            atomic,
                            TAtomic::TArray { .. }
                                | TAtomic::TNonEmptyArray { .. }
                                | TAtomic::TList { .. }
                                | TAtomic::TNonEmptyList { .. }
                                | TAtomic::TKeyedArray { .. }
                        )
                    })
                })
        });
        let mut unmatched: Vec<(String, u32)> = Vec::new();
        for (tag_index, (tag_name, _, _)) in parsed_tags.iter().enumerate() {
            let Some(tag_name) = tag_name.as_deref() else {
                continue;
            };
            let normalized_tag = tag_name.trim_start_matches('$');
            let matches_some_param = params.iter().any(|param| {
                let param_name = self.interner.lookup(param.name);
                param_name
                    .as_ref()
                    .strip_prefix('$')
                    .unwrap_or(param_name.as_ref())
                    == normalized_tag
            });
            if !matches_some_param {
                unmatched.push((
                    normalized_tag.to_string(),
                    tag_offsets.get(tag_index).copied().unwrap_or_default(),
                ));
            }
        }
        (unmatched, has_undertyped_native_parameters)
    }

    fn try_resolve_template_key_of_type(
        &self,
        type_str: &str,
        template_map: Option<&TemplateMap>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();
        let inner = trimmed.strip_prefix("key-of<")?.strip_suffix('>')?.trim();
        let template_binding = template_map.and_then(|map| map.get(inner))?;

        // Keep `key-of<T>` deferred (Psalm's TTemplateKeyOf) so a concrete key cannot
        // satisfy it before the template is bound.
        Some(TUnion::new(TAtomic::TTemplateKeyOf {
            param_name: template_binding.name,
            defining_entity: template_binding.defining_entity,
            as_type: Box::new(template_binding.as_type.clone()),
        }))
    }

    fn try_resolve_template_value_of_type(
        &self,
        type_str: &str,
        template_map: Option<&TemplateMap>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();
        let inner = trimmed.strip_prefix("value-of<")?.strip_suffix('>')?.trim();
        let template_binding = template_map.and_then(|map| map.get(inner))?;

        Some(TUnion::new(TAtomic::TTemplateValueOf {
            param_name: template_binding.name,
            defining_entity: template_binding.defining_entity,
            as_type: Box::new(template_binding.as_type.clone()),
        }))
    }

    fn try_resolve_docblock_utility_type(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();

        let (utility_name, inner) = if let Some(inner) = trimmed
            .strip_prefix("key-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("key-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("value-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("value-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("properties-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("public-properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("public-properties-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("protected-properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("protected-properties-of", inner.trim())
        } else {
            let inner = trimmed
            .strip_prefix("private-properties-of<")
            .and_then(|s| s.strip_suffix('>'))?;
            ("private-properties-of", inner.trim())
        };

        if inner.is_empty() {
            return Some(match utility_name {
                "key-of" => TUnion::array_key(),
                "value-of" => TUnion::mixed(),
                "properties-of"
                | "public-properties-of"
                | "protected-properties-of"
                | "private-properties-of" => TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::string()),
                    value_type: Box::new(TUnion::mixed()),
                }),
                _ => unreachable!(),
            });
        }

        if utility_name == "key-of"
            && let Some(template_key_of) =
                self.try_resolve_template_key_of_type(trimmed, template_map)
        {
            return Some(template_key_of);
        }

        if utility_name == "value-of"
            && let Some(template_value_of) =
                self.try_resolve_template_value_of_type(trimmed, template_map)
        {
            return Some(template_value_of);
        }

        // `key-of<static::CONST>` never resolves in a declaration (Psalm's
        // TypeExpander replaces only `self` and throws
        // UnresolvableConstantException for the rest); keep it (and a
        // reference to a missing constant) as a deferred sentinel the
        // analyzer reports as UnresolvableConstant.
        if matches!(utility_name, "key-of" | "value-of")
            && let Some((class_part, constant_part)) = inner.split_once("::")
        {
            let class_part = class_part.trim();
            let constant_part = constant_part.trim();
            let static_class = class_part.eq_ignore_ascii_case("static")
                || class_part.eq_ignore_ascii_case("$this");
            let missing_self_constant = !static_class
                && (class_part.eq_ignore_ascii_case("self")
                    || self_class.is_some_and(|self_id| {
                        self.interner.lookup(self_id).as_ref() == class_part
                    }))
                && class_constants.is_some_and(|constants| {
                    !constant_part.contains('*')
                        && !constants.keys().any(|constant_name| {
                            self.interner.lookup(*constant_name).as_ref() == constant_part
                        })
                });
            if static_class || missing_self_constant {
                return Some(TUnion::new(TAtomic::named_object(self.interner.intern(
                    &format!("{}<{}>", utility_name, inner),
                ))));
            }
        }

        let parsed_inner = crate::docblock::parse_type_string(inner, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
        let resolved_inner =
            self.resolve_docblock_union_type(parsed_inner, self_class, parent_class, template_map);
        let expanded_inner = self.expand_docblock_class_constant_wildcards(
            resolved_inner,
            self_class,
            parent_class,
            class_constants,
        );

        Some(match utility_name {
            "key-of" => resolve_key_of_template_union(&expanded_inner),
            "value-of" => self
                .resolve_enum_value_of(&expanded_inner)
                .unwrap_or_else(|| resolve_value_of_template_union(&expanded_inner)),
            "properties-of" => {
                self.properties_of_or_deferred(&expanded_inner, PropertiesOfVisibility::All)
            }
            "public-properties-of" => {
                self.properties_of_or_deferred(&expanded_inner, PropertiesOfVisibility::Public)
            }
            "protected-properties-of" => {
                self.properties_of_or_deferred(&expanded_inner, PropertiesOfVisibility::Protected)
            }
            "private-properties-of" => {
                self.properties_of_or_deferred(&expanded_inner, PropertiesOfVisibility::Private)
            }
            _ => unreachable!(),
        })
    }

    /// Resolves `value-of<E>` for a (union of) backed enum(s) to the union of the
    /// enum cases' backing values. Mirrors Psalm's `TypeExpander` expanding
    /// `value-of<EnumClass>` to the case-value union. Returns `None` when any
    /// member isn't a scanned backed enum, so the caller falls back to the
    /// generic array/template `value-of` resolution.
    fn resolve_enum_value_of(&self, union: &TUnion) -> Option<TUnion> {
        let value_property = StrId::VALUE;
        let mut value_types: Vec<TAtomic> = Vec::new();

        for atomic in &union.types {
            // A single enum case (`value-of<StringEnum::FOO>`) resolves to
            // that case's backed value.
            if let TAtomic::TEnumCase {
                enum_name,
                case_name,
            } = atomic
            {
                let case_value = self
                    .declarations
                    .classes
                    .iter()
                    .find(|class_info| class_info.name == *enum_name)
                    .and_then(|class_info| class_info.constants.get(case_name))
                    .and_then(|const_info| const_info.enum_case_value.clone())?;
                for value_atomic in case_value.types {
                    if !value_types.contains(&value_atomic) {
                        value_types.push(value_atomic);
                    }
                }
                continue;
            }

            let enum_name = match atomic {
                TAtomic::TNamedObject { name, .. } => *name,
                TAtomic::TEnum { name } => *name,
                _ => return None,
            };

            let class_info = self
                .declarations
                .classes
                .iter()
                .find(|class_info| class_info.name == enum_name)?;

            if class_info.kind != ClassLikeKind::Enum {
                return None;
            }

            // A unit enum has no backed values: value-of<UnitEnum> is empty
            // (Psalm flags such docblocks against the native signature).
            let Some(value_type) = class_info
                .properties
                .get(&value_property)
                .and_then(|property| property.property_type.as_ref())
            else {
                continue;
            };

            for value_atomic in &value_type.types {
                if !value_types.contains(value_atomic) {
                    value_types.push(value_atomic.clone());
                }
            }
        }

        if value_types.is_empty() {
            Some(TUnion::nothing())
        } else {
            Some(TUnion::from_types(value_types))
        }
    }

    /// Build `properties-of<…>` for a concrete class, or keep it deferred (Psalm's
    /// `TTemplatePropertiesOf`) when the argument is still a template parameter so it can
    /// be resolved to the bound class at the call site.
    fn properties_of_or_deferred(
        &self,
        union: &TUnion,
        visibility_filter: PropertiesOfVisibility,
    ) -> TUnion {
        if let Some(TAtomic::TTemplateParam {
            name,
            defining_entity,
            ..
        }) = union.get_single()
        {
            return TUnion::new(TAtomic::TTemplatePropertiesOf {
                param_name: *name,
                defining_entity: *defining_entity,
                visibility_filter,
            });
        }

        let visibility = match visibility_filter {
            PropertiesOfVisibility::All => None,
            PropertiesOfVisibility::Public => Some(Visibility::Public),
            PropertiesOfVisibility::Protected => Some(Visibility::Protected),
            PropertiesOfVisibility::Private => Some(Visibility::Private),
        };
        let resolved = self.resolve_properties_of_union(union, visibility);
        if !resolved.is_nothing() {
            return resolved;
        }

        // Nothing resolved here: the class may have no matching instance
        // properties (Psalm leaves the TPropertiesOf unexpanded, and uses
        // against it fail) or be declared in a file this scan hasn't seen
        // (the analyzer's TypeExpander resolves it with the full codebase).
        // Defer rather than degrading to array<string, mixed>.
        let deferred: Vec<TAtomic> = union
            .types
            .iter()
            .filter_map(|atomic| match atomic {
                TAtomic::TNamedObject { name, .. } => Some(TAtomic::TPropertiesOf {
                    classlike_name: *name,
                    visibility_filter,
                }),
                _ => None,
            })
            .collect();
        if deferred.is_empty() {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::string()),
                value_type: Box::new(TUnion::mixed()),
            })
        } else {
            TUnion::from_types(deferred)
        }
    }

    fn resolve_properties_of_union(
        &self,
        union: &TUnion,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        let mut resolved_union = TUnion::nothing();

        for atomic in &union.types {
            let resolved_atomic = self.resolve_properties_of_atomic(atomic, visibility_filter);
            resolved_union = if resolved_union.is_nothing() {
                resolved_atomic
            } else {
                combine_union_types(&resolved_union, &resolved_atomic, false)
            };
        }

        resolved_union
    }

    fn resolve_properties_of_atomic(
        &self,
        atomic: &TAtomic,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                self.resolve_properties_of_named_object(*name, visibility_filter)
            }
            TAtomic::TObjectIntersection { types } => {
                for intersection_atomic in types {
                    if let TAtomic::TNamedObject { name, .. } = intersection_atomic {
                        let resolved =
                            self.resolve_properties_of_named_object(*name, visibility_filter);
                        if !resolved.is_nothing() {
                            return resolved;
                        }
                    }
                }

                TUnion::nothing()
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                self.resolve_properties_of_union(as_type, visibility_filter)
            }
            _ => TUnion::nothing(),
        }
    }

    fn resolve_properties_of_named_object(
        &self,
        class_name: StrId,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        let mut current_class_name = Some(class_name);
        let mut all_sealed = true;
        let mut properties = FxHashMap::default();

        while let Some(current_name) = current_class_name {
            let Some(class_info) = self
                .declarations
                .classes
                .iter()
                .find(|class_info| class_info.name == current_name)
            else {
                break;
            };

            if !class_info.is_final {
                all_sealed = false;
            }

            for property in class_info.properties.values() {
                let Some(property_type) = property.get_type() else {
                    continue;
                };

                if let Some(required_visibility) = visibility_filter
                    && property.visibility != required_visibility
                {
                    continue;
                }

                if property.is_static {
                    continue;
                }

                let property_name = self.interner.lookup(property.name).to_string();
                let property_key = pzoom_code_info::t_atomic::ArrayKey::String(property_name);

                if properties.contains_key(&property_key) {
                    continue;
                }

                properties.insert(property_key, property_type.clone());
            }

            current_class_name = class_info.parent_class;
        }

        if properties.is_empty() {
            return TUnion::nothing();
        }

        let (sealed, fallback_key_type, fallback_value_type) = if all_sealed {
            (true, None, None)
        } else {
            (
                false,
                Some(Box::new(TUnion::string())),
                Some(Box::new(TUnion::mixed())),
            )
        };

        TUnion::new(TAtomic::TKeyedArray {
            properties: std::sync::Arc::new(properties),
            is_list: false,
            sealed,
            fallback_key_type,
            fallback_value_type,
        })
    }

    fn get_docblock_return_type(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        param_names: &[StrId],
    ) -> Option<(TUnion, Option<ConditionalReturnType>)> {
        let type_str = parsed
            .get_return()
            .and_then(crate::docblock::extract_type_string_from_content)?;

        // Psalm maps `@return $this` to `static` (FunctionLikeDocblockParser).
        let type_str = if type_str == "$this" { "static" } else { type_str };

        if !self.is_valid_docblock_type_string(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
            param_names,
        ) {
            return None;
        }

        let mut resolved_type = if let Some(utility_type) = self.try_resolve_docblock_utility_type(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            utility_type
        } else {
            let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );

            self.expand_docblock_class_constant_wildcards(
                resolved_type,
                self_class,
                parent_class,
                class_constants,
            )
        };

        resolved_type.from_docblock = true;
        resolved_type.sync_docblock_bits_from_union_flag();

        let conditional_return_type = self.parse_docblock_conditional_return_type(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        Some((resolved_type, conditional_return_type))
    }

    fn get_docblock_if_this_is_type(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        for key in ["psalm-if-this-is", "phpstan-if-this-is", "if-this-is"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                return Some(self.expand_docblock_class_constant_wildcards(
                    resolved_type,
                    self_class,
                    parent_class,
                    class_constants,
                ));
            }
        }

        None
    }

    /// Parse `@psalm-taint-escape (<conditional>)` tags into conditional
    /// types. Must run while the conditional-subject scope is alive so a
    /// `$param is …` subject resolves (same window as the `@return` parse).
    pub(crate) fn parse_conditional_taint_escapes(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Vec<ConditionalReturnType> {
        let Some(tags) = parsed.tags.get("psalm-taint-escape") else {
            return Vec::new();
        };

        // The docblock parser may have glued a following free-text line onto
        // the tag content (continuation merging) — keep only the balanced
        // parenthesised conditional itself.
        let raw_conditionals: Vec<String> = tags
            .values()
            .map(|content| content.trim())
            .filter(|content| content.starts_with('('))
            .map(|content| {
                let mut depth = 0usize;
                for (index, ch) in content.char_indices() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                return content[..=index].to_string();
                            }
                        }
                        _ => {}
                    }
                }
                content.to_string()
            })
            .collect();

        raw_conditionals
            .into_iter()
            .filter_map(|raw_conditional| {
                self.parse_docblock_conditional_return_type(
                    &raw_conditional,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                )
            })
            .collect()
    }

    fn parse_docblock_conditional_return_type(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<ConditionalReturnType> {
        let conditional_parts = crate::docblock::extract_conditional_type_parts(type_str)?;

        let if_true_type = self.parse_docblock_conditional_branch(
            &conditional_parts.if_true,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );
        let if_false_type = self.parse_docblock_conditional_branch(
            &conditional_parts.if_false,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        let (param_name, defining_entity, as_type, conditional_type) = self
            .parse_docblock_conditional_condition(
                &conditional_parts.condition,
                self_class,
                parent_class,
                template_map,
                class_constants,
            )?;

        Some(ConditionalReturnType {
            param_name,
            defining_entity,
            as_type,
            conditional_type,
            if_true_type,
            if_false_type,
        })
    }

    fn parse_docblock_conditional_branch(
        &mut self,
        branch_type: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> TUnion {
        // Mirror Psalm's `TypeParser::getTypeFromTree`: a conditional's if/else child
        // can itself be a `ConditionalTree`, which `getTypeFromTree` resolves
        // recursively into a nested `TConditional`. Reproduce that recursion here so a
        // deeply-nested conditional return type (e.g. `glob`'s `P is '' ? (F is ... ) :
        // (F is ...)`) keeps its structure instead of being flattened into the union of
        // both branches.
        if let Some(nested_conditional) = self.parse_docblock_conditional_return_type(
            branch_type,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            return docblock_conditional_union(nested_conditional);
        }

        if let Some(utility_type) = self.try_resolve_docblock_utility_type(
            branch_type,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            return utility_type;
        }

        let parsed_branch_type = crate::docblock::parse_type_string(branch_type, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
        let resolved_branch_type = self.resolve_docblock_union_type(
            parsed_branch_type,
            self_class,
            parent_class,
            template_map,
        );

        self.expand_docblock_class_constant_wildcards(
            resolved_branch_type,
            self_class,
            parent_class,
            class_constants,
        )
    }

    /// Parse a conditional's `<subject> is <type>` head into Psalm's
    /// TConditional fields: `(param_name, defining_entity, as_type,
    /// conditional_type)`. The subject is always a template — declared ones
    /// resolve through the template map; `$param`, `func_num_args()` and
    /// PHP-version tokens register synthetic templates on the function-like
    /// (Psalm's TGeneratedFromParam / TFunctionArgCount / TPhpMajorVersion
    /// model), which call sites then bind like any other template.
    fn parse_docblock_conditional_condition(
        &mut self,
        condition: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<(StrId, GenericParent, TUnion, TUnion)> {
        let normalized = condition.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized = normalized.trim();

        let entity = self.conditional_subject_scope.entity?;

        if let Some(rest) = normalized.strip_prefix("func_num_args() is ") {
            let count = rest.trim().parse::<i64>().ok()?;
            let name = self.interner.intern("TFunctionArgCount");
            self.conditional_subject_scope.subject_names.push(name);
            self.register_generated_conditional_template(name, entity, TUnion::int());
            return Some((
                name,
                entity,
                TUnion::int(),
                TUnion::new(TAtomic::TLiteralInt { value: count }),
            ));
        }

        let (lhs, asserted_type_str) = normalized.split_once(" is ")?;
        let lhs = lhs.trim();
        if lhs.is_empty() {
            return None;
        }

        let asserted_type = self.parse_docblock_conditional_branch(
            asserted_type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        // `$param is X`: the param becomes a generated template bound from
        // the argument at the call site (Psalm rewrites the param's type to
        // `TGeneratedFromParamN`; pzoom keeps the param type as written and
        // registers the template under the `$name` itself).
        if lhs.starts_with('$') {
            let param_id = self.interner.intern(lhs);
            let as_type = self
                .conditional_subject_scope
                .params
                .iter()
                .find(|(name, _)| *name == param_id)
                .and_then(|(_, declared)| declared.clone())
                .unwrap_or_else(TUnion::mixed);
            self.register_generated_conditional_template(param_id, entity, as_type.clone());
            return Some((param_id, entity, as_type, asserted_type));
        }

        // Psalm's FunctionLikeDocblockScanner turns PHP_MAJOR_VERSION /
        // PHP_VERSION_ID tokens into synthetic function templates
        // (TPhpMajorVersion / TPhpVersionId) bound to the analysis PHP
        // version at every call site.
        if lhs == "PHP_MAJOR_VERSION" || lhs == "PHP_VERSION_ID" {
            let name = self.interner.intern(lhs);
            self.register_generated_conditional_template(name, entity, TUnion::int());
            return Some((name, entity, TUnion::int(), asserted_type));
        }

        let binding = template_map.and_then(|map| map.get(lhs))?;
        self.conditional_subject_scope.subject_names.push(binding.name);

        Some((
            binding.name,
            binding.defining_entity,
            binding.as_type.clone(),
            asserted_type,
        ))
    }

    fn register_generated_conditional_template(
        &mut self,
        name: StrId,
        entity: GenericParent,
        as_type: TUnion,
    ) {
        let generated = &mut self.conditional_subject_scope.generated_templates;
        if !generated.iter().any(|template| template.name == name) {
            generated.push(pzoom_code_info::functionlike_info::FunctionTemplateType {
                name,
                conditional_subject: false,
                defining_entity: entity,
                as_type,
            });
        }
    }

    fn get_docblock_assertions(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> ParsedFunctionAssertions {
        let mut parsed_assertions = ParsedFunctionAssertions::default();

        for key in ["psalm-assert", "phpstan-assert", "assert"] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.assertions.push(assertion);
                    }
                }
            }
        }

        for key in [
            "psalm-assert-if-true",
            "phpstan-assert-if-true",
            "assert-if-true",
        ] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.if_true_assertions.push(assertion);
                    }
                }
            }
        }

        for key in [
            "psalm-assert-if-false",
            "phpstan-assert-if-false",
            "assert-if-false",
        ] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.if_false_assertions.push(assertion);
                    }
                }
            }
        }

        for key in ["psalm-this-out", "phpstan-this-out", "this-out"] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                    else {
                        continue;
                    };

                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                    let parsed_type = self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    );

                    parsed_assertions.assertions.push(Assertion {
                        var_id: StrId::THIS_VAR,
                        assertion_type: AssertionType::IsType(parsed_type),
                    });
                }
            }
        }

        parsed_assertions
    }

    fn parse_assertion_tag_content(
        &mut self,
        content: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Option<Assertion> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }

        let type_str = crate::docblock::extract_type_string_from_content(trimmed)?;
        let remainder = trimmed.strip_prefix(type_str)?.trim_start();
        let var_token = remainder.split_whitespace().next()?;
        // `$var`, `$var->prop`, or a static property (`self::$q`, `A::$q`).
        if !var_token.starts_with('$') && !var_token.contains("::$") {
            return None;
        }

        let mut assertion_source = type_str.trim();
        let mut is_negation = false;
        let mut is_loose_equality = false;
        let mut is_strict_equality = false;

        if let Some(rest) = assertion_source.strip_prefix('!') {
            is_negation = true;
            assertion_source = rest.trim_start();
        }

        if let Some(rest) = assertion_source.strip_prefix('~') {
            is_loose_equality = true;
            assertion_source = rest.trim_start();
        }

        if let Some(rest) = assertion_source.strip_prefix('=') {
            is_strict_equality = true;
            assertion_source = rest.trim_start();
        }

        if assertion_source.is_empty() {
            return None;
        }

        let assertion_type = if assertion_source.eq_ignore_ascii_case("truthy") {
            if is_negation {
                AssertionType::Falsy
            } else {
                AssertionType::Truthy
            }
        } else if assertion_source.eq_ignore_ascii_case("falsy")
            || assertion_source.eq_ignore_ascii_case("empty")
        {
            if is_negation {
                AssertionType::Truthy
            } else {
                AssertionType::Falsy
            }
        } else if assertion_source.eq_ignore_ascii_case("not-empty")
            || assertion_source.eq_ignore_ascii_case("non-empty")
        {
            if is_negation {
                AssertionType::Falsy
            } else {
                AssertionType::NotEmpty
            }
        } else if assertion_source.eq_ignore_ascii_case("not-null") {
            if is_negation {
                AssertionType::IsType(TUnion::new(TAtomic::TNull))
            } else {
                AssertionType::NotNull
            }
        } else {
            // A conditional assertion type (`=(T is '' ? string :
            // non-empty-string)` on str_contains and friends) keeps its
            // TConditional structure — Psalm parses it like a conditional
            // return type — so the call site picks a branch from the
            // template bounds the arguments inferred. Parsing it here also
            // registers the subject template on the function-like, exempting
            // its bounds from literal generalization.
            let conditional_type = self.parse_docblock_conditional_return_type(
                assertion_source,
                self_class,
                parent_class,
                template_map,
                None,
            );

            // `value-of<Enum::CASE>`-style utilities resolve with class
            // context (enum case values, class constants).
            let parsed_type = conditional_type
                .map(docblock_conditional_union)
                .or_else(|| {
                    self.try_resolve_docblock_utility_type(
                        assertion_source,
                        self_class,
                        parent_class,
                        template_map,
                        None,
                    )
                })
                .unwrap_or_else(|| {
                    let parsed_type = crate::docblock::parse_type_string(
                        assertion_source,
                        self.interner.parent_ref(),
                    )
                    .unwrap_or_else(|_| TUnion::mixed());
                    self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    )
                });

            if parsed_type.is_single()
                && matches!(parsed_type.get_single(), Some(TAtomic::TNull))
                && is_negation
                && !is_loose_equality
                && !is_strict_equality
            {
                AssertionType::NotNull
            } else if is_negation {
                if is_strict_equality {
                    AssertionType::IsNotEqual(parsed_type)
                } else if is_loose_equality {
                    AssertionType::IsNotLooselyEqual(parsed_type)
                } else {
                    AssertionType::IsNotType(parsed_type)
                }
            } else if is_strict_equality {
                AssertionType::IsEqual(parsed_type)
            } else if is_loose_equality {
                AssertionType::IsLooselyEqual(parsed_type)
            } else {
                AssertionType::IsType(parsed_type)
            }
        };

        Some(Assertion {
            var_id: self.interner.intern(var_token),
            assertion_type,
        })
    }

    fn get_implicit_assertions(
        &mut self,
        statements: &[Statement<'_>],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        let mut assertions = Vec::new();

        for statement in statements {
            let Statement::If(if_stmt) = statement else {
                continue;
            };

            if if_stmt.body.has_else_clause() || if_stmt.body.has_else_if_clauses() {
                continue;
            }

            if !self.statements_throw(if_stmt.body.statements()) {
                continue;
            }

            assertions.extend(self.extract_assertions_when_false(
                if_stmt.condition,
                self_class,
                parent_class,
            ));
        }

        assertions
    }

    /// Convert a borrowed scan-time summary event into the interned storage
    /// form (`FunctionLikeInfo::initializer_events`).
    fn intern_initializer_event(
        &self,
        event: &initializer_summary::SummaryEvent<'_>,
    ) -> pzoom_code_info::functionlike_info::InitializerEvent {
        use pzoom_code_info::functionlike_info::InitializerEvent;
        match event {
            initializer_summary::SummaryEvent::Assign(name) => {
                InitializerEvent::Assign(self.interner.intern(name))
            }
            initializer_summary::SummaryEvent::ThisCall(name) => {
                InitializerEvent::ThisCall(self.interner.intern(name))
            }
            initializer_summary::SummaryEvent::ParentCall(name) => {
                InitializerEvent::ParentCall(self.interner.intern(name))
            }
            initializer_summary::SummaryEvent::NamedCall(class_name, method_name) => {
                InitializerEvent::NamedCall(
                    self.interner.intern(class_name),
                    self.interner.intern(method_name),
                )
            }
            initializer_summary::SummaryEvent::Branch(branches) => InitializerEvent::Branch(
                branches
                    .iter()
                    .map(|branch| {
                        branch
                            .iter()
                            .map(|event| self.intern_initializer_event(event))
                            .collect()
                    })
                    .collect(),
            ),
        }
    }

    /// Collect the names of `$this->X` properties assigned anywhere within the
    /// given statements. Mirrors Psalm's `ReflectorVisitor`, which records every
    /// `$this->name = ...` (including compound assignments) into
    /// `MethodStorage::$this_property_mutations`.
    fn collect_this_property_mutations(&mut self, statements: &[Statement<'_>]) -> Vec<StrId> {
        use mago_syntax::walker::Walker;

        let mut names: Vec<&str> = Vec::new();
        let walker = ThisPropertyMutationWalker;
        for statement in statements {
            walker.walk_statement(statement, &mut names);
        }

        names.into_iter().map(|name| self.interner.intern(name)).collect()
    }

    /// Whether a method body reads or writes any static property
    /// (`self::$x`, `static::$x`, `Foo::$x`). Hakana tracks this during the
    /// body walk (`has_static_field_access`) to decide taint specialization:
    /// static state carries data between call sites, so such methods must
    /// keep global (unspecialized) taint nodes.
    fn method_body_accesses_static_property(
        &self,
        body: &mago_syntax::ast::ast::class_like::method::MethodBody<'_>,
    ) -> bool {
        use mago_syntax::walker::Walker;

        let mago_syntax::ast::ast::class_like::method::MethodBody::Concrete(block) = body else {
            return false;
        };

        let mut found = false;
        let walker = StaticPropertyAccessWalker;
        for statement in block.statements.iter() {
            walker.walk_statement(statement, &mut found);
            if found {
                break;
            }
        }
        found
    }

    /// Find every anonymous-class expression nested in `stmt` and register
    /// each as a classlike storage (Psalm's ReflectorVisitor does the same;
    /// the synthetic name keys the analyzer's lookup).
    fn collect_anonymous_classes(&mut self, stmt: &Statement<'_>) {
        use mago_syntax::walker::Walker;

        let walker = AnonymousClassCollectorWalker;
        let mut found = Vec::new();
        walker.walk_statement(stmt, &mut found);

        for anonymous_class in found {
            self.visit_anonymous_class(anonymous_class);
        }
    }

    /// Resolve a define()'s name argument: a string literal, or a
    /// concatenation of literals and `__NAMESPACE__` (Psalm's
    /// `ConstFetchAnalyzer::getConstName`).
    fn resolve_define_name_expression(&self, expr: &Expression<'_>) -> Option<String> {
        match expr.unparenthesized() {
            Expression::Literal(Literal::String(name_literal)) => {
                Some(php_unescape_string_literal(name_literal))
            }
            Expression::MagicConstant(
                mago_syntax::ast::ast::magic_constant::MagicConstant::Namespace(_),
            ) => Some(match self.current_namespace {
                Some(namespace) => self.interner.lookup(namespace).to_string(),
                None => String::new(),
            }),
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    mago_syntax::ast::ast::binary::BinaryOperator::StringConcat(_)
                ) =>
            {
                let lhs = self.resolve_define_name_expression(binary.lhs)?;
                let rhs = self.resolve_define_name_expression(binary.rhs)?;
                Some(format!("{}{}", lhs, rhs))
            }
            _ => None,
        }
    }

    fn collect_defined_constants_from_statements(
        &mut self,
        statements: &[Statement<'_>],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<(StrId, TUnion)> {
        self.collect_defined_constants_from_statements_inner(
            statements,
            self_class,
            parent_class,
            false,
        )
    }

    fn collect_defined_constants_from_statements_inner(
        &mut self,
        statements: &[Statement<'_>],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        top_level: bool,
    ) -> Vec<(StrId, TUnion)> {
        let mut defined_constants = Vec::new();

        for statement in statements {
            let Statement::Expression(expr_stmt) = statement else {
                continue;
            };
            let Expression::Call(mago_syntax::ast::ast::call::Call::Function(function_call)) =
                expr_stmt.expression.unparenthesized()
            else {
                continue;
            };
            let Expression::Identifier(function_name) = function_call.function.unparenthesized()
            else {
                continue;
            };
            if !function_name.value().eq_ignore_ascii_case("define") {
                continue;
            }

            let Some(name_arg) = function_call.argument_list.arguments.first() else {
                continue;
            };
            let Some(value_arg) = function_call.argument_list.arguments.get(1) else {
                continue;
            };

            let Some(constant_name) = self.resolve_define_name_expression(name_arg.value())
            else {
                continue;
            };
            let constant_name = constant_name.trim_start_matches('\\').to_string();
            if constant_name.is_empty() {
                continue;
            }
            let constant_name = constant_name.as_str();

            let qualified_name = if constant_name.contains('\\') {
                constant_name.to_string()
            } else if let Some(namespace) = self.current_namespace {
                let namespace = self.interner.lookup(namespace);
                format!("{}\\{}", namespace, constant_name)
            } else {
                constant_name.to_string()
            };

            let constant_id = self.interner.intern(&qualified_name);
            let inferred_type = simple_type_inferer::infer(value_arg.value());
            let constant_type = inferred_type.clone().unwrap_or_else(TUnion::mixed);

            // Psalm's ExpressionScanner sees every define() during scanning;
            // under allConstantsGlobal these become global constants
            // (addGlobalConstantType). define() always defines the literal
            // name, so the global entry is never namespaced.
            let define_value = match inferred_type {
                Some(value_type) => GlobalDefineValue::Resolved(value_type),
                None => {
                    self.global_define_value_from_call(value_arg.value(), self_class, parent_class)
                }
            };
            self.declarations.global_defines.push(GlobalDefine {
                name: self.interner.intern(constant_name),
                value: define_value,
                file_path: self.file_path,
                start_offset: expr_stmt.span().start.offset,
            });

            // A top-level define() declares the literal (possibly namespaced)
            // name for the file (Psalm's ExpressionScanner puts it in
            // `$file_storage->constants` regardless of allConstantsGlobal).
            if top_level {
                self.declarations.constants.push(ConstantInfo {
                    name: self.interner.intern(constant_name),
                    constant_type: constant_type.clone(),
                    file_path: self.file_path,
                    start_offset: expr_stmt.span().start.offset,
                    unresolved_initializer: None,
                });
            }

            let constant_type_for_flow = constant_type;
            if let Some((_, existing_type)) = defined_constants
                .iter_mut()
                .find(|(existing_id, _)| *existing_id == constant_id)
            {
                *existing_type = constant_type_for_flow;
            } else {
                defined_constants.push((constant_id, constant_type_for_flow));
            }
        }

        defined_constants
    }

    /// The deferred [`GlobalDefineValue`] for a define() whose value is a
    /// plain function or static-method call: the callee's declared return
    /// type stands in for the runtime value once the codebase is populated.
    fn global_define_value_from_call(
        &mut self,
        value: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> GlobalDefineValue {
        match value.unparenthesized() {
            Expression::Call(mago_syntax::ast::ast::call::Call::Function(value_call)) => {
                if let Expression::Identifier(callee) = value_call.function.unparenthesized() {
                    let callee_name = callee.value().trim_start_matches('\\');
                    if !callee_name.is_empty() {
                        return GlobalDefineValue::FunctionReturn(
                            self.interner.intern(callee_name),
                        );
                    }
                }
                GlobalDefineValue::Resolved(TUnion::mixed())
            }
            Expression::Call(mago_syntax::ast::ast::call::Call::StaticMethod(static_call)) => {
                let class_id =
                    self.resolve_class_expression(static_call.class, self_class, parent_class);
                if let (Some(class_id), ClassLikeMemberSelector::Identifier(method)) =
                    (class_id, &static_call.method)
                {
                    return GlobalDefineValue::MethodReturn(
                        class_id,
                        self.interner.intern(method.value),
                    );
                }
                GlobalDefineValue::Resolved(TUnion::mixed())
            }
            _ => GlobalDefineValue::Resolved(TUnion::mixed()),
        }
    }

    fn span_contains_variadic_builtin_calls(&self, start_offset: u32, end_offset: u32) -> bool {
        let start = start_offset as usize;
        let end = end_offset as usize;

        if start >= end || end > self.source.len() {
            return false;
        }

        let haystack = self.source[start..end].to_ascii_lowercase();
        haystack.contains("func_get_arg(")
            || haystack.contains("func_get_args(")
            || haystack.contains("func_num_args(")
    }

    fn statements_throw(&self, statements: &[Statement<'_>]) -> bool {
        if statements.len() != 1 {
            return false;
        }

        match &statements[0] {
            Statement::Expression(expr_stmt) => {
                matches!(expr_stmt.expression.unparenthesized(), Expression::Throw(_))
            }
            Statement::Block(block) => self.statements_throw(block.statements.as_slice()),
            _ => false,
        }
    }

    fn extract_assertions_when_false(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        match expr.unparenthesized() {
            Expression::UnaryPrefix(unary) if unary.operator.is_not() => {
                self.extract_assertions_when_true(unary.operand, self_class, parent_class)
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Or(_) | BinaryOperator::LowOr(_)
                ) =>
            {
                let mut assertions =
                    self.extract_assertions_when_false(binary.lhs, self_class, parent_class);
                assertions.extend(self.extract_assertions_when_false(
                    binary.rhs,
                    self_class,
                    parent_class,
                ));
                assertions
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
                ) =>
            {
                if let Some(var_name) = extract_direct_var(binary.lhs)
                    && is_null_expression(binary.rhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }

                if let Some(var_name) = extract_direct_var(binary.rhs)
                    && is_null_expression(binary.lhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }

                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn extract_assertions_when_true(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        match expr.unparenthesized() {
            Expression::Binary(binary) if binary.operator.is_instanceof() => {
                let Some(var_name) = extract_direct_var(binary.lhs) else {
                    return Vec::new();
                };
                let Some(class_id) =
                    self.resolve_class_expression(binary.rhs, self_class, parent_class)
                else {
                    return Vec::new();
                };

                vec![Assertion {
                    var_id: self.interner.intern(&var_name),
                    assertion_type: AssertionType::IsType(TUnion::new(TAtomic::TNamedObject {
                        name: class_id,
                        type_params: None,
                    is_static: false, remapped_params: false })),
                }]
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::NotEqual(_)
                        | BinaryOperator::AngledNotEqual(_)
                        | BinaryOperator::NotIdentical(_)
                ) =>
            {
                if let Some(var_name) = extract_direct_var(binary.lhs)
                    && is_null_expression(binary.rhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }

                if let Some(var_name) = extract_direct_var(binary.rhs)
                    && is_null_expression(binary.lhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }

                Vec::new()
            }
            Expression::Call(call) => {
                self.extract_builtin_call_assertions(call, self_class, parent_class)
            }
            _ => Vec::new(),
        }
    }

    fn extract_builtin_call_assertions(
        &mut self,
        call: &mago_syntax::ast::ast::call::Call<'_>,
        _self_class: Option<StrId>,
        _parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        let mago_syntax::ast::ast::call::Call::Function(function_call) = call else {
            return Vec::new();
        };

        let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
            return Vec::new();
        };

        let Some(first_arg) = function_call.argument_list.arguments.first() else {
            return Vec::new();
        };
        let Some(var_name) = extract_direct_var(first_arg.value()) else {
            return Vec::new();
        };

        let asserted_type = match function_name.value().to_ascii_lowercase().as_str() {
            "is_string" => TAtomic::TString,
            "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
            "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
            "is_bool" => TAtomic::TBool,
            "is_object" => TAtomic::TObject,
            "is_null" => TAtomic::TNull,
            "is_numeric" => TAtomic::TNumeric,
            "is_resource" => TAtomic::TResource,
            "is_scalar" => TAtomic::TScalar,
            "is_array" => TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            _ => return Vec::new(),
        };

        vec![Assertion {
            var_id: self.interner.intern(&var_name),
            assertion_type: AssertionType::IsType(TUnion::new(asserted_type)),
        }]
    }

    fn resolve_class_expression(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Option<StrId> {
        match expr.unparenthesized() {
            Expression::Identifier(identifier) => Some(self.resolve_identifier(identifier)),
            Expression::Self_(_) | Expression::Static(_) => self_class.or(Some(StrId::SELF)),
            Expression::Parent(_) => parent_class.or(Some(StrId::PARENT)),
            _ => None,
        }
    }

    fn register_namespace_type_aliases(
        &mut self,
        aliases: &FxHashMap<String, TUnion>,
        start_offset: u32,
    ) {
        for (alias_name, aliased_type) in aliases {
            let scoped_alias = self.make_fqn(alias_name);
            self.declarations.type_aliases.push(ClassTypeAlias {
                name: scoped_alias,
                aliased_type: aliased_type.clone(),
                file_path: self.file_path,
                start_offset,
            });
        }
    }

    fn collect_preceding_statement_type_aliases(&mut self, stmt_start_offset: u32) {
        let docblocks: Vec<&'p str> = self
            .trivia
            .iter()
            .filter(|trivia| {
                trivia.kind == TriviaKind::DocBlockComment
                    && trivia.span.end.offset < stmt_start_offset
            })
            .map(|trivia| trivia.value)
            .collect();

        for docblock in docblocks {
            let parsed = crate::docblock::parse(docblock, 0);
            if !(parsed.tags.contains_key("phpstan-type")
                || parsed.tags.contains_key("psalm-type")
                || parsed.tags.contains_key("phpstan-import-type")
                || parsed.tags.contains_key("psalm-import-type"))
            {
                continue;
            }

            let aliases = self.collect_docblock_type_aliases(&parsed, None, None, None, None);
            self.register_namespace_type_aliases(&aliases, stmt_start_offset);
        }
    }

    fn collect_docblock_type_aliases(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        base_aliases: Option<&FxHashMap<String, TUnion>>,
    ) -> FxHashMap<String, TUnion> {
        let mut aliases = base_aliases.cloned().unwrap_or_default();

        let mut import_entries: Vec<(usize, String)> = Vec::new();
        for key in ["phpstan-import-type", "psalm-import-type"] {
            if let Some(tags) = parsed.tags.get(key) {
                for (offset, content) in tags {
                    import_entries.push((*offset, content.clone()));
                }
            }
        }
        import_entries.sort_by_key(|(offset, _)| *offset);

        for (_, content) in import_entries {
            let Some((imported_alias, source_name, alias_name)) =
                parse_import_type_tag_content(&content)
            else {
                continue;
            };

            let source_class = self.resolve_docblock_class_name(
                self.interner.intern(&source_name),
                self_class,
                parent_class,
            );
            // Recorded for analysis-time validation (the source class may
            // live in another file, unknown at scan).
            let import_record = (source_class, imported_alias.clone());
            if !self
                .declarations
                .type_alias_imports.contains(&import_record)
            {
                self.declarations.type_alias_imports.push(import_record);
            }
            let scoped_alias = self.interner.intern(&format!(
                "{}::{}",
                self.interner.lookup(source_class),
                imported_alias
            ));

            if let Some(type_alias) = self.known_type_aliases.get(&scoped_alias) {
                aliases.insert(alias_name, type_alias.aliased_type.clone());
                continue;
            }

            if let Some(type_alias) = self
                .declarations
                .type_aliases
                .iter()
                .find(|type_alias| type_alias.name == scoped_alias)
            {
                aliases.insert(alias_name, type_alias.aliased_type.clone());
                continue;
            }

            // Keep unresolved imported aliases from triggering UndefinedClass.
            aliases.insert(alias_name, TUnion::mixed());
        }

        let mut type_entries: Vec<(usize, String)> = Vec::new();
        for key in ["phpstan-type", "psalm-type"] {
            if let Some(tags) = parsed.tags.get(key) {
                for (offset, content) in tags {
                    type_entries.push((*offset, content.clone()));
                }
            }
        }
        type_entries.sort_by_key(|(offset, _)| *offset);

        for (_, content) in type_entries {
            let Some((alias_name, type_definition)) = parse_type_alias_tag_content(&content) else {
                continue;
            };

            let previous_aliases =
                std::mem::replace(&mut self.active_docblock_type_aliases, aliases.clone());
            // A CLASSLIKE's own @psalm-type definitions resolve only against
            // the class's aliases (earlier definitions + imports); another
            // class's alias must be imported first (Psalm).
            let previous_restrict = std::mem::replace(
                &mut self.restrict_aliases_to_active,
                self_class.is_some(),
            );
            let parsed_type = crate::docblock::parse_type_string(&type_definition, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );
            self.restrict_aliases_to_active = previous_restrict;
            self.active_docblock_type_aliases = previous_aliases;

            aliases.insert(alias_name, resolved_type);
        }

        aliases
    }

    fn resolve_docblock_union_type(
        &mut self,
        mut t_union: TUnion,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> TUnion {
        let mut resolved_types = Vec::new();
        for mut atomic in t_union.types {
            if let Some(alias_union) = self.resolve_docblock_type_alias_atomic(&atomic) {
                for alias_atomic in alias_union.types {
                    if !resolved_types.contains(&alias_atomic) {
                        resolved_types.push(alias_atomic);
                    }
                }
                continue;
            }

            // Intersections resolve their members through the alias map too;
            // an intersection of array shapes collapses into the combined
            // shape (Psalm's parser merges `array{a: int}&array{b: string}`).
            if let TAtomic::TObjectIntersection { types: members } = &atomic {
                let mut resolved_members: Vec<TAtomic> = Vec::new();
                for member in members {
                    if let Some(alias_union) = self.resolve_docblock_type_alias_atomic(member) {
                        resolved_members.extend(alias_union.types);
                    } else {
                        let mut member = member.clone();
                        self.resolve_docblock_atomic_type(
                            &mut member,
                            self_class,
                            parent_class,
                            template_map,
                        );
                        resolved_members.push(member);
                    }
                }

                let merged = merge_intersected_shapes(&resolved_members)
                    .unwrap_or(TAtomic::TObjectIntersection {
                        types: resolved_members,
                    });
                if !resolved_types.contains(&merged) {
                    resolved_types.push(merged);
                }
                continue;
            }

            self.resolve_docblock_atomic_type(&mut atomic, self_class, parent_class, template_map);
            if !resolved_types.contains(&atomic) {
                resolved_types.push(atomic);
            }
        }

        merge_same_class_generic_members(&mut resolved_types);

        t_union.types = resolved_types;
        t_union
    }

    fn resolve_docblock_type_alias_atomic(&self, atomic: &TAtomic) -> Option<TUnion> {
        let TAtomic::TNamedObject { name, type_params , .. } = atomic else {
            return None;
        };

        if type_params.is_some() {
            return None;
        }

        let alias_name = self.interner.lookup(*name);
        if let Some(alias_union) = self.active_docblock_type_aliases.get(alias_name.as_ref()) {
            return Some(alias_union.clone());
        }

        if self.restrict_aliases_to_active {
            return None;
        }

        let fqn_alias = if let Some(ns) = self.current_namespace {
            let ns_str = self.interner.lookup(ns);
            self.interner
                .intern(&format!("{}\\{}", ns_str, alias_name.as_ref()))
        } else {
            self.interner.intern(alias_name.as_ref())
        };
        if let Some(type_alias) = self.known_type_aliases.get(&fqn_alias) {
            return Some(type_alias.aliased_type.clone());
        }

        if let Some(type_alias) = self
            .declarations
            .type_aliases
            .iter()
            .rev()
            .find(|type_alias| type_alias.name == fqn_alias)
        {
            return Some(type_alias.aliased_type.clone());
        }

        let raw_alias = self.interner.intern(alias_name.as_ref());
        if let Some(type_alias) = self.known_type_aliases.get(&raw_alias) {
            return Some(type_alias.aliased_type.clone());
        }

        self.declarations
            .type_aliases
            .iter()
            .rev()
            .find(|type_alias| type_alias.name == raw_alias)
            .map(|type_alias| type_alias.aliased_type.clone())
    }

    fn resolve_docblock_atomic_type(
        &mut self,
        atomic: &mut TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        match atomic {
            TAtomic::TNamedObject { name, type_params , .. } => {
                if type_params.is_none() {
                    let template_key = self.interner.lookup(*name);
                    if let Some(template_binding) =
                        template_map.and_then(|map| map.get(template_key.as_ref()))
                    {
                        *atomic = TAtomic::TTemplateParam {
                            name: template_binding.name,
                            defining_entity: template_binding.defining_entity,
                            as_type: Box::new(template_binding.as_type.clone()),
                        };
                        return;
                    }
                }

                *name = self.resolve_docblock_class_name(*name, self_class, parent_class);
                if let Some(type_params) = type_params {
                    for param in type_params {
                        *param = self.resolve_docblock_union_type(
                            param.clone(),
                            self_class,
                            parent_class,
                            template_map,
                        );
                    }
                }
            }
            // `T::class` where `T` is a template parameter denotes `class-string<T>`
            // (Psalm). Without a template context the parser produces a literal
            // class-string of a class literally named `T`; rewrite it here.
            // Concrete `C::class` literals resolve through the namespace and
            // use-aliases like any other docblock class name (Psalm fully
            // qualifies them during type resolution).
            TAtomic::TLiteralClassString { name } => {
                if let Some(template_binding) =
                    template_map.and_then(|map| map.get(name.as_str()))
                {
                    *atomic = TAtomic::TClassString {
                        as_type: Some(Box::new(TAtomic::TTemplateParam {
                            name: template_binding.name,
                            defining_entity: template_binding.defining_entity,
                            as_type: Box::new(template_binding.as_type.clone()),
                        })),
                    };
                } else {
                    let name_id = self.interner.intern(name.as_str());
                    let resolved =
                        self.resolve_docblock_class_name(name_id, self_class, parent_class);
                    *name = self.interner.lookup(resolved).to_string();
                }
            }
            // `properties-of<C>` parsed without a template context yields
            // `TPropertiesOf{classlike_name}`; here we apply the same template
            // decision the parser would have made with context (and resolve
            // self/parent/aliased class names), matching `properties_of_or_deferred`.
            TAtomic::TPropertiesOf {
                classlike_name,
                visibility_filter,
            } => {
                let template_key = self.interner.lookup(*classlike_name);
                if let Some(template_binding) =
                    template_map.and_then(|map| map.get(template_key.as_ref()))
                {
                    *atomic = TAtomic::TTemplatePropertiesOf {
                        param_name: template_binding.name,
                        defining_entity: template_binding.defining_entity,
                        visibility_filter: *visibility_filter,
                    };
                    return;
                }

                *classlike_name =
                    self.resolve_docblock_class_name(*classlike_name, self_class, parent_class);
            }
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            }
            | TAtomic::TIterable {
                key_type,
                value_type,
            } => {
                **key_type = self.resolve_docblock_union_type(
                    (**key_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
                **value_type = self.resolve_docblock_union_type(
                    (**value_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                **value_type = self.resolve_docblock_union_type(
                    (**value_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TClassStringMap {
                as_type,
                value_param,
                ..
            } => {
                if let Some(as_type) = as_type {
                    self.resolve_docblock_atomic_type(
                        as_type,
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
                **value_param = self.resolve_docblock_union_type(
                    (**value_param).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                for prop_type in std::sync::Arc::make_mut(properties).values_mut() {
                    *prop_type = self.resolve_docblock_union_type(
                        prop_type.clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
                if let Some(key_type) = fallback_key_type {
                    **key_type = self.resolve_docblock_union_type(
                        (**key_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
                if let Some(value_type) = fallback_value_type {
                    **value_type = self.resolve_docblock_union_type(
                        (**value_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TObjectWithProperties { properties, .. } => {
                for prop_type in properties.values_mut() {
                    *prop_type = self.resolve_docblock_union_type(
                        prop_type.clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                **as_type = self.resolve_docblock_union_type(
                    (**as_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                self.resolve_docblock_atomic_type(as_type, self_class, parent_class, template_map);
            }
            TAtomic::TClosure {
                params,
                return_type,
                ..
            }
            | TAtomic::TCallable {
                params,
                return_type,
                ..
            } => {
                if let Some(params) = params {
                    for param in params {
                        param.param_type = self.resolve_docblock_union_type(
                            param.param_type.clone(),
                            self_class,
                            parent_class,
                            template_map,
                        );
                    }
                }

                if let Some(return_type) = return_type {
                    **return_type = self.resolve_docblock_union_type(
                        (**return_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TClassString { as_type } => {
                if let Some(as_type) = as_type {
                    self.resolve_docblock_atomic_type(
                        as_type,
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for atomic in types.iter_mut() {
                    self.resolve_docblock_atomic_type(
                        atomic,
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            _ => {}
        }
    }

    fn resolve_docblock_class_name(
        &mut self,
        name: StrId,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> StrId {
        let name_str = self.interner.lookup(name);
        let normalized = name_str.as_ref().trim();

        if let Some((class_part, const_part)) = normalized.rsplit_once("::") {
            let resolved_class = self.resolve_docblock_class_name(
                self.interner.intern(class_part),
                self_class,
                parent_class,
            );
            let resolved_class_name = self.interner.lookup(resolved_class);
            return self
                .interner
                .intern(&format!("{}::{}", resolved_class_name, const_part));
        }

        let lower = normalized.to_ascii_lowercase();

        if lower == "self" {
            return self_class.unwrap_or(StrId::SELF);
        }

        if lower == "static" {
            return StrId::STATIC;
        }

        if lower == "parent" {
            return parent_class.unwrap_or(StrId::PARENT);
        }

        if normalized.starts_with('\\') {
            return self
                .interner
                .intern(normalized.strip_prefix('\\').unwrap_or(normalized));
        }

        let (first_segment, remainder) = match normalized.split_once('\\') {
            Some((first, rest)) => (first, Some(rest)),
            None => (normalized, None),
        };

        if let Some(alias_target) = self.use_aliases.get(&first_segment.to_ascii_lowercase()) {
            if let Some(remainder) = remainder {
                let alias_str = self.interner.lookup(*alias_target);
                return self
                    .interner
                    .intern(&format!("{}\\{}", alias_str, remainder));
            }

            return *alias_target;
        }

        if let Some(current_namespace) = self.current_namespace {
            let namespace = self.interner.lookup(current_namespace);
            return self
                .interner
                .intern(&format!("{}\\{}", namespace, normalized));
        }

        self.interner.intern(normalized)
    }

    fn expand_docblock_class_constant_wildcards(
        &mut self,
        t_union: TUnion,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> TUnion {
        let mut expanded_types = Vec::new();

        for atomic in &t_union.types {
            for expanded_atomic in self.expand_docblock_class_constant_wildcards_in_atomic(
                atomic.clone(),
                self_class,
                parent_class,
                class_constants,
            ) {
                if !expanded_types.contains(&expanded_atomic) {
                    expanded_types.push(expanded_atomic);
                }
            }
        }

        // Preserve union-level metadata that `from_types` does not reconstruct,
        // most importantly `possibly_undefined` (the optional-key marker on a
        // keyed-array shape property) and `from_docblock`.
        let mut expanded = TUnion::from_types(expanded_types);
        expanded.from_docblock = t_union.from_docblock;
        expanded.from_calculation = t_union.from_calculation;
        expanded.possibly_undefined = t_union.possibly_undefined;
        expanded.is_resolved = t_union.is_resolved;
        expanded.parent_nodes = t_union.parent_nodes;
        expanded.ignore_nullable_issues = t_union.ignore_nullable_issues;
        expanded.ignore_falsable_issues = t_union.ignore_falsable_issues;
        expanded
    }

    fn expand_docblock_class_constant_wildcards_in_atomic(
        &mut self,
        atomic: TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Vec<TAtomic> {
        if let Some(expanded_union) = self.resolve_class_constant_union_from_atomic(
            &atomic,
            self_class,
            parent_class,
            class_constants,
        ) {
            return expanded_union.types;
        }

        match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            } => vec![TAtomic::TArray {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => vec![TAtomic::TNonEmptyArray {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TIterable {
                key_type,
                value_type,
            } => vec![TAtomic::TIterable {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TList { value_type } => vec![TAtomic::TList {
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TNonEmptyList { value_type } => vec![TAtomic::TNonEmptyList {
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => vec![TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(
                    std::sync::Arc::try_unwrap(properties)
                        .unwrap_or_else(|shared| (*shared).clone())
                        .into_iter()
                        .map(|(key, prop_type)| {
                            (
                                key,
                                self.expand_docblock_class_constant_wildcards(
                                    prop_type,
                                    self_class,
                                    parent_class,
                                    class_constants,
                                ),
                            )
                        })
                        .collect(),
                ),
                is_list,
                sealed,
                fallback_key_type: fallback_key_type.map(|key_type| {
                    Box::new(self.expand_docblock_class_constant_wildcards(
                        *key_type,
                        self_class,
                        parent_class,
                        class_constants,
                    ))
                }),
                fallback_value_type: fallback_value_type.map(|value_type| {
                    Box::new(self.expand_docblock_class_constant_wildcards(
                        *value_type,
                        self_class,
                        parent_class,
                        class_constants,
                    ))
                }),
            }],
            TAtomic::TNamedObject { name, type_params , .. } => {
                if let Some(type_params) = type_params {
                    vec![TAtomic::TNamedObject {
                        name,
                        type_params: Some(
                            type_params
                                .into_iter()
                                .map(|type_param| {
                                    self.expand_docblock_class_constant_wildcards(
                                        type_param,
                                        self_class,
                                        parent_class,
                                        class_constants,
                                    )
                                })
                                .collect(),
                        ),
                    is_static: false, remapped_params: false }]
                } else {
                    vec![TAtomic::TNamedObject {
                        name,
                        type_params: None,
                    is_static: false, remapped_params: false }]
                }
            }
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => vec![TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *as_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            other => vec![other],
        }
    }

    fn resolve_class_constant_union_from_atomic(
        &mut self,
        atomic: &TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        let TAtomic::TNamedObject { name, type_params , .. } = atomic else {
            return None;
        };

        if type_params.is_some() {
            return None;
        }

        let raw_name = self.interner.lookup(*name).to_string();
        let (class_part, constant_part) = raw_name.split_once("::")?;
        if constant_part.eq_ignore_ascii_case("class") {
            return None;
        }

        let class_part = class_part.trim();
        let constant_part = constant_part.trim();
        if class_part.is_empty() || constant_part.is_empty() {
            return None;
        }

        let class_part_lower = class_part.to_ascii_lowercase();
        let resolved_class = match class_part_lower.as_str() {
            "self" | "static" => self_class?,
            "parent" => parent_class?,
            // The class part was already namespace/alias-resolved by
            // resolve_docblock_union_type (names resolve exactly once, like
            // Psalm's TypeParser); re-resolving here would prefix the current
            // namespace a second time (Foo\Bar\Scope -> Foo\Bar\Foo\Bar\Scope)
            // and miss every namespaced class.
            _ => self.interner.intern(class_part),
        };

        let constants = if Some(resolved_class) == self_class {
            class_constants
        } else {
            self.declarations
                .classes
                .iter()
                .find(|class_info| class_info.name == resolved_class)
                .map(|class_info| &class_info.constants)
        };

        let Some(constants) = constants else {
            // Keep the token-named reference: analysis resolves it against
            // the populated codebase (or reports UndefinedDocblockClass /
            // UndefinedConstant).
            return None;
        };

        let mut resolved_union: Option<TUnion> = None;

        if let Some(prefix) = constant_part.strip_suffix('*') {
            for (constant_name, constant_info) in constants {
                let candidate_name = self.interner.lookup(*constant_name);
                if candidate_name.starts_with(prefix) {
                    resolved_union = Some(if let Some(existing) = resolved_union {
                        combine_union_types(&existing, &constant_info.constant_type, false)
                    } else {
                        constant_info.constant_type.clone()
                    });
                }
            }
        } else {
            for (constant_name, constant_info) in constants {
                if self.interner.lookup(*constant_name).as_ref() == constant_part {
                    resolved_union = Some(constant_info.constant_type.clone());
                    break;
                }
            }
        }

        resolved_union.as_ref()?;

        resolved_union
    }

    fn parse_docblock_template_bindings(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        defining_entity: GenericParent,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        base_template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        docblock_issues: &mut Vec<DocblockIssue>,
    ) -> Vec<DocblockTemplateBinding> {
        let mut template_entries = Vec::new();

        if let Some(tags) = parsed.combined_tags.get("template") {
            for (offset, content) in tags {
                template_entries.push((*offset, content.as_str(), TemplateVariance::Invariant));
            }
        }

        if let Some(tags) = parsed.combined_tags.get("template-covariant") {
            for (offset, content) in tags {
                template_entries.push((*offset, content.as_str(), TemplateVariance::Covariant));
            }
        }

        if template_entries.is_empty() {
            return Vec::new();
        }

        template_entries.sort_by_key(|(offset, _, _)| *offset);

        let mut template_map = base_template_map.cloned().unwrap_or_default();
        let mut template_bindings: Vec<DocblockTemplateBinding> = Vec::new();

        for (offset, content, variance) in template_entries {
            let Some((template_name, template_bound)) = parse_template_tag_content(content) else {
                // An `@template` tag with no name (e.g. `/** @template */`) is a
                // MissingDocblockType (Psalm).
                let offset = offset as u32;
                docblock_issues.push(DocblockIssue {
                    message: "Missing docblock type".to_string(),
                    start_offset: offset,
                    end_offset: offset.saturating_add(1),
                });
                continue;
            };

            let template_name_id = self.interner.intern(&template_name);

            // Psalm `FunctionLikeDocblockScanner::handleTemplates`: a
            // function-like (or method) `@template` that re-declares a
            // template already defined by the enclosing class is an
            // InvalidDocblock, and the class-level binding stays in force.
            if let GenericParent::FunctionLike(owner) = defining_entity
                && base_template_map.is_some_and(|base| base.contains_key(&template_name))
            {
                let offset = offset as u32;
                docblock_issues.push(DocblockIssue {
                    message: format!(
                        "Duplicate template param {} in docblock for {}",
                        template_name,
                        self.interner.lookup(owner)
                    ),
                    start_offset: offset,
                    end_offset: offset.saturating_add(1),
                });
                continue;
            }

            let placeholder = DocblockTemplateBinding {
                name: template_name_id,
                defining_entity,
                as_type: TUnion::mixed(),
                variance,
            };
            template_map.insert(template_name.clone(), placeholder.clone());

            let as_type = if let Some(template_bound) = template_bound {
                self.try_resolve_template_key_of_type(&template_bound, Some(&template_map))
                    .unwrap_or_else(|| {
                        let parsed_type =
                            match crate::docblock::parse_type_string(&template_bound, self.interner.parent_ref()) {
                                Ok(parsed_type) => parsed_type,
                                Err(parse_error) => {
                                    // Psalm: a malformed `@template` bound is an
                                    // InvalidDocblock — `... in docblock for C`
                                    // (ClassLikeNodeScanner) for classes,
                                    // `Template T has invalid as type - ...`
                                    // (FunctionLikeDocblockScanner) otherwise —
                                    // and the bound falls back to mixed.
                                    let message = match defining_entity {
                                        GenericParent::ClassLike(owner) => format!(
                                            "{} in docblock for {}",
                                            parse_error.message,
                                            self.interner.lookup(owner)
                                        ),
                                        _ => format!(
                                            "Template {} has invalid as type - {}",
                                            template_name, parse_error.message
                                        ),
                                    };
                                    let offset = offset as u32;
                                    docblock_issues.push(DocblockIssue {
                                        message,
                                        start_offset: offset,
                                        end_offset: offset.saturating_add(1),
                                    });
                                    TUnion::mixed()
                                }
                            };
                        self.resolve_docblock_union_type(
                            parsed_type,
                            self_class,
                            parent_class,
                            Some(&template_map),
                        )
                    })
            } else {
                TUnion::mixed()
            };

            let as_type = self.expand_docblock_class_constant_wildcards(
                as_type,
                self_class,
                parent_class,
                class_constants,
            );

            let binding = DocblockTemplateBinding {
                as_type,
                ..placeholder
            };
            template_map.insert(template_name.clone(), binding.clone());

            if let Some(existing_binding) = template_bindings
                .iter_mut()
                .find(|existing| existing.name == template_name_id)
            {
                *existing_binding = binding;
            } else {
                template_bindings.push(binding);
            }
        }

        template_bindings
    }

    fn build_template_map_from_bindings(
        &self,
        bindings: &[DocblockTemplateBinding],
        base_template_map: Option<&TemplateMap>,
    ) -> TemplateMap {
        let mut template_map = base_template_map.cloned().unwrap_or_default();

        for binding in bindings {
            template_map.insert(
                self.interner.lookup(binding.name).to_string(),
                binding.clone(),
            );
        }

        template_map
    }

    fn build_template_map_from_class_template_types(
        &self,
        template_types: &[TemplateType],
        defining_entity: GenericParent,
    ) -> TemplateMap {
        let mut template_map = FxHashMap::default();

        for template_type in template_types {
            template_map.insert(
                self.interner.lookup(template_type.name).to_string(),
                DocblockTemplateBinding {
                    name: template_type.name,
                    defining_entity,
                    as_type: template_type.as_type.clone(),
                    variance: template_type.variance,
                },
            );
        }

        template_map
    }

    fn is_docblock_pure(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("pure")
            || parsed.tags.contains_key("psalm-pure")
            || parsed.tags.contains_key("phpstan-pure")
    }

    fn is_docblock_inheritdoc(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        if parsed
            .tags
            .keys()
            .any(|tag_name| tag_name.eq_ignore_ascii_case("inheritdoc"))
        {
            return true;
        }

        parsed
            .description
            .to_ascii_lowercase()
            .contains("@inheritdoc")
    }

    fn is_docblock_mutation_free(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        // Bare `@mutation-free` is NOT a Psalm tag (vendor docblocks use it;
        // Psalm ignores it entirely — no memoization, no body enforcement).
        parsed.tags.contains_key("psalm-mutation-free")
            || parsed.tags.contains_key("phpstan-mutation-free")
    }

    fn is_docblock_no_named_arguments(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("no-named-arguments")
            || parsed.tags.contains_key("psalm-no-named-arguments")
            || parsed.tags.contains_key("phpstan-no-named-arguments")
    }

    fn is_docblock_ignore_nullable_return(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("ignore-nullable-return")
            || parsed.tags.contains_key("psalm-ignore-nullable-return")
            || parsed.tags.contains_key("phpstan-ignore-nullable-return")
    }

    fn is_docblock_ignore_falsable_return(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("ignore-falsable-return")
            || parsed.tags.contains_key("psalm-ignore-falsable-return")
            || parsed.tags.contains_key("phpstan-ignore-falsable-return")
    }

    fn is_docblock_immutable(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("immutable") || parsed.tags.contains_key("psalm-immutable")
    }

    fn is_docblock_external_mutation_free(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("external-mutation-free")
            || parsed.tags.contains_key("psalm-external-mutation-free")
            || parsed.tags.contains_key("phpstan-external-mutation-free")
    }

    fn is_docblock_final(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("final") || parsed.tags.contains_key("psalm-final")
    }

    fn is_docblock_consistent_constructor(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("consistent-constructor")
            || parsed.tags.contains_key("psalm-consistent-constructor")
    }

    fn is_docblock_consistent_templates(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("consistent-templates")
            || parsed.tags.contains_key("psalm-consistent-templates")
    }

    fn is_docblock_no_seal_properties(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("no-seal-properties")
            || parsed.tags.contains_key("psalm-no-seal-properties")
    }

    fn get_docblock_sealed_properties(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<bool> {
        if parsed.tags.contains_key("seal-properties")
            || parsed.tags.contains_key("psalm-seal-properties")
        {
            return Some(true);
        }

        if parsed.tags.contains_key("no-seal-properties")
            || parsed.tags.contains_key("psalm-no-seal-properties")
        {
            return Some(false);
        }

        None
    }

    fn get_docblock_sealed_methods(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<bool> {
        if parsed.tags.contains_key("seal-methods")
            || parsed.tags.contains_key("psalm-seal-methods")
        {
            return Some(true);
        }

        if parsed.tags.contains_key("no-seal-methods")
            || parsed.tags.contains_key("psalm-no-seal-methods")
        {
            return Some(false);
        }

        None
    }

    fn is_docblock_deprecated(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("deprecated")
    }

    fn get_docblock_deprecation_message(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<String> {
        let deprecated_tags = parsed.tags.get("deprecated")?;
        let mut ordered_tags: Vec<_> = deprecated_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        ordered_tags.into_iter().find_map(|(_, content)| {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    fn get_docblock_internal_scopes(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
        defining_symbol: StrId,
        docblock_issues: &mut Vec<DocblockIssue>,
    ) -> Vec<StrId> {
        let mut scopes = Vec::new();

        if let Some(psalm_internal_tags) = parsed.tags.get("psalm-internal") {
            let mut ordered_tags: Vec<_> = psalm_internal_tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (offset, content) in ordered_tags {
                let normalized = content.trim().trim_start_matches('\\').trim();
                if normalized.is_empty() {
                    docblock_issues.push(DocblockIssue {
                        message: "psalm-internal annotation used without specifying namespace"
                            .to_string(),
                        start_offset: (*offset) as u32,
                        end_offset: (*offset) as u32 + 1,
                    });
                    continue;
                }

                let scope_id = self.interner.intern(normalized);
                if !scopes.contains(&scope_id) {
                    scopes.push(scope_id);
                }
            }

            if !scopes.is_empty() {
                return scopes;
            }
        }

        if parsed.tags.contains_key("internal") {
            let default_scope = self.get_default_internal_scope(defining_symbol);
            if !scopes.contains(&default_scope) {
                scopes.push(default_scope);
            }
        }

        scopes
    }

    fn get_default_internal_scope(&self, defining_symbol: StrId) -> StrId {
        let symbol = self.interner.lookup(defining_symbol);
        let normalized = symbol.trim_start_matches('\\');
        let namespace = normalized.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
        let top_level_namespace = namespace.split('\\').next().unwrap_or("");

        if top_level_namespace.is_empty() {
            StrId::EMPTY
        } else {
            self.interner.intern(top_level_namespace)
        }
    }

    fn has_attribute_named(
        &mut self,
        attribute_lists: &Sequence<'_, AttributeList<'_>>,
        expected_name: &str,
    ) -> bool {
        attribute_lists.iter().any(|attribute_list| {
            attribute_list.attributes.iter().any(|attribute| {
                let attribute_name = self.resolve_identifier(&attribute.name);
                let attribute_name = self.interner.lookup(attribute_name);
                let short_name = attribute_name
                    .as_ref()
                    .rsplit('\\')
                    .next()
                    .unwrap_or(attribute_name.as_ref());

                short_name.eq_ignore_ascii_case(expected_name)
            })
        })
    }

    fn get_attribute_flags(
        &mut self,
        class_like_name: StrId,
        attribute_lists: &Sequence<'_, AttributeList<'_>>,
    ) -> Option<u8> {
        let class_like_name = self.interner.lookup(class_like_name);
        let class_like_short_name = class_like_name
            .as_ref()
            .rsplit('\\')
            .next()
            .unwrap_or(class_like_name.as_ref());

        // Attribute itself can always be used on classes.
        if class_like_short_name.eq_ignore_ascii_case("Attribute") {
            return Some(1);
        }

        for attribute in attribute_lists
            .iter()
            .flat_map(|attribute_list| attribute_list.attributes.iter())
        {
            let attribute_name = self.resolve_identifier(&attribute.name);
            let attribute_name = self.interner.lookup(attribute_name);
            let attribute_short_name = attribute_name
                .as_ref()
                .rsplit('\\')
                .next()
                .unwrap_or(attribute_name.as_ref());

            if !attribute_short_name.eq_ignore_ascii_case("Attribute") {
                continue;
            }

            let Some(first_argument) = attribute
                .argument_list
                .as_ref()
                .and_then(|argument_list| argument_list.arguments.first())
            else {
                // No target specified means all targets.
                return Some(63);
            };

            let bits = self
                .eval_attribute_flag_expression(first_argument.value())
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(127);

            return Some(bits);
        }

        None
    }

    fn eval_attribute_flag_expression(&mut self, expr: &Expression<'_>) -> Option<i64> {
        match expr.unparenthesized() {
            Expression::Literal(Literal::Integer(integer)) => {
                integer.value.and_then(|v| i64::try_from(v).ok())
            }
            Expression::Binary(binary) => {
                let left = self.eval_attribute_flag_expression(binary.lhs)?;
                let right = self.eval_attribute_flag_expression(binary.rhs)?;

                match binary.operator {
                    BinaryOperator::BitwiseOr(_) => Some(left | right),
                    BinaryOperator::BitwiseAnd(_) => Some(left & right),
                    BinaryOperator::BitwiseXor(_) => Some(left ^ right),
                    _ => None,
                }
            }
            Expression::Access(Access::ClassConstant(class_constant_access)) => {
                let class_name = match class_constant_access.class.unparenthesized() {
                    Expression::Identifier(identifier) => {
                        let resolved = self.resolve_identifier(identifier);
                        self.interner.lookup(resolved).to_string()
                    }
                    _ => return None,
                };

                let class_short_name = class_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(class_name.as_str());
                if !class_short_name.eq_ignore_ascii_case("Attribute") {
                    return None;
                }

                let ClassLikeConstantSelector::Identifier(constant_name) =
                    &class_constant_access.constant
                else {
                    return None;
                };

                match constant_name.value.to_ascii_uppercase().as_str() {
                    "TARGET_CLASS" => Some(1),
                    "TARGET_FUNCTION" => Some(2),
                    "TARGET_METHOD" => Some(4),
                    "TARGET_PROPERTY" => Some(8),
                    "TARGET_CLASS_CONSTANT" => Some(16),
                    "TARGET_PARAMETER" => Some(32),
                    "TARGET_ALL" => Some(63),
                    "IS_REPEATABLE" => Some(64),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn push_docblock_issue(
        &self,
        class_info: &mut ClassLikeInfo,
        message: String,
        start_offset: u32,
        end_offset: u32,
    ) {
        class_info.docblock_issues.push(DocblockIssue {
            message,
            start_offset,
            end_offset,
        });
    }

    fn validate_property_docblock_tags(
        &self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        property_start: u32,
    ) {
        if parsed.tags.contains_key("property")
            || parsed.tags.contains_key("psalm-property")
            || parsed.tags.contains_key("phpstan-property")
            || parsed.tags.contains_key("property-read")
            || parsed.tags.contains_key("psalm-property-read")
            || parsed.tags.contains_key("phpstan-property-read")
            || parsed.tags.contains_key("property-write")
            || parsed.tags.contains_key("psalm-property-write")
            || parsed.tags.contains_key("phpstan-property-write")
            || parsed.tags.contains_key("method")
            || parsed.tags.contains_key("psalm-method")
            || parsed.tags.contains_key("mixin")
            || parsed.tags.contains_key("psalm-mixin")
            || parsed.tags.contains_key("phpstan-mixin")
        {
            self.push_docblock_issue(
                class_info,
                "Invalid docblock annotation on property".to_string(),
                property_start,
                property_start.saturating_add(1),
            );
        }
    }

    fn validate_type_alias_docblock_tags(
        &self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        start_offset: u32,
    ) {
        for key in ["phpstan-type", "psalm-type"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some((_, type_definition)) = parse_type_alias_tag_content(content) else {
                    self.push_docblock_issue(
                        class_info,
                        "Invalid type alias in docblock".to_string(),
                        start_offset,
                        start_offset.saturating_add(1),
                    );
                    continue;
                };

                if !has_balanced_type_delimiters(&type_definition) {
                    self.push_docblock_issue(
                        class_info,
                        "Invalid type alias in docblock".to_string(),
                        start_offset,
                        start_offset.saturating_add(1),
                    );
                }
            }
        }
    }

    /// Psalm FunctionLikeNodeScanner: "Param X of C::m should be documented
    /// as a param or a property, not both" — a promoted property carrying its
    /// own `/** @var */` while the function docblock also documents it via
    /// `@param`.
    fn check_promoted_property_duplicate_docs(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        params: &[ParamInfo],
        class_name: StrId,
        method_name: StrId,
        start_offset: u32,
        issues: &mut Vec<DocblockIssue>,
    ) {
        for param in params {
            if !param.is_promoted || !param.has_docblock_type {
                continue;
            }
            let param_name = self.interner.lookup(param.name);
            let dollar_name = format!("${}", param_name.trim_start_matches('$'));
            let param_tag_type = ["param", "psalm-param", "phpstan-param"]
                .iter()
                .filter_map(|key| parsed.tags.get(*key))
                .flat_map(|tags| tags.values())
                .find(|content| {
                    content
                        .split_whitespace()
                        .any(|word| word.trim_end_matches(',') == dollar_name)
                })
                .and_then(|content| crate::docblock::extract_type_string_from_content(content))
                .and_then(|type_str| {
                    crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).ok()
                });
            let Some(param_tag_type) = param_tag_type else {
                continue;
            };
            // Psalm marks the @param docblock type from_docblock=false when
            // every atomic loose-key-matches a signature atomic (e.g.
            // `array{...}` over a native `array` hint) — the duplicate-doc
            // check then skips it.
            let documented_as_param = match param.signature_type.as_ref() {
                Some(signature_type) => {
                    let signature_keys: Vec<String> = signature_type
                        .types
                        .iter()
                        .map(|atomic| loose_atomic_key(self.interner.parent_ref(), atomic))
                        .collect();
                    !param_tag_type.types.iter().all(|atomic| {
                        signature_keys.contains(&loose_atomic_key(self.interner.parent_ref(), atomic))
                    })
                }
                None => true,
            };
            if documented_as_param {
                issues.push(DocblockIssue {
                    message: format!(
                        "Param {} of {}::{} should be documented as a param or a property, not both",
                        param_name.trim_start_matches('$'),
                        self.interner.lookup(class_name),
                        self.interner.lookup(method_name)
                    ),
                    start_offset,
                    end_offset: start_offset.saturating_add(1),
                });
            }
        }
    }

    fn validate_function_docblock_type_tags(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        start_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        param_names: &[StrId],
        issues: &mut Vec<DocblockIssue>,
    ) {
        let mut typed_param_tags = FxHashSet::default();

        // Psalm FunctionLikeDocblockParser: @psalm-taint-escape with no
        // argument is an InvalidDocblock.
        if let Some(tags) = parsed.tags.get("psalm-taint-escape") {
            for content in tags.values() {
                if content.trim().is_empty() {
                    issues.push(DocblockIssue {
                        message: "@psalm-taint-escape expects 1 argument".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                }
            }
        }

        if parsed.tags.contains_key("var")
            || parsed.tags.contains_key("psalm-var")
            || parsed.tags.contains_key("phpstan-var")
            || parsed.tags.contains_key("import-type")
            || parsed.tags.contains_key("psalm-import-type")
            || parsed.tags.contains_key("phpstan-import-type")
        {
            issues.push(DocblockIssue {
                message: "Possibly invalid docblock tag".to_string(),
                start_offset,
                end_offset: start_offset.saturating_add(1),
            });
        }

        for key in [
            "param",
            "psalm-param",
            "phpstan-param",
            "param-out",
            "psalm-param-out",
            "phpstan-param-out",
        ] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            let mut seen_vars = FxHashSet::default();
            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    continue;
                }

                let Some(var_name) = crate::docblock::extract_var_name_from_content(content) else {
                    continue;
                };

                let normalized = var_name.trim_start_matches('$').to_string();
                if !seen_vars.insert(normalized) {
                    issues.push(DocblockIssue {
                        message: "Invalid docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    break;
                }
            }
        }

        for key in ["return", "psalm-return", "phpstan-return"] {
            if parsed.tags.get(key).is_some_and(|tags| tags.len() > 1) {
                issues.push(DocblockIssue {
                    message: "Invalid docblock type".to_string(),
                    start_offset,
                    end_offset: start_offset.saturating_add(1),
                });
            }
        }

        for key in ["param", "param-out"] {
            let Some(tags) = parsed.combined_tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    continue;
                }

                if let Some(var_name) = crate::docblock::extract_var_name_from_content(content) {
                    typed_param_tags.insert(var_name.trim_start_matches('$').to_string());
                }
            }
        }

        for key in ["param", "return", "param-out"] {
            let Some(tags) = parsed.combined_tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    if key == "return" {
                        if content.trim().eq_ignore_ascii_case("$this") {
                            continue;
                        }

                        issues.push(DocblockIssue {
                            message: "Missing return docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                    }

                    if key == "param" || key == "param-out" {
                        if let Some(var_name) =
                            crate::docblock::extract_var_name_from_content(content)
                            && typed_param_tags.contains(var_name.trim_start_matches('$')) {
                                continue;
                            }

                        // Psalm's FunctionLikeDocblockParser skips a var-only
                        // `@param $x` outright (allowEmptyVarAnnotation).
                        let trimmed = content.trim();
                        if trimmed.starts_with('$')
                            && !trimmed.contains(char::is_whitespace)
                        {
                            continue;
                        }
                    }

                    let trimmed = content.trim();
                    if trimmed.starts_with('$') && !trimmed.eq_ignore_ascii_case("$this") {
                        issues.push(DocblockIssue {
                            message: "Missing docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                    }
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    if key == "return" {
                        issues.push(DocblockIssue {
                            message: "Missing return docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                        continue;
                    }

                    if (key == "param" || key == "param-out")
                        && let Some(var_name) =
                            crate::docblock::extract_var_name_from_content(content)
                            && typed_param_tags.contains(var_name.trim_start_matches('$')) {
                                continue;
                            }

                    issues.push(DocblockIssue {
                        message: "Missing docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                if (key == "param" || key == "param-out")
                    && crate::docblock::extract_var_name_from_content(content).is_none()
                {
                    issues.push(DocblockIssue {
                        message: "Invalid docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner.parent_ref()).unwrap_or_else(|_| TUnion::mixed());
                let resolved_union = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let resolved_type = self.expand_docblock_class_constant_wildcards(
                    resolved_union,
                    self_class,
                    parent_class,
                    class_constants,
                );

                if union_has_invalid_class_string_targets(&resolved_type) {
                    issues.push(DocblockIssue {
                        message: "class-string param can only target object-like types".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                if !self.is_valid_docblock_type_string(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                    param_names,
                ) {
                    issues.push(DocblockIssue {
                        message: if key == "return" {
                            "Invalid return docblock type".to_string()
                        } else {
                            "Invalid docblock type".to_string()
                        },
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                }
            }
        }
    }

    fn is_valid_docblock_type_string(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        param_names: &[StrId],
    ) -> bool {
        // The type parser is the source of truth for syntactic validity: a parse
        // error is the equivalent of Psalm's TypeParseTreeException. The string
        // helpers below remain as belt-and-suspenders until their checks are
        // fully folded into the parser's Err paths.
        //
        // Parameter conditionals (`($x is T ? A : B)`) need the enclosing
        // function's parameter names so the parser can recognise `$x` (mirroring
        // Psalm's getConditionalSanitizedTypeTokens), so feed them via the context.
        let mut context = TypeResolutionContext::new();
        context.param_names = param_names.to_vec();
        let parsed_type =
            match crate::docblock::parse_type_string_with_context(type_str, self.interner.parent_ref(), &context)
            {
                Ok(parsed) => parsed,
                Err(_) => return false,
            };

        if has_invalid_docblock_type_syntax(type_str) {
            return false;
        }

        if !has_balanced_type_delimiters(type_str) {
            return false;
        }

        if !has_valid_int_range_bounds(type_str) {
            return false;
        }

        if !has_valid_docblock_utility_type_arity(type_str) {
            return false;
        }

        if !has_valid_docblock_class_constant_syntax(type_str) {
            return false;
        }

        let resolved_union =
            self.resolve_docblock_union_type(parsed_type, self_class, parent_class, template_map);
        let resolved_type = self.expand_docblock_class_constant_wildcards(
            resolved_union,
            self_class,
            parent_class,
            class_constants,
        );
        union_has_valid_array_keys(&resolved_type) && !has_invalid_hyphenated_named_type(type_str)
    }

    fn is_docblock_override_method_visibility(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("override-method-visibility")
            || parsed.tags.contains_key("psalm-override-method-visibility")
    }

    fn is_docblock_override_property_visibility(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("override-property-visibility")
            || parsed
                .tags
                .contains_key("psalm-override-property-visibility")
    }

    fn is_docblock_readonly(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("readonly")
            || parsed.tags.contains_key("psalm-readonly")
            || parsed.tags.contains_key("readonly-allow-private-mutation")
            || parsed
                .tags
                .contains_key("psalm-readonly-allow-private-mutation")
    }

    fn is_docblock_readonly_allow_private_mutation(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("readonly-allow-private-mutation")
            || parsed
                .tags
                .contains_key("psalm-readonly-allow-private-mutation")
            || parsed.tags.contains_key("allow-private-mutation")
            || parsed.tags.contains_key("psalm-allow-private-mutation")
    }

    fn add_old_style_constructor_alias(&self, class_info: &mut ClassLikeInfo) {
        if class_info.methods.contains_key(&StrId::CONSTRUCT) {
            return;
        }

        let class_name = self.interner.lookup(class_info.name);
        if class_name.contains('\\') {
            return;
        }

        let class_name_lc = class_name.to_ascii_lowercase();
        let old_constructor_id = class_info.methods.keys().find_map(|method_id| {
            let method_name = self.interner.lookup(*method_id);
            if method_name.eq_ignore_ascii_case(class_name_lc.as_ref()) {
                Some(*method_id)
            } else {
                None
            }
        });

        let Some(old_constructor_id) = old_constructor_id else {
            return;
        };

        let Some(constructor_info) = class_info.methods.get(&old_constructor_id).cloned() else {
            return;
        };

        // Methods with explicit signature return types are normal methods in modern PHP.
        // Do not reinterpret them as old-style constructors.
        if constructor_info.signature_return_type.is_some() {
            return;
        }

        class_info
            .methods
            .insert(StrId::CONSTRUCT, constructor_info);
    }

    /// Resolve a type hint using the type resolver.
    fn resolve_type(
        &mut self,
        hint: &Hint<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> TUnion {
        resolve_hint(
            hint,
            self.interner.parent_ref(),
            self.current_namespace,
            self_class,
            parent_class,
            Some(&self.use_aliases),
            None,
        )
    }

    /// Create a fully qualified name from a local name.
    /// Resolve a raw class identifier string against the current namespace
    /// and use-aliases without interning (usable from `Fn` closures).
    fn resolve_scanned_class_string(&self, raw: &str) -> String {
        let raw = raw.trim();
        if let Some(stripped) = raw.strip_prefix('\\') {
            return stripped.to_string();
        }
        let (first_segment, remainder) = match raw.split_once('\\') {
            Some((first, rest)) => (first, Some(rest)),
            None => (raw, None),
        };
        if let Some(alias_target) = self.use_aliases.get(&first_segment.to_ascii_lowercase()) {
            let alias = self.interner.lookup(*alias_target);
            return match remainder {
                Some(rest) => format!("{}\\{}", alias, rest),
                None => alias.to_string(),
            };
        }
        match self.current_namespace {
            Some(ns) => format!("{}\\{}", self.interner.lookup(ns), raw),
            None => raw.to_string(),
        }
    }

    /// Resolve `Enum::CASE->name` / `->value` against already-scanned enum
    /// declarations (Psalm's ConstantTypeResolver EnumName/EnumValue fetch).
    fn resolve_scanned_enum_case(
        &self,
        class_name: &str,
        case_name: &str,
        wants_name: bool,
    ) -> Option<TUnion> {
        let class_info = self.declarations.classes.iter().find(|class_info| {
            self.interner.lookup(class_info.name).as_ref() == class_name
        })?;
        if class_info.kind != ClassLikeKind::Enum {
            return None;
        }
        let const_info = class_info.constants.values().find(|const_info| {
            self.interner.lookup(const_info.name).as_ref() == case_name
        })?;
        if !matches!(
            const_info.constant_type.get_single(),
            Some(TAtomic::TEnumCase { .. })
        ) {
            return None;
        }
        if wants_name {
            Some(TUnion::new(TAtomic::TLiteralString {
                value: case_name.to_string(),
            }))
        } else {
            const_info.enum_case_value.clone()
        }
    }

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
            let value = ident.value();
            let (first_segment, remainder) = match value.split_once('\\') {
                Some((first, rest)) => (first, Some(rest)),
                None => (value, None),
            };

            if let Some(alias_target) = self.use_aliases.get(&first_segment.to_ascii_lowercase()) {
                if let Some(remainder) = remainder {
                    let alias_str = self.interner.lookup(*alias_target);
                    return self
                        .interner
                        .intern(&format!("{}\\{}", alias_str, remainder));
                }

                return *alias_target;
            }

            self.make_fqn(value)
        }
    }
}

// Helper functions that don't need self

#[derive(Default)]
struct ParsedFunctionAssertions {
    assertions: Vec<Assertion>,
    if_true_assertions: Vec<Assertion>,
    if_false_assertions: Vec<Assertion>,
}

fn extract_direct_var(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Variable(variable) => match variable {
            mago_syntax::ast::ast::variable::Variable::Direct(direct) => {
                Some(direct.name.to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_null_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Literal(mago_syntax::ast::ast::literal::Literal::Null(_))
    )
}

fn normalize_use_name(name: &str) -> String {
    name.strip_prefix('\\').unwrap_or(name).to_string()
}

/// Whether a `@param` docblock declares its parameter variadic (`type ...$name`),
/// mirroring Psalm's `$docblock_param_variadic` (CommentAnalyzer checks the
/// name token for a `...` prefix).
fn docblock_param_is_variadic(content: &str) -> bool {
    let mut depth: u32 = 0;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                return content[..idx].ends_with("...");
            }
            _ => {}
        }
    }

    false
}

/// The value type of the array atomic in a docblock union, mirroring Psalm's
/// `$new_param_type->getArray()` unwrap for variadic params (`TKeyedArray`
/// falls back to its generic value type).
fn docblock_array_value_type(docblock_type: &TUnion) -> Option<TUnion> {
    for atomic in &docblock_type.types {
        match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => {
                return Some((**value_type).clone());
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                let mut value_type: Option<TUnion> = None;
                for property_type in properties.values() {
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, property_type, false),
                        None => property_type.clone(),
                    });
                }
                if let Some(fallback) = fallback_value_type {
                    value_type = Some(match value_type {
                        Some(existing) => combine_union_types(&existing, fallback, false),
                        None => (**fallback).clone(),
                    });
                }
                return value_type;
            }
            _ => {}
        }
    }

    None
}

fn extract_param_name_from_content(content: &str) -> Option<&str> {
    let mut depth: u32 = 0;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                let start = idx + 1;
                let mut end = start;

                for (name_idx, name_ch) in content[start..].char_indices() {
                    if name_ch.is_ascii_alphanumeric() || name_ch == '_' {
                        end = start + name_idx + name_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                if end > start {
                    return Some(&content[start..end]);
                }

                return None;
            }
            _ => {}
        }
    }

    None
}

fn split_docblock_method_params(params: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for ch in params.chars() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }
            _ => {}
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }

    parts
}

fn find_docblock_method_signature_bounds(signature: &str) -> Option<(usize, usize)> {
    let mut stack = Vec::new();
    let mut pairs = Vec::new();

    for (idx, ch) in signature.char_indices() {
        match ch {
            '(' => stack.push(idx),
            ')' => {
                let open = stack.pop()?;
                pairs.push((open, idx));
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        return None;
    }

    for (open, close) in pairs.into_iter().rev() {
        let before_paren = signature[..open].trim();
        let Some((_, method_name)) = split_method_name(before_paren) else {
            continue;
        };

        if !is_valid_docblock_method_name(method_name) {
            continue;
        }

        let tail = signature[close + 1..].trim_start();
        if tail.contains(')') {
            continue;
        }

        return Some((open, close));
    }

    None
}

fn split_method_name(before_paren: &str) -> Option<(&str, &str)> {
    let mut method_start = None;

    for (idx, ch) in before_paren.char_indices().rev() {
        if ch.is_ascii_whitespace() {
            method_start = Some(idx + ch.len_utf8());
            break;
        }
    }

    let method_start = method_start.unwrap_or(0);
    let method_name = before_paren[method_start..].trim_start_matches('&').trim();
    if method_name.is_empty() {
        return None;
    }

    let return_part = before_paren[..method_start].trim();
    Some((return_part, method_name))
}

fn is_valid_docblock_method_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Like [`take_first_docblock_type_token`], but a union/intersection
/// continues across whitespace around `|`/`&` (Psalm's `splitDocLine`
/// behavior: `Left<E> | Right<A>` is one part). The joining whitespace is
/// dropped from the returned token.
fn take_first_docblock_union_token(content: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                let prev = out.chars().last();
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                let next = chars.get(j).copied();
                if matches!(prev, Some('|') | Some('&'))
                    || matches!(next, Some('|') | Some('&'))
                {
                    i = j;
                    continue;
                }
                break;
            }
            _ => {}
        }
        out.push(ch);
        i += 1;
    }

    out
}

/// Whether an `array{...}` shape definition repeats a key at the same nesting
/// level (Psalm's TypeParser "Duplicate key" error).
fn shape_has_duplicate_keys(definition: &str) -> bool {
    let mut seen_stack: Vec<rustc_hash::FxHashSet<String>> = Vec::new();
    let mut word = String::new();
    let mut in_quote: Option<char> = None;
    let chars: Vec<char> = definition.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(quote) = in_quote {
            if ch == quote {
                in_quote = None;
            } else {
                word.push(ch);
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' | '"' => in_quote = Some(ch),
            '{' => {
                seen_stack.push(rustc_hash::FxHashSet::default());
                word.clear();
            }
            '}' => {
                seen_stack.pop();
                word.clear();
            }
            ':' => {
                // `::` is a class-constant separator, not a shape key.
                if chars.get(i + 1) == Some(&':') {
                    i += 2;
                    word.clear();
                    continue;
                }
                let key = word.trim().trim_end_matches('?').to_string();
                word.clear();
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    && let Some(seen) = seen_stack.last_mut()
                    && !seen.insert(key)
                {
                    return true;
                }
            }
            ',' => word.clear(),
            c if c.is_whitespace() => {}
            _ => word.push(ch),
        }
        i += 1;
    }
    false
}

/// Whether a type alias definition references its own name (Psalm's
/// circular-reference rejection).
fn definition_references_alias(definition: &str, alias_name: &str) -> bool {
    let bytes = definition.as_bytes();
    let is_name_char =
        |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'\\' || b == b'-';
    let mut search_start = 0;
    while let Some(found) = definition[search_start..].find(alias_name) {
        let start = search_start + found;
        let end = start + alias_name.len();
        let prev_ok = start == 0 || !is_name_char(bytes[start - 1]);
        let next_ok = end >= bytes.len() || !is_name_char(bytes[end]);
        if prev_ok && next_ok {
            return true;
        }
        search_start = end;
    }
    false
}

fn take_first_docblock_type_token(content: &str) -> &str {
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            c if c.is_whitespace()
                && angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                return content[..idx].trim();
            }
            _ => {}
        }
    }

    content.trim()
}

fn resolve_key_of_template_union(union: &TUnion) -> TUnion {
    let mut key_union = TUnion::nothing();

    for atomic in &union.types {
        let atomic_key_union = resolve_key_of_template_atomic(atomic);
        key_union = if key_union.is_nothing() {
            atomic_key_union
        } else {
            combine_union_types(&key_union, &atomic_key_union, false)
        };
    }

    if key_union.is_nothing() {
        TUnion::array_key()
    } else {
        key_union
    }
}

fn resolve_key_of_template_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { key_type, .. }
        | TAtomic::TNonEmptyArray { key_type, .. }
        | TAtomic::TIterable { key_type, .. } => (**key_type).clone(),
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => TUnion::int(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            ..
        } => {
            let mut key_union = fallback_key_type
                .as_ref()
                .map(|key_type| (**key_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for key in properties.keys() {
                let key_atomic = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TAtomic::TLiteralInt { value: *value }
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value) => TAtomic::TLiteralString {
                        value: value.clone(),
                    },
                };

                key_union = if key_union.is_nothing() {
                    TUnion::new(key_atomic)
                } else {
                    combine_union_types(&key_union, &TUnion::new(key_atomic), false)
                };
            }

            if key_union.is_nothing() {
                TUnion::array_key()
            } else {
                key_union
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_key_of_template_union(as_type),
        _ => TUnion::array_key(),
    }
}

fn resolve_value_of_template_union(union: &TUnion) -> TUnion {
    let mut value_union = TUnion::nothing();

    for atomic in &union.types {
        let atomic_value_union = resolve_value_of_template_atomic(atomic);
        value_union = if value_union.is_nothing() {
            atomic_value_union
        } else {
            combine_union_types(&value_union, &atomic_value_union, false)
        };
    }

    if value_union.is_nothing() {
        TUnion::mixed()
    } else {
        value_union
    }
}

fn resolve_value_of_template_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TIterable { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => (**value_type).clone(),
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut value_union = fallback_value_type
                .as_ref()
                .map(|value_type| (**value_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for property_value in properties.values() {
                value_union = if value_union.is_nothing() {
                    property_value.clone()
                } else {
                    combine_union_types(&value_union, property_value, false)
                };
            }

            if value_union.is_nothing() {
                TUnion::mixed()
            } else {
                value_union
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_value_of_template_union(as_type),
        _ => TUnion::mixed(),
    }
}

fn parse_template_tag_content(content: &str) -> Option<(String, Option<String>)> {
    // The template name and the `as`/`of`/`super` modifier live on the tag's
    // first line; the bound type itself may continue across lines (Psalm
    // concatenates the docblock tag's lines and `splitDocLine` extracts the
    // type token, so `@template T of array{` + ` a: string }` works while
    // trailing prose is dropped).
    let first_line = content.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }

    let mut parts = first_line.split_whitespace();
    let template_name = parts.next()?.trim_matches(',');
    if template_name.is_empty() {
        return None;
    }

    let modifier = parts.next().map(|word| word.to_ascii_lowercase());
    if matches!(modifier.as_deref(), Some("as" | "of" | "super")) {
        // Rebuild the bound from the FULL tag content (whitespace-joined
        // across lines), then keep the first depth-aware type token —
        // whitespace inside brackets/braces/parens belongs to the type,
        // whitespace at depth zero starts the free-text description.
        let joined = content.split_whitespace().collect::<Vec<&str>>().join(" ");
        let after_name = joined
            .split_whitespace()
            .skip(2)
            .collect::<Vec<&str>>()
            .join(" ");
        let bound = split_first_doc_type_token(&after_name);
        if !bound.trim().is_empty() {
            return Some((template_name.to_string(), Some(bound)));
        }
    }

    Some((template_name.to_string(), None))
}

/// Psalm `CommentAnalyzer::splitDocLine`'s leading-token extraction: returns
/// the prefix of `text` up to the first whitespace that sits outside any
/// `<>`/`{}`/`()`/`[]` brackets and quotes — the type token, with any
/// trailing description dropped.
fn split_first_doc_type_token(text: &str) -> String {
    let text = text.trim();
    let mut depth: u32 = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for (index, ch) in text.char_indices() {
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => in_quote = Some(ch),
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth = depth.saturating_sub(1),
            _ if ch.is_whitespace() && depth == 0 => {
                return text[..index].to_string();
            }
            _ => {}
        }
    }

    text.to_string()
}

/// Psalm's `FunctionLikeDocblockScanner`: a docblock `@return` whose atomics
/// all match the native signature return's atomics by `Atomic::getKey` — all
/// array-ish types share the key `array`, named objects key by class, generic
/// objects/literals get distinct keys — is runtime-backed, so the union loses
/// `from_docblock`. Redundancies against such a type then report as plain
/// RedundantCondition instead of RedundantConditionGivenDocblockType.
pub(crate) fn clear_docblock_flag_when_signature_backed(
    return_type: Option<&mut TUnion>,
    signature_return_type: Option<&TUnion>,
) {
    let (Some(return_type), Some(signature_return_type)) = (return_type, signature_return_type)
    else {
        return;
    };

    let signature_keys: rustc_hash::FxHashSet<String> = signature_return_type
        .types
        .iter()
        .map(psalm_signature_match_key)
        .collect();

    let all_typehint_types_match = return_type
        .types
        .iter()
        .all(|atomic| signature_keys.contains(&psalm_signature_match_key(atomic)));

    if all_typehint_types_match {
        return_type.from_docblock = false;
    }

    // Psalm FunctionLikeDocblockScanner: "if the signature type contains
    // null, we add null into the final return type too" — a `?array`
    // signature with an `@return array{…}` docblock yields `array{…}|null`.
    // Guarded by the docblock type matching the signature (Psalm uses
    // isContainedBy; the scan-time family containment is the local proxy) or
    // containing an object (the #6931 can't-check-yet concession).
    if signature_return_type.is_nullable()
        && !return_type.is_nullable()
        && !return_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. } | TAtomic::TConditional(_)))
    {
        let has_object_type = return_type.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TNamedObject { .. }
                    | TAtomic::TObject
                    | TAtomic::TObjectWithProperties { .. }
                    | TAtomic::TObjectIntersection { .. }
            )
        });
        let contained_by_signature = return_type.types.iter().all(|atomic| {
            signature_keys.contains(&psalm_signature_match_key(atomic))
                || loose_scalar_family(atomic)
                    .is_some_and(|family| signature_keys.contains(family))
        });
        if has_object_type || contained_by_signature {
            return_type.add_type(TAtomic::TNull);
        }
    }
}

/// The native-signature family a derived/literal scalar atomic is contained
/// by — the scan-time proxy for `isContainedBy` in the docblock-nullability
/// merge above (`@return 'a'|'b'` is contained by a `?string` signature).
fn loose_scalar_family(atomic: &TAtomic) -> Option<&'static str> {
    Some(match atomic {
        TAtomic::TLiteralString { .. }
        | TAtomic::TNonEmptyString
        | TAtomic::TTruthyString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TNumericString
        | TAtomic::TNonEmptyNumericString
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. } => "string",
        TAtomic::TLiteralInt { .. } | TAtomic::TIntRange { .. } => "int",
        TAtomic::TLiteralFloat { .. } => "float",
        TAtomic::TTrue | TAtomic::TFalse => "bool",
        _ => return None,
    })
}

/// `Atomic::getKey` to the extent native signature types can express it:
/// scalars/array/object families collapse to their family key; everything a
/// signature cannot say (literals, generics, ranges) keys distinctly via the
/// debug form so it never matches.
fn psalm_signature_match_key(atomic: &TAtomic) -> String {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => "array".to_string(),
        TAtomic::TInt => "int".to_string(),
        TAtomic::TFloat => "float".to_string(),
        TAtomic::TString => "string".to_string(),
        TAtomic::TBool => "bool".to_string(),
        TAtomic::TTrue => "true".to_string(),
        TAtomic::TFalse => "false".to_string(),
        TAtomic::TNull => "null".to_string(),
        TAtomic::TObject => "object".to_string(),
        TAtomic::TNamedObject {
            name,
            type_params: None,
            ..
        } => format!("object-{:?}", name),
        _ => format!("{:?}", atomic),
    }
}

fn parse_type_alias_tag_content(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Psalm's `getTypeAliasesFromCommentLines` splits on spaces/`=` keeping
    // delimiters: the first token is the alias name, an optional `=` follows,
    // and the remainder is the type definition — both `@psalm-type Name = T`
    // and the space form `@psalm-type Name T` are valid.
    let split_at = trimmed.find([' ', '='])?;
    let (alias_name, rest) = trimmed.split_at(split_at);
    let alias_name = alias_name.trim();
    let mut type_definition = rest.trim_start();
    if let Some(after_equals) = type_definition.strip_prefix('=') {
        type_definition = after_equals.trim_start();
    }

    if alias_name.is_empty() || type_definition.is_empty() {
        return None;
    }

    // Psalm takes only the balanced type expression (splitDocLine[0]) — a
    // multi-line definition may be followed by description text in the same
    // tag content.
    let type_definition = crate::docblock::extract_type_string_from_content(type_definition)
        .unwrap_or(type_definition);

    Some((alias_name.to_string(), type_definition.to_string()))
}

fn is_missing_docblock_type(type_str: &str) -> bool {
    let trimmed = type_str.trim();
    // A bare `*` is leftover docblock decoration, not a type (Psalm's
    // sanitizeDocblockType strips it and then reports MissingDocblockType
    // via IncorrectDocblockException).
    (trimmed.starts_with('$') && !trimmed.eq_ignore_ascii_case("$this"))
        || (!trimmed.is_empty() && trimmed.chars().all(|c| c == '*'))
}

fn has_valid_docblock_utility_type_arity(type_str: &str) -> bool {
    // Mirrors Psalm's `TypeParser`: each utility's parameter arity.
    //
    // `int-mask<A, B, C, ...>` is variadic (one or more int/scalar-const members),
    // while `int-mask-of<T>` and the `properties-of` family take exactly one. The
    // search keys are distinct ("int-mask<" never matches inside "int-mask-of<"), so
    // each utility is validated independently. `min == 0` means "one or more".
    const UTILITY_ARITIES: [(&str, usize, usize); 6] = [
        ("properties-of", 1, 1),
        ("public-properties-of", 1, 1),
        ("protected-properties-of", 1, 1),
        ("private-properties-of", 1, 1),
        ("int-mask-of", 1, 1),
        ("int-mask", 1, usize::MAX),
    ];

    let lower = type_str.to_ascii_lowercase();

    for (utility, min_params, max_params) in UTILITY_ARITIES {
        let search = format!("{utility}<");
        let mut search_from = 0usize;

        while let Some(found) = lower[search_from..].find(&search) {
            let open_idx = search_from + found + utility.len();
            let Some(close_idx) = find_matching_angle_bracket(type_str, open_idx) else {
                return false;
            };

            let params = &type_str[open_idx + 1..close_idx];
            let count = count_top_level_generic_params(params);
            if count < min_params || count > max_params {
                return false;
            }

            search_from = close_idx + 1;
        }
    }

    true
}

fn find_matching_angle_bracket(input: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, ch) in input[open_idx..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_idx + idx);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }

    None
}

fn count_top_level_generic_params(params: &str) -> usize {
    let trimmed = params.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut count = 1usize;

    for ch in params.chars() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                count += 1;
            }
            _ => {}
        }
    }

    count
}

fn has_valid_docblock_class_constant_syntax(type_str: &str) -> bool {
    for part in split_docblock_union_parts(type_str) {
        if !class_constant_syntax_is_valid_in_part(part) {
            return false;
        }
    }

    true
}

fn class_constant_syntax_is_valid_in_part(part: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let bytes = part.as_bytes();
    let mut idx = 0usize;

    while idx + 1 < bytes.len() {
        let ch = bytes[idx] as char;

        if let Some(active_quote) = quote {
            if ch == '\\' && !escaped {
                escaped = true;
                idx += 1;
                continue;
            }

            if ch == active_quote && !escaped {
                quote = None;
            }

            escaped = false;
            idx += 1;
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            idx += 1;
            continue;
        }

        if ch == ':' && bytes[idx + 1] == b':' {
            let Some((class_part, const_part)) = extract_class_constant_parts(part, idx) else {
                return false;
            };

            if !is_valid_php_classlike_identifier(class_part) {
                return false;
            }

            if !const_part.eq_ignore_ascii_case("class") {
                if let Some(prefix) = const_part.strip_suffix('*') {
                    // A bare `*` (`Foo::*`, `self::*`) selects every constant
                    // of the class — the empty prefix is valid (Psalm).
                    if !prefix.is_empty() && !is_valid_php_const_identifier(prefix) {
                        return false;
                    }
                } else if !is_valid_php_const_identifier(const_part) {
                    return false;
                }
            }

            idx += 2;
            continue;
        }

        idx += 1;
    }

    true
}

fn extract_class_constant_parts(part: &str, separator_idx: usize) -> Option<(&str, &str)> {
    let left = part[..separator_idx].trim_end();
    let mut class_start = left.len();
    let left_bytes = left.as_bytes();

    while class_start > 0 {
        let ch = left_bytes[class_start - 1] as char;
        if is_class_name_char(ch) {
            class_start -= 1;
        } else {
            break;
        }
    }

    if class_start == left.len() {
        return None;
    }

    if class_start > 0 {
        let prev = left_bytes[class_start - 1] as char;
        if prev.is_ascii_alphanumeric() || prev == '_' || prev == '\\' || prev == '$' {
            return None;
        }
    }

    let class_part = &left[class_start..];
    if class_part.is_empty() || class_part.ends_with('\\') {
        return None;
    }

    let right = part[separator_idx + 2..].trim_start();
    let mut const_end = 0usize;
    for ch in right.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '*' {
            const_end += ch.len_utf8();
        } else {
            break;
        }
    }

    if const_end == 0 {
        return None;
    }

    let const_part = &right[..const_end];
    Some((class_part, const_part))
}

fn is_class_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '\\'
}

fn is_valid_php_classlike_identifier(name: &str) -> bool {
    let normalized = name.trim_start_matches('\\');
    if normalized.is_empty() {
        return false;
    }

    normalized
        .split('\\')
        .all(|segment| !segment.is_empty() && is_valid_php_const_identifier(segment))
}

fn split_docblock_union_parts(type_str: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in type_str.char_indices() {
        if let Some(active_quote) = quote {
            if ch == '\\' && !escaped {
                escaped = true;
                continue;
            }

            if ch == active_quote && !escaped {
                quote = None;
            }

            escaped = false;
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '|' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                parts.push(type_str[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }

    parts.push(type_str[start..].trim());
    parts
}

fn is_valid_php_const_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn has_invalid_hyphenated_named_type(type_str: &str) -> bool {
    // `non-empty-mixed` isn't in the tokenizer's reserved-word table but the
    // type parser accepts it (see type_parser.rs), so treat it as valid here.
    const EXTRA_VALID_HYPHENATED_TYPE_TOKENS: [&str; 1] = ["non-empty-mixed"];

    for part in split_docblock_union_parts(type_str) {
        let token = extract_docblock_base_type_token(part);

        // A quoted literal string type ('not-callable', "a-b") may contain
        // hyphens freely.
        if token.starts_with('\'') || token.starts_with('"') {
            continue;
        }

        if let Some(rest) = token.strip_prefix('-')
            && !rest.is_empty()
            && rest.chars().all(|ch| ch.is_ascii_digit())
        {
            continue;
        }

        if token.contains('-')
            && !crate::docblock::type_tokenizer::is_reserved_word_ignore_ascii_case(&token)
            && !EXTRA_VALID_HYPHENATED_TYPE_TOKENS.contains(&token.as_str())
        {
            return true;
        }
    }

    false
}

fn extract_docblock_base_type_token(part: &str) -> String {
    let trimmed = part
        .trim()
        .trim_start_matches('?')
        .trim_start_matches('(')
        .trim_end_matches(')');

    let mut end = trimmed.len();
    for (idx, ch) in trimmed.char_indices() {
        if matches!(
            ch,
            '<' | '(' | '[' | '{' | ':' | '&' | '|' | ',' | ' ' | '\t' | '\n' | '\r'
        ) {
            end = idx;
            break;
        }
    }

    trimmed[..end].trim().to_ascii_lowercase()
}

fn has_invalid_docblock_type_syntax(type_str: &str) -> bool {
    let trimmed = type_str.trim();

    if trimmed.is_empty()
        || trimmed == "[]"
        || trimmed == "()"
        || trimmed.starts_with('[')
        || trimmed.starts_with('|')
        || trimmed.starts_with('&')
        || trimmed.ends_with('|')
        || trimmed.ends_with('&')
        || trimmed.ends_with(',')
        || trimmed.ends_with(':')
    {
        return true;
    }

    if trimmed.contains(';')
        || trimmed.contains("||")
        || trimmed.contains("&&")
        || trimmed.contains("|&")
        || trimmed.contains("&|")
        || trimmed.contains(",,")
        || trimmed.starts_with("\\?")
        || trimmed.contains("array(")
        || trimmed.contains("list(")
        || trimmed.contains(":}")
        || trimmed.contains(":]")
    {
        return true;
    }

    false
}

fn has_balanced_type_delimiters(type_definition: &str) -> bool {
    let mut angle_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    for ch in type_definition.chars() {
        if let Some(active_quote) = quote_char {
            if ch == '\\' && !escaped {
                escaped = true;
                continue;
            }

            if ch == active_quote && !escaped {
                quote_char = None;
            }

            escaped = false;
            continue;
        }

        match ch {
            '\'' | '"' => quote_char = Some(ch),
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            _ => {}
        }

        if angle_depth < 0 || paren_depth < 0 || brace_depth < 0 || bracket_depth < 0 {
            return false;
        }
    }

    quote_char.is_none()
        && angle_depth == 0
        && paren_depth == 0
        && brace_depth == 0
        && bracket_depth == 0
}

fn union_has_valid_array_keys(union: &TUnion) -> bool {
    union.types.iter().all(atomic_has_valid_array_keys)
}

fn atomic_has_valid_array_keys(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => union_is_valid_array_key(key_type) && union_has_valid_array_keys(value_type),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_has_valid_array_keys(value_type)
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().all(union_has_valid_array_keys),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => atomic_has_valid_array_keys(as_type),
        TAtomic::TTemplateParam { as_type, .. } => union_has_valid_array_keys(as_type),
        _ => true,
    }
}

fn union_is_valid_array_key(union: &TUnion) -> bool {
    union.types.iter().all(|atomic| match atomic {
        TAtomic::TArrayKey
        | TAtomic::TInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TString
        | TAtomic::TNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TTruthyString
        | TAtomic::TCallableString
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TMixed
        | TAtomic::TNonEmptyMixed
        | TAtomic::TNothing
        | TAtomic::TTemplateParam { .. }
        | TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TNamedObject { .. } => true,
        // Psalm tolerates these in docblocks and reports access issues later.
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => true,
        _ => false,
    })
}

fn union_has_invalid_class_string_targets(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(atomic_has_invalid_class_string_target)
}

fn atomic_has_invalid_class_string_target(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => class_string_target_is_explicitly_invalid(as_type),
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params
            .iter()
            .any(union_has_invalid_class_string_targets),
        TAtomic::TTemplateParam { as_type, .. } => union_has_invalid_class_string_targets(as_type),
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_has_invalid_class_string_target)
        }
        _ => false,
    }
}

fn class_string_target_is_explicitly_invalid(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(class_string_target_is_explicitly_invalid),
        TAtomic::TObjectIntersection { types } => {
            let has_object_like = types.iter().any(|inner| {
                matches!(
                    inner,
                    TAtomic::TObject
                        | TAtomic::TNamedObject { .. }
                        | TAtomic::TTemplateParamClass { .. }
                        | TAtomic::TLiteralClassString { .. }
                )
            });
            let has_callable_like = types
                .iter()
                .any(|inner| matches!(inner, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }));

            has_callable_like && !has_object_like
        }
        _ => false,
    }
}

fn has_valid_int_range_bounds(type_str: &str) -> bool {
    let lower = type_str.to_ascii_lowercase();
    let mut offset = 0usize;

    while let Some(found) = lower[offset..].find("int<") {
        let int_start = offset + found;
        if int_start > 0 {
            let previous = lower.as_bytes()[int_start - 1];
            if previous.is_ascii_alphanumeric() || previous == b'_' || previous == b'\\' {
                offset = int_start + 4;
                continue;
            }
        }

        let range_start = int_start + 4;
        let mut depth = 1i32;
        let mut range_end: Option<usize> = None;

        for (idx, ch) in type_str[range_start..].char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        range_end = Some(range_start + idx);
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(range_end) = range_end else {
            return false;
        };

        if !is_valid_single_int_range(&type_str[range_start..range_end]) {
            return false;
        }

        offset = range_end + 1;
    }

    true
}

fn is_valid_single_int_range(range_content: &str) -> bool {
    let parts: Vec<&str> = range_content.split(',').map(str::trim).collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return false;
    }

    let Some(lower_bound) = parse_int_range_bound(parts[0], true) else {
        return false;
    };
    let Some(upper_bound) = parse_int_range_bound(parts[1], false) else {
        return false;
    };

    if let (Some(min), Some(max)) = (lower_bound, upper_bound) {
        return min <= max;
    }

    true
}

fn parse_int_range_bound(bound: &str, is_lower_bound: bool) -> Option<Option<i64>> {
    let lowered = bound.to_ascii_lowercase();
    if lowered == "min" {
        return if is_lower_bound { Some(None) } else { None };
    }
    if lowered == "max" {
        return if is_lower_bound { None } else { Some(None) };
    }

    bound.parse::<i64>().ok().map(Some)
}

fn parse_import_type_tag_content(content: &str) -> Option<(String, String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 3 || !parts[1].eq_ignore_ascii_case("from") {
        return None;
    }

    let imported_alias = parts[0].trim();
    let source_name = parts[2].trim();
    if imported_alias.is_empty() || source_name.is_empty() {
        return None;
    }

    let alias_name = if parts.len() >= 5 && parts[3].eq_ignore_ascii_case("as") {
        parts[4].trim()
    } else {
        imported_alias
    };

    if alias_name.is_empty() {
        return None;
    }

    Some((
        imported_alias.to_string(),
        source_name.to_string(),
        alias_name.to_string(),
    ))
}

fn parse_method_modifiers(
    modifiers: &Sequence<'_, Modifier<'_>>,
) -> (Visibility, bool, bool, bool) {
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

fn parse_visibility_modifier(modifier: &Modifier<'_>) -> Option<Visibility> {
    match modifier {
        Modifier::Public(_) => Some(Visibility::Public),
        Modifier::Protected(_) => Some(Visibility::Protected),
        Modifier::Private(_) => Some(Visibility::Private),
        _ => None,
    }
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

/// Whether `expr` is a `$this->ident` property fetch. Used by the getter /
/// constructor purity inference, mirroring the AST shape Psalm matches in
/// `FunctionLikeNodeScanner`.
fn expression_is_this_property_fetch(expr: &Expression<'_>) -> bool {
    let Expression::Access(Access::Property(property_access)) = expr.unparenthesized() else {
        return false;
    };
    let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(var)) =
        property_access.object.unparenthesized()
    else {
        return false;
    };
    var.name == "$this"
        && matches!(
            property_access.property,
            mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector::Identifier(_)
        )
}

/// Whether a method body is a single `return $this->prop;` — Psalm infers such
/// getters as mutation-free / external-mutation-free.
fn statements_are_simple_property_getter(stmts: &[Statement<'_>]) -> bool {
    let [Statement::Return(ret)] = stmts else {
        return false;
    };
    ret.value
        .as_ref()
        .is_some_and(expression_is_this_property_fetch)
}

/// Whether a constructor body only assigns simple, non-external-mutating values
/// to its own (`$this->`) properties — mirroring Psalm's
/// `inferPropertyTypeFromConstructor`, which marks such constructors
/// external-mutation-free. An empty body (e.g. constructor-promoted properties
/// only) qualifies, since it cannot mutate external state.
/// Merge an intersection whose members are all non-list array shapes into a
/// single shape (Psalm's type parser combines `array{a: int}&array{b: string}`
/// into `array{a: int, b: string}`). Conflicting duplicate keys, list shapes,
/// or non-shape members yield `None`, leaving the intersection as-is.
fn merge_intersected_shapes(members: &[TAtomic]) -> Option<TAtomic> {
    if members.len() < 2 {
        return None;
    }

    let mut properties: FxHashMap<pzoom_code_info::t_atomic::ArrayKey, TUnion> =
        FxHashMap::default();
    let mut sealed = true;
    let mut fallback_key_type: Option<Box<TUnion>> = None;
    let mut fallback_value_type: Option<Box<TUnion>> = None;

    for member in members {
        let TAtomic::TKeyedArray {
            properties: member_properties,
            is_list: false,
            sealed: member_sealed,
            fallback_key_type: member_fallback_key,
            fallback_value_type: member_fallback_value,
        } = member
        else {
            return None;
        };

        for (key, value_type) in member_properties.iter() {
            if let Some(existing) = properties.get(key) {
                if existing != value_type {
                    return None;
                }
            } else {
                properties.insert(key.clone(), value_type.clone());
            }
        }

        sealed &= *member_sealed;
        if fallback_key_type.is_none() {
            fallback_key_type = member_fallback_key.clone();
            fallback_value_type = member_fallback_value.clone();
        }
    }

    Some(TAtomic::TKeyedArray {
        properties: std::sync::Arc::new(properties),
        is_list: false,
        sealed,
        fallback_key_type,
        fallback_value_type,
    })
}

fn constructor_is_external_mutation_free(stmts: &[Statement<'_>]) -> bool {
    use mago_syntax::ast::ast::assignment::AssignmentOperator;

    stmts.iter().all(|stmt| {
        let Statement::Expression(expr_stmt) = stmt else {
            return false;
        };
        let Expression::Assignment(assignment) = expr_stmt.expression.unparenthesized() else {
            return false;
        };
        if !matches!(assignment.operator, AssignmentOperator::Assign(_)) {
            return false;
        }
        if !expression_is_this_property_fetch(assignment.lhs) {
            return false;
        }
        // The right-hand side must not itself mutate or reach external state:
        // a parameter/local variable, a literal, or another `$this->` property.
        matches!(
            assignment.rhs.unparenthesized(),
            Expression::Variable(_) | Expression::Literal(_)
        ) || expression_is_this_property_fetch(assignment.rhs)
    })
}

/// Walker that collects every anonymous-class expression nested anywhere in
/// a statement (function bodies, method bodies, nested anonymous classes),
/// so they can be registered as real classlike storages the way Psalm's
/// ReflectorVisitor registers `{parent}@anonymous` classes.
struct AnonymousClassCollectorWalker;

impl<'ast, 'arena>
    mago_syntax::walker::Walker<'ast, 'arena, Vec<&'ast mago_syntax::ast::ast::class_like::AnonymousClass<'arena>>>
    for AnonymousClassCollectorWalker
{
    fn walk_in_anonymous_class(
        &self,
        anonymous_class: &'ast mago_syntax::ast::ast::class_like::AnonymousClass<'arena>,
        context: &mut Vec<&'ast mago_syntax::ast::ast::class_like::AnonymousClass<'arena>>,
    ) {
        context.push(anonymous_class);
    }
}

/// Walker that records the names of `$this->X` properties targeted by an
/// assignment. Mirrors Psalm's `ReflectorVisitor` collection of
/// `MethodStorage::$this_property_mutations`.
struct StaticPropertyAccessWalker;

impl<'ast, 'arena> mago_syntax::walker::Walker<'ast, 'arena, bool> for StaticPropertyAccessWalker {
    fn walk_in_static_property_access(
        &self,
        _access: &'ast mago_syntax::ast::ast::access::StaticPropertyAccess<'arena>,
        context: &mut bool,
    ) {
        *context = true;
    }
}

struct ThisPropertyMutationWalker;

impl<'ast, 'arena> mago_syntax::walker::Walker<'ast, 'arena, Vec<&'arena str>>
    for ThisPropertyMutationWalker
{
    fn walk_in_assignment(
        &self,
        assignment: &'ast mago_syntax::ast::ast::assignment::Assignment<'arena>,
        context: &mut Vec<&'arena str>,
    ) {
        if let Expression::Access(Access::Property(property_access)) =
            assignment.lhs.unparenthesized()
            && let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(var)) =
                property_access.object.unparenthesized()
                && var.name == "$this"
                    && let mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector::Identifier(
                        identifier,
                    ) = &property_access.property
                        && !context.contains(&identifier.value) {
                            context.push(identifier.value);
                        }
    }
}


/// Psalm `Atomic::getKey()` approximation for the duplicate-doc rule: array
/// shapes/lists key as plain "array"; named objects key by class name.
fn loose_atomic_key(interner: &Interner, atomic: &TAtomic) -> String {
    match atomic {
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => "array".to_string(),
        TAtomic::TNamedObject { name, .. } => interner.lookup(*name).to_string(),
        other => other.get_id(Some(interner)),
    }
}

/// Hakana's `template_readonly` (classlike scanner): every class template
/// starts readonly; a template named in a public non-constructor method
/// parameter or a public property type is removed — those are the channels
/// through which later code can constrain it. Hakana reads the Hack
/// *signature* types; PHP generics are docblock-only, so the effective
/// (docblock-first) types stand in.
fn compute_template_readonly(class_info: &mut ClassLikeInfo) {
    if class_info.template_types.is_empty() {
        return;
    }

    class_info.template_readonly = class_info
        .template_types
        .iter()
        .filter(|template_type| {
            !matches!(template_type.variance, TemplateVariance::Contravariant)
        })
        .map(|template_type| template_type.name)
        .collect();

    for (method_name, method_info) in &class_info.methods {
        if *method_name == StrId::CONSTRUCT
            || method_info.visibility != Visibility::Public
            || class_info.template_readonly.is_empty()
        {
            continue;
        }

        for param in &method_info.params {
            if let Some(param_type) = param.get_type() {
                remove_used_templates(&mut class_info.template_readonly, param_type);
            }
        }
    }

    for property_info in class_info.properties.values() {
        // A `readonly` property cannot be assigned after construction, so it
        // is not a channel through which the template can be constrained
        // (Hack has no such modifier; this is the faithful translation of
        // Hakana's public-property rule).
        if property_info.visibility != Visibility::Public
            || property_info.is_readonly
            || class_info.template_readonly.is_empty()
        {
            continue;
        }

        if let Some(property_type) = property_info.get_type() {
            remove_used_templates(&mut class_info.template_readonly, property_type);
        }
    }
}

fn remove_used_templates(template_readonly: &mut FxHashSet<StrId>, union: &TUnion) {
    for atomic in &union.types {
        remove_used_templates_atomic(template_readonly, atomic);
    }
}

fn remove_used_templates_atomic(template_readonly: &mut FxHashSet<StrId>, atomic: &TAtomic) {
    match atomic {
        TAtomic::TTemplateParam { name, as_type, .. } => {
            template_readonly.remove(name);
            remove_used_templates(template_readonly, as_type);
        }
        TAtomic::TTemplateParamClass { name, as_type, .. } => {
            template_readonly.remove(name);
            remove_used_templates_atomic(template_readonly, as_type);
        }
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => {
            remove_used_templates(template_readonly, key_type);
            remove_used_templates(template_readonly, value_type);
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            remove_used_templates(template_readonly, value_type);
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            for property_type in properties.values() {
                remove_used_templates(template_readonly, property_type);
            }
            if let Some(fallback_key_type) = fallback_key_type {
                remove_used_templates(template_readonly, fallback_key_type);
            }
            if let Some(fallback_value_type) = fallback_value_type {
                remove_used_templates(template_readonly, fallback_value_type);
            }
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => {
            for type_param in type_params {
                remove_used_templates(template_readonly, type_param);
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for nested in types {
                remove_used_templates_atomic(template_readonly, nested);
            }
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => {
            remove_used_templates_atomic(template_readonly, as_type);
        }
        TAtomic::TCallable {
            params,
            return_type,
            ..
        }
        | TAtomic::TClosure {
            params,
            return_type,
            ..
        } => {
            if let Some(params) = params {
                for callable_param in params {
                    remove_used_templates(template_readonly, &callable_param.param_type);
                }
            }
            if let Some(return_type) = return_type {
                remove_used_templates(template_readonly, return_type);
            }
        }
        _ => {}
    }
}

/// Unescape a PHP string literal per PHP semantics: in double-quoted strings
/// an unrecognized escape sequence KEEPS the backslash (`"ns\\cons"` written
/// as `"ns\cons"` still contains the backslash). mago's `value` drops the
/// backslash for unknown escapes, which corrupts namespaced constant names in
/// `define("ns\\const", ...)`.
pub(crate) fn php_unescape_string_literal(literal: &mago_syntax::ast::ast::literal::LiteralString<'_>) -> String {
    use mago_syntax::ast::ast::literal::LiteralStringKind;

    let raw = literal.raw;
    let inner = if raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };

    let double_quoted = matches!(literal.kind, Some(LiteralStringKind::DoubleQuoted));
    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let Some(&next) = chars.peek() else {
            result.push('\\');
            break;
        };
        if double_quoted {
            match next {
                'n' => {
                    result.push('\n');
                    chars.next();
                }
                't' => {
                    result.push('\t');
                    chars.next();
                }
                'r' => {
                    result.push('\r');
                    chars.next();
                }
                'v' => {
                    result.push('\u{0B}');
                    chars.next();
                }
                'e' => {
                    result.push('\u{1B}');
                    chars.next();
                }
                'f' => {
                    result.push('\u{0C}');
                    chars.next();
                }
                '\\' => {
                    result.push('\\');
                    chars.next();
                }
                '$' => {
                    result.push('$');
                    chars.next();
                }
                '"' => {
                    result.push('"');
                    chars.next();
                }
                // Octal/hex/unicode escapes are irrelevant for constant
                // names; keep them verbatim.
                _ => result.push('\\'),
            }
        } else {
            match next {
                '\\' => {
                    result.push('\\');
                    chars.next();
                }
                '\'' => {
                    result.push('\'');
                    chars.next();
                }
                _ => result.push('\\'),
            }
        }
    }
    result
}

/// Psalm runs every parsed docblock union through its TypeCombiner, where
/// same-class generic objects whose param KEYS match (array shapes all key as
/// `array`) merge their params: `D<array{b: bool}>|D<array{c: string}>` is
/// `D<array{b?: bool, c?: string}>`. Distinct param keys (`C<A>|C<B>`) stay
/// separate. Only this narrow rule is applied here; the rest of the union is
/// kept as written.
fn merge_same_class_generic_members(resolved_types: &mut Vec<TAtomic>) {
    fn coarse_param_key(param: &TUnion) -> String {
        param
            .types
            .iter()
            .map(|atomic| match atomic {
                TAtomic::TArray { .. }
                | TAtomic::TNonEmptyArray { .. }
                | TAtomic::TKeyedArray { is_list: false, .. } => "array".to_string(),
                TAtomic::TList { .. }
                | TAtomic::TNonEmptyList { .. }
                | TAtomic::TKeyedArray { is_list: true, .. } => "list".to_string(),
                other => other.get_id(None),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    let mut index = 0;
    while index < resolved_types.len() {
        let (name, params, key) = match &resolved_types[index] {
            TAtomic::TNamedObject {
                name,
                type_params: Some(params),
                ..
            } => (
                *name,
                params.clone(),
                params.iter().map(coarse_param_key).collect::<Vec<_>>(),
            ),
            _ => {
                index += 1;
                continue;
            }
        };

        let mut merged_params = params;
        let mut merged_any = false;
        let mut other = index + 1;
        while other < resolved_types.len() {
            let matches = match &resolved_types[other] {
                TAtomic::TNamedObject {
                    name: other_name,
                    type_params: Some(other_params),
                    ..
                } => {
                    *other_name == name
                        && other_params.len() == merged_params.len()
                        && other_params
                            .iter()
                            .map(coarse_param_key)
                            .collect::<Vec<_>>()
                            == key
                }
                _ => false,
            };
            if matches {
                if let TAtomic::TNamedObject {
                    type_params: Some(other_params),
                    ..
                } = resolved_types.remove(other)
                {
                    for (slot, other_param) in merged_params.iter_mut().zip(other_params) {
                        *slot = pzoom_code_info::combine_union_types(slot, &other_param, false);
                    }
                    merged_any = true;
                }
            } else {
                other += 1;
            }
        }

        if merged_any
            && let TAtomic::TNamedObject { type_params, .. } = &mut resolved_types[index]
        {
            *type_params = Some(merged_params);
        }
        index += 1;
    }
}
