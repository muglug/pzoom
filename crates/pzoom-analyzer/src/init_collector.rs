//! Constructor property-initialisation collection — pzoom's port of Psalm's
//! `collect_initializations` machinery.
//!
//! `ClassAnalyzer::checkPropertyInitialization` re-analyses a constructor to
//! decide which `$this->prop` it definitely assigns. Every `$this->`/`parent::`
//! /ancestor method the constructor calls must be followed *at the call site*, so
//! the property writes land flow-sensitively (a write reached only on one branch,
//! or after a `never`-returning call, must be treated accordingly). Psalm threads
//! this through `Context::collect_initializations` +
//! `CallAnalyzer::collectSpecialInformation` (instance calls, visibility-gated) /
//! `ExistingAtomicStaticCallAnalyzer` (static calls, ungated) +
//! `FileAnalyzer::getMethodMutations`, which re-parses the callee's file on
//! demand. This module is the faithful port: the call analyzers invoke
//! [`follow_instance_init_call`] / [`follow_static_init_call`] while collecting,
//! and [`reanalyze_method_body_into`] re-parses and re-runs the callee body.

use bumpalo::Bump;

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::MethodBody;
use mago_syntax::ast::ast::namespace::NamespaceBody;
use mago_syntax::ast::ast::statement::Statement;

use pzoom_code_info::class_like_info::{ClassLikeKind, Visibility};
use pzoom_code_info::{CodebaseInfo, FunctionLikeInfo, TUnion, VarName};
use pzoom_str::StrId;
use pzoom_syntax::{FileId, parse_file_content, resolve_names};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

/// Re-parse `method_info`'s declaring file, locate its body, and re-analyse it
/// against `method_context` (Psalm's `getMethodMutations` body re-run). Works
/// cross-file: the body is found even when it lives in a parent/trait file
/// (inherited constructors, helper methods elsewhere). The body's own
/// `$this->`/`parent::` calls re-enter this machinery through the call-analyzer
/// hook, so following stays flow-sensitive. Constructor parameters are seeded
/// for a clean analysis; promoted parameters seed their `$this->prop` as
/// initialised at entry. Issues raised by the re-analysis are discarded.
pub(crate) fn reanalyze_method_body_into(
    analyzer: &StatementsAnalyzer<'_>,
    method_info: &FunctionLikeInfo,
    method_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    // Seed the method's parameters so the body analyses cleanly; their precise
    // types are irrelevant (`$this->prop = $param` records `$this->prop`
    // regardless, and this pass's issues are thrown away — only
    // `collected_uninitialized_reads` and the `$this->prop` scope are kept).
    for param in &method_info.params {
        let param_name = analyzer.interner.lookup(param.name);
        let param_type = param.get_type().cloned().unwrap_or_else(TUnion::mixed);
        method_context.set_var_type(VarName::new(param_name.as_ref()), param_type);
    }

    // A constructor's promoted parameters assign their properties at entry.
    if method_info.name == StrId::CONSTRUCT
        && let Some(declaring_class) = method_info.declaring_class
        && let Some(declaring_info) = analyzer.codebase.get_class(declaring_class)
    {
        for (property_name, property) in &declaring_info.properties {
            if property.is_promoted && property.declaring_class == declaring_class {
                let key = format!("$this->{}", analyzer.interner.lookup(*property_name));
                method_context.locals.insert(
                    VarName::from(key),
                    property.get_type().cloned().unwrap_or_else(TUnion::mixed),
                );
                if let Some(self_class) = method_context.self_class {
                    method_context
                        .initialized_prop_classes
                        .insert(*property_name, self_class);
                }
            }
        }
    }

    let Some(file_info) = analyzer.codebase.files.get(&method_info.file_path) else {
        return;
    };

    let arena = Bump::new();
    let path_str = analyzer.interner.lookup(method_info.file_path);
    let file_id = FileId::new(&*path_str);
    let (program, _parse_error) = parse_file_content(&arena, file_id, &file_info.contents);
    let resolved_names = resolve_names(&program, analyzer.interner);

    let Some(body_stmts) =
        find_method_body_by_offset(program.statements.as_slice(), method_info.start_offset)
    else {
        return;
    };

    let file_analyzer = StatementsAnalyzer::new(
        analyzer.codebase,
        analyzer.interner,
        method_info.file_path,
        &file_info.contents,
        &resolved_names,
        analyzer.config,
    )
    .with_arena(&arena);
    let method_analyzer = file_analyzer.for_nested_function(Some(method_info));

    let _ = crate::stmt_analyzer::analyze_stmts(
        &method_analyzer,
        body_stmts,
        analysis_data,
        method_context,
    );
}

