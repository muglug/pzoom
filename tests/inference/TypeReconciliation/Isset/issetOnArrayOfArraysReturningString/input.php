<?php
function foo(int $i) : ?string {
    /** @var array<array> */
    $tokens = [];

    if (!isset($tokens[$i]["a"])) {
        return $tokens[$i]["a"];
    }

    return "hello";
}
