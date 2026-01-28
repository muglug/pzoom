<?php
function foo(int $i) : void {
    /** @var array<int, array<string, string>> */
    $tokens = [];

    if (!isset($tokens[$i]["a"])) {
        echo $tokens[$i]["b"];
    }
}