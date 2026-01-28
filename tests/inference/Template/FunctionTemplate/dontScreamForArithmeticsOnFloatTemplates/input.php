<?php
/**
 * @template T of ?float
 * @param T $p
 * @return (T is null ? null : float)
 */
function foo(?float $p): ?float
{
    if ($p === null) {
        return null;
    }
    return $p - 1;
}
