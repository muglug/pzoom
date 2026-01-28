<?php
function foo(string $str): ?int {
    $pos = 5;

    if (rand(0, 1) && !($pos = $str)) {
        return null;
    }

    return $pos;
}
