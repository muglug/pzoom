<?php
namespace Bar;

/**
 * Asserts that two variables are the same.
 *
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @psalm-assert !=T $actual
 */
function assertNotSame($expected, $actual) : void {}

$c = 4;
$d = rand(0, 1) ? 4 : 5;
assertNotSame($d, $c);

function foo(string $a, string $b) : void {
    assertNotSame($a, $b);
}