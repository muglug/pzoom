<?php
/**
 * @template TKey as array-key
 * @template TValue as array-key
 *
 * @param array<TKey, TValue> $arr
 * @return array<TValue, TKey>
 */
function my_array_flip($arr) {
    return array_flip($arr);
}

$b = my_array_flip(["hello" => 5, "goodbye" => 6]);