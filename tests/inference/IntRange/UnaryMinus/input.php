<?php
function getInt(): int{return 0;}
$a = $c = $e = getInt();
assert($a > 5);
$b = -$a;
assert($c > 0);
$d = -$c;
assert($e > 5);
assert($e < 10);
$f = -$e;
                    
