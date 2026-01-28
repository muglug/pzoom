<?php
$result = [];

foreach (["a", "b"] as $k) {
    $result[$k] = true;
}

$resultOpt = [];

foreach (["a", "b"] as $k) {
    if (random_int(0, 1)) {
        continue;
    }
    $resultOpt[$k] = true;
}
