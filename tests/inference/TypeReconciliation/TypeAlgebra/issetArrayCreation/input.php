<?php
$arr = [];

foreach ([0, 1, 2, 3] as $i) {
    $a = (int) (rand(0, 1) ? 5 : "010");

    if (!isset($arr[$a])) {
        $arr[$a] = 5;
    } else {
        $arr[$a] += 4;
    }
}
