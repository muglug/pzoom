# `declaration_collector/mod.rs` — Psalm / Hakana equivalents

Maps every function in `crates/pzoom-syntax/src/declaration_collector/mod.rs` to
its closest Psalm and Hakana counterpart.

`declaration_collector` is pzoom's **scanner / declaration-collection** layer: it
walks the mago PHP AST and the PHPDoc docblocks and builds `code_info`. Its
analogues are:

- **Psalm** — `Psalm\Internal\PhpVisitor\Reflector\*` (`ClassLikeNodeScanner`,
  `FunctionLikeNodeScanner`, `FunctionLikeDocblockScanner`,
  `ClassLikeDocblockParser`, `FunctionLikeDocblockParser`, `ExpressionScanner`,
  `ExpressionResolver`, `AttributeResolver`, `TypeHintResolver`), plus
  `Internal\Scanner\DocblockParser`, `Analyzer\CommentAnalyzer`, and
  `Internal\Type\TypeParser`.
- **Hakana** — `hakana-core/src/code_info_builder/` (`lib.rs`,
  `classlike_scanner.rs`, `functionlike_scanner.rs`, `typehint_resolver.rs`).

**Big caveat:** Hakana analyses **Hack**, which has native generics, typedefs,
enums, attributes and type hints — and essentially **no PHPDoc**. So the large
family of pzoom methods that parse `@param`/`@return`/`@template`/`@method`/
`@property`/`@psalm-*` docblock tags has a Psalm equivalent but **no Hakana
equivalent** (Hakana gets the same information from Hack syntax instead).

Legend: ✅ direct equivalent · ≈ partial / different mechanism · — none.

---

## AST walking & top-level visiting

| pzoom method | Psalm | Hakana |
|---|---|---|
| `visit_statement` | ✅ `ReflectorVisitor::enterNode` | ✅ `visit_stmt_` / `visit_def` |
| `visit_if` | ≈ `ReflectorVisitor` (conditional stub blocks) | — |
| `evaluate_stub_conditional` | ≈ `ExpressionScanner` (PHP_VERSION_ID stub guards) | — |
| `visit_namespace` | ✅ `ReflectorVisitor` (namespace) | ≈ namespacing in name resolution |
| `visit_use` / `register_use_alias` / `normalize_use_name` | ✅ `Aliases` / use handling | ≈ resolved names (parser-side) |
| `visit_constant` | ✅ `ExpressionScanner::registerClassMapFunctionCall` (`define`) | ✅ `visit_gconst` |
| `collect_defined_constants_from_statements` | ≈ `ExpressionScanner` | ✅ `visit_gconst` |
| `find_next_non_whitespace_offset` | — (lexer helper) | — |
| `Walker::walk_in_assignment` | — (mago-walker specific) | — |
| `extract_direct_var`, `is_null_expression` | — (AST helpers) | — |

## Docblock discovery & inline annotations

| pzoom method | Psalm | Hakana |
|---|---|---|
| `find_preceding_docblock` | ✅ `getDocComment` / `CommentAnalyzer` | ≈ `adjust_location_from_comments` |
| `collect_inline_var_annotations_from_docblock` | ✅ `CommentAnalyzer::getVarComments` / `arrayToDocblocks` | — |
| `collect_inline_docblock_annotations_in_span` | ≈ `CommentAnalyzer` (inline `@var`) | — |
| `collect_top_level_inline_docblock_annotations` | ≈ `CommentAnalyzer` | — |
| `collect_inline_callable_annotations_from_docblock` | ≈ closure param/return inference from `@param`/`@template`/`@return` (internal plumbing, not a tag) | — |
| `collect_inline_trace_annotations_from_docblock` / `..._from_source` | ✅ `@psalm-trace` / `@trace` (`StatementsAnalyzer` trace) | — |
| `collect_inline_check_type_annotations_from_docblock` / `..._from_source` | ✅ `@psalm-check-type[-exact]` (`StatementsAnalyzer` check-type) | — |

## Classlikes & members

