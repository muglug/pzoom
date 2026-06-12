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

$c = 4;
$d = rand(0, 1) ? 5.0 : 6.0;
assertEqual($c, $d);
