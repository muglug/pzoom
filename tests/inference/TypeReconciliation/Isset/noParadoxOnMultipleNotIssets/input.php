<?php
/** @var array */
$array = [];
function sameString(string $string): string {
    return $string;
}

if (isset($array[sameString("key1")]) || isset($array[sameString("key2")])) {
    throw new \InvalidArgumentException();
}

if (!isset($array[sameString("key3")]) || !isset($array[sameString("key4")])) {
    throw new \InvalidArgumentException();
}