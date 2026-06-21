//! Compiled-in analysis plugins.
//!
//! Modeled on Hakana's plugin system (`CustomHook`/`InternalHook`): plugins are
//! statically linked into the binary rather than loaded at runtime — pzoom can't
//! `eval` PHP, so a Psalm/PHPStan-style runtime plugin (a `<pluginClass>` whose
//! PHP is executed) is reduced to a Rust [`Plugin`] that implements the subset of
//! behaviors expressible without executing user code.
//!
//! Like Hakana's [`InternalHook`], [`Plugin`] is a "fat" trait whose every hook
//! has a default no-op/`None` body, so a plugin overrides only the behaviors it
//! cares about. The set of plugins is fixed at build time ([`builtin_plugins`]);
//! [`Plugin::is_enabled`] decides per-project activation from the project's
//! declared dependencies (its `composer.json`), so a plugin "just works" when the
//! relevant package is installed — e.g. the [`phpunit`] plugin activates when
//! `phpunit/phpunit` is required.
//!
//! The activated plugins live on [`crate::config::Config::plugins`] (mirroring
//! Hakana's `Config.hooks`), so every analyzer that already holds a `&Config`
//! reaches them; the dispatch helpers in this module fan a call out to them.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use pzoom_code_info::CodebaseInfo;
use pzoom_code_info::class_like_info::ClassLikeInfo;
use pzoom_code_info::issue::Issue;
use pzoom_str::{Interner, StrId};

pub mod phpunit;

/// What a compiled-in plugin needs to decide whether it should activate for the
/// project under analysis. Mirrors how Hakana plugins are compiled in but gated;
/// here the gate is the project's declared dependencies, not a build flag.
pub struct PluginActivationContext<'a> {
    /// Composer package names (`vendor/name`, lowercased) drawn from the
    /// project's `composer.json` `require` + `require-dev`.
    pub composer_packages: &'a FxHashSet<String>,

    /// Stub file paths registered by psalm.xml `<pluginClass>` entries (the
    /// existing [`crate::config::Config::plugin_stubs`]). A project that opts a
    /// first-party Psalm plugin in via `<pluginClass>` but doesn't (or can't)
    /// express it through Composer still activates the matching pzoom plugin.
    pub plugin_stubs: &'a [String],
}

impl PluginActivationContext<'_> {
    /// Whether the project requires the given Composer package (e.g.
    /// `"phpunit/phpunit"`).
    pub fn requires_package(&self, package: &str) -> bool {
        self.composer_packages.contains(package)
    }

    /// Whether any registered plugin-stub path mentions `needle` (used to honor
    /// a psalm.xml `<pluginClass>` for a known first-party plugin).
    pub fn plugin_stub_mentions(&self, needle: &str) -> bool {
        self.plugin_stubs.iter().any(|stub| stub.contains(needle))
    }
}

/// A compiled-in analysis plugin (the pzoom analogue of a Hakana `CustomHook`).
///
/// Every hook has a default body so an implementation overrides only what it
/// needs. The trait is `Send + Sync` because the activated plugins are shared by
/// reference across the analysis worker threads (the same constraint Hakana puts
/// on `CustomHook`), and `Debug` so [`crate::config::Config`] can derive it.
pub trait Plugin: Send + Sync + std::fmt::Debug {
    /// Stable, lowercase identifier, e.g. `"phpunit"`.
    fn name(&self) -> &'static str;

    /// Whether this plugin activates for the project described by `ctx`.
    fn is_enabled(&self, ctx: &PluginActivationContext<'_>) -> bool;

    /// Mutate the populated codebase once, before analysis — the place to record
    /// framework knowledge the type system can't infer. Runs after the populate
    /// phase, so inheritance is flattened (`all_parent_classes` is complete) and
    /// every method is resolvable. A plugin sets flags such as
    /// [`ClassLikeInfo::dynamically_callable`] /
    /// [`pzoom_code_info::functionlike_info::FunctionLikeInfo::dynamically_callable`]
    /// (e.g. the PHPUnit plugin marks `TestCase` subclasses and their `test*`
    /// methods) and may return diagnostics (e.g. `@dataProvider` validation).
    /// Analogous to Hakana's `after_populate` / Psalm's
    /// `AfterCodebasePopulatedInterface`.
    fn after_populate(&self, codebase: &mut CodebaseInfo, interner: &Interner) -> Vec<Issue> {
        let _ = (codebase, interner);
        vec![]
    }

    /// Suppress `MissingConstructor` (and the folded-in
    /// `PropertyNotSetInConstructor`) for a class whose typed properties are
    /// initialized outside a constructor — e.g. a PHPUnit `setUp()`/`@before`
    /// method, which the runner calls before each test.
    fn initializes_properties_externally(
        &self,
        codebase: &CodebaseInfo,
        interner: &Interner,
        class_id: StrId,
        class_info: &ClassLikeInfo,
    ) -> bool {
        let _ = (codebase, interner, class_id, class_info);
        false
    }
}

/// Every plugin compiled into pzoom, regardless of activation. Activation is
/// decided per project by [`activate_plugins`].
pub fn builtin_plugins() -> Vec<Arc<dyn Plugin>> {
    vec![Arc::new(phpunit::PhpUnitPlugin)]
}

/// The compiled-in plugins that activate for the project described by `ctx`.
pub fn activate_plugins(ctx: &PluginActivationContext<'_>) -> Vec<Arc<dyn Plugin>> {
    builtin_plugins()
        .into_iter()
        .filter(|plugin| plugin.is_enabled(ctx))
        .collect()
}

/// Run every active plugin's [`Plugin::after_populate`] hook against the
/// populated codebase, collecting the diagnostics they emit.
pub fn run_after_populate(
    plugins: &[Arc<dyn Plugin>],
    codebase: &mut CodebaseInfo,
    interner: &Interner,
) -> Vec<Issue> {
    let mut issues = Vec::new();
    for plugin in plugins {
        issues.extend(plugin.after_populate(codebase, interner));
    }
    issues
}

/// Whether any active plugin treats `class_id` as initializing its properties
/// outside a constructor (see [`Plugin::initializes_properties_externally`]).
pub fn initializes_properties_externally(
    plugins: &[Arc<dyn Plugin>],
    codebase: &CodebaseInfo,
    interner: &Interner,
    class_id: StrId,
    class_info: &ClassLikeInfo,
) -> bool {
    plugins.iter().any(|plugin| {
        plugin.initializes_properties_externally(codebase, interner, class_id, class_info)
    })
}
