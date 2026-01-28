<?php
$out = [];

$bits = [];

foreach ([1, 2, 3, 4, 5] as $value) {
    if (rand(0,100) > 50) {
        $out[] = $bits;
        $bits = [];
    }

    $bits[] = 4;
}

$out[] = $bits;
