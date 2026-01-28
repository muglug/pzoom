<?php
$arr = [2, 3, 4, 5];

$direct_closure_result = array_reduce(
    $arr,
    function (int $carry, int $item) {
        return $GLOBALS["boo"];
    },
    1
);
