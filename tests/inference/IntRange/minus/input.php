<?php
function getInt(): int{return 0;}
$a = $b = $d = $e = getInt();
assert($a > 5);
assert($a <= 10);
assert($b > -10);
assert($b <= 100);
$c = $a - $b;
$f = $a - $d;
assert($e > 0);
$g = $a - $e;
                    
