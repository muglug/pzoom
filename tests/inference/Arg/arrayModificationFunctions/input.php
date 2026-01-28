<?php
$a = ["b" => 5, "a" => 8];
array_unshift($a, (bool)rand(0, 1));
$b = ["b" => 5, "a" => 8];
array_push($b, (bool)rand(0, 1));
                
