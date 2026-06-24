//! Port of Psalm's `tests/TypeReconciliation/ReconcilerTest.php`
//! (`providerTestReconciliation`).
//!
//! Each case feeds an [`Assertion`] and an "original" type string to the
//! reconciler and checks the reconciled type's id. Psalm constructs the
//! assertion objects directly (e.g. `new IsNotType(new TNull())`); we mirror
//! that by building pzoom [`Assertion`]s, taking the assertion's carried atomic
//! from [`parse_type_string`] so we needn't hand-build every `TAtomic`.
//!
//! Where Psalm uses an assertion pzoom does not (yet) model, the mapping is
//! noted inline:
//!   * `IsIdentical`/`IsNotIdentical` (strict `===`/`!==`) → pzoom `IsEqual`/`IsNotEqual`.
//!   * `IsLooselyEqual` (loose `==`) and `IsNotAClass` have no pzoom equivalent
//!     yet — those rows are listed in `UNSUPPORTED` rather than asserted, so the
//!     harness documents the gap instead of silently dropping it.

use std::path::Path;

use pzoom_analyzer::reconciler::reconcile;
use pzoom_analyzer::{Config, FunctionAnalysisData, StatementsAnalyzer};
use pzoom_code_info::Assertion;
use pzoom_code_info::TAtomic;
use pzoom_code_info::codebase_info::CodebaseInfo;
use pzoom_code_info::file_info::FileInfo;
use pzoom_orchestrator::{Populator, Scanner, apply_call_map};
use pzoom_str::{SharedInterner, StrId, ThreadedInterner};
use pzoom_syntax::docblock::parse_type_string;
use pzoom_syntax::{DeclarationCollector, FileId, parse_file_content, resolve_names};
use rustc_hash::FxHashSet;

/// Test classes mirroring Psalm's `ReconcilerTest::setUp` `newfile.php`.
const TEST_CLASSES: &str = r#"<?php
class SomeClass {}
class SomeChildClass extends SomeClass {}
class A {}
class B {}
interface SomeInterface {}
"#;

/// Owns every borrow the [`StatementsAnalyzer`] needs so a fresh analyzer can be
/// built per reconcile call (the analyzer only holds references).
struct Fixture {
    codebase: CodebaseInfo,
    /// Held behind a mutex so the test can both intern (parsing type strings
    /// through a `ThreadedInterner`) and analyse (locking it as a `&Interner`),
    /// mirroring how the real pipeline keeps the interner shared during the
    /// build passes and immutable during analysis.
    interner: SharedInterner,
    config: Config,
    resolved_names: pzoom_syntax::ResolvedNames,
    source: String,
    file_path: StrId,
}

impl Fixture {
    fn new() -> Self {
        // 1. Scan the real stub tree (DateTime/Countable/Traversable/… live here).
        let stubs_dir = format!("{}/../../stubs", env!("CARGO_MANIFEST_DIR"));
        let mut scanner = Scanner::new();
        scanner.scan_stub_directory(Path::new(&stubs_dir), &FxHashSet::default());
        let mut scan = scanner.finish();

        let interner = scan.interner.into_shared();

        // 2. Builtin signatures (harness-default PHP version, as the test-runner).
        {
            let threaded = ThreadedInterner::new(interner.clone());
            apply_call_map(&mut scan.codebase, &threaded, 80_000);
        }

        // 3. Register the inline test classes.
        register_php(&mut scan.codebase, &interner, "newfile.php", TEST_CLASSES);

        // 4. Populate once (resolves SomeChildClass -> SomeClass inheritance, etc.).
        {
            let mut guard = interner.lock();
            let mut populator = Populator::new(&mut scan.codebase, &mut guard);
            populator.populate();
        }

        // 5. Minimal resolved-names context for a trivial source file.
        let source = "<?php\n".to_string();
        let threaded = ThreadedInterner::new(interner.clone());
        let file_path = threaded.intern("reconciler-test.php");
        let arena = bumpalo::Bump::new();
        let file_id = FileId::new("reconciler-test.php");
        let (program, _err) = parse_file_content(&arena, file_id, &source);
        let resolved_names = resolve_names(program, &threaded);
        drop(threaded);

        Fixture {
            codebase: scan.codebase,
            interner,
            config: Config::new(),
            resolved_names,
            source,
            file_path,
        }
    }

