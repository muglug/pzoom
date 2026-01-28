<?php
/**
 * @param array{a?:int,b?:string} $p
 */
function f(array $p) : void {
    if (!$p) {
        throw new RuntimeException("");
    }
    if (rand(0, 1)) {
        $p["a"] = 3;
    }
    assert(!!$p);
}
