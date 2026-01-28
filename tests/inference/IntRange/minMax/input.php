<?php
function getInt(): int{return 0;}
$a = $b = $c = $d = $e = getInt();
assert($b > 10);
assert($c < -15);
assert($d === 20);
assert($e > 0);
$f = min($a, $b, $c, $d);
$g = min($b, $c, $d);
$h = min($d, $e);
$i = max($b, $c, $d);
$j = max($d, $e);
$k = max($e, 40);
$l = min($a, ...[$b, $c], $d);
$m = max(...[$a, ...[$b, $c]], $d);
                    
