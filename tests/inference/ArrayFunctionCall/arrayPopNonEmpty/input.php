<?php
/** @var array<string, int> */
$a = ["a" => 5, "b" => 6, "c" => 7];
$b = 5;
if ($a) {
    $b = array_pop($a);
}
$c = array_pop($a);
