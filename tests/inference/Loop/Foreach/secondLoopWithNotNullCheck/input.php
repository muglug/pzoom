<?php
/** @return void **/
function takesInt(int $i) {}

$a = null;

foreach ([1, 2, 3] as $i) {
    if ($a !== null) takesInt($a);
    $a = $i;
}
