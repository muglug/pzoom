<?php
/**
 * @template T1 as array-key
 * @template T2
 *
 * @param iterable<T1,T2> $x
 *
 * @return array<T1,T2>
 */
function iterableToArray (iterable $x): array {
    if (is_array($x)) {
        return $x;
    }
    else {
        return iterator_to_array($x);
    }
}

/**
 * @param Traversable<int, int> $t
 * @return array<int, int>
 */
function withParams(Traversable $t) : array {
    return iterableToArray($t);
}