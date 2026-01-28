<?php
/** @param "a"|"b" $b */
function type(string $b): void {}

function foo(string $a) : void {
    if ($a === "a" || $a === "b") {
        type($a);
    }
}