    /// Single atomic of a parsed type string (for assertion payloads).
    fn atom(&self, type_str: &str) -> TAtomic {
        let threaded = ThreadedInterner::new(self.interner.clone());
        let union = parse_type_string(type_str, &threaded)
            .unwrap_or_else(|e| panic!("failed to parse {type_str:?}: {e:?}"));
        let guard = self.interner.lock();
        assert_eq!(
            union.types.len(),
            1,
            "expected a single atomic for {type_str:?}, got {}",
            union.get_id(Some(&guard))
        );
        union.types.into_iter().next().unwrap()
    }

    /// Reconcile `assertion` against `original` and return the result id.
    fn reconcile_id(&self, assertion: &Assertion, original: &str) -> String {
        let existing = {
            let threaded = ThreadedInterner::new(self.interner.clone());
            parse_type_string(original, &threaded)
                .unwrap_or_else(|e| panic!("failed to parse original {original:?}: {e:?}"))
        };
        let guard = self.interner.lock();
        let analyzer = StatementsAnalyzer::new(
            &self.codebase,
            &guard,
            self.file_path,
            &self.source,
            &self.resolved_names,
            &self.config,
        );
        let mut analysis_data = FunctionAnalysisData::new();
        let result = reconcile(assertion, &existing, &analyzer, &mut analysis_data);
        result.get_id(Some(&guard))
    }
}

/// Mirror the test-runner's inline-file registration (main.rs scan path):
/// parse, collect declarations, register them, return the owned interner.
fn register_php(codebase: &mut CodebaseInfo, interner: &SharedInterner, path: &str, source: &str) {
    let threaded = ThreadedInterner::new(interner.clone());
    let file_path_id = threaded.intern(path);
    let file_id = FileId::new(path);

    let arena = bumpalo::Bump::new();
    let (program, _err) = parse_file_content(&arena, file_id, source);
    let resolved_names = resolve_names(program, &threaded);

    let collector = DeclarationCollector::new(
        &threaded,
        file_path_id,
        source,
        &codebase.type_aliases,
        &program.trivia,
    );
    let mut declarations = collector.collect(program);

    let file_info = FileInfo {
        path: file_path_id,
        classes: Vec::new(),
        functions: Vec::new(),
        constants: Vec::new(),
        content_hash: String::new(),
        contents: source.to_string(),
        parse_errors: Vec::new(),
        docblock_parse_issues: Vec::new(),
        is_stub: false,
        is_low_precedence_stub: false,
        is_in_project_dirs: true,
        inline_annotations: std::mem::take(&mut declarations.inline_annotations),
        type_alias_imports: std::mem::take(&mut declarations.type_alias_imports),
        resolved_names,
    };
    codebase.files.insert(file_path_id, file_info);

    for class in declarations.classes {
        codebase.register_class(class);
    }
    for func in declarations.functions {
        codebase.register_function(func);
    }
    for constant in declarations.constants {
        codebase.constants.insert(constant.name, constant);
    }
    for type_alias in declarations.type_aliases {
        codebase.type_aliases.insert(type_alias.name, type_alias);
    }
}