/// Locate, in a re-parsed program, the concrete body of the method whose node
/// begins at `method_start_offset` (the `FunctionLikeInfo::start_offset` recorded
/// at scan time — the same parser produces the same span). Scans every class-like
/// in every namespace; offsets are unique within a file, so no class identity is
/// needed. Returns `None` for an abstract method or a missing offset.
fn find_method_body_by_offset<'ast>(
    statements: &'ast [Statement<'ast>],
    method_start_offset: u32,
) -> Option<&'ast [Statement<'ast>]> {
    for statement in statements {
        let members = match statement {
            Statement::Class(class) => Some(&class.members),
            Statement::Trait(trait_stmt) => Some(&trait_stmt.members),
            Statement::Enum(enum_stmt) => Some(&enum_stmt.members),
            Statement::Interface(interface) => Some(&interface.members),
            Statement::Namespace(namespace) => {
                let nested = match &namespace.body {
                    NamespaceBody::Implicit(body) => body.statements.as_slice(),
                    NamespaceBody::BraceDelimited(body) => body.statements.as_slice(),
                };
                if let Some(found) = find_method_body_by_offset(nested, method_start_offset) {
                    return Some(found);
                }
                None
            }
            _ => None,
        };

        if let Some(members) = members {
            for member in members.iter() {
                if let ClassLikeMember::Method(method) = member
                    && method.span().start.offset == method_start_offset
                {
                    return match &method.body {
                        MethodBody::Concrete(block) => Some(block.statements.as_slice()),
                        MethodBody::Abstract(_) => None,
                    };
                }
            }
        }
    }

    None
}

/// Follow a `$this->method()` call while collecting initialisations (Psalm's
/// `CallAnalyzer::collectSpecialInformation`, instance branch). Visibility-gated:
/// a non-private, non-final method is followed only when no uninitialised
/// property is private (`collect_nonprivate_initializations`) — an overridable
/// method can't be trusted to set a private property. Static methods can't touch
/// `$this`, so they're skipped.
pub(crate) fn follow_instance_init_call(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    resolved_class: StrId,
    method_info: &FunctionLikeInfo,
) {
    if method_info.is_static {
        return;
    }
    let is_final = method_info.is_final
        || analyzer
            .codebase
            .get_class(resolved_class)
            .is_some_and(|info| info.is_final);
    if !(context.collect_nonprivate_initializations
        || matches!(method_info.visibility, Visibility::Private)
        || is_final)
    {
        return;
    }
    follow_init_call(analyzer, context, resolved_class, method_info);
}

/// Follow a `parent::`/`self::`/`static::`/ancestor `Class::method()` call while
/// collecting initialisations (Psalm's `ExistingAtomicStaticCallAnalyzer`,
/// `collect_initializations` branch). Unlike the instance path this is ungated:
/// the caller has already confirmed the target is the current class or an
/// ancestor. Used both for explicit static-dispatch calls and for the synthesised
/// `parent::__construct()` of an inherited constructor.
pub(crate) fn follow_static_init_call(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    resolved_class: StrId,
    method_info: &FunctionLikeInfo,
) {
    follow_init_call(analyzer, context, resolved_class, method_info);
}

