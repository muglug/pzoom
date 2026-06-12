<?php
$func = function(int $arg1, int $arg2) : int {
    return $arg1 * $arg2;
};

$a = call_user_func($func, 2, 4);