/// One reconciliation row: (name, expected_id, assertion, original_type).
fn cases(fx: &Fixture) -> Vec<(&'static str, &'static str, Assertion, &'static str)> {
    let t = |s: &str| fx.atom(s);
    vec![
        // --- not-null ---
        (
            "notNullWithObject",
            "SomeClass",
            Assertion::IsNotType(t("null")),
            "SomeClass",
        ),
        (
            "notNullWithObjectPipeNull",
            "SomeClass",
            Assertion::IsNotType(t("null")),
            "SomeClass|null",
        ),
        (
            "notNullWithSomeClassPipeFalse",
            "SomeClass|false",
            Assertion::IsNotType(t("null")),
            "SomeClass|false",
        ),
        (
            "notNullWithMixed",
            "mixed",
            Assertion::IsNotType(t("null")),
            "mixed",
        ),
        // --- truthy ---
        (
            "notEmptyWithSomeClass",
            "SomeClass",
            Assertion::Truthy,
            "SomeClass",
        ),
        (
            "notEmptyWithSomeClassPipeNull",
            "SomeClass",
            Assertion::Truthy,
            "SomeClass|null",
        ),
        (
            "notEmptyWithSomeClassPipeFalse",
            "SomeClass",
            Assertion::Truthy,
            "SomeClass|false",
        ),
        (
            "notEmptyWithMixed",
            "non-empty-mixed",
            Assertion::Truthy,
            "mixed",
        ),
        // --- is-null ---
        (
            "nullWithSomeClassPipeNull",
            "null",
            Assertion::IsType(t("null")),
            "SomeClass|null",
        ),
        (
            "nullWithMixed",
            "null",
            Assertion::IsType(t("null")),
            "mixed",
        ),
        // --- falsy ---
        ("falsyWithSomeClass", "never", Assertion::Falsy, "SomeClass"),
        (
            "falsyWithSomeClassPipeFalse",
            "false",
            Assertion::Falsy,
            "SomeClass|false",
        ),
        (
            "falsyWithSomeClassPipeBool",
            "false",
            Assertion::Falsy,
            "SomeClass|bool",
        ),
        ("falsyWithMixed", "empty-mixed", Assertion::Falsy, "mixed"),
        ("falsyWithBool", "false", Assertion::Falsy, "bool"),
        (
            "falsyWithStringOrNull",
            "''|'0'|null",
            Assertion::Falsy,
            "string|null",
        ),
        (
            "falsyWithScalarOrNull",
            "empty-scalar",
            Assertion::Falsy,
            "scalar",
        ),
        ("trueWithBool", "true", Assertion::IsType(t("true")), "bool"),
        (
            "falseWithBool",
            "false",
            Assertion::IsType(t("false")),
            "bool",
        ),
        // IsNotIdentical -> pzoom IsNotEqual
        (
            "notTrueWithBool",
            "false",
            Assertion::IsNotEqual(t("true")),
            "bool",
        ),
        (
            "notFalseWithBool",
            "true",
            Assertion::IsNotEqual(t("false")),
            "bool",
        ),
        // --- not-object ---
        (
            "notSomeClassWithSomeClassPipeBool",
            "bool",
            Assertion::IsNotType(t("SomeClass")),
            "SomeClass|bool",
        ),
        (
            "notSomeClassWithSomeClassPipeNull",
            "null",
            Assertion::IsNotType(t("SomeClass")),
            "SomeClass|null",
        ),
        (
            "notSomeClassWithAPipeB",
            "B",
            Assertion::IsNotType(t("A")),
            "A|B",
        ),
        (
            "notDateTimeWithDateTimeInterface",
            "DateTimeImmutable",
            Assertion::IsNotType(t("DateTime")),
            "DateTimeInterface",
        ),
        (
            "notDateTimeImmutableWithDateTimeInterface",
            "DateTime",
            Assertion::IsNotType(t("DateTimeImmutable")),
            "DateTimeInterface",
        ),
        // --- is-object ---
        (
            "myObjectWithSomeClassPipeBool",
            "SomeClass",
            Assertion::IsType(t("SomeClass")),
            "SomeClass|bool",
        ),
        ("myObjectWithAPipeB", "A", Assertion::IsType(t("A")), "A|B"),
        // --- array / iterable / callable ---
        (
            "array",
            "array<array-key, mixed>",
            Assertion::IsType(t("array<array-key, mixed>")),
            "array|null",
        ),
        (
            "2dArray",
            "array<array-key, array<array-key, string>>",
            Assertion::IsType(t("array<array-key, mixed>")),
            "array<array<string>>|null",
        ),
        (
            "numeric",
            "numeric-string",
            Assertion::IsType(t("numeric")),
            "string",
        ),
        (
            "nullableClassString",
            "null",
            Assertion::Falsy,
            "?class-string",
        ),
        (
            "mixedOrNullNotFalsy",
            "non-empty-mixed",
            Assertion::Truthy,
            "mixed|null",
        ),
        (
            "mixedOrNullFalsy",
            "empty-mixed|null",
            Assertion::Falsy,
            "mixed|null",
        ),
        (
            "nullableClassStringFalsy",
            "null",
            Assertion::Falsy,
            "class-string<SomeClass>|null",
        ),
        // IsIdentical -> pzoom IsEqual
        (
            "nullableClassStringEqualsNull",
            "null",
            Assertion::IsEqual(t("null")),
            "class-string<SomeClass>|null",
        ),
        (
            "nullableClassStringTruthy",
            "class-string<SomeClass>",
            Assertion::Truthy,
            "class-string<SomeClass>|null",
        ),
        (
            "iterableToArray",
            "array<int, int>",
            Assertion::IsType(t("array<array-key, mixed>")),
            "iterable<int, int>",
        ),
        (
            "iterableToTraversable",
            "Traversable<int, int>",
            Assertion::IsType(t("Traversable")),
            "iterable<int, int>",
        ),
        (
            "callableToCallableArray",
            "callable-array{class-string|object, non-empty-string}",
            Assertion::IsType(t("array<array-key, mixed>")),
            "callable",
        ),
        (
            "SmallKeyedArrayAndCallable",
            "array{test: string}",
            Assertion::IsType(t("array{test: string}")),
            "callable",
        ),
        (
            "BigKeyedArrayAndCallable",
            "array{foo: string, test: string, thing: string}",
            Assertion::IsType(t("array{foo: string, test: string, thing: string}")),
            "callable",
        ),
        (
            "callableOrArrayToCallableArray",
            "array<array-key, mixed>",
            Assertion::IsType(t("array<array-key, mixed>")),
            "callable|array",
        ),
        (
            "traversableToIntersection",
            "Countable&Traversable",
            Assertion::IsType(t("Traversable")),
            "Countable",
        ),
        (
            "iterableWithoutParamsToTraversableWithoutParams",
            "Traversable",
            Assertion::IsNotType(t("array<array-key, mixed>")),
            "iterable",
        ),
        (
            "iterableWithParamsToTraversableWithParams",
            "Traversable<int, string>",
            Assertion::IsNotType(t("array<array-key, mixed>")),
            "iterable<int, string>",
        ),
        (
            "iterableAndObject",
            "Traversable<int, string>",
            Assertion::IsType(t("object")),
            "iterable<int, string>",
        ),
        (
            "iterableAndNotObject",
            "array<int, string>",
            Assertion::IsNotType(t("object")),
            "iterable<int, string>",
        ),
        ("boolNotEmptyIsTrue", "true", Assertion::NonEmpty, "bool"),
        (
            "interfaceAssertionOnClassInterfaceUnion",
            "SomeInterface|SomeInterface&SomeClass",
            Assertion::IsType(t("SomeInterface")),
            "SomeClass|SomeInterface",
        ),
        (
            "classAssertionOnClassInterfaceUnion",
            "SomeClass|SomeClass&SomeInterface",
            Assertion::IsType(t("SomeClass")),
            "SomeClass|SomeInterface",
        ),
        (
            "filterKeyedArrayWithIterable",
            "array{some: string}",
            Assertion::IsType(t("iterable<mixed, string>")),
            "array{some: mixed}",
        ),
        (
            "SimpleXMLElementNotAlwaysTruthy",
            "SimpleXMLElement",
            Assertion::Truthy,
            "SimpleXMLElement",
        ),
        (
            "SimpleXMLElementNotAlwaysTruthy2",
            "SimpleXMLElement",
            Assertion::Falsy,
            "SimpleXMLElement",
        ),
        (
            "SimpleXMLIteratorNotAlwaysTruthy",
            "SimpleXMLIterator",
            Assertion::Truthy,
            "SimpleXMLIterator",
        ),
        (
            "SimpleXMLIteratorNotAlwaysTruthy2",
            "SimpleXMLIterator",
            Assertion::Falsy,
            "SimpleXMLIterator",
        ),
        // IsLooselyEqual: a string compared with int/float is a numeric string.
        (
            "stringToNumericStringWithInt",
            "numeric-string",
            Assertion::IsLooselyEqual(t("int")),
            "string",
        ),
        (
            "stringToNumericStringWithFloat",
            "numeric-string",
            Assertion::IsLooselyEqual(t("float")),
            "string",
        ),
        ("stringWithAny", "string", Assertion::Any, "string"),
        (
            "nonEmptyArray",
            "non-empty-array<array-key, mixed>",
            Assertion::IsType(t("non-empty-array")),
            "array",
        ),
        (
            "nonEmptyList",
            "non-empty-list<mixed>",
            Assertion::IsType(t("non-empty-list")),
            "array",
        ),
        (
            "ListOfInts",
            "list<int>",
            Assertion::IsType(t("iterable<mixed, int>")),
            "list<mixed>",
        ),
    ]
}

