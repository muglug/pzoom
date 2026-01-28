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

$int = add(3, 5);
$float1 = add(2.5, 3);
$float2 = add(2.7, 3.1);
$float3 = add(3, 3.5);
/** @psalm-suppress PossiblyNullArgument */
$int = add(rand(0, 1) ? null : 1, 1);