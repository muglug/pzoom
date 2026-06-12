<?php
/**
 * @psalm-suppress InvalidReturnType
 * @return array{a: 15, ...}
 */
function getUnsealedArray() {}
function takesString(string $str): void {}

$foo = "foo";
$a = getUnsealedArray();
extract($a);
takesString($foo);
