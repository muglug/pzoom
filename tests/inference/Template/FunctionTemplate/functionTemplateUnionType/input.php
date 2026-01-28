<?php
/**
 * @template T0 as int|string
 * @param T0 $t
 * @return T0
 */
function foo($t) {
    return $t;
}

$s = foo("hello");
$i = foo(5);