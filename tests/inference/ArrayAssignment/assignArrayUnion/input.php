<?php
/**
 */
function foo(array $out) : array {
    $key = 1;

    if (rand(0, 1)) {
        /** @var mixed */
        $key = null;
    }

    $out[$key] = 5;
    return $out;
}
