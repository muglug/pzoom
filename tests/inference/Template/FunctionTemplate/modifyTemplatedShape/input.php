<?php
/**
 * @template T as array{a: int}
 * @param T $s
 * @return T
 */
function foo(array $s) : array {
    $s["a"] = 123;
    return $s;
}
