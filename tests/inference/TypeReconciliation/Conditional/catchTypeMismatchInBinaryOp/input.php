<?php
/** @return array<int, string|int> */
function getStrings(): array {
    return ["hello", "world", 50];
}

$a = getStrings();

if (is_bool($a[0]) && $a[0]) {}
