<?php
function getInt(): int{return 0;}
$a = $b = $c = $d = $e = $f = $g = $h = $i = $j = $k = $l = $m = $n = $o = $p = getInt();
assert($b <= -2);
assert($c <= 2);
assert($d >= -2);
assert($e >= 2);
assert($f >= -2);
assert($f <= 2);

$g = $a * $b;
$h = $a * $c;
$i = $a * $d;
$j = $a * $e;
$k = $a * $f;
$l = $b * $b;
$m = $b * $c;
$n = $b * $d;
$o = $b * $e;
$p = $b * $f;
$q = $c * $c;
$r = $c * $d;
$s = $c * $e;
$t = $c * $f;
$u = $d * $d;
$v = $d * $e;
$w = $d * $f;
$x = $e * $e;
$y = $d * $f;
$z = $f * $f;
