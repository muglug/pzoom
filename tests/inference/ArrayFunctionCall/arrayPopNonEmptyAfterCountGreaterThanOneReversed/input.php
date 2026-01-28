<?php
/** @var array<string, int> */
$a = ["a" => 5, "b" => 6, "c" => 7];
$b = 5;
if (0 < count($a)) {
    $b = array_pop($a);
}
