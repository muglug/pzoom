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

function bar(string $i, array $j) : void {
    assertNotSame($i, $j);
}