/// Shared body of both follow paths: re-entry-guard, build a fresh call context
/// seeded with the caller's `$this` and `$this->prop` scope (Psalm's
/// `getMethodMutations` call_context), re-analyse the callee body, then copy the
/// resulting `$this->prop` (and their assigning-class tags) back so the writes
/// land at the call site.
fn follow_init_call(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    resolved_class: StrId,
    method_info: &FunctionLikeInfo,
) {
    let identity = (
        method_info.declaring_class.unwrap_or(resolved_class),
        method_info.name,
    );
    // Don't follow the same method twice, and break mutual recursion.
    if !context.initialized_methods.insert(identity) {
        return;
    }

    let mut call_context = BlockContext::new();
    call_context.collect_initializations = true;
    call_context.collect_nonprivate_initializations = context.collect_nonprivate_initializations;
    call_context.initialized_methods = context.initialized_methods.clone();
    call_context.initialized_prop_classes = context.initialized_prop_classes.clone();
    // Inside the callee, `self`/`parent` resolve against the callee's class; but
    // `$this` stays the originally-constructed object (carried below).
    call_context.self_class = Some(resolved_class);
    call_context.has_this = true;

    if let Some(this_type) = context.locals.get("$this") {
        call_context
            .locals
            .insert(VarName::new_static("$this"), this_type.clone());
    }
    for (var_name, var_type) in context.locals.iter() {
        if var_name.as_str().starts_with("$this->") {
            call_context
                .locals
                .insert(var_name.clone(), var_type.clone());
        }
    }

    // A followed method's body is re-analysed for its `$this->prop` writes only;
    // its own issues (and any uninitialised-read records, which Psalm raises only
    // for the constructor body itself) are discarded.
    let mut throwaway = FunctionAnalysisData::new();
    reanalyze_method_body_into(analyzer, method_info, &mut call_context, &mut throwaway);

    // The caller inherits the callee's resulting property scope (Psalm's
    // getMethodMutations writeback).
    for (var_name, var_type) in call_context.locals.iter() {
        if var_name.as_str().starts_with("$this->") {
            context
                .locals
                .insert(var_name.clone(), var_type.clone());
        }
    }
    for (property_name, assigning_class) in &call_context.initialized_prop_classes {
        context
            .initialized_prop_classes
            .insert(*property_name, *assigning_class);
    }
}

/// Whether an assignment made by a method running in `from_class` initialises
/// `child_class`'s property `property_name` (Psalm's `initialized_class` check).
/// A private property only counts when both sides resolve to the same
/// declaration: assigning `$this->b` in a parent constructor sets the *parent's*
/// private `$b`, not a same-named private `$b` on the child.
pub(crate) fn assignment_initializes(
    codebase: &CodebaseInfo,
    child_class: StrId,
    from_class: StrId,
    property_name: StrId,
) -> bool {
    let Some(child_info) = codebase.get_class(child_class) else {
        return true;
    };
    let Some(property) = child_info.properties.get(&property_name) else {
        return true;
    };
    if !matches!(property.visibility, Visibility::Private) || from_class == child_class {
        return true;
    }
    let from_info = codebase.get_class(from_class);
    if from_info.is_some_and(|info| info.kind == ClassLikeKind::Trait) {
        // Trait methods run in the using class's context.
        return true;
    }
    let from_declaring =
        from_info.and_then(|info| info.declaring_property_ids.get(&property_name).copied());
    from_declaring.is_some()
        && from_declaring == child_info.declaring_property_ids.get(&property_name).copied()
}

/// Whether the current `self` is `class_id` or a descendant of it — the gate for
/// following a `parent::`/`self::`/`static::`/ancestor `Class::method()` static
/// call while collecting (Psalm's `classExtends($context->self, …)` check). An
/// unrelated `Other::method()` is not followed.
pub(crate) fn self_is_or_extends(
    codebase: &CodebaseInfo,
    context: &BlockContext,
    class_id: StrId,
) -> bool {
    let Some(self_class) = context.self_class else {
        return false;
    };
    self_class == class_id
        || codebase
            .get_class(self_class)
            .is_some_and(|info| info.all_parent_classes.contains(&class_id))
}
