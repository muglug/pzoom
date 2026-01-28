<?php
/** @param array<int, string> $arr */
function foo(array $arr) : string {
    if (!isset($arr[0])) {
        $arr[0] = "hello";
    }

    return $arr[0];
}