<?php
$a = random_int(0,1) ? [] : [0, 1];

$b = null;
if (count($a) === 2) {
    $b = $a;
}
