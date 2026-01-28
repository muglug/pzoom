<?php
$arr = [2, 3, 4, 5];

function multiply (int $carry, int $item) : int {
    return $carry * $item;
}

$f2 = function (int $carry, int $item) : int {
    return $carry * $item;
};

$direct_closure_result = array_reduce(
    $arr,
    function (int $carry, int $item) : int {
        return $carry * $item;
    },
    1
);

$passed_closure_result = array_reduce(
    $arr,
    $f2,
    1
);

$function_call_result = array_reduce(
    $arr,
    "multiply",
    1
);
