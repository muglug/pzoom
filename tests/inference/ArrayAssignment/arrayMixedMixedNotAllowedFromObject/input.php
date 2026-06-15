<?php
function foo(ArrayObject $a) : array {
    $arr = [];

    /**
     */
    foreach ($a as $k => $v) {
        $arr[$k] = $v;
    }

    return $arr;
}
