<?php
function foo(string $a): void {
    if ($a === "foo") {
        // do something
    } elseif ($a === "bar") {
        // can never get here
    }
}