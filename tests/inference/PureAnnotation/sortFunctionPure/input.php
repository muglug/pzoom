<?php
/**
 * @psalm-pure
 *
 * @param int[] $ar
 */
function foo(array $ar): int
{
    usort($ar, static function (int $a, int $b): int {
        return $a <=> $b;
    });

    return $ar[0] ?? 0;
}
