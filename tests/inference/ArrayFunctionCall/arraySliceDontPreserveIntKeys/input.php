<?php
$a = [1 => "a", 4 => "b", 3 => "c"];
$b = array_slice($a, 1, 2, true);
$c = array_slice($a, 1, 2, false);
$d = array_slice($a, 1, 2);
