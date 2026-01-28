<?php
$a = [1, 2, 3, 4, 5];
$b = ["one", "two", "three", "four", "five"];
$c = ["uno", "dos", "tres", "cuatro", "cinco", "seis"];

$d = array_map(null, $a, $b, $c);
