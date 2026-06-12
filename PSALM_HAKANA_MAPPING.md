# Pzoom ↔ Psalm / Hakana Catalog

Current-state mapping of pzoom Rust sources to their Psalm (PascalCase PHP, `../psalm/src/Psalm/`) and Hakana (snake_case Rust, `../hakana/hakana-core/src/`) equivalents, with a code-line (cloc) comparison.

Conventions: pzoom/Hakana share snake_case file & function names; Psalm uses PascalCase classes / camelCase methods. pzoom/Hakana `*_info.rs` ↔ Psalm `Storage/*Storage.php`. Blank = no equivalent / pzoom-specific. `[STUB]` = scaffolded, not implemented.


## File mapping (207 pzoom files)

| Pzoom file | Hakana | Psalm |
|---|---|---|
| `crates/pzoom-analyzer/src/algebra_analyzer.rs` | algebra_analyzer.rs | AlgebraAnalyzer.php |
| `crates/pzoom-analyzer/src/assertion_finder.rs` | assertion_finder.rs | AssertionFinder.php |
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
| `crates/pzoom-analyzer/src/expr/call/atomic_method_call_analyzer.rs` | atomic_method_call_analyzer.rs | AtomicMethodCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/atomic_static_call_analyzer.rs` | atomic_static_call_analyzer.rs | AtomicStaticCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/callable_validation.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/call/class_template_param_collector.rs` | class_template_param_collector.rs | ClassTemplateParamCollector.php |
| `crates/pzoom-analyzer/src/expr/call/existing_atomic_method_call_analyzer.rs` | existing_atomic_method_call_analyzer.rs | ExistingAtomicMethodCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/existing_atomic_static_call_analyzer.rs` |  | ExistingAtomicStaticCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/function_call_analyzer.rs` | function_call_analyzer.rs | FunctionCallAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/call/function_call_assertion_analyzer.rs` |  |  |
| `crates/pzoom-analyzer/src/expr/call/function_call_return_type_fetcher.rs` | function_call_return_type_fetcher.rs | FunctionCallReturnTypeFetcher.php |
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
| `crates/pzoom-analyzer/src/expr/fetch/class_constant_fetch_analyzer.rs` | class_constant_fetch_analyzer.rs |  |
| `crates/pzoom-analyzer/src/expr/fetch/instance_property_fetch_analyzer.rs` | instance_property_fetch_analyzer.rs | InstancePropertyFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/fetch/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/fetch/static_property_fetch_analyzer.rs` | static_property_fetch_analyzer.rs | StaticPropertyFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/include_analyzer.rs` | include_analyzer.rs | IncludeAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/isset_analyzer.rs` | isset_analyzer.rs | IssetAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/match_analyzer.rs` |  | MatchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/expr/ternary_analyzer.rs` | ternary_analyzer.rs | TernaryAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/throw_analyzer.rs` |  | ThrowAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/unop_analyzer.rs` | unop_analyzer.rs |  |
| `crates/pzoom-analyzer/src/expr/variable_fetch_analyzer.rs` | variable_fetch_analyzer.rs | VariableFetchAnalyzer.php |
| `crates/pzoom-analyzer/src/expr/yield_analyzer.rs` | yield_analyzer.rs | YieldAnalyzer.php |
| `crates/pzoom-analyzer/src/expression_analyzer.rs` | expression_analyzer.rs | ExpressionAnalyzer.php |
| `crates/pzoom-analyzer/src/expression_identifier.rs` | expression_identifier.rs | ExpressionIdentifier.php |
| `crates/pzoom-analyzer/src/formula_generator.rs` | formula_generator.rs | FormulaGenerator.php |
| `crates/pzoom-analyzer/src/file_analyzer.rs` | file_analyzer.rs | FileAnalyzer.php |
| `crates/pzoom-analyzer/src/function_analysis_data.rs` | function_analysis_data.rs |  |
| `crates/pzoom-analyzer/src/function_like_analyzer.rs` | functionlike_analyzer.rs | FunctionLikeAnalyzer.php |
| `crates/pzoom-analyzer/src/internal_access.rs` |  |  |
| `crates/pzoom-analyzer/src/issue_suppression.rs` |  |  |
| `crates/pzoom-analyzer/src/lib.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/psalm_baseline.rs` |  |  |
| `crates/pzoom-analyzer/src/psalm_config.rs` |  |  |
| `crates/pzoom-analyzer/src/reconciler/assertion_reconciler.rs` | assertion_reconciler.rs | AssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/reconciler/negated_assertion_reconciler.rs` | negated_assertion_reconciler.rs | NegatedAssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/simple_assertion_reconciler.rs` | simple_assertion_reconciler.rs | SimpleAssertionReconciler.php |
| `crates/pzoom-analyzer/src/reconciler/simple_negated_assertion_reconciler.rs` | simple_negated_assertion_reconciler.rs | SimpleNegatedAssertionReconciler.php |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_combine.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_fill.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_filter.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_key_first_last.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_keys.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_map.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_merge.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_pointer.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/array_values.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/count.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/hrtime.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/is_a.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/iterator_to_array.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/microtime.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/preg_replace.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/preg_split.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/range.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/simple.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/sprintf.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/str_replace.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/type_check.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/function/var_export.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/date_time.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/dom_document.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/dom_node.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/message_formatter.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/pdo_statement.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/method/simple_xml_element.rs` |  |  |
| `crates/pzoom-analyzer/src/return_type_provider/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/scope/if_conditional_scope.rs` | if_conditional_scope.rs | IfConditionalScope.php |
| `crates/pzoom-analyzer/src/scope/if_scope.rs` | if_scope.rs | IfScope.php |
| `crates/pzoom-analyzer/src/scope/loop_scope.rs` | loop_scope.rs | LoopScope.php |
| `crates/pzoom-analyzer/src/scope/switch_scope.rs` | switch_scope.rs | SwitchScope.php |
| `crates/pzoom-analyzer/src/scope/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/statements_analyzer.rs` | statements_analyzer.rs | StatementsAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/attribute_analyzer.rs` |  |  |
| `crates/pzoom-analyzer/src/stmt/break_analyzer.rs` | break_analyzer.rs | BreakAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/class_analyzer.rs` |  | ClassAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/continue_analyzer.rs` | continue_analyzer.rs | ContinueAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/scope_analyzer.rs` | scope_analyzer.rs | ScopeAnalyzer.php |
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
| `crates/pzoom-analyzer/src/stmt/static_analyzer.rs` |  | StaticAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/switch_analyzer.rs` | switch_analyzer.rs | SwitchAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/switch_case_analyzer.rs` | switch_case_analyzer.rs | SwitchCaseAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/try_analyzer.rs` | try_analyzer.rs | TryAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/unset_analyzer.rs` |  | UnsetAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt/while_analyzer.rs` | while_analyzer.rs | WhileAnalyzer.php |
| `crates/pzoom-analyzer/src/stmt_analyzer.rs` | stmt_analyzer.rs |  |
| `crates/pzoom-analyzer/src/template/inferred_type_replacer.rs` | inferred_type_replacer.rs |  |
| `crates/pzoom-analyzer/src/template/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/template/standin_type_replacer.rs` | standin_type_replacer.rs |  |
| `crates/pzoom-analyzer/src/type_comparator/array_type_comparator.rs` |  | ArrayTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/atomic_type_comparator.rs` | atomic_type_comparator.rs | AtomicTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/callable_type_comparator.rs` |  | CallableTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/class_like_string_comparator.rs` |  | ClassLikeStringComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/generic_type_comparator.rs` | generic_type_comparator.rs | GenericTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/integer_range_comparator.rs` |  | IntegerRangeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/keyed_array_comparator.rs` |  | KeyedArrayComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/mod.rs` | (module root) |  |
| `crates/pzoom-analyzer/src/type_comparator/object_type_comparator.rs` | object_type_comparator.rs |  |
| `crates/pzoom-analyzer/src/type_comparator/scalar_type_comparator.rs` | scalar_type_comparator.rs | ScalarTypeComparator.php |
| `crates/pzoom-analyzer/src/type_comparator/type_comparison_result.rs` | type_comparison_result.rs | TypeComparisonResult.php |
| `crates/pzoom-analyzer/src/type_comparator/union_type_comparator.rs` | union_type_comparator.rs | UnionTypeComparator.php |
| `crates/pzoom-analyzer/src/type_expander.rs` | type_expander.rs | TypeExpander.php |
| `crates/pzoom-cli/src/main.rs` | (module root) |  |
| `crates/pzoom-code-info/src/algebra/clause.rs` | clause.rs | Clause.php |
| `crates/pzoom-code-info/src/algebra/mod.rs` | (module root) |  |
| `crates/pzoom-code-info/src/assertion.rs` | assertion.rs | Assertion.php |
| `crates/pzoom-code-info/src/class_constant_info.rs` | class_constant_info.rs | ClassConstantStorage.php |
| `crates/pzoom-code-info/src/class_type_alias.rs` | class_type_alias.rs | ClassTypeAlias.php |
| `crates/pzoom-code-info/src/code_location.rs` | code_location.rs | CodeLocation.php |
| `crates/pzoom-code-info/src/method_identifier.rs` | method_identifier.rs | MethodIdentifier.php |
| `crates/pzoom-code-info/src/class_like_info.rs` | classlike_info.rs | ClassLikeStorage.php |
| `crates/pzoom-code-info/src/codebase_info.rs` |  | Codebase.php |
| `crates/pzoom-code-info/src/data_flow/graph.rs` | graph.rs |  |
| `crates/pzoom-code-info/src/data_flow/mod.rs` | (module root) |  |
| `crates/pzoom-code-info/src/data_flow/node.rs` | node.rs |  |
| `crates/pzoom-code-info/src/data_flow/path.rs` | path.rs | Path.php |
| `crates/pzoom-code-info/src/data_flow/tainted_node.rs` | tainted_node.rs |  |
| `crates/pzoom-code-info/src/file_info.rs` | file_info.rs | FileStorage.php |
| `crates/pzoom-code-info/src/functionlike_info.rs` | functionlike_info.rs | FunctionLikeStorage.php |
| `crates/pzoom-code-info/src/issue.rs` | issue.rs |  |
| `crates/pzoom-code-info/src/lib.rs` | (module root) |  |
| `crates/pzoom-code-info/src/member_visibility.rs` | member_visibility.rs |  |
| `crates/pzoom-code-info/src/property_info.rs` | property_info.rs | PropertyStorage.php |
| `crates/pzoom-code-info/src/symbol.rs` |  |  |
| `crates/pzoom-code-info/src/t_atomic.rs` | t_atomic.rs |  |
| `crates/pzoom-code-info/src/t_union.rs` | t_union.rs |  |
| `crates/pzoom-code-info/src/ttype/key_value_of.rs` |  |  |
| `crates/pzoom-code-info/src/ttype/mod.rs` | (module root) |  |
| `crates/pzoom-code-info/src/ttype/type_combination.rs` | type_combination.rs | TypeCombination.php |
| `crates/pzoom-code-info/src/ttype/type_combiner.rs` | type_combiner.rs | TypeCombiner.php |
| `crates/pzoom-orchestrator/src/analyzer.rs` | analyzer.rs | Analyzer.php |
| `crates/pzoom-orchestrator/src/ast_differ.rs` | ast_differ.rs | AstDiffer.php |
| `crates/pzoom-orchestrator/src/cache.rs` | cache.rs | Cache.php |
| `crates/pzoom-orchestrator/src/lib.rs` | (module root) |  |
| `crates/pzoom-orchestrator/src/populator.rs` | populator.rs | Populator.php |
| `crates/pzoom-orchestrator/src/scanner.rs` | scanner.rs | Scanner.php |
| `crates/pzoom-str/build.rs` | build.rs |  |
| `crates/pzoom-str/src/lib.rs` | (module root) |  |
| `crates/pzoom-syntax/src/declaration_collector/classlike_scanner.rs` | classlike_scanner.rs |  |
| `crates/pzoom-syntax/src/declaration_collector/functionlike_scanner.rs` | functionlike_scanner.rs |  |
| `crates/pzoom-syntax/src/declaration_collector/mod.rs` | (module root) |  |
| `crates/pzoom-syntax/src/docblock/mod.rs` | (module root) |  |
| `crates/pzoom-syntax/src/docblock/parsed_docblock.rs` |  | ParsedDocblock.php |
| `crates/pzoom-syntax/src/docblock/type_tokenizer.rs` |  | TypeTokenizer.php |
| `crates/pzoom-syntax/src/docblock/parse_tree.rs` |  | ParseTree.php (+ ParseTree/* subclasses) |
| `crates/pzoom-syntax/src/docblock/parse_tree_creator.rs` |  | ParseTreeCreator.php |
| `crates/pzoom-syntax/src/docblock/type_parser.rs` |  | TypeParser.php |
| `crates/pzoom-syntax/src/lib.rs` | (module root) |  |
| `crates/pzoom-syntax/src/name_resolver.rs` |  |  |
| `crates/pzoom-syntax/src/type_resolver.rs` |  |  |
| `crates/pzoom-test-runner/src/main.rs` | (module root) |  |

## Present in Psalm + Hakana, absent from pzoom

| Hakana / Psalm name |
|---|
| `aliases.rs` / `Aliases.php` |

## cloc comparison — code lines (pzoom vs Hakana vs Psalm)

Rust vs PHP counts aren't directly comparable; the signal is *relative* size. The raw ratio over-counts files where pzoom **distributes** Psalm's bundling across several files (a renamed/split concern, not a gap) or where pzoom intentionally **stubs** a subsystem. Filter those out before reading the ratio as a gap.

### pzoom thinner than Psalm/Hakana (raw ratio)
| pz | hk | psalm | ratio | file |
|---:|---:|---:|---:|---|
| 102 | 3354 | 3710 | 36.4× | `analyzer/function_like_analyzer.rs` |
| 66 | 880 | 1798 | 27.2× | `analyzer/expr/call_analyzer.rs` |
| 20 | 188 | 424 | 21.2× | `orchestrator/cache.rs` |
| 236 | 24 | 3904 | 16.5× | `analyzer/config.rs` |
| 30 | 478 | 168 | 15.9× | `orchestrator/ast_differ.rs` |
| 266 | 330 | 1992 | 7.5× | `analyzer/statements_analyzer.rs` |
| 60 | 256 | 382 | 6.4× | `analyzer/stmt/else_analyzer.rs` |
| 470 | – | 2458 | 5.2× | `analyzer/expr/binop/arithmetic_op_analyzer.rs` |
| 464 | 516 | 2162 | 4.7× | `orchestrator/analyzer.rs` |
| 178 | – | 660 | 3.7× | `analyzer/type_comparator/keyed_array_comparator.rs` |
| 88 | – | 322 | 3.7× | `analyzer/expr/call/method_call_purity_analyzer.rs` |
| 228 | 324 | 832 | 3.6× | `analyzer/expr/fetch/instance_property_fetch_analyzer.rs` |
| 216 | 134 | 756 | 3.5× | `analyzer/expr/include_analyzer.rs` |

**Not real gaps (distributed across renamed files, or intentional stubs):** `function_like_analyzer` (Psalm/Hakana's FunctionLikeAnalyzer runs the whole function-like analysis; pzoom distributes that across `function_analyzer`, class-method analysis in `class_analyzer`, `closure_analyzer`, and the `function_analysis_data` body run — this file holds the shared purity-inference subset). `call_analyzer` (Psalm CallAnalyzer bundles argument checking + `collectSpecialInformation` + `checkMethodArgs`; pzoom splits these into `argument_analyzer`/`arguments_analyzer`/the call analyzers). `statements_analyzer`/`analyzer` (mega-dispatchers; pzoom splits into `stmt_analyzer`+`expression_analyzer` / orchestrator). `else_analyzer` (small Psalm-named branch entrypoint; logic shared with `if_else_analyzer`). `instance_property_fetch_analyzer` (most logic in `atomic_property_fetch_analyzer`). `config` (Psalm parses an XML schema + plugins; pzoom is minimal by design). `cache`/`ast_differ` (stubs — no incremental-analysis subsystem).

### Genuine remaining logic gaps
| pz | psalm | file | gap |
|---:|---:|---|---|
| 470 | 2458 | `expr/binop/arithmetic_op_analyzer.rs` | literal folding + `mod`/`pow` int-range result rules (folding blocked, see notes) |
| 178 | 660 | `type_comparator/keyed_array_comparator.rs` | `isContainedByObjectWithProperties` / `coerceToObjectWithProperties` — `object{...}` types pzoom doesn't model |
| 88 | 322 | `expr/call/method_call_purity_analyzer.rs` | Psalm's full purity model (`pure`/`mutation_free`/`external_mutation_free` contexts, memoization, `UnusedMethodCall`); pzoom has the impure-call check only |
| 216 | 756 | `expr/include_analyzer.rs` | include path resolution (partly intentional — no filesystem resolution) |

### Open gaps / deferred (notes)
- Still absent from pzoom as standalone files (need deeper integration, not just a rename): `switch_scope.rs` — Hakana's `SwitchScope` tracks variable-scope merging + leftover statements, whereas pzoom's switch analysis uses an exhaustiveness/fallthrough-type model, so a faithful port is coupled to adopting that model (the deferred switch fallthrough-threading work); `simple_type_inferer.rs` (const-expr inference, currently inline in the declaration collector); `class_template_param_collector.rs` (template-param resolution in `standin_type_replacer`/`function_call_analyzer`); `class_type_alias.rs` (`@psalm-type` aliases, handled in the scanner).
- `NoValue` is reserved but unemitted (blocked on reconciliation `never`-precision).
- Implicit `Stringable` (Psalm's `ReflectorVisitor` auto-adds `Stringable` to a class's `class_implements` when it declares `__toString` on PHP ≥ 8.0) is deferred: pzoom doesn't model `analysis_php_version_id`, so adding it unconditionally is strictly net-zero — it fixes `ToString/implicitStringable` (PHP 8.0) but regresses `ToString/implicitStringableDisallowed` (PHP 7.4). Blocked on per-test PHP-version modelling (same gap behind the `Php71`/`Php84`/`ReservedWord`/native-union-and-intersection version tests).
- Arithmetic literal+literal folding (`5 + 3` ⇒ `8`) is deferred: it makes constant comparisons (`5 + 3 === 8`) fold to `true`, which pzoom's `RedundantCondition` then flags even though Psalm doesn't (pzoom already flags a bare `5 === 5`) — coupled to aligning that comparison-folding behaviour.
- Generic subclass-param remapping in `object_type_comparator` is deferred: needs faithful built-in-interface template variance first (a codebase-only `getMappedGenericTypeParams` port regressed -5 tests).
- Full `collect_mutations` / `@psalm-mutation-free` verification passes remain unbuilt (`function_like_analyzer.rs` owns the closure/arrow purity inference subset only).
- Resolved (`scalar_type_match_found` alignment): the comparator stack now produces `TypeComparisonResult::scalar_type_match_found` the way Psalm's `ScalarTypeComparator` does, instead of `argument_analyzer` reconstructing it from ad-hoc `is_scalar_union` heuristics. `scalar_type_match_found` is now `Option<bool>` (matching Psalm's `?bool`): `union_type_comparator` seeds it to `Some(true)` and clears it to `Some(false)` on the first non-scalar mismatch; `scalar_type_comparator` sets it via a catch-all on scalar-vs-scalar mismatch (non-literal container), also recording `type_coerced_from_scalar` for a bare `scalar` input. Because PHP array/shape/generic element types are always docblock-declared (Psalm gates the flag on the container element's `from_docblock`, which pzoom can't see per-atomic), `atomic_type_comparator` preserves the incoming `scalar_type_match_found` across the array element dispatch so an in-array scalar mismatch stays `InvalidArgument`. Fixes `FunctionCall/rangeWithFloatStart`, `NativeUnions/invalidNativeUnionArgument`.
- Resolved (`argument_analyzer` verifyType migration + `isMixed` vs `hasMixed`): `argument_analyzer::verify_type` was rewritten to follow Psalm `ArgumentAnalyzer::verifyType`'s decision flow — `is_contained_by(ignore_null=true, ignore_false=!param_has_true)` → `type_coerced` (`MixedArgumentTypeCoercion`/`ArgumentTypeCoercion`) → `to_string_cast` (`ImplicitToStringCast`) → not-contained mismatch (`InvalidScalarArgument`/`PossiblyInvalidArgument`/`InvalidArgument`, driven by `scalar_type_match_found`) → null checks (`NullArgument`/`PossiblyNullArgument`) — instead of the previous pile of ad-hoc scalar/null/mixed heuristics. The masking root cause was `TUnion::is_mixed()` implementing Psalm's `hasMixed` (any atomic is mixed) but being used where Psalm needs `isMixed` (every atomic is mixed): the fully-mixed early return now uses the new `TUnion::is_only_mixed()`, so a `mixed|null`/`mixed|<type>` argument continues to the containment + null checks (yielding `PossiblyNullArgument`/`PossiblyInvalidArgument`) the way Psalm does. The combiner already preserves these unions. Fixes `ArrayFunctionCall/arrayMergeTwoPossiblyFalse`, `ArrayFunctionCall/arrayReplaceTwoPossiblyFalse`; no regressions. Deferred within the new flow (documented inline): `MixedArgument`/`NoValue` emission (reconciliation over-produces `mixed`/`never`) and the false-argument checks (no `PossiblyFalseArgument` issue kind yet).
- Resolved (`is_falsable` masking + Psalm issue-kind parity): `TAtomic::is_falsable()` previously meant "could hold a falsy value" (`0`, `""`, `[]`, int, string, array, …) — conflating *falsy* with Psalm's `Union::isFalsable` (the union contains the `false` atomic, or a template bound that is falsable). It now matches Psalm: only `TFalse` (and falsable template bounds) are falsable; `is_falsy()` remains for the falsy notion. This unblocks the faithful false-argument check in `verify_type` (a definite `false` passed to a parameter that does not accept it → `InvalidArgument`). Added the missing Psalm issue kinds (`PossiblyFalseArgument`, `InvalidLiteralArgument`, `InvalidTemplateParam`, `MixedOperand`, `NullIterator`, `PossiblyInvalidCast`, … ). Deferred: the `PossiblyFalseArgument` *emission* (possibly-`false` input) — it needs accurate `false`/`ignore_falsable` tracking that pzoom lacks for template-bound args and conditional-return stubs (`glob`'s `@psalm-ignore-falsable-return` is dropped when its conditional `@return` is used). Also note `TUnion` derives `PartialEq` over `is_falsable`, so the flag participates in union-equality used by reconciliation/dedup — a separate fidelity gap vs Psalm's id-based equality.
- Resolved (stub precedence): pzoom embeds both its own curated stubs (`CoreGenericFunctions.phpstub`, `Php*`, `SPL`, …) and phpstorm-derived `stubs/extensions/*`. Duplicate declarations were previously reconciled purely by a quality score + field-merge, so a thin `extensions/*` signature could shadow or downgrade the curated one (e.g. `glob`/`array_keys`/`array_sum`: the curated stub's richer/`@psalm-ignore-falsable-return` return type was lost). `FileInfo` now carries `is_low_precedence_stub` (set for `stubs/extensions/*`), and `register_function` uses an explicit precedence tier — project code > curated stubs > phpstorm-derived stubs — where a higher tier replaces a lower outright and a lower tier is ignored, mirroring Psalm (CallMap > Psalm stubs > phpstorm-stubs). Net +2 (fixes `arrayMergeNoNamed`, `arraySumNumeric`, `intersectParentTemplateReturnWithConcreteChildReturn`, `literalFalseArgument`); exposes the pre-existing "expects array, parent type array provided" spurious-`type_coerced` comparator bug on a templated array param (`noCrashOnArrayKeyExistsBracket`, same class as `arrayEnd`/`arrayReset`MaybeEmptyTKeyedArray). Class registration still uses the merge path (precedence applied to functions first to limit blast radius).
- Resolved (stub dedup — one symbol, one file): the dual-universe setup above is gone. All Psalm-imported root stubs (`CoreGeneric*`, `CoreImmutableClasses`, `SPL`, `Reflection`, `Php74`–`Php85`) were dissolved into `stubs/extensions/*` via `tools/stub_graft.php`: extension-file signatures (param names + native types) kept, Psalm docblocks grafted per tag-group (Psalm wins per `@param $x`/`@return`/template-set/etc.; ext-only tags retained; docblock param refs positionally renamed), classes merged per-member (Psalm partial classes only touch the members they declare; Psalm-only members appended). Version overlays (`Php8x`) folded newest-wins; version-introduced symbols carry `@since X.Y PHP` (the marker Psalm itself parses in phpstubs). Psalm's `extensions/simplexml.phpstub` folded into the JetBrains `simple-xml.phpstub` under the `simplexml.phpstub` name. Deliberate exceptions preserved: `fclose(&$stream)` keeps Psalm's by-ref signature (its `@param-out closed-resource` device); `Exception`/`Error` property defaults restored (JetBrains omits them → spurious `PropertyNotSetInConstructor` in subclasses). Roots now hold only `phpparser.phpstub` + `TestRequirementTraits.phpstub`. Enforced by `test_stubs_declare_each_symbol_in_one_file` in `scanner.rs`. Validated: test suite failure-set unchanged; dogfood-on-Psalm issue total identical (2537). Note: extension-file native param types are now live for previously tier-shadowed functions (the tier system still exists for user stub dirs).
