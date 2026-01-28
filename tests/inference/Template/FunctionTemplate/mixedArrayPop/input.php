<?php
/**
 * @template TValue
 *
 * @param array<array-key, TValue> $arr
 * @return TValue|null
 */
function my_array_pop(array &$arr) {
    return array_pop($arr);
}

/** @var mixed */
$b = ["a" => 5, "c" => 6];
$a = my_array_pop($b);