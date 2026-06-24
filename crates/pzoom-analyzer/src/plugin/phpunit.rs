//! The PHPUnit plugin — pzoom's port of `psalm/plugin-phpunit`.
//!
//! psalm-plugin-phpunit teaches the analyzer about PHPUnit so idiomatic test
//! code stops producing false positives. Its *imperative* hooks — behaviors that
//! can't be expressed as type annotations on PHPUnit's own classes, and so
//! genuinely need plugin code — are reproduced here, in [`PhpUnitPlugin`]:
//!   * **Dead-code exemption for test cases** (`TestCaseHandler::afterStatementAnalysis`):
//!     PHPUnit discovers and runs `TestCase` subclasses and their `test*`/`@test`/
//!     `#[Test]` methods reflectively, so nothing in the analyzed code references
//!     them. [`after_populate`](PhpUnitPlugin::after_populate) flags those
//!     classes/methods `dynamically_callable`, exempting them from the
//!     unused-definition pass.
//!   * **`@dataProvider` references**: a provider is also called reflectively;
//!     [`after_functionlike_analysis`](PhpUnitPlugin::after_functionlike_analysis)
//!     records a reference from the test method to its provider method, which
//!     keeps the provider (and a cross-class provider's class) out of the
//!     unused-definition report.
//!   * **`@dataProvider` validation**: [`after_populate`](PhpUnitPlugin::after_populate)
//!     checks each provider exists, returns an iterable, and supplies datasets
//!     matching the test method's parameters.
//!   * **`setUp()` initializes properties** (`TestCaseHandler::afterCodebasePopulated`):
//!     a `TestCase` subclass with a `setUp()` initializer doesn't need a
//!     constructor, so it shouldn't get `MissingConstructor`. See
//!     [`PhpUnitPlugin::initializes_properties_externally`].
//!
//! The `@test`/`@dataProvider` docblock tags are read from the scanner's generic
//! `custom_docblock_tags`, and the `#[Test]`/`#[DataProvider]` attribute forms
//! from its generic `attributes` store — so no PHPUnit specifics live in the core.
//!
//! The plugin's *declarative* behaviors need no code here — pzoom reads PHPUnit's
//! own docblocks like any other source, so they apply as soon as PHPUnit is
//! installed: `Assert::assertInstanceOf()` &c. carry `@psalm-assert` (narrowing),
//! `createMock()` carries `@psalm-return MockObject&T` (mock object types), and
//! `expectException()` carries `@param class-string<Throwable>`.

use std::sync::Arc;

use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::data_flow::node::FunctionLikeIdentifier;
use pzoom_code_info::functionlike_info::FunctionLikeInfo;
use pzoom_code_info::issue::{Issue, IssueKind};
use pzoom_code_info::t_atomic::ArrayKey;
use pzoom_code_info::{CodebaseInfo, TAtomic, TUnion};
use pzoom_str::{Interner, StrId};

use super::{Plugin, PluginActivationContext};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::type_comparator::is_contained_by_with_codebase;

/// The PHPUnit base class every test case ultimately extends.
const TEST_CASE_FQN: &str = "PHPUnit\\Framework\\TestCase";

/// PHPUnit's PHP-attribute forms (PHPUnit 10+) of `@test`/`@dataProvider`.
const TEST_ATTRIBUTE_FQN: &str = "PHPUnit\\Framework\\Attributes\\Test";
const DATA_PROVIDER_ATTRIBUTE_FQN: &str = "PHPUnit\\Framework\\Attributes\\DataProvider";
const DATA_PROVIDER_EXTERNAL_ATTRIBUTE_FQN: &str =
    "PHPUnit\\Framework\\Attributes\\DataProviderExternal";

/// Whether `class_info` is `PHPUnit\Framework\TestCase` or descends from it.
/// Inheritance is flattened by the populator, so the whole ancestor chain
/// (including a project's own intermediate base test class, and `TestCase`
/// itself resolved on-demand from `vendor/`) is in `all_parent_classes`.
fn is_test_case(interner: &Interner, class_id: StrId, class_info: &ClassLikeInfo) -> bool {
    let test_case_id = interner
        .find(TEST_CASE_FQN)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    class_id == test_case_id || class_info.all_parent_classes.contains(&test_case_id)
}

