<?php
/** @return array<int, mixed> */
function getStrings(): array {
    return ["hello", "world", 50];
}

$a = getStrings();

if (is_string($a[0]) && strlen($a[0]) > 3) {}