<?php
function foo(string $str): ?int {
    if (rand(0, 1) || (!($pos = strpos($str, "a")) && !($pos = strpos($str, "b")))) {
        return null;
    }

    return $pos;
}