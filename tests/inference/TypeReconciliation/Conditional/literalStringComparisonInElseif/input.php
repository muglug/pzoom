<?php
function foo(string $t, bool $b) : void {
    if ($t === "a") {
    } elseif ($t === "b" && $b) {}
}

function bar(string $t, bool $b) : void {
    if ($t === "a") {
    } elseif ($t === "b" || $b) {}
}