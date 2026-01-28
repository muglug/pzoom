<?php
function getInt(): int{return 0;}
$a = $b = $c = $d = getInt();
assert($a >= 20);//positive range
assert($b <= -20);//negative range
/** @var int<0, 0> $c */; // 0 range
assert($d >= -100);// mixed range
assert($d <= 100);// mixed range
/** @var int<5, 5> $e */; // 5 range

$f = $a % $e;
$g = $b % $e;
$h = $d % $e;
$i = -3 % $a;
$j = -3 % $b;
/** @psalm-suppress NoValue */
$k = -3 % $c;
$l = -3 % $d;
$m = 3 % $a;
$n = 3 % $b;
/** @psalm-suppress NoValue */
$o = 3 % $c;
$p = 3 % $d;
/** @psalm-suppress NoValue */
$q = $a % 0;
$r = $a % 3;
$s = $a % -3;
/** @psalm-suppress NoValue */
$t = $b % 0;
$u = $b % 3;
$v = $b % -3;
/** @psalm-suppress NoValue */
$w = $c % 0;
$x = $c % 3;
$y = $c % -3;
/** @psalm-suppress NoValue */
$z = $d % 0;
$aa = $d % 3;
$ab = $d % -3;
