<?php
function getArray() : array {
    return [];
}

$arr = getArray();

if (rand(0, 1)) {
    /** @psalm-suppress MixedArrayAssignment */
    $arr["hello"]["goodbye"] = 5;
}
