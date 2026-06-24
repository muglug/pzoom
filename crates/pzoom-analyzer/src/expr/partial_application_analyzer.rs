//! First-class callable syntax: `strlen(...)`, `$obj->method(...)`,
//! `Foo::bar(...)`.
//!
//! Psalm's FunctionCallAnalyzer / MethodCallAnalyzer handle
//! `isFirstClassCallable()` by typing the expression as a Closure carrying
//! the callee's params and return type. Partial application with argument
//! placeholders (PHP 8.5) is not modeled yet and stays mixed.

use mago_span::HasSpan;
use mago_syntax::cst::cst::expression::Expression;
use mago_syntax::cst::cst::partial_application::PartialApplication;

use pzoom_code_info::{TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

type Pos = (u32, u32);

pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    partial_application: &PartialApplication<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if !partial_application.is_first_class_callable() {
        analysis_data
            .expr_types
            .insert(pos, Rc::new(TUnion::mixed()));
        return;
    }

    // For `C::method(...)`, `static` in the signature binds through the class
    // named at the creation site (Psalm localizes the closure's return).
    let mut static_binding_class: Option<pzoom_str::StrId> = None;
    let function_info = match partial_application {
        PartialApplication::Function(function_pa) => {
            let (name, is_fully_qualified, name_offset) =
                match function_pa.function.unparenthesized() {
                    Expression::Identifier(id) => (
                        Some(pzoom_syntax::bytes_to_str(id.value())),
                        id.is_fully_qualified(),
                        Some(id.span().start.offset),
                    ),
                    _ => (None, false, None),
                };
            if name.is_none() {
                // `$test(...)` — a first-class callable of an invokable
                // value: a Closure passes through; an object with __invoke
                // takes that method's signature (Psalm's FunctionCallAnalyzer
                // first-class branch).
                let callee_pos = expression_analyzer::analyze(
                    analyzer,
                    function_pa.function,
                    analysis_data,
                    context,
                );
                if let Some(callee_type) = analysis_data.expr_types.get(&callee_pos).cloned() {
                    if callee_type
                        .types
                        .iter()
                        .any(|atomic| matches!(atomic, TAtomic::TClosure { .. }))
                    {
                        analysis_data
                            .expr_types
                            .insert(pos, Rc::new((*callee_type).clone()));
                        return;
                    }
                    let invoke_info = callee_type.types.iter().find_map(|atomic| {
                        let TAtomic::TNamedObject { name, .. } = atomic else {
                            return None;
                        };
                        analyzer
                            .codebase
                            .get_class(*name)
                            .and_then(|class_info| {
                                class_info.methods.get(&pzoom_str::StrId::INVOKE)
                            })
                            .map(|method| (**method).clone())
                    });
                    if let Some(invoke_info) = invoke_info {
                        Some(invoke_info)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                name.and_then(|name| {
                    let resolved = crate::expr::call::function_call_analyzer::resolve_function(
                        analyzer,
                        name,
                        is_fully_qualified,
                        name_offset,
                        context,
                    )
                    .cloned();
                    // `unknown(...)` is a Closure of an undefined function: Psalm
                    // reports it like a plain call (FunctionCallAnalyzer's
                    // function-existence check runs before the first-class
                    // branch). Mirror the call analyzer's guards.
                    if resolved.is_none()
                        && !crate::expr::call::function_call_analyzer::is_language_construct(name)
                        && !crate::expr::call::function_call_analyzer::is_function_guarded_by_function_exists(
                            context, name,
                        )
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(pzoom_code_info::Issue::new(
                            pzoom_code_info::IssueKind::UndefinedFunction,
                            crate::class_casing::undefined_function_message(
                                analyzer,
                                name,
                                context.namespace,
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                    resolved
                })
            }
        }
        PartialApplication::Method(method_pa) => {
            // Type the receiver (for dataflow and member resolution).
            expression_analyzer::analyze(analyzer, method_pa.object, analysis_data, context);
            let object_pos = (
                method_pa.object.span().start.offset,
                method_pa.object.span().end.offset,
            );
            let object_type = analysis_data
                .expr_types
                .get(&object_pos)
                .cloned()
                .map(|t| (*t).clone());
            let method_name = match &method_pa.method {
                mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector::Identifier(
                    id,
                ) => Some(pzoom_syntax::bytes_to_str(id.value).to_string()),
                // `$test->$name(...)` — a literal-string selector resolves
                // like an identifier (Psalm's getSingleStringLiteral).
                mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector::Variable(
                    var,
                ) => {
                    let var_pos = expression_analyzer::analyze(
                        analyzer,
                        &Expression::Variable(var.clone()),
                        analysis_data,
                        context,
                    );
                    analysis_data
                        .expr_types
                        .get(&var_pos)
                        .cloned()
                        .and_then(|selector_type| match selector_type.get_single() {
                            Some(TAtomic::TLiteralString { value }) => Some(value.clone()),
                            _ => None,
                        })
                }
                _ => None,
            };

            match (object_type, method_name) {
                (Some(object_type), Some(method_name)) => {
                    let resolved_with_class = object_type
                        .types
                        .iter()
                        .find_map(|atomic| {
                            let TAtomic::TNamedObject {
                                name, type_params, ..
                            } = atomic
                            else {
                                return None;
                            };
                            let class_info = analyzer.codebase.get_class(*name)?;
                            crate::expr::call::atomic_method_call_analyzer::resolve_named_object_instance_method(
                                analyzer,
                                class_info,
                                type_params.as_deref(),
                                &method_name,
                                Some(&analysis_data.type_variable_bounds),
                            )
                            .map(|(_, _, method_info)| (*name, method_info, class_info, type_params.clone()))
                        });

                    // A first-class callable `$obj->method(...)` *uses* the
                    // method, so record it for find_unused_code (Psalm records
                    // the reference through Methods::isMethodUsed regardless of
                    // whether the closure is ever invoked).
                    if analyzer.config.find_unused_code
                        && let Some((receiver_id, method_info, _, _)) = resolved_with_class.as_ref()
                    {
                        crate::expr::call::atomic_method_call_analyzer::record_method_reference(
                            analyzer,
                            *receiver_id,
                            method_info.declaring_class,
                            &method_name,
                            context,
                            analysis_data,
                        );
                    }

                    let resolved =
                        resolved_with_class.map(|(_, mut method_info, class_info, type_params)| {
                            // Localize the declaring class's templates through
                            // the receiver's type params, so the resulting
                            // closure signature carries the receiver's bindings
                            // (`(new SplQueue())->enqueue(...)` takes the
                            // queue's value type, not an abstract TValue).
                            let mut template_result = crate::expr::call::function_call_analyzer::
                                infer_class_template_replacements_from_type_params(
                                    class_info,
                                    type_params.as_deref(),
                                );
                            crate::expr::call::function_call_analyzer::
                                infer_class_template_replacements_from_extended_params(
                                    &mut template_result,
                                    class_info,
                                );
                            if !crate::template::template_result_is_empty(&template_result) {
                                for param in method_info.params.iter_mut() {
                                    if let Some(param_type) = param.param_type.as_ref() {
                                        param.param_type =
                                            Some(crate::template::inferred_type_replacer::replace(
                                                param_type,
                                                &template_result,
                                            ));
                                    }
                                    if let Some(signature_type) = param.signature_type.as_ref() {
                                        param.signature_type =
                                            Some(crate::template::inferred_type_replacer::replace(
                                                signature_type,
                                                &template_result,
                                            ));
                                    }
                                }
                                if let Some(return_type) = method_info.return_type.as_ref() {
                                    method_info.return_type =
                                        Some(crate::template::inferred_type_replacer::replace(
                                            return_type,
                                            &template_result,
                                        ));
                                }
                                if let Some(signature_return) =
                                    method_info.signature_return_type.as_ref()
                                {
                                    method_info.signature_return_type =
                                        Some(crate::template::inferred_type_replacer::replace(
                                            signature_return,
                                            &template_result,
                                        ));
                                }
                            }
                            method_info
                        });

                    // Psalm reports UndefinedMethod for a first-class callable
                    // of a method the (sealed) receiver class lacks.
                    if resolved.is_none() {
                        for atomic in &object_type.types {
                            let TAtomic::TNamedObject { name, .. } = atomic else {
                                continue;
                            };
                            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                                continue;
                            };
                            // A __call class without the pseudo method gets
                            // UndefinedMagicMethod (Psalm); other sealed
                            // classes get UndefinedMethod.
                            let has_magic_call =
                                crate::expr::call::existing_atomic_static_call_analyzer::class_has_magic_call(class_info);
                            let kind = if has_magic_call {
                                let method_id = analyzer
                                    .interner
                                    .find(&method_name)
                                    .unwrap_or(pzoom_str::StrId::EMPTY);
                                if class_info.pseudo_methods.contains_key(&method_id) {
                                    continue;
                                }
                                pzoom_code_info::IssueKind::UndefinedMagicMethod
                            } else {
                                pzoom_code_info::IssueKind::UndefinedMethod
                            };
                            let (line, col) = analyzer.get_line_column(pos.0);
                            analysis_data.add_issue(pzoom_code_info::Issue::new(
                                kind,
                                format!(
                                    "Method {}::{} does not exist",
                                    analyzer.interner.lookup(*name),
                                    method_name
                                ),
                                analyzer.file_path,
                                pos.0,
                                pos.1,
                                line,
                                col,
                            ));
                            break;
                        }
                    }

                    resolved
                }
                _ => None,
            }
        }
        PartialApplication::StaticMethod(static_pa) => {
            let class_id = match static_pa.class.unparenthesized() {
                Expression::Identifier(id) => analyzer
                    .get_resolved_name(id.span().start.offset)
                    .or_else(|| {
                        Some(
                            analyzer
                                .interner
                                .find(pzoom_syntax::bytes_to_str(id.value()))
                                .unwrap_or(pzoom_str::StrId::EMPTY),
                        )
                    }),
                Expression::Self_(_) | Expression::Static(_) => analyzer.get_declaring_class(),
                // `$class::method(...)` — the receiver's class-string type
                // names the class whose method signature the closure takes
                // (Psalm resolves these through the expression type).
                class_expr => {
                    let class_pos = crate::expression_analyzer::analyze(
                        analyzer,
                        class_expr,
                        analysis_data,
                        context,
                    );
                    analysis_data
                        .expr_types
                        .get(&class_pos)
                        .cloned()
                        .and_then(|class_type| {
                            class_type.types.iter().find_map(|atomic| match atomic {
                                TAtomic::TClassString {
                                    as_type: Some(bound),
                                } => {
                                    if let TAtomic::TNamedObject { name, .. } = bound.as_ref() {
                                        Some(*name)
                                    } else {
                                        None
                                    }
                                }
                                TAtomic::TLiteralClassString { name } => Some(
                                    analyzer
                                        .interner
                                        .find(name.trim_start_matches('\\'))
                                        .unwrap_or(pzoom_str::StrId::EMPTY),
                                ),
                                TAtomic::TNamedObject { name, .. } => Some(*name),
                                _ => None,
                            })
                        })
                }
            };
            let method_name = match &static_pa.method {
                mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector::Identifier(
                    id,
                ) => Some(pzoom_syntax::bytes_to_str(id.value).to_string()),
                // `Test::$name(...)` — a literal-string selector resolves
                // like an identifier.
                mago_syntax::cst::cst::class_like::member::ClassLikeMemberSelector::Variable(
                    var,
                ) => {
                    let var_pos = expression_analyzer::analyze(
                        analyzer,
                        &Expression::Variable(var.clone()),
                        analysis_data,
                        context,
                    );
                    analysis_data
                        .expr_types
                        .get(&var_pos)
                        .cloned()
                        .and_then(|selector_type| match selector_type.get_single() {
                            Some(TAtomic::TLiteralString { value }) => Some(value.clone()),
                            _ => None,
                        })
                }
                _ => None,
            };
            match (class_id, method_name) {
                (Some(class_id), Some(method_name)) => {
                    static_binding_class = Some(class_id);
                    let class_info = analyzer.codebase.get_class(class_id);
                    let resolved = class_info.and_then(|class_info| {
                        // Static resolution includes pseudo (@method) methods
                        // behind __callStatic.
                        crate::expr::call::existing_atomic_static_call_analyzer::resolve_named_object_static_method(
                            analyzer,
                            class_info,
                            &method_name,
                        )
                        .map(|(_, _, method_info, _)| method_info)
                        .or_else(|| {
                            crate::expr::call::atomic_method_call_analyzer::resolve_named_object_instance_method(
                                analyzer,
                                class_info,
                                None,
                                &method_name,
                                Some(&analysis_data.type_variable_bounds),
                            )
                            .map(|(_, _, method_info)| method_info)
                        })
                    });

                    // A first-class callable `Class::method(...)` *uses* the
                    // method, so record it for find_unused_code (mirrors the
                    // instance-method branch above and Psalm's behavior).
                    if analyzer.config.find_unused_code
                        && let Some(method_info) = resolved.as_ref()
                    {
                        crate::expr::call::atomic_method_call_analyzer::record_method_reference(
                            analyzer,
                            class_id,
                            method_info.declaring_class,
                            &method_name,
                            context,
                            analysis_data,
                        );
                    }

                    // Psalm reports UndefinedMethod for a first-class callable
                    // of a static method the known class lacks.
                    if resolved.is_none()
                        && let Some(class_info) = class_info
                        && !crate::expr::call::existing_atomic_static_call_analyzer::class_has_magic_callstatic(class_info)
                    {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(pzoom_code_info::Issue::new(
                            pzoom_code_info::IssueKind::UndefinedMethod,
                            format!(
                                "Method {}::{} does not exist",
                                analyzer.interner.lookup(class_id),
                                method_name
                            ),
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }

                    resolved
                }
                _ => None,
            }
        }
    };

    let Some(function_info) = function_info else {
        // Unknown callee: still a closure, just an unspecified one.
        analysis_data.expr_types.insert(
            pos,
            Rc::new(TUnion::new(TAtomic::TClosure {
                params: None,
                return_type: None,
                is_pure: None,
            })),
        );
        return;
    };

    let mut return_type = function_info.get_return_type().cloned();
    if let Some(return_type) = return_type.as_mut() {
        let static_class = static_binding_class.or(function_info.declaring_class);
        crate::type_expander::expand_union(
            analyzer.codebase,
            analyzer.interner,
            return_type,
            &crate::type_expander::TypeExpansionOptions {
                self_class: function_info.declaring_class,
                static_class_type: match static_class {
                    Some(class_id) => crate::type_expander::StaticClassType::Name(class_id),
                    None => crate::type_expander::StaticClassType::None,
                },
                // The class is named literally at the creation site, so the
                // late-static binding is firm.
                function_is_final: static_binding_class.is_some(),
                ..Default::default()
            },
        );
    }

    let params = function_info
        .params
        .iter()
        .map(|param| pzoom_code_info::FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect();

    analysis_data.expr_types.insert(
        pos,
        Rc::new(TUnion::new(TAtomic::TClosure {
            params: Some(params),
            return_type: return_type.map(Box::new),
            is_pure: Some(function_info.is_pure),
        })),
    );
}
