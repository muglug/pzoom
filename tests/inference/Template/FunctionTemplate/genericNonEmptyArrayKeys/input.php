<?php
/**
 * @template T as array-key
 *
 * @param non-empty-array<T, mixed> $arr
 * @return non-empty-list<T>
 */
function my_array_keys($arr) {
    return array_keys($arr);
}

$a = my_array_keys(["hello" => 5, "goodbye" => new \Exception()]);