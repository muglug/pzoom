<?php
/** @param array<string, string> $array */
function foo(array $array) : void {
    $c = 0;

    if ($array["a"] === "a") {
        foreach ([rand(0, 1), rand(0, 1)] as $i) {
            if ($array["b"] === "c") {}
            $c++;
        }
    }
}