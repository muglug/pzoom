<?php
/**
 * @template TKey as array-key
 * @template TValue
 *
 * @param array<TKey, TValue> $arr
 * @param-out list<TValue> $arr
 */
function example_sort_by_ref(array &$arr): bool {
    $arr = array_values($arr);
    return true;
}

/**
 * @param array<int, array{0: int, 1: string}> $array
 * @return list<array{0: int, 1: string}>
 */
function example(array $array): array {
    example_sort_by_ref($array);
    return $array;
}