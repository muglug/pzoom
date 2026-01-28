<?php
/**
 * @param array{a?:int,b?:string} $p
 */
function f(array $p) : void {
    if (!$p) {
        throw new RuntimeException("");
    }
    assert(!!$p);
}
