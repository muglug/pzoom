<?php

/**
 * @template T
 * @param mixed $input
 * @param array<array-key,T> $values
 * @psalm-assert =T $input
 */
function assertOneOf($input, array $values): void {}

/** @param "a" $value */
function consumeSpecificStringValue(string $value): void {}

/** @param literal-string $value */
function consumeLiteralStringValue(string $value): void {}

function consumeAnyIntegerValue(int $value): void {}

function consumeAnyFloatValue(float $value): void {}

/** @var string $string */
$string;

/** @var string $anotherString */
$anotherString;

/** @var null|string $nullableString */
$nullableString;

/** @var mixed $maybeInt */
$maybeInt;
/** @var mixed $maybeFloat */
$maybeFloat;

assertOneOf($string, ["a"]);
consumeSpecificStringValue($string);

assertOneOf($anotherString, ["a", "b", "c"]);
consumeLiteralStringValue($anotherString);

assertOneOf($nullableString, ["a", "b", "c"]);
assertOneOf($nullableString, ["a", "c"]);

assertOneOf($maybeInt, [1, 2, 3]);
consumeAnyIntegerValue($maybeInt);

assertOneOf($maybeFloat, [1.5, 2.5, 3.5]);
consumeAnyFloatValue($maybeFloat);

/** @var "a"|"b"|"c" $abc */
$abc;

/** @param "a"|"b" $aOrB */
function consumeAOrB(string $aOrB): void {}
assertOneOf($abc, ["a", "b"]);
consumeAOrB($abc);
