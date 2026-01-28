<?php
function baz (int $a, int $b): int { return $a <=> $b; }
$array = ["foo" => 123, "bar" => 456];
usort($array, "baz");
$emptyArray = [];
usort($emptyArray, "baz");
