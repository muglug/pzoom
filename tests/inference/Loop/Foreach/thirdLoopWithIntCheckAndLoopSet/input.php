<?php
/** @return void **/
function takesInt(int $i) {}

$a = null;
$b = null;

foreach ([1, 2, 3] as $i) {
    if ($b !== null) {
        takesInt($b);
    }

    if ($a !== null) {
        takesInt($a);
        $b = $a;
    }

    $a = $i;
}
