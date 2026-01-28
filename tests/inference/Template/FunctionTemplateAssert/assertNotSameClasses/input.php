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

class a {}
class b {}
final class c {}

$expected = rand(0, 1) ? new a : new b;
$actual = new c;
assertSame($expected, $actual);
