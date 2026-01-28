<?php
function getInt(): int{return 0;}
$a = $b = $c = $d = getInt();
assert($a >= 2);//positive range
assert($b <= -2);//negative range
/** @var int<0, 0> $c */; // 0 range
assert($d >= -100);// mixed range
assert($d <= 100);// mixed range

$e = 0 ** $a;
$f = 0 ** $b;
$g = 0 ** $c;
$h = 0 ** $d;
$i = (-2) ** $a;
$j = (-2) ** $b;
$k = (-2) ** $c;
$l = (-2) ** $d;
$m = 2 ** $a;
$n = 2 ** $b;
$o = 2 ** $c;
$p = 2 ** $d;
$q = $a ** 0;
$r = $a ** 2;
$s = $a ** -2;
$t = $b ** 0;
$u = $b ** 2;
$v = $b ** -2;
$w = $c ** 0;
$x = $c ** 2;
$y = $c ** -2;
$z = $d ** 0;
$aa = $d ** 2;
$ab = $d ** -2;
