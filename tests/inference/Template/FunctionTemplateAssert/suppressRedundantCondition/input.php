<?php
namespace Bar;

class A {}

/**
 * @param class-string<T> $expected
 * @param mixed  $actual
 * @param string $message
 *
 * @template T
 * @psalm-assert T $actual
 */
function assertInstanceOf($expected, $actual) : void {
}

/**
 * @psalm-suppress RedundantCondition
 */
function takesA(A $a) : void {
    assertInstanceOf(A::class, $a);
}