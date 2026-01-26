//! Class declaration analyzer.
//!
//! Analyzes method bodies with proper context.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::method::{Method, MethodBody};
use mago_syntax::ast::ast::class_like::Class;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer;

/// Analyze a class declaration.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    analyze_with_namespace(analyzer, class, None, analysis_data, context)
}

/// Analyze a class declaration with a namespace context.
pub fn analyze_with_namespace(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    namespace: Option<&str>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Get the class name - use FQN if in a namespace
    let class_name = class.name.value;
    let fqn = if let Some(ns) = namespace {
        format!("{}\\{}", ns, class_name)
    } else {
        class_name.to_string()
    };
    let class_name_id = analyzer.interner.intern(&fqn);

    // Look up the class info from the codebase
    let class_info = analyzer.codebase.get_class(class_name_id);

    // Check for unimplemented abstract methods (only for non-abstract classes)
    if let Some(info) = class_info {
        if !info.is_abstract {
            check_unimplemented_abstract_methods(analyzer, class, info, analysis_data);
        }
        // Check for missing property types
        check_missing_property_types(analyzer, &fqn, info, analysis_data);
    }

    // Analyze each method in the class
    for member in class.members.iter() {
        if let ClassLikeMember::Method(method) = member {
            analyze_method(
                analyzer,
                method,
                class_name_id,
                class_info,
                context.namespace,
                analysis_data,
            )?;
        }
    }

    Ok(())
}

/// Check for properties without type declarations.
fn check_missing_property_types(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    for (_prop_name, prop_info) in &class_info.properties {
        // Skip properties with explicit type declarations (native PHP types or docblocks)
        if prop_info.has_type() {
            continue;
        }

        // Skip promoted properties (they get their type from constructor param)
        if prop_info.is_promoted {
            continue;
        }

        let prop_name_str = analyzer.interner.lookup(prop_info.name);
        let property_id = format!("{}::${}", class_name, prop_name_str);
        let (line, col) = analyzer.get_line_column(prop_info.start_offset);

        analysis_data.add_issue(Issue::new(
            IssueKind::MissingPropertyType,
            format!("Property {} does not have a declared type", property_id),
            analyzer.file_path,
            prop_info.start_offset,
            prop_info.start_offset + 1,
            line,
            col,
        ));
    }
}

/// Check for unimplemented abstract methods.
fn check_unimplemented_abstract_methods(
    analyzer: &StatementsAnalyzer<'_>,
    class: &Class<'_>,
    class_info: &pzoom_code_info::ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
) {
    // Collect all implemented methods in this class
    let mut implemented_methods = rustc_hash::FxHashSet::default();
    for (method_name, method_info) in &class_info.methods {
        if !method_info.is_abstract {
            implemented_methods.insert(*method_name);
        }
    }

    // Check parent class for abstract methods
    if let Some(parent_name) = class_info.parent_class {
        if let Some(parent_info) = analyzer.codebase.get_class(parent_name) {
            for (method_name, method_info) in &parent_info.methods {
                if method_info.is_abstract && !implemented_methods.contains(method_name) {
                    let method_name_str = analyzer.interner.lookup(*method_name);
                    let parent_name_str = analyzer.interner.lookup(parent_name);
                    let span = class.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UnimplementedAbstractMethod,
                        format!(
                            "Class {} does not implement abstract method {}::{}",
                            class.name.value, parent_name_str, method_name_str
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }
        }
    }

    // Check interfaces for unimplemented methods
    for iface_name in &class_info.interfaces {
        if let Some(iface_info) = analyzer.codebase.get_class(*iface_name) {
            for (method_name, _method_info) in &iface_info.methods {
                if !implemented_methods.contains(method_name) {
                    let method_name_str = analyzer.interner.lookup(*method_name);
                    let iface_name_str = analyzer.interner.lookup(*iface_name);
                    let span = class.span();
                    let (line, col) = analyzer.get_line_column(span.start.offset);
                    analysis_data.add_issue(Issue::new(
                        IssueKind::UnimplementedInterfaceMethod,
                        format!(
                            "Class {} does not implement interface method {}::{}",
                            class.name.value, iface_name_str, method_name_str
                        ),
                        analyzer.file_path,
                        span.start.offset,
                        span.end.offset,
                        line,
                        col,
                    ));
                }
            }
        }
    }
}

/// Analyze a method declaration.
fn analyze_method(
    analyzer: &StatementsAnalyzer<'_>,
    method: &Method<'_>,
    class_name_id: pzoom_str::StrId,
    class_info: Option<&pzoom_code_info::ClassLikeInfo>,
    namespace: Option<pzoom_str::StrId>,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    // Get the method name
    let method_name = method.name.value;
    let method_name_id = analyzer.interner.intern(method_name);

    // Look up the method info from the class
    let method_info = class_info.and_then(|ci| ci.methods.get(&method_name_id));

    // Create a function-like info wrapper for the method
    let func_info = method_info.map(|mi| {
        // Create a temporary FunctionLikeInfo for the method
        pzoom_code_info::FunctionLikeInfo {
            name: method_name_id,
            return_type: mi.return_type.clone(),
            params: mi.params.clone(),
            is_static: mi.is_static,
            declaring_class: Some(class_name_id),
            ..Default::default()
        }
    });

    // Create a new analyzer with the method context
    let method_analyzer = StatementsAnalyzer {
        codebase: analyzer.codebase,
        interner: analyzer.interner,
        function_info: func_info.as_ref(),
        file_path: analyzer.file_path,
        source: analyzer.source,
        resolved_names: analyzer.resolved_names,
    };

    // Create a new context for the method body with namespace preserved
    let mut method_context = BlockContext::new();
    method_context.namespace = namespace;

    // Add $this if not static
    if !method_info.is_some_and(|mi| mi.is_static) {
        let this_type = TUnion::new(pzoom_code_info::TAtomic::TNamedObject {
            name: class_name_id,
            type_params: None,
        });
        let this_id = analyzer.interner.intern("$this");
        method_context.set_var_type(this_id, this_type);
    }

    // Add parameters to context
    for param in method.parameter_list.parameters.iter() {
        let param_name = param.variable.name;
        let param_name_id = analyzer.interner.intern(param_name);

        // Get parameter info from method info
        let param_info = method_info.and_then(|mi| {
            mi.params.iter().find(|p| p.name == param_name_id)
        });

        // Get parameter type - for variadic params, wrap in array type
        let param_type = if let Some(info) = param_info {
            let base_type = info.get_type().cloned().unwrap_or_else(TUnion::mixed);
            if info.is_variadic {
                // Variadic parameters become arrays inside the function body
                TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::int()),
                    value_type: Box::new(base_type),
                })
            } else {
                base_type
            }
        } else {
            TUnion::mixed()
        };

        method_context.set_var_type(param_name_id, param_type);
    }

    // Analyze the method body (only if it has a concrete body)
    if let MethodBody::Concrete(block) = &method.body {
        stmt_analyzer::analyze_stmts(
            &method_analyzer,
            block.statements.as_slice(),
            analysis_data,
            &mut method_context,
        )?;
    }

    Ok(())
}
