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

class Hello {}
class Goodbye {}

$goodbye_or_hello = rand(0, 1) ? new Goodbye() : new Hello();
$hello_or_goodbye = rand(0, 1) ? new Hello() : new Goodbye();
assertNotSame($goodbye_or_hello, $hello_or_goodbye);