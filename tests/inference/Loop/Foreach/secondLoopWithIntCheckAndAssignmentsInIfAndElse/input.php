<?php
/** @return void **/
function takesInt(int $i) {}

$a = null;

foreach ([1, 2, 3] as $i) {
    if (is_int($a)) {
        $a = 6;
    } else {
        $a = $i;
    }
}
