<?php
/**
 * @param pure-callable $c
 */
function foo(callable $c) : void {
    if (is_object($c)) {
        $c();
    }
}
