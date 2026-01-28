<?php
function getInt(): int{return 0;}
$a = $b = $c = getInt();
assert($a >= 500);
assert($a < 5000);
assert($b >= -5000);
assert($b < -501);
assert(-60 > $c);
assert(-500 < $c);
