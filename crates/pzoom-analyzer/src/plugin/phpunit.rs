//! The PHPUnit plugin — pzoom's port of `psalm/plugin-phpunit`.
//!
//! psalm-plugin-phpunit teaches the analyzer about PHPUnit so idiomatic test
//! code stops producing false positives. Its *imperative* hooks — behaviors that
//! can't be expressed as type annotations on PHPUnit's own classes, and so
//! genuinely need plugin code — are reproduced here, in [`PhpUnitPlugin`]:
//!   * **Dead-code exemption for test cases** (`TestCaseHandler::afterStatementAnalysis`):
//!     PHPUnit discovers and runs `TestCase` subclasses, their `test*`/`@test`
//!     methods and their `@dataProvider` providers reflectively, so nothing in
//!     the analyzed code references them. [`after_populate`](PhpUnitPlugin::after_populate)
//!     flags those classes/methods `dynamically_callable`, exempting them from
//!     the unused-definition pass.
//!   * **`@dataProvider` validation**: [`after_populate`](PhpUnitPlugin::after_populate)
//!     also checks each provider exists and returns an iterable
//!     (`TestCaseHandler::afterStatementAnalysis`'s provider checks).
//!   * **`setUp()` initializes properties** (`TestCaseHandler::afterCodebasePopulated`):
//!     a `TestCase` subclass with a `setUp()` initializer doesn't need a
//!     constructor, so it shouldn't get `MissingConstructor`. See
//!     [`PhpUnitPlugin::initializes_properties_externally`].
//!
//! The plugin's *declarative* behaviors need no code here — pzoom reads PHPUnit's
//! own docblocks like any other source, so they apply as soon as PHPUnit is
//! installed: `Assert::assertInstanceOf()` &c. carry `@psalm-assert` (narrowing),
//! `createMock()` carries `@psalm-return MockObject&T` (mock object types), and
//! `expectException()` carries `@param class-string<Throwable>`.

use std::sync::Arc;

use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::functionlike_info::FunctionLikeInfo;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};

use super::{Plugin, PluginActivationContext};

/// The PHPUnit base class every test case ultimately extends.
const TEST_CASE_FQN: &str = "PHPUnit\\Framework\\TestCase";

/// Whether `class_info` is `PHPUnit\Framework\TestCase` or descends from it.
/// Inheritance is flattened by the populator, so the whole ancestor chain
/// (including a project's own intermediate base test class, and `TestCase`
/// itself resolved on-demand from `vendor/`) is in `all_parent_classes`.
fn is_test_case(interner: &Interner, class_id: StrId, class_info: &ClassLikeInfo) -> bool {
    let test_case_id = interner.intern(TEST_CASE_FQN);
    class_id == test_case_id || class_info.all_parent_classes.contains(&test_case_id)
}

/// Resolve a class name (as written in a `@dataProvider`, possibly with a
/// leading `\`) to its interned id, if the class exists.
fn resolve_class(codebase: &CodebaseInfo, interner: &Interner, name: &str) -> Option<StrId> {
    let name = name.trim_start_matches('\\');
    let direct = interner.intern(name);
    if codebase.get_class(direct).is_some() {
        return Some(direct);
    }
    let fq = interner.intern(&format!("\\{}", name));
    codebase.get_class(fq).is_some().then_some(fq)
}

/// Build an issue at `span` in `file_path`, resolving the 1-based line/column.
fn issue_at(
    codebase: &CodebaseInfo,
    kind: IssueKind,
    message: String,
    file_path: StrId,
    span: (u32, u32),
) -> Issue {
    let (line, col) = codebase
        .files
        .get(&file_path)
        .map(|file| {
            let starts = crate::unused_symbols::line_start_offsets(&file.contents);
            crate::unused_symbols::line_column(&starts, span.0)
        })
        .unwrap_or((1, 1));
    Issue::new(kind, message, file_path, span.0, span.1, line, col)
}

