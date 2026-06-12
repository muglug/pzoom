<?php

/** @param array<string, int> $arr */
function f(array $arr): int
{
    $n = 0;
    foreach ($arr as $v) {
        if ($v > 2) {
            $n++;
        }
    }
    assert($n !== 0);
    /** @psalm-check-type-exact $n = int<1, max> */
    return $n;
}