| pzoom method | Psalm | Hakana |
|---|---|---|
| `collect_class_members` | ✅ `ClassLikeNodeScanner::start` + `visit*` | ✅ `classlike_scanner::scan` |
| `precollect_class_constants` | ✅ `ClassLikeNodeScanner::visitClassConstDeclaration` | ✅ `visit_class_const_declaration` |
| `precollect_enum_case_constants` | ✅ `ClassLikeNodeScanner::visitEnumDeclaration` | ✅ `visit_class_const` / enum handling |
| `inject_builtin_enum_methods` | ≈ enum `cases()/from()/tryFrom()` (storage) | ≈ enum method synthesis |
| `collect_property` / `add_property_item` | ✅ `ClassLikeNodeScanner::visitPropertyDeclaration` | ✅ `visit_property_declaration` |
| `collect_params` | ✅ `FunctionLikeNodeScanner::getTranslatedFunctionParam` | ✅ `convert_param_nodes` |
| `collect_promoted_properties` | ✅ `inferPropertyTypeFromConstructor` (promotion) | ✅ `add_promoted_param_property` |
| `collect_this_property_mutations` | ≈ `FunctionLikeNodeScanner::inferPropertyTypeFromConstructor` | ≈ constructor inference |
| `add_old_style_constructor_alias` | ✅ `ClassLikeNodeScanner` (PHP4 ctor) | — (Hack) |
| `statements_throw` | ≈ (Psalm computes in analysis, not scan) | — |
| `span_contains_variadic_builtin_calls` | ≈ `func_get_args`/variadic detection | — |

## Docblock class-level tags

| pzoom method | Psalm | Hakana |
|---|---|---|
| `apply_docblock_magic_properties` | ✅ `ClassLikeDocblockParser::addMagicPropertyToInfo` | — |
| `apply_docblock_magic_methods` | ✅ `ClassLikeDocblockParser` (`@method`) | — |
| `parse_docblock_method_info` / `parse_docblock_method_params` | ✅ `ClassLikeDocblockParser::getMethodOffset` + `MethodTree` | — |
| `apply_docblock_mixins` | ✅ `ClassLikeDocblockParser` (`@mixin`) | — |
| `apply_docblock_requirements` / `collect_required_classlikes_from_docblock_tags` | ✅ `ClassLikeDocblockParser` (`@psalm-require-*`) | ≈ `handle_reqs` (native `require extends/implements`) |
| `apply_docblock_template_extends` | ✅ `extendTemplatedType`/`implementTemplatedType`/`useTemplatedType` | ≈ native generic `extends Foo<int>` |
| `get_docblock_sealed_properties` / `get_docblock_sealed_methods` | ✅ `@psalm-seal-properties/-methods` | — |
| `is_docblock_no_seal_properties` | ✅ `@psalm-no-seal-properties` | — |
| `is_docblock_consistent_constructor` | ✅ `@psalm-consistent-constructor` | — |

## Docblock method/function tags

| pzoom method | Psalm | Hakana |
|---|---|---|
| `apply_docblock_param_types` | ✅ `FunctionLikeDocblockScanner::improveParamsFromDocblock` | — |
| `apply_docblock_param_out_types` | ✅ `FunctionLikeDocblockScanner::handleParamOut` (`@param-out`) | — |
| `get_docblock_return_type` | ✅ `FunctionLikeDocblockScanner::handleReturn` / `extractReturnType` | — |
| `get_docblock_assertions` / `parse_assertion_tag_content` | ✅ `handleAssertions` / `getAssertionParts` / `sanitizeAssertionLineParts` | — |
| `get_implicit_assertions` | ≈ implicit assertions (Psalm analysis) | — |
| `extract_assertions_when_true` / `..._when_false` / `extract_builtin_call_assertions` | ≈ `@psalm-assert-if-true/-false` handling | — |
| `get_docblock_if_this_is_type` | ✅ `@psalm-if-this-is` | — |
| `parse_docblock_conditional_return_type` / `_branch` / `_condition` | ✅ `getConditionalSanitizedTypeTokens` + `TypeParser` `ConditionalTree` | — |
| `parse_docblock_template_bindings` | ✅ `FunctionLikeDocblockScanner::handleTemplates` | ≈ native generics |
| `build_template_map_from_bindings` / `build_template_map_from_class_template_types` | ≈ Psalm `template_type_map` assembly | ≈ `TypeResolutionContext` build |
| `parse_template_tag_content` | ✅ `@template` parsing | — |

## Boolean / flag docblock tags

