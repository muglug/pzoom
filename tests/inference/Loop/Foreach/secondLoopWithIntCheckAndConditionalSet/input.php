<?php
/** @return void **/
function takesInt(int $i) {}

$a = null;

foreach ([1, 2, 3] as $i) {
    if (is_int($a)) takesInt($a);

    if (rand(0, 1)) {
        $a = $i;
    }
}
