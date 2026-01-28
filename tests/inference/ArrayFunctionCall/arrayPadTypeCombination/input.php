<?php
$a = array_pad(["foo" => 1, "bar" => "two"], 5, false);
$b = array_pad(["a", 2, 3.14], 5, null);
/** @var list<string|bool> $list */
$c = array_pad($list, 5, 0);
/** @var array<string, string> $array */
$d = array_pad($array, 5, null);
