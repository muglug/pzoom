<?php
/** @var array<string, int> */
$a = ["a" => 5, "b" => 6, "c" => 7];
$b = 5;
if (count($a) > 0) {
    $b = array_pop($a);
}
