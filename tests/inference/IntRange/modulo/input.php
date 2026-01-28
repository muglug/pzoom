<?php
function getInt(): int{return 0;}
$a = getInt();
$b = $a % 10;
assert($a > 0);
$c = $a % 10;
$d = $a % $a;
