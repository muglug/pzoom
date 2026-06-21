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
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};

use super::{Plugin, PluginActivationContext};
use crate::type_comparator::is_contained_by_with_codebase;

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

/// Whether PHPUnit runs `method` as a test — named `test*`, or carrying the
/// `@test` annotation (read from the generic docblock tags the scanner records).
fn is_test_method(interner: &Interner, method_id: StrId, method: &FunctionLikeInfo) -> bool {
    interner.lookup(method_id).starts_with("test")
        || method
            .custom_docblock_tags
            .iter()
            .any(|(tag, _)| tag == "test")
}

/// The provider references of a method's `@dataProvider` tags — the first
/// whitespace-delimited token of each (`providerName` or `Class::providerName`).
fn data_provider_refs(method: &FunctionLikeInfo) -> impl Iterator<Item = &str> {
    method
        .custom_docblock_tags
        .iter()
        .filter(|(tag, _)| tag == "dataProvider")
        .filter_map(|(_, content)| content.split_whitespace().next())
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

/// The per-call dataset type of a provider — the value type of its iterable
/// return (`V` of `iterable<K, V>` / `array<K, V>` / `Generator<K, V>`). `None`
/// when the return isn't a single, shape-determinable iterable, so callers skip
/// the dataset checks rather than guess.
fn provider_dataset_type(codebase: &CodebaseInfo, return_type: &TUnion) -> Option<TUnion> {
    match return_type.get_single()? {
        TAtomic::TIterable { value_type, .. } => Some((**value_type).clone()),
        TAtomic::TArray {
            params: Some(params),
            known_values,
            ..
        } if known_values.is_empty() => Some(params.1.clone()),
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } if object_is_iterable(codebase, *name) => type_params.get(1).cloned(),
        _ => None,
    }
}

/// Port of psalm-plugin-phpunit's `$checkParam`: match a provider's `dataset`
/// (one row of arguments) positionally against the test method `consumer`'s
/// parameters, emitting `TooFewArguments`/`InvalidArgument` on a definite
/// mismatch. Conservative: only a single, integer-keyed (positional) array
/// dataset is reasoned about, `mixed` elements are accepted, and `TooFewArguments`
/// fires only for a sealed (fixed-length) dataset — so a loosely-typed provider
/// is never falsely flagged.
fn check_dataset_against_params(
    codebase: &CodebaseInfo,
    interner: &Interner,
    consumer: &FunctionLikeInfo,
    dataset: &TUnion,
) -> Vec<Issue> {
    let mut issues = Vec::new();
    let Some(TAtomic::TArray {
        known_values,
        params,
        is_sealed,
        ..
    }) = dataset.get_single()
    else {
        return issues;
    };
    // Only positional datasets; a named-argument dataset (string keys) is skipped.
    if known_values
        .keys()
        .any(|key| !matches!(key, ArrayKey::Int(_)))
    {
        return issues;
    }

    let span = consumer
        .name_location
        .unwrap_or((consumer.start_offset, consumer.start_offset + 1));
    for (index, param) in consumer.params.iter().enumerate() {
        // A variadic parameter soaks up the remaining datasets of any length.
        if param.is_variadic {
            break;
        }
        let param_name = interner.lookup(param.name);
        if let Some((_, element)) = known_values.get(&ArrayKey::Int(index as i64)) {
            let Some(param_type) = param.get_type() else {
                continue;
            };
            if !element.is_mixed() && !is_contained_by_with_codebase(element, param_type, codebase)
            {
                issues.push(issue_at(
                    codebase,
                    IssueKind::InvalidArgument,
                    format!(
                        "Data provider supplies {} for ${} (#{}), which expects {}",
                        element.get_id(Some(interner)),
                        param_name,
                        index + 1,
                        param_type.get_id(Some(interner)),
                    ),
                    consumer.file_path,
                    span,
                ));
            }
        } else if let Some(fallback) = params {
            let Some(param_type) = param.get_type() else {
                continue;
            };
            if !fallback.1.is_mixed()
                && !is_contained_by_with_codebase(&fallback.1, param_type, codebase)
            {
                issues.push(issue_at(
                    codebase,
                    IssueKind::InvalidArgument,
                    format!(
                        "Data provider supplies {} for ${} (#{}), which expects {}",
                        fallback.1.get_id(Some(interner)),
                        param_name,
                        index + 1,
                        param_type.get_id(Some(interner)),
                    ),
                    consumer.file_path,
                    span,
                ));
            }
        } else if *is_sealed && !param.is_optional {
            issues.push(issue_at(
                codebase,
                IssueKind::TooFewArguments,
                format!(
                    "Data provider supplies no value for required parameter ${} (#{})",
                    param_name,
                    index + 1
                ),
                consumer.file_path,
                span,
            ));
        }
    }
    issues
}

