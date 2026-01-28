<?php
function getInt(): int{return 0;}
$a = getInt();
assert($a >= 500);
assert($a < 5000);
$b = $a % 10;
$c = $a ** 2;
$d = $a - 5;
$e = $a * 1;
