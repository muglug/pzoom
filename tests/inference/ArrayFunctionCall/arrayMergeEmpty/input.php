<?php

$test = [[]];
$a = array_merge(...$test);

$test = [[], ["test" => 0]];
$b = array_merge(...$test);
                
