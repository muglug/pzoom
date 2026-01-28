<?php
function takesString(string $string): void {}

$date = new DateTime();

$a = [$date->format("Y-m-d")];

takesString($a[0]);
array_map("takesString", $a);
