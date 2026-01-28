<?php
/**
 * @template T as array
 * @template K as key-of<T>
 *
 * @param T $o
 * @param K $name
 *
 * @return T[K]
 */
function getOffset(array $o, $name) {
    return $o[$name];
}

$a = ["foo" => "hello", "bar" => 2];

$b = getOffset($a, "foo");
$c = getOffset($a, "bar");