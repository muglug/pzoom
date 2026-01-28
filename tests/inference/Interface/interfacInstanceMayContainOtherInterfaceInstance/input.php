<?php
interface I1 {}
interface I2 {}
class C implements I1,I2 {}

function f(I1 $a, I2 $b): bool {
    return $a === $b;
}

/**
 * @param  array<I1> $a
 * @param  array<I2> $b
 */
function g(array $a, array $b): bool {
    return $a === $b;
}

$o = new C;
f($o, $o);
