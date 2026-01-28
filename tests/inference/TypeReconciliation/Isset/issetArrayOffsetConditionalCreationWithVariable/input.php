<?php
/** @param array<int, string> $arr */
function foo(array $arr) : string {
    $b = 5;

    if (!isset($arr[$b])) {
        $arr[$b] = "hello";
    }

    return $arr[$b];
}