<?php
/**
 * @template T
 *
 * @param Closure(T):void $t1
 * @param T $t2
 */
function apply(Closure $t1, $t2) : void
{
    $t1($t2);
}

apply(function(int $_i) : void {}, "hello");
