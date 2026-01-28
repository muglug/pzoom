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

$hello = new Hello();
$hello_or_goodbye = rand(0, 1) ? new Hello() : new Goodbye();
assertNotSame($hello, $hello_or_goodbye);