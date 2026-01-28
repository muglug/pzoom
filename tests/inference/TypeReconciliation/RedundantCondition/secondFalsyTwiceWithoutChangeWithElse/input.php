<?php
/**
 * @param array{a?:int,b?:string} $p
 */
function f(array $p) : void {
    if (!$p) {
        throw new RuntimeException("");
    } else {}
    assert(!!$p);
}
