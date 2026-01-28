<?php
$vars = ["x" => "a", "y" => "b"];
$b = 3;
$c = array_rand($vars, 1);
$d = array_rand($vars, 2);
$e = array_rand($vars, 3);
$f = array_rand($vars, $b);
