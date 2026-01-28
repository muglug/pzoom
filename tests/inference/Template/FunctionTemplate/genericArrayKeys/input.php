<?php
/**
 * @template T as array-key
 *
 * @param array<T, mixed> $arr
 * @return list<T>
 */
function my_array_keys($arr) {
    return array_keys($arr);
}

$a = my_array_keys(["hello" => 5, "goodbye" => new \Exception()]);