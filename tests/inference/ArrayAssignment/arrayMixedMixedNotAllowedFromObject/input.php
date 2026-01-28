<?php
function foo(ArrayObject $a) : array {
    $arr = [];

    /**
     * @psalm-suppress MixedAssignment
     */
    foreach ($a as $k => $v) {
        $arr[$k] = $v;
    }

    return $arr;
}
