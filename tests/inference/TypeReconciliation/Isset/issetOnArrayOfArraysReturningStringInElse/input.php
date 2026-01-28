<?php
function foo(int $i) : string {
    /** @var array<int, array<string, string>> */
    $tokens = [];

    if (isset($tokens[$i]["a"])) {
        return "hello";
    } else {
        return $tokens[$i]["b"];
    }
}