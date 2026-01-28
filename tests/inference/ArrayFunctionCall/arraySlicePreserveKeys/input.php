<?php
$a = ["a" => 1, "b" => 2, "c" => 3];
$b = array_slice($a, 1, 2, true);
$c = array_slice($a, 1, 2, false);
$d = array_slice($a, 1, 2);
