<?php
$array1 = array("a" => "green", "b" => "brown", "c" => "blue", "red");
$array2 = array("a" => "GREEN", "B" => "brown", "yellow", "red");
$array3 = array("a" => "GREEN");

function compareKey(string $a, string $b): int { return $a <=> $b; }
function compareValue(mixed $a, mixed $b): int { return -1; }

// Key comparison
array_diff_ukey($array1, $array2, $array3, "compareKey");
array_diff_uassoc($array1, $array2, $array3, "compareKey");
array_intersect_ukey($array1, $array2, $array3, "compareKey");
array_intersect_uassoc($array1, $array2, $array3, "compareKey");

// Key+value comparison
array_udiff_uassoc($array1, $array2, $array3, "compareKey", "compareValue");
array_uintersect_uassoc($array1, $array2, $array3, "compareKey", "compareValue");

// Value comparison
array_udiff($array1, $array2, $array3, "compareValue");
array_udiff_assoc($array1, $array2, $array3,  "compareValue");
array_uintersect($array1, $array2, $array3, "compareValue");
array_uintersect_assoc($array1, $array2, $array3,  "compareValue");
                
