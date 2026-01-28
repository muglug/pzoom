<?php
namespace Bar;

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
class Goodbye {}

$a = rand(0, 1) ? new Goodbye() : new Hello();
$b = rand(0, 1) ? new Hello() : new Goodbye();
assertSame($a, $b);

$c = new Hello();
$d = rand(0, 1) ? new Hello() : new Goodbye();
assertSame($c, $d);

$c = new Hello();
$d = rand(0, 1) ? new Hello() : new Goodbye();
assertSame($d, $c);

$c = 4;
$d = rand(0, 1) ? 4 : 5;
assertSame($d, $c);

$d = rand(0, 1) ? 4 : null;
assertSame(null, $d);

function assertStringsAreSame(string $a, string $b) : void {
    assertSame($a, $b);
}

/** @param mixed $a */
function assertMaybeStringsAreSame($a, string $b) : void {
    assertSame($a, $b);
}

/** @param mixed $b */
function alsoAssertMaybeStringsAreSame(string $a, $b) : void {
    assertSame($a, $b);
}