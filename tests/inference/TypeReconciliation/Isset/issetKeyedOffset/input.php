<?php
function getArray() : array {
    return [];
}

$foo = getArray();

if (!isset($foo["a"])) {
    $foo["a"] = "hello";
}