<?php

/**
 * Asserts that two variables are the same.
 *
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @psalm-assert =T $actual
 */
function assertSame($expected, $actual) : void {}

class Hello {}

$a = 5;
$b = new Hello();
assertSame($a, $b);
