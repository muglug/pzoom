<?php
/**
 * @template T
 *
 * @param Closure():T $t1
 * @param T $t2
 */
function foo(Closure $t1, $t2) : void {}
foo(
    function () : int {
        return 5;
    },
    "hello"
);