/// Whether `name` is — or implements — `Traversable` (so a provider returning it
/// is iterable). An unknown/unresolvable class is treated as iterable to avoid a
/// false positive.
fn object_is_iterable(codebase: &CodebaseInfo, name: StrId) -> bool {
    if matches!(
        name,
        StrId::TRAVERSABLE | StrId::ITERATOR | StrId::ITERATOR_AGGREGATE | StrId::GENERATOR
    ) {
        return true;
    }
    match codebase.get_class(name) {
        Some(class_info) => {
            class_info.interfaces.contains(&StrId::TRAVERSABLE)
                || class_info
                    .all_parent_interfaces
                    .contains(&StrId::TRAVERSABLE)
        }
        None => true,
    }
}

/// Whether a data-provider return type is acceptable (`iterable`/`array`/a
/// `Traversable` object/`null`). Conservative: only a type with *no* iterable
/// component — a purely scalar/`void` return — is rejected, so an unusual but
/// plausibly-iterable return is never falsely flagged.
fn return_type_is_iterable(codebase: &CodebaseInfo, return_type: &TUnion) -> bool {
    return_type
        .types
        .iter()
        .any(|atomic| atomic_is_iterable(codebase, atomic))
}

fn atomic_is_iterable(codebase: &CodebaseInfo, atomic: &TAtomic) -> bool {
    match atomic {
        // `null` is PHPUnit's "no datasets"; `mixed`/`object` are too uncertain
        // to reject.
        TAtomic::TArray { .. }
        | TAtomic::TIterable { .. }
        | TAtomic::TObject
        | TAtomic::TMixed
        | TAtomic::TNull => true,
        TAtomic::TNamedObject { name, .. } => object_is_iterable(codebase, *name),
        TAtomic::TTemplateParam { as_type, .. } => return_type_is_iterable(codebase, as_type),
        _ => false,
    }
}

/// Resolve and validate one `@dataProvider` reference of the test method
/// `consumer` (on `consumer_class`). Returns the provider `(class, method)` to
/// mark used (when it exists) and any diagnostic. A reference that doesn't
/// resolve to an existing method is reported missing only when unambiguous — a
/// bare name, or a `Class::name` whose class resolves — so a class name we can't
/// resolve here never yields a false `UndefinedMethod`.
fn check_data_provider(
    codebase: &CodebaseInfo,
    interner: &Interner,
    consumer_class: StrId,
    consumer: &FunctionLikeInfo,
    reference: &str,
) -> (Option<(StrId, StrId)>, Option<Issue>) {
    let reference = reference.trim().trim_end_matches("()").trim_end();
    let (class_id, method_name) = match reference.rsplit_once("::") {
        Some((class_name, method_name)) => match resolve_class(codebase, interner, class_name) {
            Some(class_id) => (class_id, method_name),
            None => return (None, None),
        },
        None => (consumer_class, reference),
    };

    let method_id = interner.intern(method_name);
    let Some(class_info) = codebase.get_class(class_id) else {
        return (None, None);
    };

    let Some(provider) = class_info.methods.get(&method_id) else {
        let span = consumer
            .name_location
            .unwrap_or((consumer.start_offset, consumer.start_offset + 1));
        let message = format!(
            "Data provider method {}::{} does not exist",
            interner.lookup(class_id),
            method_name
        );
        return (
            None,
            Some(issue_at(
                codebase,
                IssueKind::UndefinedMethod,
                message,
                consumer.file_path,
                span,
            )),
        );
    };

    let issue = provider.get_return_type().and_then(|return_type| {
        if return_type_is_iterable(codebase, return_type) {
            return None;
        }
        let span = provider
            .return_type_location
            .or(provider.name_location)
            .unwrap_or((provider.start_offset, provider.start_offset + 1));
        Some(issue_at(
            codebase,
            IssueKind::InvalidReturnType,
            format!(
                "Data provider {}::{} must return iterable<array-key, array>",
                interner.lookup(class_id),
                method_name
            ),
            provider.file_path,
            span,
        ))
    });

    (Some((class_id, method_id)), issue)
}