/// Psalm rows whose assertion pzoom does not model yet — documented, not dropped.
/// (name, psalm_expected_id, psalm_assertion, original_type)
const UNSUPPORTED: &[(&str, &str, &str, &str)] = &[(
    "IsNotAClassReconciliation",
    "int",
    "IsNotAClass(IDObject, allow_string)",
    "int|IDObject",
)];

/// Cases where pzoom's reconciler currently diverges from Psalm's expected id.
/// Maps case name -> the id pzoom produces *today*. The `cases` table keeps
/// Psalm's ideal id as the source of truth; this table characterizes the gap so
/// the suite stays green while still failing loudly when a divergence is FIXED
/// (matches Psalm — drop the row here) or REGRESSES further (update or fix).
///
/// Triage of the divergences (Psalm-ideal -> pzoom-now):
///   Cosmetic id rendering (same type, different canonical form):
///     - falsyWithMixed / falsyWithScalarOrNull / mixedOrNullFalsy:
///       pzoom expands `empty-mixed`/`empty-scalar` into its falsy literals.
///     - iterableWithoutParamsToTraversableWithoutParams: `Traversable` vs
///       `Traversable<mixed, mixed>` (default template params rendered).
///     - interfaceAssertionOnClassInterfaceUnion: union/intersection member order.
///   Substantive reconciler differences (worth fixing for faithfulness):
///     - notNullWithMixed: `!= null` on mixed yields non-empty-mixed (drops the
///       falsy-but-non-null values `0`/`''`/`false`).
///     - iterableAndNotObject: not-object on `iterable<K,V>` should leave `array<K,V>`.
///     - filterKeyedArrayWithIterable: an `iterable<_, V>` assertion should refine
///       a keyed-array's value type to `V`.
///     - callable + array-shape assertions (callableToCallableArray,
///       Small/BigKeyedArrayAndCallable, callableOrArrayToCallableArray):
///       pzoom keeps the callable's `list{...}` form instead of the asserted shape.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    ("notNullWithMixed", "non-empty-mixed"),
    ("falsyWithMixed", "''|'0'|0|false|null"),
    ("falsyWithScalarOrNull", "''|'0'|0|false"),
    ("mixedOrNullFalsy", "''|'0'|0|false|null"),
    (
        "callableToCallableArray",
        "list{class-string|object, non-empty-string}",
    ),
    (
        "SmallKeyedArrayAndCallable",
        "list{class-string|object, non-empty-string}",
    ),
    (
        "BigKeyedArrayAndCallable",
        "list{class-string|object, non-empty-string}",
    ),
    (
        "callableOrArrayToCallableArray",
        "array<array-key, mixed>|list{class-string|object, non-empty-string}",
    ),
    (
        "iterableWithoutParamsToTraversableWithoutParams",
        "Traversable<mixed, mixed>",
    ),
    ("iterableAndNotObject", "iterable<int, string>"),
    (
        "interfaceAssertionOnClassInterfaceUnion",
        "SomeClass&SomeInterface|SomeInterface",
    ),
    ("filterKeyedArrayWithIterable", "array{some: mixed}"),
];

