<?php
/** @param array<string, string> $array */
function f(array $array) : void {
    if ($array["foo"] === "ok") {
        if ($array["bar"] === "a") {}
        if ($array["bar"] === "b") {}
    }
}