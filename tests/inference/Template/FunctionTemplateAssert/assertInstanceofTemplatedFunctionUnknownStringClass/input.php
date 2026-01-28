<?php
namespace Bar;

/**
 * @template T
 * @param class-string<T> $expected
 * @param mixed  $actual
 * @psalm-assert T $actual
 */
function assertInstanceOf($expected, $actual) : void {}

function bar(string $c, object $e) : void {
    assertInstanceOf($c, $e);
    echo $e->getCode();
}