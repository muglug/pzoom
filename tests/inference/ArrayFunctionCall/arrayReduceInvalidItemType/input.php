<?php
$arr = [2, 3, 4, 5];

$direct_closure_result = array_reduce(
    $arr,
    function (int $carry, stdClass $item) {
        return $_GET["boo"];
    },
    1
);