| pzoom method | Psalm | Hakana |
|---|---|---|
| `is_docblock_pure` | ✅ `@psalm-pure` | ≈ `<<__Pure>>` attribute |
| `is_docblock_mutation_free` / `is_docblock_immutable` | ✅ `@psalm-mutation-free`/`@psalm-immutable` | ≈ attributes |
| `is_docblock_final` | ✅ `@psalm-final` / `@final` | ≈ native `final` |
| `is_docblock_readonly` / `is_docblock_readonly_allow_private_mutation` | ✅ `@psalm-readonly`/`-allow-private-mutation` | ≈ native `readonly` |
| `is_docblock_deprecated` / `get_docblock_deprecation_message` | ✅ `@deprecated` | ≈ `<<__Deprecated>>` |
| `is_docblock_no_named_arguments` | ✅ `@no-named-arguments` | — |
| `is_docblock_ignore_nullable_return` / `is_docblock_ignore_falsable_return` | ✅ `@psalm-ignore-nullable-return`/`-falsable-return` | — |
| `is_docblock_inheritdoc` | ✅ `{@inheritdoc}` handling | — |
| `is_docblock_override_method_visibility` / `is_docblock_override_property_visibility` | ✅ `@psalm-suppress`/visibility overrides | — |
| `get_docblock_internal_scopes` / `get_default_internal_scope` | ✅ `@internal` / `@psalm-internal` (`DocblockParser::handlePsalmInternal`) | — |

## Attributes (`#[...]`)

| pzoom method | Psalm | Hakana |
|---|---|---|
| `has_attribute_named` | ✅ `AttributeResolver::getAttributeStorageFromStatement` | ✅ attribute scanning |
| `get_attribute_flags` / `eval_attribute_flag_expression` | ≈ `AttributeResolver` (e.g. `Attribute::TARGET_*`) | ≈ `get_spread_params_from_attribute` |

## Type aliases (`@psalm-type` / `@psalm-import-type`)

| pzoom method | Psalm | Hakana |
|---|---|---|
| `collect_docblock_type_aliases` | ✅ `ClassLikeNodeScanner::getTypeAliasesFromComment(Lines)` | ≈ `visit_typedef` (native `type`/`newtype`) |
| `register_namespace_type_aliases` | ≈ namespaced alias registration | ≈ `visit_typedef` |
| `collect_preceding_statement_type_aliases` | ≈ file-level alias collection | — |
| `parse_type_alias_tag_content` | ✅ `@psalm-type` parsing | — |
| `parse_import_type_tag_content` / `parse_import_type_tag_content` | ✅ `getImportedTypeAliases` (`@psalm-import-type`) | — |
| `resolve_class_expression` | ≈ `ExpressionResolver` | ≈ name resolution |

## Type resolution & utility types

| pzoom method | Psalm | Hakana |
|---|---|---|
| `resolve_docblock_union_type` | ✅ `TypeParser::parseTokens` (post-resolution) | ✅ `typehint_resolver::get_type_from_hint` |
| `resolve_docblock_atomic_type` | ✅ `TypeParser::getTypeFromTree` (template/self rewriting) | ✅ `typehint_resolver` |
| `resolve_docblock_type_alias_atomic` | ✅ `TypeParser` (`TTypeAlias` resolution) | ≈ typedef resolution |
| `resolve_docblock_class_name` | ✅ `Type::getFQCLNFromString` (self/parent/static) | ≈ resolved names |
| `resolve_type` / `make_fqn` / `resolve_identifier` | ✅ `Aliases` / `getFQCLNFromString` | ≈ resolved names |
| `try_resolve_docblock_utility_type` | ✅ `TypeParser::getTypeFromGenericTree` (key-of/value-of/properties-of) | ≈ `typehint_resolver` (limited) |
| `try_resolve_template_key_of_type` / `resolve_key_of_template_union` / `resolve_key_of_template_atomic` | ✅ `TypeParser` `key-of` + `TTemplateKeyOf` | ≈ `get_template_type` |
| `try_resolve_template_value_of_type` / `resolve_value_of_template_union` / `resolve_value_of_template_atomic` | ✅ `TypeParser` `value-of` + `TTemplateValueOf` | ≈ `get_template_type` |
| `resolve_enum_value_of` | ✅ `TypeExpander` (`value-of<Enum>`) | ≈ enum value expansion |
| `properties_of_or_deferred` / `resolve_properties_of_union` / `resolve_properties_of_atomic` / `resolve_properties_of_named_object` | ✅ `TypeParser` `properties-of` + `TypeExpander` (`TPropertiesOf`) | — |
| `expand_docblock_class_constant_wildcards` / `_in_atomic` / `resolve_class_constant_union_from_atomic` | ≈ `TypeExpander` class-const (`Foo::BAR_*`) | — |

