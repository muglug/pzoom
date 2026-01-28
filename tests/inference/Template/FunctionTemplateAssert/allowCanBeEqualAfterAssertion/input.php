<?php

/**
 * Asserts that two variables are the same.
 *
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @psalm-assert ~T $actual
 */
function assertEqual($expected, $actual) : void {}

class Hello {}
class Goodbye {}

$a = rand(0, 1) ? new Goodbye() : new Hello();
$b = rand(0, 1) ? new Hello() : new Goodbye();
assertEqual($a, $b);

$c = new Hello();
$d = rand(0, 1) ? new Hello() : new Goodbye();
assertEqual($c, $d);

$c = new Hello();
$d = rand(0, 1) ? new Hello() : new Goodbye();
assertEqual($d, $c);

$c = 4;
$d = rand(0, 1) ? 3.0 : 4.0;
assertEqual($d, $c);

$c = 4.0;
$d = rand(0, 1) ? 3 : 4;
assertEqual($d, $c);

function foo(string $a, string $b) : void {
    assertEqual($a, $b);
}