/// Resolve and validate one `@dataProvider` reference of the test method
/// `consumer` (on `consumer_class`). Returns the provider `(class, method)` to
/// mark used (when it exists) and any diagnostics: the provider must exist
/// (`UndefinedMethod`), return an iterable (`InvalidReturnType`), and supply
/// datasets matching the test method's parameters
/// (`TooFewArguments`/`InvalidArgument`). A reference that doesn't resolve to an
/// existing method is reported missing only when unambiguous — a bare name, or a
/// `Class::name` whose class resolves — so a class name we can't resolve here
/// never yields a false `UndefinedMethod`.
fn check_data_provider(
    codebase: &CodebaseInfo,
    interner: &Interner,
    consumer_class: StrId,
    consumer: &FunctionLikeInfo,
    reference: &str,
) -> (Option<(StrId, StrId)>, Vec<Issue>) {
    let reference = reference.trim().trim_end_matches("()").trim_end();
    let (class_id, method_name) = match reference.rsplit_once("::") {
        Some((class_name, method_name)) => match resolve_class(codebase, interner, class_name) {
            Some(class_id) => (class_id, method_name),
            None => return (None, Vec::new()),
        },
        None => (consumer_class, reference),
    };

    let method_id = interner.intern(method_name);
    let Some(class_info) = codebase.get_class(class_id) else {
        return (None, Vec::new());
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
            vec![issue_at(
                codebase,
                IssueKind::UndefinedMethod,
                message,
                consumer.file_path,
                span,
            )],
        );
    };

    let mut issues = Vec::new();
    if let Some(return_type) = provider.get_return_type() {
        if !return_type_is_iterable(codebase, return_type) {
            let span = provider
                .return_type_location
                .or(provider.name_location)
                .unwrap_or((provider.start_offset, provider.start_offset + 1));
            issues.push(issue_at(
                codebase,
                IssueKind::InvalidReturnType,
                format!(
                    "Data provider {}::{} must return iterable<array-key, array>",
                    interner.lookup(class_id),
                    method_name
                ),
                provider.file_path,
                span,
            ));
        } else if let Some(dataset) = provider_dataset_type(codebase, return_type) {
            issues.extend(check_dataset_against_params(
                codebase, interner, consumer, &dataset,
            ));
        }
    }

    (Some((class_id, method_id)), issues)
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
        let mut classes_to_reference: Vec<StrId> = Vec::new();
        let mut issues: Vec<Issue> = Vec::new();
        for &class_id in &test_cases {
            let Some(class_info) = codebase.get_class(class_id) else {
                continue;
            };
            for (method_id, method_info) in &class_info.methods {
                // `@dataProvider` applies only to test methods (named `test*` or
                // tagged `@test`).
                if !is_test_method(interner, *method_id, method_info) {
                    continue;
                }
                methods_to_mark.push((class_id, *method_id));
                for provider in data_provider_refs(method_info) {
                    let (provider_to_mark, provider_issues) =
                        check_data_provider(codebase, interner, class_id, method_info, provider);
                    if let Some((provider_class, provider_method)) = provider_to_mark {
                        methods_to_mark.push((provider_class, provider_method));
                        // A provider on another class is referenced by this test
                        // (Psalm registers a classlike reference), so it isn't
                        // `UnusedClass` even when nothing else uses it.
                        if provider_class != class_id {
                            classes_to_reference.push(provider_class);
                        }
                    }
                    issues.extend(provider_issues);
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
        codebase
            .plugin_referenced_classes
            .extend(classes_to_reference);

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
