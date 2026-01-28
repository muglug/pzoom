<?php
$a = [1, 2, 3];
$c = $a;
$b = ["a", "b", "c"];
array_splice($a, rand(-10, 0), rand(0, 10), $b);
