<?php
/**
 * @param  array<int, int> $arr
 */
function foo(array $arr) : int {
    $b = rand(0, 3);
    if (!isset($arr[$b])) {
        throw new \Exception("bad");
    }
    return $arr[$b];
}