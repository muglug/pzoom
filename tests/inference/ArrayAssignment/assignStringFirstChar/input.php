<?php
/** @param non-empty-list<string> $arr */
function foo(array $arr) : string {
    $arr[0][0] = "a";
    return $arr[0];
}
