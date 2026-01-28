<?php
/**
 * @template T
 * @param T::class $s
 * @return T::class
 */
function foo(string $s) : string {
    return $s;
}

/**
 * @param  A::class $s
 */
function bar(string $s) : void {
}

class A {}

bar(foo(A::class));