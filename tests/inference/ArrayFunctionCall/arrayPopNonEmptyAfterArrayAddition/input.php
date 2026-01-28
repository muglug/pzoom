<?php
/** @var array<string, int> */
$a = ["a" => 5, "b" => 6, "c" => 7];
$a["foo"] = 10;
$b = array_pop($a);
