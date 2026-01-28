<?php
class A {
    public int $x = 0;
}

/**
 * @template T as A
 * @param T $obj
 * @param-out T $obj
 */
function foo(A &$obj): void {
    $obj->x = 1;
}