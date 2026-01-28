<?php
/** @var array{0: string, 1: int} **/
$a = ["hello", 5];
$b = $a[0]; // string
$c = $a[1]; // int
list($d, $e) = $a; // $d is string, $e is int
