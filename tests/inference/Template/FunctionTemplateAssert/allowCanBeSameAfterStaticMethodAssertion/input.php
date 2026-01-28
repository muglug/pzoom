<?php
namespace Bar;

class Assertion {
    /**
     * Asserts that two variables are the same.
     *
     * @template T
     * @param T      $expected
     * @param mixed  $actual
     * @psalm-assert =T $actual
     */
    public static function assertSame($expected, $actual) : void {}
}

class Hello {}
class Goodbye {}

$a = rand(0, 1) ? new Goodbye() : new Hello();
$b = rand(0, 1) ? new Hello() : new Goodbye();
Assertion::assertSame($a, $b);