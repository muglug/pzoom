<?php
/**
 * @param Closure(int, int): bool $op
 * @return Closure(int): bool
 */
function reflexive(Closure $op): Closure {
    return fn ($x) => $op($x, $x);
}

$res = reflexive(fn(int $a, int $b): bool => $a === $b);
