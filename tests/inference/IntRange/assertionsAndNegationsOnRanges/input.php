<?php
/** @var int $int */
$int = 1;
$a = $b = $c = $d = $e = $f = $g = $h = $int;

if ($a < 1) {
    $res1 = $a; //should be int<min, 0>
    throw new Exception();
}

$res2 = $a; //should be int<1, max>

if ($b > 1) {
    $res3 = $b; //should be int<2, max>
    throw new Exception();
}

$res4 = $b; //should be int<min, 1>

if ($c <= 1) {
    $res5 = $c; //should be int<min, 1>
    throw new Exception();
}

$res6 = $c; //should be int<2, max>

if ($d >= 1) {
    $res7 = $d; //should be int<1, max>
    throw new Exception();
}

$res8 = $d; //should be int<min, 0>



if (1 < $e) {
    $res9 = $e; //should be int<2, max>
    throw new Exception();
}

$res10 = $e; //should be int<min, 1>

if (1 > $f) {
    $res11 = $f; //should be int<min, 0>
    throw new Exception();
}

$res12 = $f; //should be int<1, max>

if (1 <= $g) {
    $res13 = $g; //should be int<1, max>
    throw new Exception();
}

$res14 = $g; //should be int<min, 0>

if (1 >= $h) {
    $res15 = $h; //should be int<min, 1>
    throw new Exception();
}

$res16 = $h; //should be int<2, max>
