<?php
$array1 = array("a" => "green", "b" => "brown", "c" => "blue", "red");
$array2 = array("a" => "GREEN", "B" => "brown", "yellow", "red");
$array3 = array("a" => "GREEN");

function compareKey(string $a, string $b): int { return $a <=> $b; }
function compareValue(mixed $a, mixed $b): int { return -1; }

// Key+value comparison
array_udiff_uassoc($array1, $array2, $array3, "compareKey");
                