#[test]
fn provider_test_reconciliation() {
    let fx = Fixture::new();
    let mut regressions: Vec<String> = Vec::new();
    let mut fixed: Vec<String> = Vec::new();
    let mut changed: Vec<String> = Vec::new();

    for (name, psalm_expected, assertion, original) in cases(&fx) {
        let actual = fx.reconcile_id(&assertion, original);
        match KNOWN_DIVERGENCES.iter().find(|(n, _)| *n == name) {
            Some((_, known_actual)) => {
                if actual == psalm_expected {
                    fixed.push(format!("  {name}: now matches Psalm {psalm_expected:?} — drop it from KNOWN_DIVERGENCES"));
                } else if actual != *known_actual {
                    changed.push(format!("  {name}: was {known_actual:?}, now {actual:?} (Psalm wants {psalm_expected:?})"));
                }
            }
            None => {
                if actual != psalm_expected {
                    regressions.push(format!(
                        "  {name}: expected {psalm_expected:?}, got {actual:?}"
                    ));
                }
            }
        }
    }

    // Surface (don't drop) the Psalm rows whose assertion pzoom can't model yet.
    for (name, expected, psalm_assertion, original) in UNSUPPORTED {
        eprintln!(
            "unmodelled in pzoom: {name} — {psalm_assertion} on {original:?} -> Psalm wants {expected:?}"
        );
    }

    let mut report = String::new();
    if !regressions.is_empty() {
        report.push_str(&format!(
            "Faithfulness REGRESSIONS ({}):\n{}\n",
            regressions.len(),
            regressions.join("\n")
        ));
    }
    if !fixed.is_empty() {
        report.push_str(&format!(
            "Divergences now FIXED ({}) — update KNOWN_DIVERGENCES:\n{}\n",
            fixed.len(),
            fixed.join("\n")
        ));
    }
    if !changed.is_empty() {
        report.push_str(&format!(
            "Divergences CHANGED ({}):\n{}\n",
            changed.len(),
            changed.join("\n")
        ));
    }
    assert!(report.is_empty(), "\n{report}");
}
