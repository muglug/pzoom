//! Port of Psalm's `tests/TypeCombinationTest.php` (providerTestValidTypeCombination).
//!
//! Each case parses Psalm type strings into atomics (from_docblock = true,
//! as in Psalm's test) and asserts the combined union's rendering — in both
//! input orders, mirroring Psalm's forward + reversed assertions.
//!
//! Expected strings are Psalm's, translated to pzoom's `get_id` rendering
//! where the two display the same type differently (literals, list fallbacks).
//! Cases where pzoom's *combination* genuinely differs are listed in
//! `KNOWN_DIVERGENCES` with the current output, so progress on them is
//! visible: fixing one turns up as a test failure asking for promotion.

use pzoom_code_info::TUnion;
use pzoom_code_info::ttype::type_combiner;
use pzoom_str::Interner;
use pzoom_syntax::docblock::parse_type_string;

fn combine_to_id(types: &[&str], interner: &Interner, reverse: bool) -> String {
    let mut atomics = Vec::new();
    for type_str in types {
        let parsed = parse_type_string(type_str, interner)
            .unwrap_or_else(|e| panic!("failed to parse {type_str:?}: {e:?}"));
        atomics.extend(parsed.types);
    }
    if reverse {
        atomics.reverse();
    }
    let combined = type_combiner::combine(atomics, false);
    TUnion::from_types(combined).get_id(Some(interner))
}

