<?php
/** @var list<int> */
$keys = [1, 2, 3];
$a = array_fill_keys($keys, true);

$keys = [1, 2, 3];
$b = array_fill_keys($keys, true);

$keys = [0, 1, 2];
$c = array_fill_keys($keys, true);

$keys = random_int(0, 1) ? [0] : [0, 1];
$d = array_fill_keys($keys, true);

$keys = random_int(0, 1) ? ["a"] : ["a", "b"];
$e = array_fill_keys($keys, true);
                
