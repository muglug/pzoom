<?php
/**
 * @template T
 * @param T      $expected
 * @param mixed  $actual
 * @param string $message
 * @psalm-assert !=T $actual
 * @return void
 */
function assertNotSame($expected, $actual, $message = "") {}

class Hello {}

function bar(array $j) : void {
    assertNotSame(new Hello(), $j);
}
