<?php
/**
 * @template T as array{a: int}
 * @return T
 */
function foo() : array {
    $b = ["a" => 123];
    return $b;
}
