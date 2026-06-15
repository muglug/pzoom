<?php
/** @param array<string> $key */
function foo(object $a, string $k) : string {
    $sortA = "";

    $sortA .= $a->$k;

    return $sortA;
}
