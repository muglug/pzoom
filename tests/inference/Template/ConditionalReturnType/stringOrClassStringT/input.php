<?php
class A {}

/**
 * @template T
 * @param literal-string|class-string<T> $name
 * @return ($name is class-string ? T : mixed)
 */
function get(string $name) {
    return;
}

$lowercase_a = "a";

/** @var class-string $class_string */
$class_string = "b";

/** @psalm-suppress MixedAssignment */
$expect_mixed = get($lowercase_a);
$expect_object = get($class_string);

$expect_a_object = get(A::class);

$expect_mixed_from_literal = get("LiteralDirect");