<?php
interface Foo {}

/**
 * @template T as object
 *
 * @param mixed $value
 * @param class-string<T> $type
 * 
 * @psalm-assert T $value
 */
function assertInstanceOf($value, string $type): void {
    // some code
}

function getFoo() : Foo {
    return new class implements Foo {};
}

$f = getFoo();
/**
 * @var mixed
 */
$class = "hello";

/** @psalm-suppress MixedArgument */
assertInstanceOf($f, $class);