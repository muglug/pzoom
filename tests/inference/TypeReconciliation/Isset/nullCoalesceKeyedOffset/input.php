<?php
function getArray() : array {
    return [];
}

$foo = getArray();

$foo["a"] = $foo["a"] ?? "hello";