struct Case {
    name: &'static str,
    expected: &'static str,
    types: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case { name: "complexArrayFallback1", expected: "array{other_references: list<Psalm\\Internal\\Analyzer\\DataFlowNodeData>|null, taint_trace: list<array<array-key, mixed>>|null, ...<string, mixed>}", types: &["array{other_references: list<Psalm\\Internal\\Analyzer\\DataFlowNodeData>|null, taint_trace: null}&array<string, mixed>", "array{other_references: list<Psalm\\Internal\\Analyzer\\DataFlowNodeData>|null, taint_trace: list<array<array-key, mixed>>}&array<string, mixed>"] },
    Case { name: "complexArrayFallback2", expected: "list{0?: 0|a, 1?: 0|a, ...<a>}", types: &["list<a>", "list{0, 0}"] },
    Case { name: "intOrString", expected: "int|string", types: &["int", "string"] },
    Case { name: "mixedOrNull", expected: "mixed|null", types: &["mixed", "null"] },
    Case { name: "mixedOrNever", expected: "mixed", types: &["never", "mixed"] },
    Case { name: "mixedOrObject", expected: "mixed|object", types: &["mixed", "object"] },
    Case { name: "mixedOrEmptyArray", expected: "array<never, never>|mixed", types: &["mixed", "array<never, never>"] },
    Case { name: "falseTrueToBool", expected: "bool", types: &["false", "true"] },
    Case { name: "trueFalseToBool", expected: "bool", types: &["true", "false"] },
    Case { name: "trueBoolToBool", expected: "bool", types: &["true", "bool"] },
    Case { name: "boolTrueToBool", expected: "bool", types: &["bool", "true"] },
    Case { name: "intOrTrueOrFalseToBool", expected: "bool|int", types: &["int", "false", "true"] },
    Case { name: "intOrBoolOrTrueToBool", expected: "bool|int", types: &["int", "bool", "true"] },
    Case { name: "intOrTrueOrBoolToBool", expected: "bool|int", types: &["int", "true", "bool"] },
    Case { name: "arrayOfIntOrString", expected: "array<array-key, int|string>", types: &["array<int>", "array<string>"] },
    Case { name: "arrayOfIntOrAlsoString", expected: "array<array-key, int>|string", types: &["array<int>", "string"] },
    Case { name: "emptyArrays", expected: "array<never, never>", types: &["array<never, never>", "array<never, never>"] },
    Case { name: "arrayStringOrEmptyArray", expected: "array<array-key, string>", types: &["array<never>", "array<string>"] },
    Case { name: "arrayMixedOrString", expected: "array<array-key, mixed|string>", types: &["array<mixed>", "array<string>"] },
    Case { name: "arrayMixedOrStringKeys", expected: "array<array-key, string>", types: &["array<int|string,string>", "array<mixed,string>"] },
    Case { name: "arrayMixedOrEmpty", expected: "array<array-key, mixed>", types: &["array<never>", "array<mixed>"] },
    Case { name: "arrayBigCombination", expected: "array<array-key, float|int|string>", types: &["array<int|float>", "array<string>"] },
    Case { name: "arrayTraversableToIterable", expected: "iterable<array-key|mixed, mixed>", types: &["array", "Traversable"] },
    Case { name: "arrayIterableToIterable", expected: "iterable<mixed, mixed>", types: &["array", "iterable"] },
    Case { name: "iterableArrayToIterable", expected: "iterable<mixed, mixed>", types: &["iterable", "array"] },
    Case { name: "traversableIterableToIterable", expected: "iterable<mixed, mixed>", types: &["Traversable", "iterable"] },
    Case { name: "iterableTraversableToIterable", expected: "iterable<mixed, mixed>", types: &["iterable", "Traversable"] },
    Case { name: "arrayTraversableToIterableWithParams", expected: "iterable<int, bool|string>", types: &["array<int, string>", "Traversable<int, bool>"] },
    Case { name: "arrayIterableToIterableWithParams", expected: "iterable<int, bool|string>", types: &["array<int, string>", "iterable<int, bool>"] },
    Case { name: "iterableArrayToIterableWithParams", expected: "iterable<int, bool|string>", types: &["iterable<int, string>", "array<int, bool>"] },
    Case { name: "traversableIterableToIterableWithParams", expected: "iterable<int, bool|string>", types: &["Traversable<int, string>", "iterable<int, bool>"] },
    Case { name: "iterableTraversableToIterableWithParams", expected: "iterable<int, bool|string>", types: &["iterable<int, string>", "Traversable<int, bool>"] },
    Case { name: "arrayObjectAndParamsWithEmptyArray", expected: "ArrayObject<int, string>|array<never, never>", types: &["ArrayObject<int, string>", "array<never, never>"] },
    Case { name: "emptyArrayWithArrayObjectAndParams", expected: "ArrayObject<int, string>|array<never, never>", types: &["array<never, never>", "ArrayObject<int, string>"] },
    Case { name: "emptyArrayAndFalse", expected: "array<never, never>|false", types: &["array<never, never>", "false"] },
    Case { name: "emptyArrayAndTrue", expected: "array<never, never>|true", types: &["array<never, never>", "true"] },
    Case { name: "emptyArrayWithTrueAndFalse", expected: "array<never, never>|bool", types: &["array<never, never>", "true", "false"] },
    Case { name: "falseDestruction", expected: "bool", types: &["false", "bool"] },
    Case { name: "onlyFalse", expected: "false", types: &["false"] },
    Case { name: "onlyTrue", expected: "true", types: &["true"] },
    Case { name: "falseFalseDestruction", expected: "false", types: &["false", "false"] },
    Case { name: "aAndAOfB", expected: "A|A<B>", types: &["A", "A<B>"] },
    Case { name: "combineObjectType1", expected: "array{a?: int, b?: string}", types: &["array{a: int}", "array{b: string}"] },
    Case { name: "combineObjectType2", expected: "array{a: int|string, b?: string}", types: &["array{a: int}", "array{a: string,b: string}"] },
    Case { name: "combinePossiblyUndefinedKeys", expected: "array{a: bool, b?: mixed, d?: mixed}", types: &["array{a: false, b: mixed}", "array{a: true, d: mixed}", "array{a: true, d: mixed}"] },
    Case { name: "combinePossiblyUndefinedKeysAndString", expected: "array{a: string, b?: int}|string", types: &["array{a: string, b?: int}", "string"] },
    Case { name: "combineMixedArrayWithTKeyedArray", expected: "array<array-key, mixed>", types: &["array{a: int}", "array"] },
    Case { name: "traversableAorB", expected: "Traversable<mixed, A|B>", types: &["Traversable<A>", "Traversable<B>"] },
    Case { name: "iterableAorB", expected: "iterable<mixed, A|B>", types: &["iterable<A>", "iterable<B>"] },
    Case { name: "FooAorB", expected: "Foo<A>|Foo<B>", types: &["Foo<A>", "Foo<B>"] },
    Case { name: "traversableOfMixed", expected: "Traversable<mixed, mixed>", types: &["Traversable", "Traversable<mixed, mixed>"] },
    // pzoom renders intersection members alphabetically (Psalm keeps input
    // order: `Traversable&Iterator`).
    Case { name: "traversableAndIterator", expected: "Iterator&Traversable", types: &["Traversable&Iterator", "Traversable&Iterator"] },
    Case { name: "traversableOfMixedAndIterator", expected: "Iterator&Traversable<mixed, mixed>", types: &["Traversable<mixed, mixed>&Iterator", "Traversable<mixed, mixed>&Iterator"] },
    Case { name: "combineClosures", expected: "Closure(A):void|Closure(B):void", types: &["Closure(A):void", "Closure(B):void"] },
    Case { name: "combineClassStringWithString", expected: "string", types: &["class-string", "string"] },
    Case { name: "combineClassStringWithFalse", expected: "class-string|false", types: &["class-string", "false"] },
    Case { name: "combineRefinedClassStringWithString", expected: "string", types: &["class-string<Exception>", "string"] },
    Case { name: "combineRefinedClassStrings", expected: "class-string<Exception>|class-string<Iterator>", types: &["class-string<Exception>", "class-string<Iterator>"] },
    Case { name: "combineClassStringsWithLiteral", expected: "class-string", types: &["class-string", "Exception::class"] },
    Case { name: "combineClassStringWithNumericString", expected: "class-string|numeric-string", types: &["class-string", "numeric-string"] },
    Case { name: "combineRefinedClassStringWithNumericString", expected: "class-string<Exception>|numeric-string", types: &["class-string<Exception>", "numeric-string"] },
    Case { name: "combineClassStringWithTraitString", expected: "class-string|trait-string", types: &["class-string", "trait-string"] },
    Case { name: "combineRefinedClassStringWithTraitString", expected: "class-string<Exception>|trait-string", types: &["class-string<Exception>", "trait-string"] },
    Case { name: "combineCallableAndCallableString", expected: "callable", types: &["callable", "callable-string"] },
    Case { name: "combineCallableStringAndCallable", expected: "callable", types: &["callable-string", "callable"] },
    Case { name: "combineCallableAndCallableObject", expected: "callable", types: &["callable", "callable-object"] },
    Case { name: "combineCallableObjectAndCallable", expected: "callable", types: &["callable-object", "callable"] },
    Case { name: "combineCallableAndCallableArray", expected: "callable", types: &["callable", "callable-array"] },
    Case { name: "combineCallableArrayAndCallable", expected: "callable", types: &["callable-array", "callable"] },
    Case { name: "combineCallableAndCallableList", expected: "callable", types: &["callable", "callable-list"] },
    Case { name: "combineCallableListAndCallable", expected: "callable", types: &["callable-list", "callable"] },
    Case { name: "combineCallableArrayAndArray", expected: "array<array-key, mixed>", types: &["callable-array{class-string, string}", "array"] },
    Case { name: "combineGenericArrayAndMixedArray", expected: "array<array-key, int|mixed>", types: &["array<string, int>", "array<array-key, mixed>"] },
    Case { name: "combineTKeyedArrayAndArray", expected: "array<array-key, mixed>", types: &["array{hello: int}", "array<array-key, mixed>"] },
    Case { name: "combineTKeyedArrayAndNestedArray", expected: "array<array-key, mixed>", types: &["array{hello: array{goodbye: int}}", "array<array-key, mixed>"] },
    Case { name: "combineNumericStringWithLiteralString", expected: "numeric-string", types: &["numeric-string", "\"1\""] },
    Case { name: "combineLiteralStringWithNumericString", expected: "numeric-string", types: &["\"1\"", "numeric-string"] },
    Case { name: "combineNonEmptyListWithTKeyedArrayList", expected: "list{null|string, ...<string>}", types: &["non-empty-list<string>", "array{null}"] },
    Case { name: "combineZeroAndPositiveInt", expected: "int<0, max>", types: &["0", "positive-int"] },
    Case { name: "combinePositiveIntAndZero", expected: "int<0, max>", types: &["positive-int", "0"] },
    Case { name: "combinePositiveIntAndMinusOne", expected: "int<-1, max>", types: &["positive-int", "-1"] },
    Case { name: "combinePositiveIntZeroAndMinusOne", expected: "int<-1, max>", types: &["0", "positive-int", "-1"] },
    Case { name: "combineMinusOneAndPositiveInt", expected: "int<-1, max>", types: &["-1", "positive-int"] },
    Case { name: "combineZeroMinusOneAndPositiveInt", expected: "int<-1, max>", types: &["0", "-1", "positive-int"] },
    Case { name: "combineZeroOneAndPositiveInt", expected: "int<0, max>", types: &["0", "1", "positive-int"] },
    Case { name: "combinePositiveIntOneAndZero", expected: "int<0, max>", types: &["positive-int", "1", "0"] },
    Case { name: "combinePositiveInts", expected: "int<1, max>", types: &["positive-int", "positive-int"] },
    Case { name: "combineNonEmptyArrayAndKeyedArray", expected: "array<int, int>", types: &["non-empty-array<int, int>", "array{0?:int}"] },
    Case { name: "combineNonEmptyStringAndLiteral", expected: "non-empty-string", types: &["non-empty-string", "\"foo\""] },
    Case { name: "combineLiteralAndNonEmptyString", expected: "non-empty-string", types: &["\"foo\"", "non-empty-string"] },
    Case { name: "combineTruthyStringAndNonEmptyString", expected: "non-empty-string", types: &["truthy-string", "non-empty-string"] },
    Case { name: "combineNonFalsyNonEmptyString", expected: "non-empty-string", types: &["non-falsy-string", "non-empty-string"] },
    Case { name: "combineNonEmptyNonFalsyString", expected: "non-empty-string", types: &["non-empty-string", "non-falsy-string"] },
    Case { name: "combineNonEmptyStringAndNumericString", expected: "non-empty-string", types: &["non-empty-string", "numeric-string"] },
    Case { name: "combineNumericStringAndNonEmptyString", expected: "non-empty-string", types: &["numeric-string", "non-empty-string"] },
    Case { name: "combineNonEmptyLowercaseAndNonFalsyString", expected: "non-empty-string", types: &["non-falsy-string", "non-empty-lowercase-string"] },
    Case { name: "combineNonEmptyAndEmptyScalar", expected: "scalar", types: &["non-empty-scalar", "empty-scalar"] },
    Case { name: "combineLiteralStringAndNonspecificLiteral", expected: "literal-string", types: &["literal-string", "\"foo\""] },
    Case { name: "combineNonspecificLiteralAndLiteralString", expected: "literal-string", types: &["\"foo\"", "literal-string"] },
    Case { name: "combineLiteralIntAndNonspecificLiteral", expected: "literal-int", types: &["literal-int", "5"] },
    Case { name: "combineNonspecificLiteralAndLiteralInt", expected: "literal-int", types: &["5", "literal-int"] },
    Case { name: "combineNonspecificLiteralAndPositiveInt", expected: "int", types: &["positive-int", "literal-int"] },
    Case { name: "combinePositiveAndLiteralInt", expected: "int", types: &["literal-int", "positive-int"] },
    Case { name: "combineNonEmptyStringAndNonEmptyNonSpecificLiteralString", expected: "non-empty-string", types: &["non-empty-literal-string", "non-empty-string"] },
    Case { name: "combineNonEmptyNonSpecificLiteralStringAndNonEmptyString", expected: "non-empty-string", types: &["non-empty-string", "non-empty-literal-string"] },
    Case { name: "nonFalsyStringAndFalsyLiteral", expected: "non-empty-string", types: &["non-falsy-string", "\"0\""] },
    Case { name: "unionOfClassStringAndClassStringWithIntersection", expected: "class-string<IFoo>", types: &["class-string<IFoo>", "class-string<IFoo & IBar>"] },
    Case { name: "unionNonEmptyLiteralStringAndLiteralString", expected: "literal-string", types: &["non-empty-literal-string", "literal-string"] },
    Case { name: "unionLiteralStringAndNonEmptyLiteralString", expected: "literal-string", types: &["literal-string", "non-empty-literal-string"] },
];

