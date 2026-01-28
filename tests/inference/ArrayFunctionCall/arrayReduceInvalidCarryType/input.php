<?php
$arr = [2, 3, 4, 5];

$direct_closure_result = array_reduce(
    $arr,
    function (stdClass $carry, int $item) {
        return $_GET["boo"];
    },
    1
);