/// Whether PHPUnit runs `method` as a test — named `test*`, or carrying `@test`
/// (a generic docblock tag) or `#[Test]` (a generic attribute) the scanner
/// records.
fn is_test_method(interner: &Interner, method_id: StrId, method: &FunctionLikeInfo) -> bool {
    interner.lookup(method_id).starts_with("test")
        || method
            .custom_docblock_tags
            .iter()
            .any(|(tag, _)| tag == "test")
        || method.attributes.contains_key(
            &interner
                .find(TEST_ATTRIBUTE_FQN)
                .unwrap_or(pzoom_str::StrId::EMPTY),
        )
}

/// The provider references of a method — from `@dataProvider` docblock tags
/// (`providerName` / `Class::providerName`) and the `#[DataProvider('name')]` /
/// `#[DataProviderExternal(Class::class, 'name')]` attribute forms.
fn data_provider_refs(interner: &Interner, method: &FunctionLikeInfo) -> Vec<String> {
    let mut refs: Vec<String> = method
        .custom_docblock_tags
        .iter()
        .filter(|(tag, _)| tag == "dataProvider")
        .filter_map(|(_, content)| content.split_whitespace().next())
        .map(str::to_string)
        .collect();

    // `#[DataProvider('providerName')]` — a method on the test class.
    if let Some(occurrences) = method.attributes.get(
        &interner
            .find(DATA_PROVIDER_ATTRIBUTE_FQN)
            .unwrap_or(pzoom_str::StrId::EMPTY),
    ) {
        for args in occurrences {
            if let Some(name) = args.first().and_then(literal_string) {
                refs.push(name);
            }
        }
    }
    // `#[DataProviderExternal(OtherClass::class, 'providerName')]`.
    if let Some(occurrences) = method.attributes.get(
        &interner
            .find(DATA_PROVIDER_EXTERNAL_ATTRIBUTE_FQN)
            .unwrap_or(pzoom_str::StrId::EMPTY),
    ) {
        for args in occurrences {
            if let (Some(class), Some(name)) = (
                args.first().and_then(literal_class_string),
                args.get(1).and_then(literal_string),
            ) {
                refs.push(format!("{}::{}", class, name));
            }
        }
    }

    refs
}

/// The string value of a folded attribute argument (`'name'`), if it is a single
/// literal string. Attribute arguments are stored as the same constant [`TUnion`]
/// the scanner infers for any const expression.
fn literal_string(arg: &TUnion) -> Option<String> {
    match arg.get_single()? {
        TAtomic::TLiteralString { value } => Some(value.clone()),
        _ => None,
    }
}

/// The class name of a folded `Foo::class` attribute argument, if it is one.
fn literal_class_string(arg: &TUnion) -> Option<String> {
    match arg.get_single()? {
        TAtomic::TLiteralClassString { name } => Some(name.clone()),
        _ => None,
    }
}

/// Resolve a class name (as written in a `@dataProvider`, possibly with a
/// leading `\`) to its interned id, if the class exists.
fn resolve_class(codebase: &CodebaseInfo, interner: &Interner, name: &str) -> Option<StrId> {
    let name = name.trim_start_matches('\\');
    let direct = interner.find(name).unwrap_or(pzoom_str::StrId::EMPTY);
    if codebase.get_class(direct).is_some() {
        return Some(direct);
    }
    let fq = interner
        .find(&format!("\\{}", name))
        .unwrap_or(pzoom_str::StrId::EMPTY);
    codebase.get_class(fq).is_some().then_some(fq)
}

