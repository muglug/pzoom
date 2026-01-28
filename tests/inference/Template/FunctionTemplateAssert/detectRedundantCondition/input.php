<?php
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

function takesA(A $a) : void {
    assertInstanceOf(A::class, $a);
}
