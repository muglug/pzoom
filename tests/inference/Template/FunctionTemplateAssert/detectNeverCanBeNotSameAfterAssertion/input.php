<?php

/**
 * Asserts that two variables are the same.
 *
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @psalm-assert !=T $actual
 */
function assertNotSame($expected, $actual) : void {}

class Hello {}
class Helloa {}
class Goodbye {}

$c = new Helloa();
$d = rand(0, 1) ? new Hello() : new Goodbye();
assertNotSame($c, $d);
