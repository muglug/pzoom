<?php
/**
 * @template TValue
 * @template TKey as array-key
 *
 * @param array<TKey, TValue> $arr
 * @return TValue|null
 */
function my_array_pop(array &$arr) {
    return array_pop($arr);
}

$b = ["a" => 5, "c" => 6];
$a = my_array_pop($b);