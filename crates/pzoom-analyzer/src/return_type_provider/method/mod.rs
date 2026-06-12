//! Method return-type providers (mirrors Psalm's `MethodReturnTypeProviderInterface`).
//!
//! The trait, event and dispatch live here; each concrete provider has its own file
//! and colocates any helpers only it uses.

mod date_time;
mod dom_document;
mod dom_node;
mod message_formatter;
mod mockery_mock;
mod pdo_statement;
mod simple_xml_element;

use mago_syntax::ast::ast::argument::Argument;
use pzoom_code_info::TUnion;
use pzoom_str::StrId;

use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Context passed to a [`MethodReturnTypeProvider`], analogous to Psalm's
/// `MethodReturnTypeProviderEvent`.
pub struct MethodReturnTypeProviderEvent<'a, 'arena> {
    pub analyzer: &'a StatementsAnalyzer<'a>,
    /// The (resolved) class the method is called on.
    pub class_id: StrId,
    pub method_name: &'a str,
    pub args: &'a [&'a Argument<'arena>],
    pub arg_positions: &'a [Pos],
    pub analysis_data: &'a FunctionAnalysisData,
}

/// A provider that supplies a return type for calls to particular methods.
///
/// Implementors declare the class names they apply to (matched against the called
/// class or any of its ancestors) and compute a return type for a given call.
pub trait MethodReturnTypeProvider: Sync {
    /// Class names (case-insensitive, without leading `\`) this provider applies to.
    fn class_names(&self) -> &'static [&'static str];

    /// Compute the return type for the call, or `None` to defer to the next provider
    /// / the declared signature.
    fn get_method_return_type(&self, event: &MethodReturnTypeProviderEvent<'_, '_>)
    -> Option<TUnion>;
}

/// The registered method return-type providers, in priority order.
fn providers() -> &'static [&'static (dyn MethodReturnTypeProvider + 'static)] {
    &[
        &pdo_statement::PdoStatementReturnTypeProvider,
        &date_time::DateTimeReturnTypeProvider,
        &dom_document::DomDocumentReturnTypeProvider,
        &dom_node::DomNodeReturnTypeProvider,
        &simple_xml_element::SimpleXmlElementReturnTypeProvider,
        &message_formatter::MessageFormatterReturnTypeProvider,
        &mockery_mock::MockeryMockReturnTypeProvider,
    ]
}

/// Dispatch a method call to the registered providers, returning the first match.
pub fn dispatch_method_return_type(
    event: &MethodReturnTypeProviderEvent<'_, '_>,
) -> Option<TUnion> {
    for provider in providers() {
        if provider
            .class_names()
            .iter()
            .any(|name| class_matches_or_descends_from(event.analyzer, event.class_id, name))
            && let Some(return_type) = provider.get_method_return_type(event)
        {
            return Some(return_type);
        }
    }

    None
}

/// Whether `class_id` is `target_class` or one of its descendants.
pub(super) fn class_matches_or_descends_from(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    target_class: &str,
) -> bool {
    let direct = resolve_class_id(analyzer, target_class);
    if class_id == direct {
        return true;
    }

    analyzer
        .codebase
        .all_classlike_descendants
        .get(&direct)
        .is_some_and(|descendants| descendants.contains(&class_id))
}

pub(super) fn resolve_class_id(analyzer: &StatementsAnalyzer<'_>, class_name: &str) -> StrId {
    let direct = analyzer.interner.intern(class_name);
    if analyzer.codebase.get_class(direct).is_some() {
        return direct;
    }

    let fq = analyzer.interner.intern(&format!("\\{}", class_name));
    if analyzer.codebase.get_class(fq).is_some() {
        return fq;
    }

    direct
}
