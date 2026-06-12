<?php
function foo(int $i, int $j) : void {
    $arr = [];
    $arr[0] = rand(0, 1);
    $arr[1] = rand(0, 1);

    if (empty($arr[$i]) && empty($arr[$j])) {}
}