## Docblock validation helpers (emit `InvalidDocblock`)

These have no discrete Hakana counterpart, and in Psalm validation is mostly a
side effect of `TypeParser` throwing `TypeParseTreeException` plus
`FunctionLikeDocblockParser::check*` — not separate functions.

| pzoom method | Psalm | Hakana |
|---|---|---|
| `validate_function_docblock_type_tags` | ≈ `FunctionLikeDocblockParser::checkUnexpectedTags/checkDuplicatedParams/checkDuplicatedTags` | — |
| `validate_property_docblock_tags` / `validate_type_alias_docblock_tags` | ≈ `FunctionLikeDocblockParser` checks | — |
| `is_valid_docblock_type_string` / `is_valid_php_classlike_identifier` / `is_valid_php_const_identifier` / `is_class_name_char` | ≈ `TypeParser` regex validation | — |
| `has_valid_docblock_utility_type_arity` / `find_matching_angle_bracket` / `count_top_level_generic_params` | ≈ `TypeParser` arity checks | — |
| `has_valid_docblock_class_constant_syntax` / `class_constant_syntax_is_valid_in_part` / `extract_class_constant_parts` | ≈ `TypeParser` `::` class-constant validation | — |
| `has_invalid_hyphenated_named_type` / `extract_docblock_base_type_token` / `has_invalid_docblock_type_syntax` / `has_balanced_type_delimiters` / `split_docblock_union_parts` | ≈ `TypeTokenizer`/`TypeParser` validation | — |
| `union_has_valid_array_keys` / `atomic_has_valid_array_keys` / `union_is_valid_array_key` | ≈ `TypeParser` array-key validation | — |
| `union_has_invalid_class_string_targets` / `atomic_has_invalid_class_string_target` / `class_string_target_is_explicitly_invalid` | ≈ `TypeParser` `class-string<…>` validation | — |
| `has_valid_int_range_bounds` / `is_valid_single_int_range` / `parse_int_range_bound` | ≈ `TypeParser` `int<…>` range validation | — |
| `is_missing_docblock_type` | ≈ Psalm missing-type checks | — |
| `push_docblock_issue` | ≈ `IssueBuffer` / `DocblockParseException` | — |

## `@method` / docblock parsing helpers

| pzoom method | Psalm | Hakana |
|---|---|---|
| `split_docblock_method_params` / `find_docblock_method_signature_bounds` / `split_method_name` / `is_valid_docblock_method_name` | ✅ `ClassLikeDocblockParser` / `FunctionLikeDocblockParser` `@method` helpers | — |
| `extract_param_name_from_content` | ✅ `FunctionLikeDocblockParser::extractAllParamNames` | — |
| `take_first_docblock_type_token` | ≈ `DocblockParser` token helpers | — |

## Modifier parsing (native syntax)

| pzoom method | Psalm | Hakana |
|---|---|---|
| `parse_method_modifiers` / `parse_visibility_modifier` / `parse_property_modifiers` / `parse_const_visibility` | ✅ PhpParser node flags | ✅ AAST visibility/modifiers |

## `is_stub_file`

| pzoom method | Psalm | Hakana |
|---|---|---|
| `is_stub_file` | ≈ `Codebase::$register_stub_files` context | ≈ stub handling |

---

### Summary

- **Native scanning** (classes, members, params, constants, attributes,
  modifiers, name resolution) → equivalents in **both** Psalm and Hakana.
- **PHPDoc-tag parsing** (`@param`, `@return`, `@template`, `@method`,
  `@property`, `@mixin`, `@psalm-assert`, conditional returns, `@psalm-type`,
  flag tags) → **Psalm only**; Hakana derives these from native Hack syntax or
  not at all.
- **Docblock-type *validation* helpers** are largely **pzoom-specific**: Psalm
  reports the same problems as a side effect of `TypeParser` exceptions rather
  than dedicated predicates, and Hakana has no equivalent. (These should move
  into `type_parser`, which should return a `Result` — Psalm's
  `TypeParseTreeException`.)
- There are **no** `@pzoom-*` docblock tags. The `collect_inline_*` methods
  implement Psalm's `@psalm-trace` and `@psalm-check-type[-exact]` (used by the
  test suite) plus internal closure-signature plumbing.
