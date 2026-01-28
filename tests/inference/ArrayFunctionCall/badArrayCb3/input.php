<?php
$array1 = array("a" => "green", "b" => "brown", "c" => "blue", "red");
$array2 = array("a" => "GREEN", "B" => "brown", "yellow", "red");
$array3 = array("a" => "GREEN");

function compareKey(int $a): int { return $a; }

// Value comparison
array_udiff($array1, $array2, $array3, "compareKey");
                
