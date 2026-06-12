<?php
/** @param int|string $_k */
function takesArrayKey($_k): void {}
function takesString(string $_s): void {}

/** @param array<string, int> $a @param non-empty-array<string, int> $b */
function f(array $a, array $b): void {
    $k1 = key($a);
    if ($k1 !== null) {
        takesString($k1);
    }
    $k2 = key($b);
    takesArrayKey($k2);
}
