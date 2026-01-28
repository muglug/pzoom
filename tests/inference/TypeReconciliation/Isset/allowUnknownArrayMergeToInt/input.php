<?php
$arr = [1, 1, 1, 1, 2, 5, 3, 2];
$cumulative = [];

foreach ($arr as $val) {
    if (isset($cumulative[$val])) {
        $cumulative[$val] = array_merge($cumulative[$val], [$val]);
    } else {
        $cumulative[$val] = [$val];
    }
}

foreach ($cumulative as $arr) {
    foreach ($arr as $val) {
        takesInt($val);
    }
}

function takesInt(int $i) : void {}