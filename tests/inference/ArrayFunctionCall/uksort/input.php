<?php
function foo (string $a, string $b): int {
    return $a <=> $b;
}

$array = ["b" => 1, "a" => 2];
uksort(
    $array,
    "foo"
);
$emptyArray = [];
uksort(
    $emptyArray,
    "foo"
);
