<?php
/** @var array<string, int> */
$a = ["a" => 5, "b" => 6, "c" => 7];
$b = 5;
if (isset($a["a"])) {
    $b = array_pop($a);
}
