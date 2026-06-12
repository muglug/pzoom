# Pzoom ↔ Psalm / Hakana Catalog

Current-state mapping of pzoom Rust sources to their Psalm (PascalCase PHP, `../psalm/src/Psalm/`) and Hakana (snake_case Rust, `../hakana/src/`) equivalents, with a code-line comparison.

The machine-readable, authoritative pzoom→Psalm map for the `pzoom-analyzer` and `pzoom-code-info` crates is **`PSALM_FILE_MAP.json`** (full Psalm paths; `null` = pzoom-specific; a list = one pzoom file covering several small Psalm files). It is consumed by `scripts/psalm_parity.py`; the table below is the human-readable catalog over **all** crates and mirrors the JSON for the two scope crates.

Conventions: pzoom/Hakana share snake_case file & function names; Psalm uses PascalCase classes / camelCase methods. pzoom/Hakana `*_info.rs` ↔ Psalm `Storage/*Storage.php`. Blank = no equivalent / pzoom-specific. `[STUB]` = scaffolded, not implemented.


## File mapping (250 pzoom files)

| Pzoom file | Hakana | Psalm |
|---|---|---|
| `crates/pzoom-analyzer/src/algebra_analyzer.rs` | algebra_analyzer.rs | AlgebraAnalyzer.php |
| `crates/pzoom-analyzer/src/assertion_finder.rs` | assertion_finder.rs | AssertionFinder.php |
| `crates/pzoom-analyzer/src/class_casing.rs` |  |  |
| `crates/pzoom-analyzer/src/config.rs` | config.rs | Config.php |
| `crates/pzoom-analyzer/src/context.rs` | context.rs | Context.php |
| `crates/pzoom-analyzer/src/data_flow.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/array_analyzer.rs` |  | ArrayAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/assignment/array_assignment_analyzer.rs` | array_assignment_analyzer.rs | ArrayAssignmentAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/assignment/instance_property_assignment_analyzer.rs` | instance_property_assignment_analyzer.rs | InstancePropertyAssignmentAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/assignment/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/assignment/static_property_assignment_analyzer.rs` | static_property_assignment_analyzer.rs | StaticPropertyAssignmentAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/assignment_analyzer.rs` | assignment_analyzer.rs | AssignmentAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/and_analyzer.rs` | and_analyzer.rs | AndAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/arithmetic_analyzer.rs` | arithmetic_analyzer.rs |  |
| `crates/pzoom-analyzer/src/expr/binop/arithmetic_op_analyzer.rs` |  | ArithmeticOpAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/coalesce_analyzer.rs` | coalesce_analyzer.rs | CoalesceAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/concat_analyzer.rs` | concat_analyzer.rs | ConcatAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/binop/non_comparison_op_analyzer.rs` |  | NonComparisonOpAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop/or_analyzer.rs` | or_analyzer.rs | OrAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/binop_analyzer.rs` | binop_analyzer.rs |  |
| `crates/pzoom-analyzer/src/expr/call/argument_analyzer.rs` | argument_analyzer.rs | ArgumentAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/arguments_analyzer.rs` | arguments_analyzer.rs | ArgumentsAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/array_function_arguments_analyzer.rs` |  | ArrayFunctionArgumentsAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/atomic_method_call_analyzer.rs` | atomic_method_call_analyzer.rs | AtomicMethodCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/atomic_static_call_analyzer.rs` | atomic_static_call_analyzer.rs | AtomicStaticCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/callable_validation.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/call/class_template_param_collector.rs` | class_template_param_collector.rs | ClassTemplateParamCollector.php |
| `crates/pzoom-analyzer/src/expr/call/existing_atomic_method_call_analyzer.rs` | existing_atomic_method_call_analyzer.rs | ExistingAtomicMethodCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/existing_atomic_static_call_analyzer.rs` |  | ExistingAtomicStaticCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/function_call_analyzer.rs` | function_call_analyzer.rs | FunctionCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/function_call_assertion_analyzer.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/call/function_call_return_type_fetcher.rs` | function_call_return_type_fetcher.rs | FunctionCallReturnTypeFetcher.php |
| `crates/pzoom-analyzer/src/expr/call/impure_functions_list.rs` |  | ImpureFunctionsList.php |
| `crates/pzoom-analyzer/src/expr/call/method_call_analyzer.rs` |  | MethodCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/method_call_prohibition_analyzer.rs` |  | MethodCallProhibitionAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/method_call_purity_analyzer.rs` |  | MethodCallPurityAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/method_call_return_type_fetcher.rs` | method_call_return_type_fetcher.rs | MethodCallReturnTypeFetcher.php |
| `crates/pzoom-analyzer/src/expr/call/method_visibility_analyzer.rs` |  | MethodVisibilityAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/missing_method_call_handler.rs` |  | MissingMethodCallHandler.php |
| `crates/pzoom-analyzer/src/expr/call/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/call/named_function_call_handler.rs` |  | NamedFunctionCallHandler.php |
| `crates/pzoom-analyzer/src/expr/call/new_analyzer.rs` | new_analyzer.rs | NewAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/static_call_analyzer.rs` | static_call_analyzer.rs | StaticCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call_analyzer.rs` | call_analyzer.rs | CallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/cast_analyzer.rs` | cast_analyzer.rs | CastAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/clone_analyzer.rs` |  | CloneAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/closure_analyzer.rs` | closure_analyzer.rs | ClosureAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/const_fetch_analyzer.rs` | const_fetch_analyzer.rs | ConstFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/echo_analyzer.rs` | echo_analyzer.rs | EchoAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/exit_analyzer.rs` | exit_analyzer.rs | ExitAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/array_fetch_analyzer.rs` | array_fetch_analyzer.rs | ArrayFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/atomic_property_fetch_analyzer.rs` | atomic_property_fetch_analyzer.rs | AtomicPropertyFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/class_constant_fetch_analyzer.rs` | class_constant_fetch_analyzer.rs | ClassConstAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/instance_property_fetch_analyzer.rs` | instance_property_fetch_analyzer.rs | InstancePropertyFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/fetch/static_property_fetch_analyzer.rs` | static_property_fetch_analyzer.rs | StaticPropertyFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/include_analyzer.rs` | include_analyzer.rs | IncludeAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/isset_analyzer.rs` | isset_analyzer.rs | IssetAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/match_analyzer.rs` |  | MatchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/partial_application_analyzer.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/ternary_analyzer.rs` | ternary_analyzer.rs | TernaryAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/throw_analyzer.rs` |  | ThrowAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/unop_analyzer.rs` | unop_analyzer.rs | UnaryPlusMinusAnalyzer.php + BooleanNotAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/variable_fetch_analyzer.rs` | variable_fetch_analyzer.rs | VariableFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/yield_analyzer.rs` | yield_analyzer.rs | YieldAnalyzer.php |
| `crates/pzoom-analyzer/src/expression_analyzer.rs` | expression_analyzer.rs | ExpressionAnalyzer.php |
| `crates/pzoom-analyzer/src/expression_identifier.rs` | expression_identifier.rs | ExpressionIdentifier.php |
| `crates/pzoom-analyzer/src/file_analyzer.rs` | file_analyzer.rs | FileAnalyzer.php |
| `crates/pzoom-analyzer/src/formula_generator.rs` | formula_generator.rs | FormulaGenerator.php |
| `crates/pzoom-analyzer/src/function_analysis_data.rs` | function_analysis_data.rs | FunctionLikeAnalyzer.php |
| `crates/pzoom-analyzer/src/function_like_analyzer.rs` | functionlike_analyzer.rs | FunctionLikeAnalyzer.php |
| `crates/pzoom-analyzer/src/internal_access.rs` |  |  |
| `crates/pzoom-analyzer/src/issue_suppression.rs` |  |  |
| `crates/pzoom-analyzer/src/lib.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/params_provider/function/array_filter.rs` |  | ArrayFilterParamsProvider.php |
| `crates/pzoom-analyzer/src/params_provider/function/array_multisort.rs` |  | ArrayMultisortParamsProvider.php |
| `crates/pzoom-analyzer/src/params_provider/function/array_u_array.rs` |  | ArrayUArrayParamsProvider.php |
| `crates/pzoom-analyzer/src/params_provider/function/min_max.rs` |  |  |
| `crates/pzoom-analyzer/src/params_provider/function/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/params_provider/method/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/params_provider/method/pdo_statement_set_fetch_mode.rs` |  | PdoStatementSetFetchMode.php |
| `crates/pzoom-analyzer/src/params_provider/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/psalm_baseline.rs` |  | ErrorBaseline.php |
| `crates/pzoom-analyzer/src/psalm_config.rs` |  | Config.php |
| `crates/pzoom-analyzer/src/reconciler/assertion_reconciler.rs` | assertion_reconciler.rs | AssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/macros.rs` | macros.rs |  |
| `crates/pzoom-analyzer/src/reconciler/mod.rs` | (module root) | Reconciler.php |
| `crates/pzoom-analyzer/src/reconciler/negated_assertion_reconciler.rs` | negated_assertion_reconciler.rs | NegatedAssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/simple_assertion_reconciler.rs` | simple_assertion_reconciler.rs | SimpleAssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/simple_negated_assertion_reconciler.rs` | simple_negated_assertion_reconciler.rs | SimpleNegatedAssertionReconciler.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_column.rs` |  | ArrayColumnReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_combine.rs` |  | ArrayCombineReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_fill.rs` |  | ArrayFillReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_filter.rs` |  | ArrayFilterReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_key_first_last.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_keys.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_map.rs` |  | ArrayMapReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_merge.rs` |  | ArrayMergeReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_pointer.rs` |  | ArrayPointerAdjustmentReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_rand.rs` |  | ArrayRandReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_reduce.rs` |  | ArrayReduceReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_reverse.rs` |  | ArrayReverseReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_splice.rs` |  | ArraySpliceReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_values.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/call_user_func.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/count.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/filter_var.rs` |  | FilterVarReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/get_object_vars.rs` |  | GetObjectVarsReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/hrtime.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/is_a.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/iterator_to_array.rs` |  | IteratorToArrayReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/microtime.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/min_max.rs` |  | MinMaxReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/parse_url.rs` |  | ParseUrlReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/preg_replace.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/preg_split.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/rand.rs` |  | RandReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/range.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/simple.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/sprintf.rs` |  | SprintfReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/str_replace.rs` |  | StrReplaceReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/trigger_error.rs` |  | TriggerErrorReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/type_check.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/var_export.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/date_time.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/dom_document.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/dom_node.rs` |  | DomNodeAppendChild.php |
| `crates/pzoom-analyzer/src/return_type_provider/method/message_formatter.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/mockery_mock.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/pdo_statement.rs` |  | PdoStatementReturnTypeProvider.php |
| `crates/pzoom-analyzer/src/return_type_provider/method/simple_xml_element.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/scope/if_conditional_scope.rs` | if_conditional_scope.rs | IfConditionalScope.php |
| `crates/pzoom-analyzer/src/scope/if_scope.rs` | if_scope.rs | IfScope.php |
| `crates/pzoom-analyzer/src/scope/loop_scope.rs` | loop_scope.rs | LoopScope.php |
| `crates/pzoom-analyzer/src/scope/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/scope/switch_scope.rs` | switch_scope.rs | SwitchScope.php |
| `crates/pzoom-analyzer/src/statements_analyzer.rs` | statements_analyzer.rs | StatementsAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/attribute_analyzer.rs` |  | AttributesAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/break_analyzer.rs` | break_analyzer.rs | BreakAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/class_analyzer.rs` |  | ClassAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/continue_analyzer.rs` | continue_analyzer.rs | ContinueAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/do_analyzer.rs` | do_analyzer.rs | DoAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/echo_analyzer.rs` | echo_analyzer.rs | EchoAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/else_analyzer.rs` | else_analyzer.rs | ElseAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/elseif_analyzer.rs` | elseif_analyzer.rs | ElseIfAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/expression_stmt_analyzer.rs` |  |  |
| `crates/pzoom-analyzer/src/stmt/for_analyzer.rs` | for_analyzer.rs | ForAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/foreach_analyzer.rs` | foreach_analyzer.rs | ForeachAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/function_analyzer.rs` |  | FunctionAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/global_analyzer.rs` |  | GlobalAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/if_conditional_analyzer.rs` | if_conditional_analyzer.rs | IfConditionalAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/if_else_analyzer.rs` | if_analyzer.rs | IfElseAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/loop_/assignment_map_visitor.rs` | assignment_map_visitor.rs | AssignmentMapVisitor.php |
| `crates/pzoom-analyzer/src/stmt/loop_/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/stmt/loop_/tast_cleaner.rs` | tast_cleaner.rs |  |
| `crates/pzoom-analyzer/src/stmt/loop_analyzer.rs` | loop_analyzer.rs | LoopAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/stmt/return_analyzer.rs` | return_analyzer.rs | ReturnAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/scope_analyzer.rs` | scope_analyzer.rs | ScopeAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/static_analyzer.rs` |  | StaticAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/switch_analyzer.rs` | switch_analyzer.rs | SwitchAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/switch_case_analyzer.rs` | switch_case_analyzer.rs | SwitchCaseAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/try_analyzer.rs` | try_analyzer.rs | TryAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/unset_analyzer.rs` |  | UnsetAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/while_analyzer.rs` | while_analyzer.rs | WhileAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt_analyzer.rs` | stmt_analyzer.rs | StatementsAnalyzer.php |
| `crates/pzoom-analyzer/src/taint_analyzer.rs` |  | TaintFlowGraph.php |
| `crates/pzoom-analyzer/src/template/inferred_type_replacer.rs` | inferred_type_replacer.rs | TemplateInferredTypeReplacer.php |
| `crates/pzoom-analyzer/src/template/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/template/standin_type_replacer.rs` | standin_type_replacer.rs | TemplateStandinTypeReplacer.php |
| `crates/pzoom-analyzer/src/type_comparator/array_type_comparator.rs` |  | ArrayTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/atomic_type_comparator.rs` | atomic_type_comparator.rs | AtomicTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/callable_type_comparator.rs` |  | CallableTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/class_like_string_comparator.rs` |  | ClassLikeStringComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/generic_type_comparator.rs` | generic_type_comparator.rs | GenericTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/integer_range_comparator.rs` |  | IntegerRangeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/keyed_array_comparator.rs` |  | KeyedArrayComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/type_comparator/object_type_comparator.rs` | object_type_comparator.rs | ObjectComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/scalar_type_comparator.rs` | scalar_type_comparator.rs | ScalarTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/type_comparison_result.rs` | type_comparison_result.rs | TypeComparisonResult.php |
| `crates/pzoom-analyzer/src/type_comparator/union_type_comparator.rs` | union_type_comparator.rs | UnionTypeComparator.php |
| `crates/pzoom-analyzer/src/type_expander.rs` | type_expander.rs | TypeExpander.php |
| `crates/pzoom-analyzer/src/unused_variable_analyzer.rs` | unused_variable_analyzer.rs | VariableUseGraph.php |
| `crates/pzoom-cli/src/main.rs` | (module root) |  |
| `crates/pzoom-cli/src/shortcodes.rs` |  |  |
| `crates/pzoom-code-info/src/algebra/clause.rs` | clause.rs | Clause.php |
| `crates/pzoom-code-info/src/algebra/mod.rs` | (module root) | Algebra.php |
| `crates/pzoom-code-info/src/assertion.rs` | assertion.rs | Assertion.php |
| `crates/pzoom-code-info/src/class_constant_info.rs` | class_constant_info.rs | ClassConstantStorage.php |
| `crates/pzoom-code-info/src/class_like_info.rs` | classlike_info.rs | ClassLikeStorage.php |
| `crates/pzoom-code-info/src/class_type_alias.rs` | class_type_alias.rs | ClassTypeAlias.php |
| `crates/pzoom-code-info/src/code_location.rs` | code_location.rs | CodeLocation.php |
| `crates/pzoom-code-info/src/codebase_info.rs` |  | Codebase.php |
| `crates/pzoom-code-info/src/data_flow/graph.rs` | graph.rs | DataFlowGraph.php |
| `crates/pzoom-code-info/src/data_flow/mod.rs` | (module root) |  |
| `crates/pzoom-code-info/src/data_flow/node.rs` | node.rs | DataFlowNode.php |
| `crates/pzoom-code-info/src/data_flow/path.rs` | path.rs | Path.php |
| `crates/pzoom-code-info/src/data_flow/tainted_node.rs` | tainted_node.rs | TaintSink.php + TaintSource.php |
| `crates/pzoom-code-info/src/file_info.rs` | file_info.rs | FileStorage.php |
| `crates/pzoom-code-info/src/functionlike_info.rs` | functionlike_info.rs | FunctionLikeStorage.php |
| `crates/pzoom-code-info/src/issue.rs` | issue.rs | CodeIssue.php |
| `crates/pzoom-code-info/src/lib.rs` | (module root) |  |
| `crates/pzoom-code-info/src/member_visibility.rs` | member_visibility.rs |  |
| `crates/pzoom-code-info/src/method_identifier.rs` | method_identifier.rs | MethodIdentifier.php |
| `crates/pzoom-code-info/src/property_info.rs` | property_info.rs | PropertyStorage.php |
| `crates/pzoom-code-info/src/runtime_constants.rs` |  |  |
| `crates/pzoom-code-info/src/symbol.rs` |  |  |
| `crates/pzoom-code-info/src/t_atomic.rs` | t_atomic.rs | Atomic.php |
| `crates/pzoom-code-info/src/t_union.rs` | t_union.rs | Union.php |
| `crates/pzoom-code-info/src/ttype/key_value_of.rs` |  | TKeyOf.php + TValueOf.php |
| `crates/pzoom-code-info/src/ttype/mod.rs` | (module root) | Type.php |
| `crates/pzoom-code-info/src/ttype/template/mod.rs` | (module root) | TemplateResult.php + TemplateBound.php |
| `crates/pzoom-code-info/src/ttype/type_combination.rs` | type_combination.rs | TypeCombination.php |
| `crates/pzoom-code-info/src/ttype/type_combiner.rs` | type_combiner.rs | TypeCombiner.php |
| `crates/pzoom-code-info/src/type_resolution.rs` | type_resolution.rs |  |
| `crates/pzoom-code-info/src/var_name.rs` | var_name.rs |  |
| `crates/pzoom-orchestrator/src/analyzer.rs` | analyzer.rs | Analyzer.php |
| `crates/pzoom-orchestrator/src/ast_differ.rs` | ast_differ.rs | AstDiffer.php |
| `crates/pzoom-orchestrator/src/cache.rs` | cache.rs | Cache.php |
| `crates/pzoom-orchestrator/src/callmap.rs` |  |  |
| `crates/pzoom-orchestrator/src/extensions.rs` |  |  |
| `crates/pzoom-orchestrator/src/lib.rs` | (module root) |  |
| `crates/pzoom-orchestrator/src/populator.rs` | populator.rs | Populator.php |
| `crates/pzoom-orchestrator/src/scanner.rs` | scanner.rs | Scanner.php |
| `crates/pzoom-str/build.rs` | build.rs |  |
| `crates/pzoom-str/src/lib.rs` | (module root) |  |
| `crates/pzoom-syntax/src/declaration_collector/classlike_scanner.rs` | classlike_scanner.rs |  |
| `crates/pzoom-syntax/src/declaration_collector/functionlike_scanner.rs` | functionlike_scanner.rs |  |
| `crates/pzoom-syntax/src/declaration_collector/initializer_summary.rs` |  |  |
| `crates/pzoom-syntax/src/declaration_collector/mod.rs` | (module root) |  |
| `crates/pzoom-syntax/src/declaration_collector/simple_type_inferer.rs` | simple_type_inferer.rs | SimpleTypeInferer.php |
| `crates/pzoom-syntax/src/declaration_collector/taint_scanner.rs` |  |  |
| `crates/pzoom-syntax/src/docblock/mod.rs` | (module root) |  |
| `crates/pzoom-syntax/src/docblock/parse_tree.rs` |  | ParseTree.php (+ ParseTree/* subclasses) |
| `crates/pzoom-syntax/src/docblock/parse_tree_creator.rs` |  | ParseTreeCreator.php |
| `crates/pzoom-syntax/src/docblock/parsed_docblock.rs` |  | ParsedDocblock.php |
| `crates/pzoom-syntax/src/docblock/type_parser.rs` |  | TypeParser.php |
| `crates/pzoom-syntax/src/docblock/type_tokenizer.rs` |  | TypeTokenizer.php |
| `crates/pzoom-syntax/src/lib.rs` | (module root) |  |
| `crates/pzoom-syntax/src/name_resolver.rs` |  |  |
| `crates/pzoom-syntax/src/property_map.rs` |  |  |
| `crates/pzoom-syntax/src/type_resolver.rs` |  |  |
| `crates/pzoom-test-runner/src/main.rs` | (module root) |  |
| `crates/pzoom-wasm/src/lib.rs` | (module root) |  |

## Present in Psalm + Hakana, absent from pzoom

| Hakana / Psalm name |
|---|
| `aliases.rs` / `Aliases.php` |

## Code-line comparison (pzoom vs Hakana vs Psalm)

Non-blank, non-comment lines per mapped file pair. Rust vs PHP counts aren't directly comparable; the signal is *relative* size. The raw ratio over-counts files where pzoom **distributes** Psalm's bundling across several files (a renamed/split concern, not a gap) or where pzoom intentionally **stubs** a subsystem. Filter those out before reading the ratio as a gap.

### pzoom thinner than Psalm/Hakana (raw ratio)
| pz | hk | psalm | ratio | file |
|---:|---:|---:|---:|---|
| 15 | – | 784 | 52.3× | `code-info/src/ttype/mod.rs` |
| 60 | 1655 | 1865 | 31.1× | `analyzer/src/function_like_analyzer.rs` |
| 10 | 94 | 212 | 21.2× | `orchestrator/src/cache.rs` |
| 81 | 255 | 1077 | 13.3× | `orchestrator/src/analyzer.rs` |
| 143 | 599 | 1865 | 13.0× | `analyzer/src/function_analysis_data.rs` |
| 168 | 11 | 2009 | 12.0× | `analyzer/src/config.rs` |
| 26 | 47 | 290 | 11.2× | `code-info/src/code_location.rs` |
| 36 | – | 231 | 6.4× | `analyzer/src/params_provider/function/array_filter.rs` |
| 161 | 161 | 980 | 6.1× | `analyzer/src/statements_analyzer.rs` |
| 14 | 237 | 84 | 6.0× | `orchestrator/src/ast_differ.rs` |
| 212 | 438 | 899 | 4.2× | `analyzer/src/expr/call_analyzer.rs` |
| 95 | 66 | 382 | 4.0× | `analyzer/src/expr/include_analyzer.rs` |
| 46 | – | 161 | 3.5× | `analyzer/src/expr/call/method_call_purity_analyzer.rs` |

**Not real gaps (distributed across renamed files, or intentional stubs):** `function_like_analyzer` + `function_analysis_data` (Psalm/Hakana's FunctionLikeAnalyzer runs the whole function-like analysis; pzoom distributes that across `function_analyzer`, class-method analysis in `class_analyzer`, `closure_analyzer`, and the `function_analysis_data` body run — both rows share `FunctionLikeAnalyzer.php` in the map, so each looks thin alone). `ttype/mod.rs` (Psalm's `Type.php` is a static helper facade; pzoom keeps the equivalents on `t_union`/`t_atomic`/`type_combiner`, leaving mostly re-exports here). `call_analyzer` (Psalm CallAnalyzer bundles argument checking + `collectSpecialInformation` + `checkMethodArgs`; pzoom splits these into `argument_analyzer`/`arguments_analyzer`/the call analyzers). `statements_analyzer`/`analyzer` (mega-dispatchers; pzoom splits into `stmt_analyzer`+`expression_analyzer` / orchestrator). `code_location.rs` (Psalm's CodeLocation computes selection bounds/columns lazily from the AST; pzoom carries byte spans by design). `config` (Psalm bundles the XML schema + plugin registration in Config.php; pzoom's psalm.xml parsing lives in `psalm_config.rs` (624 lines) and `config.rs` is just the runtime settings struct). `cache`/`ast_differ` (stubs — no incremental-analysis subsystem).

### Genuine remaining logic gaps
| pz | psalm | file | gap |
|---:|---:|---|---|
| 95 | 382 | `expr/include_analyzer.rs` | include path resolution (partly intentional — no filesystem resolution) |
| 36 | 231 | `params_provider/function/array_filter.rs` | the breadth of Psalm's `ArrayFilterParamsProvider` callback-signature synthesis |

For the current file-level gap list against Psalm (whole files with no pzoom port in the scope crates — `FilterUtils.php`, `MethodComparator.php`, `ClassLikeAnalyzer.php`, …) see the auto-generated `docs/PSALM_PARITY_BACKLOG.md`.

### Open gaps / deferred (notes)
- Purity-context modelling is collapsed: `@psalm-pure`/`@psalm-mutation-free`/`@psalm-external-mutation-free`/`@psalm-immutable` are all enforced (the `Impure*`, `MissingImmutableAnnotation` and `MutableDependency` issues; the `PureAnnotation`/`ImmutableAnnotation`/`MutationFree` suites pass), but pzoom carries a single `enforce_mutation_free` context flag where Psalm distinguishes `pure`/`mutation_free`/`external_mutation_free` context modes, and closure purity is inferred by reconstructing from the emitted impurity issues rather than Psalm's threaded `inferred_impure`/`inferred_has_mutation` flags (see `function_like_analyzer.rs` / `method_call_purity_analyzer.rs` module docs).