/// name -> current (divergent) pzoom output. Kept failing-as-documented:
/// if pzoom starts producing Psalm's expected value, the entry must be removed
/// (the test will fail asking for the promotion). Causes:
///
/// - parse-vocabulary gaps: `callable-list` / `empty-scalar` /
///   `non-empty-scalar` parse as opaque named types, `callable-object`
///   lowers to `object`, `trait-string` to `class-string`, and
///   `callable-array` lowers to its keyed-array shape
///   `list{class-string|object, non-empty-string}` (losing the callable
///   provenance Psalm's TCallableKeyedArray keeps) — so the combiner cannot
///   relate them the way Psalm does;
/// - `arrayTraversableToIterable`: Psalm only recombines a *docblock*
///   param-less `Traversable` with an array into `iterable`; the combiner has
///   no per-atomic docblock provenance to make that call;
/// - `complexArrayFallback2` / `combineNonEmptyListWithTKeyedArrayList`:
///   Psalm rebuilds positional `list{0?: ..., ...<V>}` entries from list
///   min/max counts in getArrayTypeFromGenericParams; pzoom collapses to a
///   generic (non-empty) list.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    (
        "arrayTraversableToIterable",
        "Traversable|array<array-key, mixed>",
    ),
    (
        "combineCallableAndCallableArray",
        "callable|list{class-string|object, non-empty-string}",
    ),
    ("combineCallableAndCallableList", "callable|callable-list"),
    ("combineCallableAndCallableObject", "callable|object"),
    (
        "combineCallableArrayAndCallable",
        "callable|list{class-string|object, non-empty-string}",
    ),
    ("combineCallableListAndCallable", "callable|callable-list"),
    ("combineCallableObjectAndCallable", "callable|object"),
    ("combineClassStringWithTraitString", "class-string"),
    (
        "combineNonEmptyAndEmptyScalar",
        "empty-scalar|non-empty-scalar",
    ),
    (
        "combineNonEmptyListWithTKeyedArrayList",
        "non-empty-list<null|string>",
    ),
    (
        "combineRefinedClassStringWithTraitString",
        "class-string|class-string<Exception>",
    ),
    ("complexArrayFallback2", "list<0|a>"),
];

#[test]
fn type_combination_matches_psalm() {
    let interner = Interner::default();
    let mut failures = Vec::new();
    for case in CASES {
        let expected = KNOWN_DIVERGENCES
            .iter()
            .find(|(name, _)| *name == case.name)
            .map(|(_, current)| *current)
            .unwrap_or(case.expected);
        for reverse in [false, true] {
            let actual = combine_to_id(case.types, &interner, reverse);
            if actual != expected {
                failures.push(format!(
                    "{} ({}): expected `{}`, got `{}`",
                    case.name,
                    if reverse { "reversed" } else { "forward" },
                    expected,
                    actual
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
