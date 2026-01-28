<?php
/**
 * Asserts that two variables are not the same.
 *
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @psalm-assert T $actual
 */
function assertSame($expected, $actual) : void {}

$expected = rand(0, 1) ? 4 : 5;
$actual = 6;
assertSame($expected, $actual);