/// Resolve a `@dataProvider` reference to the provider `(class, method)` — the
/// named class for `Class::method` (if it exists), else `consumer_class` for a
/// bare `method` — returning it only when that method exists. The method id is the
/// `methods`-map key (the original-case interned name), so it can both flag the
/// method and key a reference. Resolution only — no diagnostics; validation lives
/// in [`check_data_provider`].
fn resolve_provider_method(
    codebase: &CodebaseInfo,
    interner: &Interner,
    consumer_class: StrId,
    reference: &str,
) -> Option<(StrId, StrId)> {
    let reference = reference.trim().trim_end_matches("()").trim_end();
    let (class_id, method_name) = match reference.rsplit_once("::") {
        Some((class_name, method_name)) => {
            (resolve_class(codebase, interner, class_name)?, method_name)
        }
        None => (consumer_class, reference),
    };
    let method_id = interner
        .find(method_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    codebase
        .get_class(class_id)?
        .methods
        .contains_key(&method_id)
        .then_some((class_id, method_id))
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

/// Validate one `@dataProvider` reference of the test method `consumer` (on
/// `consumer_class`): the provider must exist (`UndefinedMethod`), return an
/// iterable (`InvalidReturnType`), and supply datasets matching the test method's
/// parameters (`TooFewArguments`/`InvalidArgument`). A reference that doesn't
/// resolve to an existing method is reported missing only when unambiguous — a
/// bare name, or a `Class::name` whose class resolves — so a class name we can't
/// resolve here never yields a false `UndefinedMethod`. The reference a provider
/// creates is recorded separately, in
/// [`PhpUnitPlugin::after_functionlike_analysis`].
fn check_data_provider(
    codebase: &CodebaseInfo,
    interner: &Interner,
    consumer_class: StrId,
    consumer: &FunctionLikeInfo,
    reference: &str,
) -> Vec<Issue> {
    let reference = reference.trim().trim_end_matches("()").trim_end();
    let (class_id, method_name) = match reference.rsplit_once("::") {
        Some((class_name, method_name)) => match resolve_class(codebase, interner, class_name) {
            Some(class_id) => (class_id, method_name),
            None => return Vec::new(),
        },
        None => (consumer_class, reference),
    };

    let method_id = interner
        .find(method_name)
        .unwrap_or(pzoom_str::StrId::EMPTY);
    let Some(class_info) = codebase.get_class(class_id) else {
        return Vec::new();
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
        return vec![issue_at(
            codebase,
            IssueKind::UndefinedMethod,
            message,
            consumer.file_path,
            span,
        )];
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

    issues
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
        let test_case_id = interner
            .find(TEST_CASE_FQN)
            .unwrap_or(pzoom_str::StrId::EMPTY);

        let test_cases: Vec<StrId> = codebase
            .classlike_infos
            .iter()
            .filter(|(class_id, class_info)| {
                **class_id != test_case_id && class_info.all_parent_classes.contains(&test_case_id)
            })
            .map(|(class_id, _)| *class_id)
            .collect();

        // Read-only pass: collect the methods PHPUnit drives reflectively — the
        // `test*`/`@test` methods, and the provider methods their `@dataProvider`s
        // name — so the unused pass exempts them entirely (a provider's return
        // value is consumed reflectively, so it must also be spared
        // `PossiblyUnusedReturnValue`, which a plain reference wouldn't do). Each
        // provider is validated here; the cross-class *reference* a provider
        // creates (keeping its class alive) is recorded separately, in
        // [`PhpUnitPlugin::after_functionlike_analysis`] during analysis.
        let mut methods_to_mark: Vec<(StrId, StrId)> = Vec::new();
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
                for provider in data_provider_refs(interner, method_info) {
                    issues.extend(check_data_provider(
                        codebase,
                        interner,
                        class_id,
                        method_info,
                        &provider,
                    ));
                    if let Some(provider_method) =
                        resolve_provider_method(codebase, interner, class_id, &provider)
                    {
                        methods_to_mark.push(provider_method);
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

    fn after_functionlike_analysis(
        &self,
        codebase: &CodebaseInfo,
        interner: &Interner,
        function_id: &FunctionLikeIdentifier,
        function_info: &FunctionLikeInfo,
        analysis_data: &mut FunctionAnalysisData,
    ) {
        // Only a method can be a test method with a data provider.
        let FunctionLikeIdentifier::Method(class_id, method_id) = function_id else {
            return;
        };
        if !is_test_method(interner, *method_id, function_info) {
            return;
        }
        for reference in data_provider_refs(interner, function_info) {
            // The test method references its data provider — Psalm records the
            // same reference. A member→member reference keeps the provider method
            // alive, and (via the implied class→class reference) a cross-class
            // provider's class too, so neither is reported unused.
            if let Some(provider) =
                resolve_provider_method(codebase, interner, *class_id, &reference)
            {
                analysis_data
                    .symbol_references
                    .add_class_member_reference_to_class_member(
                        (*class_id, *method_id),
                        provider,
                        false,
                    );
            }
        }
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
