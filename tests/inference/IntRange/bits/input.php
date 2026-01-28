<?php
function getInt(): int{return 0;}
$a = $b = $c = getInt();
assert($a > 5);
assert($b <= 6);
$d = $a ^ $b;
$e = $a & $b;
$f = $a | $b;
$g = $a << $b;
$h = $a >> $b;
                    
