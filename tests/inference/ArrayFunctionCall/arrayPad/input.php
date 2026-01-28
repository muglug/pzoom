<?php
$a = array_pad(["foo" => 1, "bar" => 2], 10, 123);
$b = array_pad(["a", "b", "c"], 10, "x");
/** @var list<int> $list */
$c = array_pad($list, 10, 0);
/** @var array<string, string> $array */
$d = array_pad($array, 10, "");
