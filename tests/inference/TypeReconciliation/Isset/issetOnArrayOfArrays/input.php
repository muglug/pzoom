<?php
/**
 * @psalm-suppress MixedArgument
 */
function foo(int $i) : void {
    /** @var array<array> */
    $tokens = [];

    if (!isset($tokens[$i]["a"])) {
        echo $tokens[$i]["b"];
    }
}