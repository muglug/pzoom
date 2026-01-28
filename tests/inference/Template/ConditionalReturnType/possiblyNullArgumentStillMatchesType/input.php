<?php
/**
 * @template T as int|float
 * @param T $a
 * @param T $b
 * @return int|float
 * @psalm-return (T is int ? int : float)
 */
function add($a, $b) {
    return $a + $b;
}

/** @psalm-suppress PossiblyNullArgument */
$int = add(rand(0, 1) ? null : 1, 4);