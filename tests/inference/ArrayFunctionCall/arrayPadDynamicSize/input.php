<?php
function getSize(): int { return random_int(1, 10); }

$a = array_pad(["foo" => 1, "bar" => 2], getSize(), 123);
$b = array_pad(["a", "b", "c"], getSize(), "x");
/** @var list<int> $list */
$c = array_pad($list, getSize(), 0);
/** @var array<string, string> $array */
$d = array_pad($array, getSize(), "");