/// Whether the class declares a PHPUnit per-test initializer. PHPUnit calls
/// `setUp()` before every test, so properties assigned there are always set by
/// the time test code runs — mirroring psalm-plugin-phpunit's `hasInitializers`
/// (which keys off a `setUp` method, case-insensitively).
fn has_setup_initializer(interner: &Interner, class_info: &ClassLikeInfo) -> bool {
    class_info
        .methods
        .keys()
        .any(|method_id| interner.lookup(*method_id).eq_ignore_ascii_case("setup"))
}

/// pzoom's port of `psalm/plugin-phpunit`.
#[derive(Debug)]
pub struct PhpUnitPlugin;

impl Plugin for PhpUnitPlugin {
    fn name(&self) -> &'static str {
        "phpunit"
    }

    fn is_enabled(&self, ctx: &PluginActivationContext<'_>) -> bool {
        // Activate natively when the project depends on PHPUnit (the headline
        // requirement: works as soon as `composer require --dev phpunit/phpunit`
        // is in place), or when psalm.xml explicitly opts the Psalm plugin in via
        // `<pluginClass>Psalm\PhpUnitPlugin\Plugin>` (recorded as a plugin stub).
        ctx.requires_package("phpunit/phpunit") || ctx.plugin_stub_mentions("plugin-phpunit")
    }

    fn after_populate(&self, codebase: &mut CodebaseInfo, interner: &Interner) -> Vec<Issue> {
        let test_case_id = interner.intern(TEST_CASE_FQN);

        let test_cases: Vec<StrId> = codebase
            .classlike_infos
            .iter()
            .filter(|(class_id, class_info)| {
                **class_id != test_case_id && class_info.all_parent_classes.contains(&test_case_id)
            })
            .map(|(class_id, _)| *class_id)
            .collect();

        // Read-only pass: collect the methods PHPUnit drives reflectively — the
        // `test*`/`@test` methods, and the `@dataProvider` providers they name
        // (which a trait-supplied test method like
        // ValidCodeAnalysisTestTrait::testValidCode points at, per-subclass) — so
        // the unused pass doesn't flag them, and validate each provider.
        let mut methods_to_mark: Vec<(StrId, StrId)> = Vec::new();
        let mut issues: Vec<Issue> = Vec::new();
        for &class_id in &test_cases {
            let Some(class_info) = codebase.get_class(class_id) else {
                continue;
            };
            for (method_id, method_info) in &class_info.methods {
                // PHPUnit runs a method as a test if its name starts with `test`
                // or it carries `@test`/`#[Test]`; `@dataProvider` applies only to
                // those.
                let is_test = method_info.has_test_annotation
                    || interner.lookup(*method_id).starts_with("test");
                if !is_test {
                    continue;
                }
                methods_to_mark.push((class_id, *method_id));
                for provider in &method_info.data_providers {
                    let (provider_to_mark, issue) =
                        check_data_provider(codebase, interner, class_id, method_info, provider);
                    if let Some(resolved) = provider_to_mark {
                        methods_to_mark.push(resolved);
                    }
                    if let Some(issue) = issue {
                        issues.push(issue);
                    }
                }
            }
        }

        // Mutation pass: flag the test classes and those methods.
        for class_id in &test_cases {
            if let Some(class_info) = codebase.get_class_mut(*class_id) {
                class_info.dynamically_callable = true;
            }
        }
        for (class_id, method_id) in methods_to_mark {
            if let Some(class_info) = codebase.get_class_mut(class_id)
                && let Some(method) = class_info.methods.get_mut(&method_id)
            {
                Arc::make_mut(method).dynamically_callable = true;
            }
        }

        issues
    }

    fn initializes_properties_externally(
        &self,
        _codebase: &CodebaseInfo,
        interner: &Interner,
        class_id: StrId,
        class_info: &ClassLikeInfo,
    ) -> bool {
        is_test_case(interner, class_id, class_info) && has_setup_initializer(interner, class_info)
    }
}
