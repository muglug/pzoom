<?php
/**
 * @template TKey as array-key
 * @template TValue
 *
 * @param array<TKey, TValue> $arr
 * @param array $arr2
 * @return array<TKey, TValue>
 */
function splat_proof(array $arr, array $arr2) {
    return $arr;
}

$foo = [
    [1, 2, 3],
    [1, 2],
];

$a = splat_proof(...$foo);