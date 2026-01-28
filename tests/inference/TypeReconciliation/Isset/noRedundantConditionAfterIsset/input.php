<?php
/** @param array<string, array<int, string>> $arr */
function foo(array $arr, string $k) : void {
    if (!isset($arr[$k])) {
        return;
    }

    if ($arr[$k][0]) {}
}