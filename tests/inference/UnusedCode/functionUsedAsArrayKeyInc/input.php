<?php
/** @param array<int, int> $arr */
function inc(array $arr) : array {
    $arr[strlen("hello")]++;
    return $arr;
}
